//! Resources & regional grids: national commodity balances and regional
//! electricity. Spec: docs/design/systems/resources-and-grids.md
//!
//! v1 computes and publishes balances; later systems (production,
//! construction, planning interfaces) consume them. All integer math.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{CountryTag, DepositKind, RegionId};

use crate::agriculture::Agriculture;
use crate::demography::{Demographics, SimScenario};
use crate::SimClock;

pub mod tuning {
    //! Abstract units: industry points (from CountryDef.industry,
    //! distributed to regions by urban population), commodity points.

    /// Grain output per rural person, permille of one person-ration/month.
    pub fn grain_yield_permille(terrain: ugs_data::Terrain) -> u64 {
        use ugs_data::Terrain::*;
        match terrain {
            Plains => 2000,
            Forest => 1400,
            Hills => 1200,
            Urban => 1500,
            Marsh => 1000,
            Jungle => 1100,
            Mountain => 700,
            Desert => 400,
            Tundra => 300,
        }
    }
    /// Commodity points per deposit size point per month.
    pub const EXTRACTION_PER_SIZE: u64 = 100;
    /// Coal demand per industry point (steel + heat).
    pub const COAL_PER_INDUSTRY: u64 = 3;
    /// Oil demand per industry point.
    pub const OIL_PER_INDUSTRY: u64 = 1;
    /// Power capacity per regional industry point, plus a small base.
    pub const POWER_CAP_PER_INDUSTRY: u64 = 12;
    pub const POWER_CAP_BASE: u64 = 2;
    /// Power demand per regional industry point / per 100k urban pop.
    pub const POWER_DEMAND_PER_INDUSTRY: u64 = 10;
    pub const POWER_DEMAND_PER_100K_URBAN: u64 = 1;
    /// Soft floor: shortage never throttles below this permille.
    pub const POWER_FLOOR_PERMILLE: u64 = 500;
    /// Fuel coupling floor: generation runs at least this permille of
    /// capacity even in total coal shortage (hydro, local stocks).
    pub const FUEL_FLOOR_PERMILLE: u64 = 500;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountryBalance {
    pub grain_prod: u64,
    pub grain_demand: u64,
    pub coal_prod: u64,
    pub coal_demand: u64,
    pub oil_prod: u64,
    pub oil_demand: u64,
    pub steel_prod: u64,
    /// Cumulative uranium extracted (kg-equivalent points).
    pub uranium_stock: u64,
}

impl CountryBalance {
    pub fn grain_ratio_permille(&self) -> u64 {
        ratio_permille(self.grain_prod, self.grain_demand)
    }
    pub fn coal_ratio_permille(&self) -> u64 {
        ratio_permille(self.coal_prod, self.coal_demand)
    }
    pub fn oil_ratio_permille(&self) -> u64 {
        ratio_permille(self.oil_prod, self.oil_demand)
    }
}

fn ratio_permille(prod: u64, demand: u64) -> u64 {
    if demand == 0 {
        1000
    } else {
        (prod as u128 * 1000 / demand as u128).min(2000) as u64
    }
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct NationalBalances {
    pub by_country: BTreeMap<CountryTag, CountryBalance>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerStatus {
    pub capacity: u64,
    pub generation: u64,
    pub demand: u64,
    /// clamp(generation/demand, floor, 1000) — the throttle future
    /// industry output multiplies by.
    pub factor_permille: u64,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct RegionalPower {
    pub by_region: BTreeMap<RegionId, PowerStatus>,
}

impl RegionalPower {
    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (id, p) in &self.by_region {
            for v in [id.0 as u64, p.generation, p.demand, p.factor_permille] {
                h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// Static distribution computed once from scenario data: regional industry
/// points and per-country deposit sums.
#[derive(Resource, Debug, Default, Clone)]
pub struct EconomyStatic {
    pub region_industry: BTreeMap<RegionId, u64>,
    pub region_owner: BTreeMap<RegionId, CountryTag>,
    pub deposits: BTreeMap<CountryTag, BTreeMap<DepositKind, u64>>,
    pub initialized: bool,
}

/// Monthly balances. Runs after demography in `TickSet::Economy`.
pub fn update_economy(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    demo: Res<Demographics>,
    agri: Res<Agriculture>,
    mut stat: ResMut<EconomyStatic>,
    mut national: ResMut<NationalBalances>,
    mut power: ResMut<RegionalPower>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    if data.regions.is_empty() || demo.provinces.is_empty() {
        return;
    }

    if !stat.initialized {
        // Distribute country industry to regions by urban population share.
        let mut region_urban: BTreeMap<RegionId, u64> = BTreeMap::new();
        let mut country_urban: BTreeMap<&CountryTag, u64> = BTreeMap::new();
        for p in data.provinces.values() {
            *region_urban.entry(p.region).or_default() += p.urban_k as u64;
            *country_urban.entry(&p.owner).or_default() += p.urban_k as u64;
            stat.region_owner.entry(p.region).or_insert_with(|| p.owner.clone());
            for &(kind, size) in &p.deposits {
                *stat
                    .deposits
                    .entry(p.owner.clone())
                    .or_default()
                    .entry(kind)
                    .or_default() += size as u64;
            }
        }
        for (region, urban) in &region_urban {
            let owner = &stat.region_owner[region];
            let total = country_urban.get(owner).copied().unwrap_or(0).max(1);
            let industry = data.countries.get(owner).map(|c| c.industry as u64).unwrap_or(0);
            stat.region_industry.insert(*region, industry * urban / total);
        }
        stat.initialized = true;
        return;
    }

    if !clock.new_month {
        return;
    }

    use tuning::*;

    // --- National balances ------------------------------------------------
    let mut balances: BTreeMap<CountryTag, CountryBalance> = BTreeMap::new();
    for (id, p) in &data.provinces {
        let Some(cohorts) = demo.provinces.get(id) else {
            continue;
        };
        let b = balances.entry(p.owner.clone()).or_default();
        b.grain_prod += cohorts.rural * grain_yield_permille(p.terrain) / 1000
            * agri.yield_permille(&p.owner)
            / 1000;
        b.grain_demand += cohorts.total();
    }
    for (tag, country) in &data.countries {
        let b = balances.entry(tag.clone()).or_default();
        let deposits = stat.deposits.get(tag);
        let dep = |kind: DepositKind| {
            deposits.and_then(|d| d.get(&kind)).copied().unwrap_or(0)
        };
        b.coal_prod = dep(DepositKind::Coal) * EXTRACTION_PER_SIZE;
        b.oil_prod = dep(DepositKind::Oil) * EXTRACTION_PER_SIZE;
        let industry = country.industry as u64;
        b.coal_demand = industry * COAL_PER_INDUSTRY;
        b.oil_demand = industry * OIL_PER_INDUSTRY;
        b.steel_prod = industry * b.coal_ratio_permille().min(1000) / 1000;
        let prev = national
            .by_country
            .get(tag)
            .map(|old| old.uranium_stock)
            .unwrap_or(0);
        b.uranium_stock = prev + dep(DepositKind::Uranium);
    }

    // --- Regional power ---------------------------------------------------
    let mut region_urban: BTreeMap<RegionId, u64> = BTreeMap::new();
    for (id, p) in &data.provinces {
        if let Some(c) = demo.provinces.get(id) {
            *region_urban.entry(p.region).or_default() += (c.urban + c.educated) / 1000;
        }
    }
    let mut statuses = BTreeMap::new();
    for (region, industry) in &stat.region_industry {
        let owner = &stat.region_owner[region];
        let coal_ratio = balances
            .get(owner)
            .map(|b| b.coal_ratio_permille())
            .unwrap_or(1000)
            .clamp(FUEL_FLOOR_PERMILLE, 1000);
        let urban_k = region_urban.get(region).copied().unwrap_or(0);
        let capacity = industry * POWER_CAP_PER_INDUSTRY + POWER_CAP_BASE;
        let generation = capacity * coal_ratio / 1000;
        let demand =
            industry * POWER_DEMAND_PER_INDUSTRY + urban_k * POWER_DEMAND_PER_100K_URBAN / 100;
        let factor = if demand == 0 {
            1000
        } else {
            (generation as u128 * 1000 / demand as u128)
                .clamp(POWER_FLOOR_PERMILLE as u128, 1000) as u64
        };
        statuses.insert(
            *region,
            PowerStatus {
                capacity,
                generation,
                demand,
                factor_permille: factor,
            },
        );
    }

    national.by_country = balances;
    power.by_region = statuses;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::GameDate;
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

    #[test]
    fn balances_populate_after_first_month() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 32);
        let national = app.world().resource::<NationalBalances>();
        let power = app.world().resource::<RegionalPower>();
        assert!(!national.by_country.is_empty());
        assert!(power.by_region.len() > 200, "{}", power.by_region.len());
        let usa = &national.by_country[&CountryTag("USA".into())];
        assert!(usa.coal_prod > 0 && usa.oil_prod > 0 && usa.uranium_stock > 0);
        let sov = &national.by_country[&CountryTag("SOV".into())];
        assert!(sov.uranium_stock > 0, "Fergana should feed the Soviet stockpile");
    }

    #[test]
    fn world_grain_ratio_is_plausible() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 32);
        let national = app.world().resource::<NationalBalances>();
        let (prod, demand) = national
            .by_country
            .values()
            .fold((0u64, 0u64), |(p, d), b| (p + b.grain_prod, d + b.grain_demand));
        let ratio = prod * 1000 / demand;
        // The 1950 world fed itself, tightly: expect 900-1600 permille.
        assert!((900..1600).contains(&ratio), "world grain ratio {ratio}");
    }

    #[test]
    fn industrialized_regions_have_power_and_poor_ones_do_not() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 32);
        let power = app.world().resource::<RegionalPower>();
        let factors: Vec<u64> = power.by_region.values().map(|p| p.factor_permille).collect();
        let full = factors.iter().filter(|&&f| f >= 999).count();
        let starved = factors.iter().filter(|&&f| f <= 600).count();
        assert!(full > 20, "some regions should be fully powered ({full})");
        assert!(starved > 20, "the unelectrified world should exist ({starved})");
    }
}
