//! Demography: per-province population cohorts evolving monthly.
//! Spec: docs/design/systems/demography.md
//!
//! The universal denominator of the economy (research principle 3):
//! rural / urban / educated cohorts in integer persons, seeded from the
//! real 1950 census data, evolving by SoL-driven vital rates.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{CountryTag, ProvinceId, ScenarioData};

use crate::SimClock;

pub mod tuning {
    //! Annual rates in ppm unless noted; formulas in the design doc.
    //! Calibrated against 1950 vital statistics (see doc for known
    //! deviations, e.g. the Western baby boom).

    /// births/yr per 1000 = clamp(48 - 0.38 * SoL, 16, 48)
    pub const BIRTH_BASE_PER_MILLE: i64 = 48;
    pub const BIRTH_SOL_SLOPE_CENTI: i64 = 38; // 0.38 per SoL point
    pub const BIRTH_MIN_PER_MILLE: i64 = 16;
    /// deaths/yr per 1000 = clamp(34 - 0.42 * SoL, 9, 35)
    pub const DEATH_BASE_PER_MILLE: i64 = 34;
    pub const DEATH_SOL_SLOPE_CENTI: i64 = 42;
    pub const DEATH_MIN_PER_MILLE: i64 = 9;
    pub const DEATH_MAX_PER_MILLE: i64 = 35;
    /// % of rural moving to cities per year = clamp(0.2 + 0.035*SoL, .2, 2.5)
    pub const URBANIZE_BASE_BP: i64 = 20; // basis points (0.2%)
    pub const URBANIZE_SOL_SLOPE_BP_CENTI: i64 = 350; // 3.5 bp per SoL point
    pub const URBANIZE_MAX_BP: i64 = 250;
    /// % of urban converting to educated per year = 0.3 + 0.015*SoL
    pub const EDUCATE_BASE_BP: i64 = 30;
    pub const EDUCATE_SOL_SLOPE_BP_CENTI: i64 = 150;
    /// Initial educated share of urban = SoL / 400 (fraction).
    pub const INITIAL_EDU_DIVISOR: u64 = 400;
}

/// Static scenario data injected by the presentation layer (or tests)
/// before the first tick.
#[derive(Resource, Clone)]
pub struct SimScenario(pub Arc<ScenarioData>);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cohorts {
    pub rural: u64,
    pub urban: u64,
    pub educated: u64,
}

impl Cohorts {
    pub fn total(&self) -> u64 {
        self.rural + self.urban + self.educated
    }
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Demographics {
    pub provinces: BTreeMap<ProvinceId, Cohorts>,
}

impl Demographics {
    pub fn world_population(&self) -> u64 {
        self.provinces.values().map(Cohorts::total).sum()
    }

    /// Order-stable digest for determinism testing.
    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (id, c) in &self.provinces {
            for v in [id.0 as u64, c.rural, c.urban, c.educated] {
                h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// Per-country standard of living, 0-100. Static in v1.
#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct LivingStandards {
    pub by_country: BTreeMap<CountryTag, i64>,
}

fn compute_sol(industry: u32, pop_k: u64, urban_k: u64) -> i64 {
    if pop_k == 0 {
        return 5;
    }
    // SoL = clamp(5 + 60*(industry per M pop) + 40*urban_share, 5, 80)
    let ipc_x100 = industry as i64 * 100_000 / pop_k as i64; // industry per M, x100
    let urban_share_x100 = (urban_k * 100 / pop_k) as i64;
    (5 + 60 * ipc_x100 / 100 + 40 * urban_share_x100 / 100).clamp(5, 80)
}

/// Apply `rate` (annual ppm) to `pop` for one month, floor-rounded.
fn monthly(pop: u64, rate_ppm: i64) -> u64 {
    (pop as u128 * rate_ppm.max(0) as u128 / 12_000_000) as u64
}

/// Lazy init + monthly evolution. Initialization happens on the first
/// tick (not at plugin build) so the schedule stays the only mutation
/// path and tests can inject scenarios freely.
pub fn update_demographics(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    mut demo: ResMut<Demographics>,
    mut sol: ResMut<LivingStandards>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;

    if demo.provinces.is_empty() {
        // Seed cohorts and SoL from scenario data.
        let mut country_pop: BTreeMap<&CountryTag, (u64, u64)> = BTreeMap::new();
        for p in data.provinces.values() {
            let e = country_pop.entry(&p.owner).or_default();
            e.0 += p.population_k as u64;
            e.1 += p.urban_k as u64;
        }
        for (tag, c) in &data.countries {
            let (pop_k, urban_k) = country_pop.get(tag).copied().unwrap_or((0, 0));
            sol.by_country
                .insert(tag.clone(), compute_sol(c.industry, pop_k, urban_k));
        }
        for (id, p) in &data.provinces {
            let total = p.population_k as u64 * 1000;
            let urban_total = (p.urban_k as u64 * 1000).min(total);
            let country_sol = sol.by_country.get(&p.owner).copied().unwrap_or(5) as u64;
            let educated = urban_total * country_sol / tuning::INITIAL_EDU_DIVISOR;
            demo.provinces.insert(
                *id,
                Cohorts {
                    rural: total - urban_total,
                    urban: urban_total - educated,
                    educated,
                },
            );
        }
        return;
    }

    if !clock.new_month {
        return;
    }

    use tuning::*;
    for (id, cohorts) in demo.provinces.iter_mut() {
        if cohorts.total() == 0 {
            continue;
        }
        let Some(p) = data.provinces.get(id) else {
            continue;
        };
        let s = sol.by_country.get(&p.owner).copied().unwrap_or(5);

        let births_pm = (BIRTH_BASE_PER_MILLE - BIRTH_SOL_SLOPE_CENTI * s / 100)
            .clamp(BIRTH_MIN_PER_MILLE, BIRTH_BASE_PER_MILLE);
        let deaths_pm = (DEATH_BASE_PER_MILLE - DEATH_SOL_SLOPE_CENTI * s / 100)
            .clamp(DEATH_MIN_PER_MILLE, DEATH_MAX_PER_MILLE);
        let natural_ppm = |pop: u64| {
            monthly(pop, births_pm * 1000) as i64 - monthly(pop, deaths_pm * 1000) as i64
        };
        cohorts.rural = cohorts
            .rural
            .saturating_add_signed(natural_ppm(cohorts.rural));
        cohorts.urban = cohorts
            .urban
            .saturating_add_signed(natural_ppm(cohorts.urban));
        cohorts.educated = cohorts
            .educated
            .saturating_add_signed(natural_ppm(cohorts.educated));

        // Rural -> urban migration.
        let urbanize_bp = (URBANIZE_BASE_BP + URBANIZE_SOL_SLOPE_BP_CENTI * s / 100)
            .clamp(URBANIZE_BASE_BP, URBANIZE_MAX_BP);
        let movers = monthly(cohorts.rural, urbanize_bp * 100);
        cohorts.rural -= movers;
        cohorts.urban += movers;

        // Urban -> educated conversion.
        let educate_bp = EDUCATE_BASE_BP + EDUCATE_SOL_SLOPE_BP_CENTI * s / 100;
        let learners = monthly(cohorts.urban, educate_bp * 100);
        cohorts.urban -= learners;
        cohorts.educated += learners;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{calendar::GameDate, run_ticks, SimPlugin};
    use bevy_app::App;
    use std::path::Path;

    fn real_scenario() -> Arc<ScenarioData> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        Arc::new(ScenarioData::load(&dir).expect("1950 scenario"))
    }

    fn app_with_scenario() -> App {
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 7,
        });
        app.insert_resource(SimScenario(real_scenario()));
        app
    }

    #[test]
    fn seeds_world_population_from_census() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 1);
        let world_pop = app.world().resource::<Demographics>().world_population();
        // HYDE 1950 total ≈ 2.53B.
        assert!(
            (2_300_000_000..2_700_000_000).contains(&world_pop),
            "{world_pop}"
        );
    }

    #[test]
    fn ten_years_of_growth_lands_near_history() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 1);
        let start = app.world().resource::<Demographics>().world_population();
        run_ticks(&mut app, 24 * 365 * 10);
        let end = app.world().resource::<Demographics>().world_population();
        let growth = end as f64 / start as f64;
        // Historical 1950->1960: 2.53B -> ~3.02B, x1.19. Accept a band.
        assert!(
            (1.10..1.32).contains(&growth),
            "10y growth factor {growth:.3} out of band ({start} -> {end})"
        );
    }

    #[test]
    fn sol_ordering_is_sane() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 1);
        let sol = app.world().resource::<LivingStandards>();
        let get = |t: &str| sol.by_country[&CountryTag(t.into())];
        assert!(
            get("USA") > get("SOV"),
            "USA {} vs SOV {}",
            get("USA"),
            get("SOV")
        );
        assert!(
            get("SOV") > get("IND"),
            "SOV {} vs IND {}",
            get("SOV"),
            get("IND")
        );
        assert!(get("GBR") > get("IND"));
    }

    #[test]
    fn urbanization_moves_people_into_cities() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 1);
        let before: u64 = app
            .world()
            .resource::<Demographics>()
            .provinces
            .values()
            .map(|c| c.urban + c.educated)
            .sum();
        let total_before = app.world().resource::<Demographics>().world_population();
        run_ticks(&mut app, 24 * 366 * 2);
        let demo = app.world().resource::<Demographics>();
        let after: u64 = demo.provinces.values().map(|c| c.urban + c.educated).sum();
        let total_after = demo.world_population();
        // Urban share must rise faster than total population.
        assert!(
            after as f64 / total_after as f64 > before as f64 / total_before as f64,
            "urban share did not rise"
        );
    }
}
