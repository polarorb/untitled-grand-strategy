//! The two planning interfaces: planned economies set quantities, market
//! economies set parameters, over the shared production substrate.
//! Spec: docs/design/systems/planning-interfaces.md

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{Alignment, CountryTag};

use crate::demography::{Demographics, LivingStandards, SimScenario};
use crate::economy::{EconomyStatic, NationalBalances, RegionalPower};
use crate::SimClock;

pub mod tuning {
    /// Planned default allocation (permille): heavy-industry tilt.
    pub const PLANNED_DEFAULT: (u16, u16, u16) = (350, 450, 200);
    /// Market defaults: interest 4.00%, tax 25.0%, procurement Med.
    pub const MARKET_DEFAULT_INTEREST_BP: u16 = 400;
    pub const MARKET_DEFAULT_TAX_PERMILLE: u16 = 250;
    /// Market investment share = BASE + (PIVOT - interest_bp) * SLOPE/100,
    /// clamped. At 4%: 240 permille.
    pub const MARKET_INVEST_BASE: i64 = 200;
    pub const MARKET_INVEST_PIVOT_BP: i64 = 500;
    pub const MARKET_INVEST_SLOPE_CENTI: i64 = 40;
    pub const MARKET_INVEST_MIN: i64 = 100;
    pub const MARKET_INVEST_MAX: i64 = 450;
    /// Military share by procurement level (Low/Med/High), permille.
    pub const PROCUREMENT_PERMILLE: [u16; 3] = [100, 200, 300];
    /// Industry gained per point of invested output (permille).
    pub const INVEST_CONVERT_PERMILLE: u64 = 45;
    /// Monthly depreciation of industry, permille.
    pub const DEPRECIATION_PERMILLE: u64 = 2;
    /// Misreporting: max padding of reported over actual, permille.
    pub const MISREPORT_CAP_PERMILLE: u64 = 150;
    /// Fraction of a growth shortfall that gets papered over, permille.
    pub const MISREPORT_PAD_PERMILLE: u64 = 300;
    /// Inflation: target = max(0, PIVOT - interest_bp/2)
    ///                   + max(0, procurement - tax) (permille-ish units).
    pub const INFLATION_RATE_PIVOT: i64 = 200;
    /// SoL penalty divisor: penalty = inflation / this.
    pub const INFLATION_SOL_DIVISOR: i64 = 25;
    /// SoL consumer component: consumer_output * K / pop_k.
    pub const SOL_CONSUMER_K: u64 = 116_000;
    pub const SOL_URBAN_WEIGHT: u64 = 40;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EconomicSystem {
    Planned,
    Market,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Procurement {
    Low,
    Med,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Policy {
    Planned {
        /// Permille shares, must sum to 1000.
        consumer: u16,
        investment: u16,
        military: u16,
    },
    Market {
        interest_bp: u16,
        tax_permille: u16,
        procurement: Procurement,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndustryState {
    /// True industrial base, centi-points (CountryDef.industry * 100).
    pub actual_centi: u64,
    /// What the statistics bureau claims (== actual for market systems).
    pub reported_centi: u64,
    /// Last month's true growth in centi-points (signed).
    pub last_growth_centi: i64,
    /// Smoothed inflation (market failure mode), permille-ish.
    pub inflation: i64,
    /// Military output accumulated (inert until military systems).
    pub military_stock: u64,
}

/// Per-country economic system, policy, and industry state.
#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Economies {
    pub system: BTreeMap<CountryTag, EconomicSystem>,
    pub policy: BTreeMap<CountryTag, Policy>,
    pub industry: BTreeMap<CountryTag, IndustryState>,
}

impl Economies {
    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (tag, st) in &self.industry {
            for v in [
                tag.0.bytes().map(u64::from).sum::<u64>(),
                st.actual_centi,
                st.reported_centi,
                st.inflation.unsigned_abs(),
                st.military_stock,
            ] {
                h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }

    /// The industry figure this country's own dashboard shows.
    pub fn dashboard_industry_centi(&self, tag: &CountryTag) -> u64 {
        match self.system.get(tag) {
            Some(EconomicSystem::Planned) => self
                .industry
                .get(tag)
                .map(|s| s.reported_centi)
                .unwrap_or(0),
            _ => self.industry.get(tag).map(|s| s.actual_centi).unwrap_or(0),
        }
    }

    /// What a foreign observer with `economic_penetration` (permille)
    /// believes `tag`'s industry to be: the reported figure at zero
    /// intel, sliding toward the truth as penetration deepens. For
    /// market systems reported == actual, so intel changes nothing.
    pub fn observed_industry_centi(&self, tag: &CountryTag, penetration: u32) -> u64 {
        let Some(st) = self.industry.get(tag) else {
            return 0;
        };
        let reported = st.reported_centi as i128;
        let actual = st.actual_centi as i128;
        let p = penetration.min(1000) as i128;
        (reported + (actual - reported) * p / 1000).max(0) as u64
    }
}

fn default_policy(system: EconomicSystem) -> Policy {
    match system {
        EconomicSystem::Planned => Policy::Planned {
            consumer: tuning::PLANNED_DEFAULT.0,
            investment: tuning::PLANNED_DEFAULT.1,
            military: tuning::PLANNED_DEFAULT.2,
        },
        EconomicSystem::Market => Policy::Market {
            interest_bp: tuning::MARKET_DEFAULT_INTEREST_BP,
            tax_permille: tuning::MARKET_DEFAULT_TAX_PERMILLE,
            procurement: Procurement::Med,
        },
    }
}

/// Permille split (consumer, investment, military) from policy.
fn split(policy: &Policy) -> (u64, u64, u64) {
    use tuning::*;
    match policy {
        Policy::Planned {
            consumer,
            investment,
            military,
        } => (*consumer as u64, *investment as u64, *military as u64),
        Policy::Market {
            interest_bp,
            procurement,
            ..
        } => {
            let invest = (MARKET_INVEST_BASE
                + (MARKET_INVEST_PIVOT_BP - *interest_bp as i64) * MARKET_INVEST_SLOPE_CENTI / 100)
                .clamp(MARKET_INVEST_MIN, MARKET_INVEST_MAX) as u64;
            let military = PROCUREMENT_PERMILLE[*procurement as usize] as u64;
            let consumer = 1000u64.saturating_sub(invest + military);
            (consumer, invest, military)
        }
    }
}

/// Monthly production, growth, misreporting, inflation, and SoL update.
/// Runs after `update_economy` (needs balances & power).
#[allow(clippy::too_many_arguments)]
pub fn update_production(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    demo: Res<Demographics>,
    stat: Res<EconomyStatic>,
    balances: Res<NationalBalances>,
    power: Res<RegionalPower>,
    military: Res<crate::military::Military>,
    mut econ: ResMut<Economies>,
    mut sol: ResMut<LivingStandards>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    if data.regions.is_empty() {
        return;
    }

    if econ.system.is_empty() {
        for (tag, country) in &data.countries {
            let system = if country.alignment == Alignment::EasternBloc || tag.0 == "YUG" {
                EconomicSystem::Planned
            } else {
                EconomicSystem::Market
            };
            econ.system.insert(tag.clone(), system);
            econ.policy.insert(tag.clone(), default_policy(system));
            let centi = country.industry as u64 * 100;
            econ.industry.insert(
                tag.clone(),
                IndustryState {
                    actual_centi: centi,
                    reported_centi: centi,
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

    // Country-average power factor, weighted by regional industry.
    let mut power_num: BTreeMap<&CountryTag, u64> = BTreeMap::new();
    let mut power_den: BTreeMap<&CountryTag, u64> = BTreeMap::new();
    for (region, industry) in &stat.region_industry {
        let owner = &stat.region_owner[region];
        let factor = power
            .by_region
            .get(region)
            .map(|s| s.factor_permille)
            .unwrap_or(1000);
        *power_num.entry(owner).or_default() += industry * factor;
        *power_den.entry(owner).or_default() += *industry;
    }

    // Country populations for SoL.
    let mut country_pop_k: BTreeMap<&CountryTag, u64> = BTreeMap::new();
    let mut country_urban_k: BTreeMap<&CountryTag, u64> = BTreeMap::new();
    for (id, p) in &data.provinces {
        if let Some(c) = demo.provinces.get(id) {
            *country_pop_k.entry(&p.owner).or_default() += c.total() / 1000;
            *country_urban_k.entry(&p.owner).or_default() += (c.urban + c.educated) / 1000;
        }
    }

    let tags: Vec<CountryTag> = econ.industry.keys().cloned().collect();
    for tag in tags {
        let system = econ.system[&tag];
        let policy = econ.policy[&tag];
        let (consumer_pm, invest_pm, military_pm) = split(&policy);

        let power_factor = match (power_num.get(&tag), power_den.get(&tag)) {
            (Some(num), Some(den)) if *den > 0 => num / den,
            _ => 1000,
        };
        let materials = balances
            .by_country
            .get(&tag)
            .map(|b| b.coal_ratio_permille().min(1000))
            .unwrap_or(1000)
            .clamp(500, 1000);

        // Occupied home provinces produce nothing for anyone: the
        // holder gains no output (integration is a treaty matter), and
        // the owner has lost it (war-termination design).
        let (held, total) =
            data.provinces
                .values()
                .filter(|p| p.owner == tag)
                .fold((0u64, 0u64), |(h, t), p| {
                    let holder = military.owner_of(p.id, &p.owner);
                    (h + (holder == tag) as u64, t + 1)
                });
        let held_permille = (held * 1000).checked_div(total).unwrap_or(1000);
        let st = econ.industry.get_mut(&tag).unwrap();
        // Effective monthly output in centi-points.
        let output =
            st.actual_centi * power_factor / 1000 * materials / 1000 * held_permille / 1000;
        let consumer_out = output * consumer_pm / 1000;
        let invest_out = output * invest_pm / 1000;
        st.military_stock += output * military_pm / 1000 / 100;

        // Growth minus depreciation.
        let gained = invest_out * INVEST_CONVERT_PERMILLE / 1000;
        let lost = st.actual_centi * DEPRECIATION_PERMILLE / 1000;
        let growth = gained as i64 - lost as i64;
        st.actual_centi = st.actual_centi.saturating_add_signed(growth);
        st.last_growth_centi = growth;

        // Inflation (market) and reported statistics (planned).
        match system {
            EconomicSystem::Market => {
                let (interest_bp, tax_pm, proc) = match policy {
                    Policy::Market {
                        interest_bp,
                        tax_permille,
                        procurement,
                    } => (
                        interest_bp as i64,
                        tax_permille as i64,
                        PROCUREMENT_PERMILLE[procurement as usize] as i64,
                    ),
                    _ => unreachable!(),
                };
                let target =
                    (INFLATION_RATE_PIVOT - interest_bp / 2).max(0) + (proc - tax_pm).max(0);
                st.inflation = (st.inflation * 9 + target) / 10;
                st.reported_centi = st.actual_centi; // honest statistics
            }
            EconomicSystem::Planned => {
                // Expectation implied by the investment quota; shortfall is
                // partially papered over, capped relative to actual.
                let expected =
                    (st.actual_centi * invest_pm / 1000 * INVEST_CONVERT_PERMILLE / 1000) as i64;
                let shortfall = (expected - growth).max(0) as u64;
                let padding = shortfall * MISREPORT_PAD_PERMILLE / 1000;
                let cap = st.actual_centi * (1000 + MISREPORT_CAP_PERMILLE) / 1000;
                st.reported_centi = (st
                    .reported_centi
                    .saturating_add_signed(growth)
                    .saturating_add(padding))
                .clamp(st.actual_centi, cap);
            }
        }

        // Standard of living: consumer goods per capita + urbanization,
        // minus inflation pain.
        let pop_k = country_pop_k.get(&tag).copied().unwrap_or(0).max(1);
        let urban_k = country_urban_k.get(&tag).copied().unwrap_or(0);
        let consumer_component = consumer_out / 100 * SOL_CONSUMER_K / pop_k / 1000;
        let urban_component = SOL_URBAN_WEIGHT * (urban_k * 100 / pop_k) / 100;
        let inflation_penalty = st.inflation / INFLATION_SOL_DIVISOR;
        let value = (5 + consumer_component as i64 + urban_component as i64 - inflation_penalty)
            .clamp(5, 85);
        sol.by_country.insert(tag.clone(), value);
    }
}

/// Command handlers, called from `apply_commands`.
pub fn set_planned_allocation(
    econ: &mut Economies,
    country: &CountryTag,
    consumer: u16,
    investment: u16,
    military: u16,
) {
    if consumer as u32 + investment as u32 + military as u32 != 1000 {
        return; // invalid allocation, ignore
    }
    if econ.system.get(country) != Some(&EconomicSystem::Planned) {
        return; // wrong interface for this country
    }
    econ.policy.insert(
        country.clone(),
        Policy::Planned {
            consumer,
            investment,
            military,
        },
    );
}

pub fn set_market_policy(
    econ: &mut Economies,
    country: &CountryTag,
    interest_bp: u16,
    tax_permille: u16,
    procurement: Procurement,
) {
    if econ.system.get(country) != Some(&EconomicSystem::Market) {
        return;
    }
    econ.policy.insert(
        country.clone(),
        Policy::Market {
            interest_bp: interest_bp.clamp(50, 1200),
            tax_permille: tax_permille.clamp(50, 600),
            procurement,
        },
    );
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

    fn usa() -> CountryTag {
        CountryTag("USA".into())
    }
    fn sov() -> CountryTag {
        CountryTag("SOV".into())
    }

    #[test]
    fn systems_assigned_by_alignment() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 1);
        let econ = app.world().resource::<Economies>();
        assert_eq!(econ.system[&usa()], EconomicSystem::Market);
        assert_eq!(econ.system[&sov()], EconomicSystem::Planned);
        assert_eq!(
            econ.system[&CountryTag("YUG".into())],
            EconomicSystem::Planned
        );
    }

    #[test]
    fn heavier_investment_grows_industry_faster() {
        let run = |investment: u16| {
            let mut app = app_with_scenario();
            run_ticks(&mut app, 1);
            let military = 200;
            let consumer = 1000 - investment - military;
            app.world_mut().resource_mut::<PendingCommands>().push(
                SimCommand::SetPlannedAllocation {
                    country: sov(),
                    consumer,
                    investment,
                    military,
                },
            );
            run_ticks(&mut app, 24 * 366 * 2);
            app.world().resource::<Economies>().industry[&sov()].actual_centi
        };
        let heavy = run(600);
        let light = run(200);
        assert!(heavy > light, "invest 60% {heavy} vs 20% {light}");
    }

    #[test]
    fn cheap_money_boosts_growth_and_inflation() {
        let run = |interest_bp: u16| {
            let mut app = app_with_scenario();
            run_ticks(&mut app, 1);
            app.world_mut()
                .resource_mut::<PendingCommands>()
                .push(SimCommand::SetMarketPolicy {
                    country: usa(),
                    interest_bp,
                    tax_permille: 250,
                    procurement: Procurement::Med,
                });
            run_ticks(&mut app, 24 * 366 * 2);
            let st = app.world().resource::<Economies>().industry[&usa()];
            (st.actual_centi, st.inflation)
        };
        let (loose_ind, loose_infl) = run(100);
        let (tight_ind, tight_infl) = run(900);
        assert!(
            loose_ind > tight_ind,
            "loose {loose_ind} vs tight {tight_ind}"
        );
        assert!(
            loose_infl > tight_infl,
            "inflation {loose_infl} vs {tight_infl}"
        );
    }

    #[test]
    fn planned_statistics_pad_but_stay_capped() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 366 * 3);
        let econ = app.world().resource::<Economies>();
        let st = &econ.industry[&sov()];
        assert!(
            st.reported_centi >= st.actual_centi,
            "reported below actual"
        );
        assert!(
            st.reported_centi <= st.actual_centi * 1150 / 1000,
            "padding exceeded cap: {} vs {}",
            st.reported_centi,
            st.actual_centi
        );
        // Market countries never pad.
        let usa_st = &econ.industry[&usa()];
        assert_eq!(usa_st.reported_centi, usa_st.actual_centi);
    }

    #[test]
    fn wrong_interface_commands_are_rejected() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 1);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetPlannedAllocation {
                country: usa(),
                consumer: 400,
                investment: 400,
                military: 200,
            });
        run_ticks(&mut app, 1);
        let econ = app.world().resource::<Economies>();
        assert!(matches!(econ.policy[&usa()], Policy::Market { .. }));
    }
}
