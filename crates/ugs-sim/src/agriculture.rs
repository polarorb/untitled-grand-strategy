//! Agriculture & procurement: weather, collectivization, quotas, famine.
//! Spec: docs/design/systems/agriculture.md

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::CountryTag;

use crate::demography::{Demographics, LivingStandards, SimScenario};
use crate::economy::NationalBalances;
use crate::planning::{EconomicSystem, Economies};
use crate::rng::SimRng;
use crate::SimClock;

pub mod tuning {
    /// Harvest factor roll range, permille.
    pub const HARVEST_MIN: u64 = 850;
    pub const HARVEST_MAX: u64 = 1150;
    /// Quota extraction multipliers (Low/Normal/High), permille.
    pub const QUOTA_EXTRACTION: [u64; 3] = [900, 1000, 1100];
    /// Collectivization: permanent extraction bonus / yield malus, permille.
    pub const COLLECTIVE_EXTRACTION_BONUS: u64 = 150;
    pub const COLLECTIVE_YIELD_MALUS: u64 = 100;
    /// Transition shock: extra yield malus, and its duration in months.
    pub const TRANSITION_YIELD_MALUS: u64 = 250;
    pub const TRANSITION_MONTHS: u8 = 12;
    /// Famine threshold and death scaling: (threshold - ratio) * this,
    /// in ppm/year of excess deaths.
    pub const FAMINE_THRESHOLD: u64 = 750;
    pub const FAMINE_DEATHS_PPM_PER_POINT: u64 = 40;
    /// Shortage band: SoL penalty = (1000 - ratio) / this, applied 750..900.
    pub const SHORTAGE_SOL_DIVISOR: i64 = 20;
    pub const FAMINE_SOL_PENALTY: i64 = 15;
    pub const WELL_FED_SOL_BONUS: i64 = 2;
    /// Rural share of famine deaths under High quota, permille.
    pub const HIGH_QUOTA_RURAL_SHARE: u64 = 700;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quota {
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgriPolicy {
    pub collectivized: bool,
    pub quota: Quota,
    /// Months of collectivization transition shock remaining.
    pub shock_months: u8,
}

impl Default for AgriPolicy {
    fn default() -> Self {
        Self {
            collectivized: false,
            quota: Quota::Normal,
            shock_months: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgriStatus {
    pub harvest_permille: u64,
    pub food_ratio_permille: u64,
    pub famine: bool,
    pub famine_deaths: u64,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Agriculture {
    pub policy: BTreeMap<CountryTag, AgriPolicy>,
    pub status: BTreeMap<CountryTag, AgriStatus>,
    /// Permanent yield bonuses from completed projects
    /// (mechanization, Virgin Lands), permille.
    #[serde(default)]
    pub bonus_permille: BTreeMap<CountryTag, u64>,
}

impl Agriculture {
    /// Yield modifier applied to grain production, permille.
    pub fn yield_permille(&self, tag: &CountryTag) -> u64 {
        use tuning::*;
        let policy = self.policy.get(tag).copied().unwrap_or_default();
        let harvest = self
            .status
            .get(tag)
            .map(|s| s.harvest_permille)
            .filter(|&h| h > 0)
            .unwrap_or(1000);
        let mut modifier = harvest;
        if policy.collectivized {
            modifier = modifier.saturating_sub(COLLECTIVE_YIELD_MALUS);
        }
        modifier += self.bonus_permille.get(tag).copied().unwrap_or(0);
        if policy.shock_months > 0 {
            modifier = modifier.saturating_sub(TRANSITION_YIELD_MALUS);
        }
        modifier.max(300)
    }

    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (tag, b) in &self.bonus_permille {
            h = (h ^ tag.0.bytes().map(u64::from).sum::<u64>() ^ *b)
                .wrapping_mul(0x0000_0100_0000_01b3);
        }
        for (tag, s) in &self.status {
            for v in [
                tag.0.bytes().map(u64::from).sum::<u64>(),
                s.harvest_permille,
                s.food_ratio_permille,
                s.famine_deaths,
            ] {
                h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// Monthly: roll weather each January, decay transition shocks, compute
/// food ratios from the balances, and apply consequences (SoL, famine
/// deaths). Runs LAST in the Economy chain.
#[allow(clippy::too_many_arguments)]
pub fn update_agriculture(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    balances: Res<NationalBalances>,
    econ: Res<Economies>,
    mut rng: ResMut<SimRng>,
    mut agri: ResMut<Agriculture>,
    mut demo: ResMut<Demographics>,
    mut sol: ResMut<LivingStandards>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    if data.regions.is_empty() {
        return;
    }

    if agri.policy.is_empty() {
        for tag in data.countries.keys() {
            agri.policy.insert(tag.clone(), AgriPolicy::default());
            agri.status.insert(
                tag.clone(),
                AgriStatus {
                    harvest_permille: 1000,
                    food_ratio_permille: 1000,
                    ..Default::default()
                },
            );
        }
        return;
    }

    if !clock.new_month {
        return;
    }

    use tuning::*;

    // January: roll this year's harvests (deterministic: one forked
    // stream, countries in BTreeMap order).
    if clock.date.month == 1 {
        let mut stream = rng.fork(b"harvest");
        for status in agri.status.values_mut() {
            let span = (HARVEST_MAX - HARVEST_MIN) as u32;
            status.harvest_permille = HARVEST_MIN + stream.below(span + 1) as u64;
        }
    }

    // Transition shocks tick down.
    for policy in agri.policy.values_mut() {
        policy.shock_months = policy.shock_months.saturating_sub(1);
    }

    // Consequences per country.
    let tags: Vec<CountryTag> = agri.status.keys().cloned().collect();
    let mut famine_deaths_by_country: BTreeMap<CountryTag, (u64, u64)> = BTreeMap::new();
    for tag in &tags {
        let policy = agri.policy.get(tag).copied().unwrap_or_default();
        let grain_ratio = balances
            .by_country
            .get(tag)
            .map(|b| b.grain_ratio_permille())
            .unwrap_or(1000);
        let extraction = {
            let mut e = QUOTA_EXTRACTION[policy.quota as usize];
            if policy.collectivized {
                e += COLLECTIVE_EXTRACTION_BONUS;
            }
            e
        };
        let ratio = grain_ratio * extraction / 1000;

        let status = agri.status.get_mut(tag).unwrap();
        status.food_ratio_permille = ratio;
        status.famine = ratio < FAMINE_THRESHOLD;

        // SoL adjustment on top of what planning wrote this month.
        let adjust: i64 = if ratio >= 1000 {
            WELL_FED_SOL_BONUS
        } else if ratio >= 900 {
            0
        } else if ratio >= FAMINE_THRESHOLD {
            -(((1000 - ratio) as i64) / SHORTAGE_SOL_DIVISOR)
        } else {
            -FAMINE_SOL_PENALTY
        };
        if adjust != 0 {
            if let Some(v) = sol.by_country.get_mut(tag) {
                *v = (*v + adjust).clamp(5, 85);
            }
        }

        if status.famine {
            let severity = FAMINE_THRESHOLD - ratio; // points below threshold
            let annual_ppm = severity * FAMINE_DEATHS_PPM_PER_POINT;
            let rural_share = if policy.quota == Quota::High {
                HIGH_QUOTA_RURAL_SHARE
            } else {
                500
            };
            famine_deaths_by_country.insert(tag.clone(), (annual_ppm, rural_share));
        }
    }

    // Apply famine deaths to cohorts (monthly fraction of the annual rate).
    if !famine_deaths_by_country.is_empty() {
        for (id, p) in &data.provinces {
            let Some(&(annual_ppm, rural_share)) = famine_deaths_by_country.get(&p.owner) else {
                continue;
            };
            let Some(c) = demo.provinces.get_mut(id) else {
                continue;
            };
            let monthly = |pop: u64, share: u64| {
                (pop as u128 * annual_ppm as u128 * share as u128 / 1000 / 12_000_000) as u64
            };
            let rural_dead = monthly(c.rural, rural_share) * 2; // share-weighted
            let urban_dead = monthly(c.urban, 1000 - rural_share) * 2;
            c.rural = c.rural.saturating_sub(rural_dead);
            c.urban = c.urban.saturating_sub(urban_dead);
            if let Some(status) = agri.status.get_mut(&p.owner) {
                status.famine_deaths += rural_dead + urban_dead;
            }
        }
    }

    // Planned economies with High quotas divert grain to the state; the
    // economy sees it as slightly better material supply (future: exports).
    let _ = &econ;
}

/// Command handler (planned economies only).
pub fn set_agri_policy(
    agri: &mut Agriculture,
    econ: &Economies,
    country: &CountryTag,
    collectivized: bool,
    quota: Quota,
) {
    if econ.system.get(country) != Some(&EconomicSystem::Planned) {
        return;
    }
    let policy = agri.policy.entry(country.clone()).or_default();
    // Collectivization is a one-way ratchet in v1; switching it on
    // triggers the transition shock.
    if collectivized && !policy.collectivized {
        policy.collectivized = true;
        policy.shock_months = tuning::TRANSITION_MONTHS;
    }
    policy.quota = quota;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::GameDate;
    use crate::command::{PendingCommands, SimCommand};
    use crate::{run_ticks, SimPlugin};
    use bevy_app::App;
    use std::path::Path;
    use std::sync::Arc;

    fn app_with_scenario() -> App {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 7,
        });
        app.insert_resource(SimScenario(Arc::new(data)));
        app
    }

    fn sov() -> CountryTag {
        CountryTag("SOV".into())
    }

    #[test]
    fn harvests_vary_by_year_and_are_deterministic() {
        let year =
            |app: &mut App| app.world().resource::<Agriculture>().status[&sov()].harvest_permille;
        let mut a = app_with_scenario();
        let mut b = app_with_scenario();
        run_ticks(&mut a, 24 * 40);
        run_ticks(&mut b, 24 * 40);
        let (ya, yb) = (year(&mut a), year(&mut b));
        assert_eq!(ya, yb, "same seed, same weather");
        assert!((tuning::HARVEST_MIN..=tuning::HARVEST_MAX).contains(&ya));
        // Roll into next year: factor should (with this seed) change.
        run_ticks(&mut a, 24 * 366);
        assert_ne!(year(&mut a), ya, "different year, different harvest");
    }

    #[test]
    fn collectivization_shock_cuts_yield() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 40);
        let before = app.world().resource::<Agriculture>().yield_permille(&sov());
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetAgriPolicy {
                country: sov(),
                collectivized: true,
                quota: Quota::High,
            });
        run_ticks(&mut app, 24 * 40);
        let during = app.world().resource::<Agriculture>().yield_permille(&sov());
        assert!(
            during <= before.saturating_sub(300),
            "transition shock: {before} -> {during}"
        );
        // After the shock passes, permanent malus only.
        run_ticks(&mut app, 24 * 366);
        let after = app.world().resource::<Agriculture>().yield_permille(&sov());
        assert!(after > during, "shock should expire: {during} -> {after}");
    }

    #[test]
    fn famine_is_reachable_and_kills() {
        // Collectivize with High quotas and hope for bad weather over a
        // few years — with this seed, somewhere in the world (or in the
        // SOV itself) famine must occur.
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 40);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetAgriPolicy {
                country: sov(),
                collectivized: true,
                quota: Quota::High,
            });
        run_ticks(&mut app, 24 * 366 * 4);
        let agri = app.world().resource::<Agriculture>();
        let total_famine_deaths: u64 = agri.status.values().map(|s| s.famine_deaths).sum();
        assert!(total_famine_deaths > 0, "no famine anywhere in 4 years");
    }

    #[test]
    fn market_economies_cannot_collectivize() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 1);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetAgriPolicy {
                country: CountryTag("USA".into()),
                collectivized: true,
                quota: Quota::High,
            });
        run_ticks(&mut app, 1);
        let agri = app.world().resource::<Agriculture>();
        assert!(!agri.policy[&CountryTag("USA".into())].collectivized);
    }
}
