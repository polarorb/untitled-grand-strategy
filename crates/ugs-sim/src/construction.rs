//! Economic agency: regional industry as live state, the construction
//! pool with its capped project portfolio, the market investment
//! allocator, and the monthly region snapshots that make 250 regions a
//! triage list (docs/design/systems/economic-agency.md). The loop:
//! see a place, read its computed constraint, commit a project, get
//! graded by next month's document.

use std::collections::{BTreeMap, BTreeSet};

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{
    CountryTag, GreatProjectDef, OfferCondition, ProjectPayload, RegionId, ScenarioData,
};

use crate::demography::SimScenario;
use crate::economy::{EconomyStatic, NationalBalances, RegionalPower};
use crate::planning::Economies;
use crate::SimClock;

pub mod tuning {
    /// Share of investment gains routed to the construction pool.
    pub const DIRECTED_PERMILLE: u64 = 500;
    /// Pool above (cap_months x typical cost) auto-converts to growth —
    /// the AI/passive-player guardrail (their economy behaves as today).
    pub const POOL_CAP_CENTI: u64 = 4000;
    /// Generic project slots; Great Projects get one more.
    pub const GENERIC_SLOTS: usize = 2;
    /// Monthly per-project pool draw ceiling, centi.
    pub const INTAKE_MAX_CENTI: u64 = 120;
    /// Generic project costs, centi.
    pub const COST_INDUSTRIAL: u64 = 600;
    pub const COST_POWER_STATION: u64 = 900;
    pub const COST_AGRI_MECH: u64 = 500;
    /// Completion payloads for generic projects.
    pub const EXPANSION_CENTI: u64 = 400;
    /// Power station capacity = region demand x this permille (min floor).
    pub const STATION_DEMAND_PERMILLE: u64 = 400;
    pub const STATION_MIN_CAPACITY: u64 = 40;
    pub const MECH_YIELD_PERMILLE: u64 = 50;
    /// Site modifiers applied to cost at start, permille deltas.
    pub const SITE_DEPOSIT_DISCOUNT: u64 = 100;
    pub const SITE_POWER_SURPLUS_DISCOUNT: u64 = 50;
    pub const SITE_POWER_DEFICIT_SURCHARGE: u64 = 100;
    /// Cancel refund share of remaining cost.
    pub const CANCEL_REFUND_PERMILLE: u64 = 300;
    /// Market allocator: share of invest gains firms place themselves.
    pub const PRIVATE_PERMILLE: u64 = 700;
    pub const ZONE_BONUS: u64 = 40;
    pub const ZONE_CAP: usize = 3;
    /// Constraint severity bands (permille of the limiting factor).
    pub const SEVERITY_CRITICAL: u64 = 700;
    pub const SEVERITY_STRAINED: u64 = 900;
    /// Labor constraint: urban workers required per centi-point of
    /// industry (1 centi = 0.01 industry points; ~40 workers/centi
    /// makes labor bind in crammed, under-urbanized regions).
    pub const LABOR_URBAN_PER_CENTI: u64 = 40;
    /// Monthly wire lines cap.
    pub const WIRE_LINES: usize = 6;
}

/// Live regional industry distribution, centi-points. The country
/// scalar (`IndustryState.actual_centi`) becomes the maintained cache
/// of this map's per-owner sums.
#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct RegionalIndustry {
    pub by_region: BTreeMap<RegionId, u64>,
    pub initialized: bool,
}

impl RegionalIndustry {
    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (id, v) in &self.by_region {
            for x in [id.0 as u64, *v] {
                h = (h ^ x).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectKind {
    IndustrialExpansion,
    PowerStation,
    AgriMechanization,
    Great(String),
}

impl ProjectKind {
    pub fn label(&self) -> &str {
        match self {
            ProjectKind::IndustrialExpansion => "INDUSTRIAL EXPANSION",
            ProjectKind::PowerStation => "POWER STATION",
            ProjectKind::AgriMechanization => "AGRICULTURAL MECHANIZATION",
            ProjectKind::Great(_) => "GREAT PROJECT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub country: CountryTag,
    /// None = national project (Interstates): no site gating, national
    /// payload distribution.
    pub region: Option<RegionId>,
    pub kind: ProjectKind,
    pub progress_centi: u64,
    /// Pool actually paid in (the refund basis — inherited AtStart
    /// progress is NOT refundable; cancel-mint exploits die here).
    #[serde(default)]
    pub paid_centi: u64,
    pub cost_centi: u64,
    /// Physical schedule floor: max monthly intake (0 = uncapped),
    /// from the catalog's min_months for Great Projects.
    #[serde(default)]
    pub monthly_cap_centi: u64,
    pub started_tick: u64,
    /// What slowed last month's intake, for the card and the wire.
    pub slowed_by: Option<ConstraintKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConstraintKind {
    Power,
    Materials,
    Labor,
    Contested,
    /// The construction pool is empty (project cards only; never a
    /// region verdict).
    Funding,
    #[default]
    Healthy,
}

impl ConstraintKind {
    pub fn label(self) -> &'static str {
        match self {
            ConstraintKind::Power => "POWER-LIMITED",
            ConstraintKind::Materials => "MATERIALS-LIMITED",
            ConstraintKind::Labor => "LABOR-LIMITED",
            ConstraintKind::Contested => "CONTESTED",
            ConstraintKind::Funding => "UNFUNDED",
            ConstraintKind::Healthy => "HEALTHY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Severity {
    #[default]
    Healthy,
    Strained,
    Critical,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Construction {
    /// Directed-investment pool per country, centi.
    pub pool: BTreeMap<CountryTag, u64>,
    pub next_id: u32,
    pub projects: BTreeMap<ProjectId, Project>,
    /// Power-station capacity built into regions (added to grid capacity).
    pub built_power: BTreeMap<RegionId, u64>,
    /// Market development zones per country (<= ZONE_CAP).
    pub zones: BTreeMap<CountryTag, BTreeSet<RegionId>>,
    /// Last month's private investment per region, centi (attribution).
    pub attribution: BTreeMap<RegionId, u64>,
    /// Completed Great Project ids (never re-offered).
    pub completed_great: BTreeSet<String>,
    /// Econ wire ring: (tick, line). Derived narrative; capped;
    /// excluded from the digest.
    pub log: Vec<(u64, String)>,
}

impl Construction {
    pub fn log_line(&mut self, tick: u64, line: String) {
        self.log.push((tick, line));
        let overflow = self.log.len().saturating_sub(60);
        if overflow > 0 {
            self.log.drain(..overflow);
        }
    }

    pub fn active_for(&self, country: &CountryTag) -> (usize, usize) {
        let generic = self
            .projects
            .values()
            .filter(|p| &p.country == country && !matches!(p.kind, ProjectKind::Great(_)))
            .count();
        let great = self
            .projects
            .values()
            .filter(|p| &p.country == country && matches!(p.kind, ProjectKind::Great(_)))
            .count();
        (generic, great)
    }

    pub fn digest(&self) -> u64 {
        fn fold(h: &mut u64, v: u64) {
            *h = (*h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        }
        fn fold_tag(h: &mut u64, tag: &CountryTag) {
            for b in tag.0.bytes() {
                fold(h, b as u64);
            }
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (t, v) in &self.pool {
            fold_tag(&mut h, t);
            fold(&mut h, *v);
        }
        for (id, p) in &self.projects {
            fold(&mut h, id.0 as u64);
            fold_tag(&mut h, &p.country);
            fold(&mut h, p.region.map(|r| r.0 as u64 + 1).unwrap_or(0));
            fold(&mut h, p.progress_centi);
            fold(&mut h, p.paid_centi);
            fold(&mut h, p.cost_centi);
            if let ProjectKind::Great(gid) = &p.kind {
                for b in gid.bytes() {
                    fold(&mut h, b as u64);
                }
            } else {
                fold(&mut h, p.kind.label().len() as u64);
            }
        }
        for (r, v) in &self.built_power {
            fold(&mut h, r.0 as u64);
            fold(&mut h, *v);
        }
        for (t, zones) in &self.zones {
            fold_tag(&mut h, t);
            for z in zones {
                fold(&mut h, z.0 as u64 + 1);
            }
        }
        for (r, v) in &self.attribution {
            fold(&mut h, r.0 as u64);
            fold(&mut h, *v);
        }
        for g in &self.completed_great {
            for b in g.bytes() {
                fold(&mut h, b as u64);
            }
        }
        h
    }
}

/// UI-facing monthly snapshot per region. DERIVED — rebuilt each
/// month, excluded from the digest, and (per the settlement review's
/// codified lesson) never read by sim decision logic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegionSnapshot {
    pub pop: u64,
    pub pop_trend_permille: i64,
    pub industry_centi: u64,
    pub reported_centi: u64,
    pub power_generation: u64,
    pub power_demand: u64,
    pub power_permille: u64,
    pub constraint: ConstraintKind,
    pub severity: Severity,
    pub private_last_centi: u64,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct RegionSnapshots {
    pub by_region: BTreeMap<RegionId, RegionSnapshot>,
    /// Ranked wire lines from last month's deltas, per owner country.
    pub wire: BTreeMap<CountryTag, Vec<String>>,
    /// The month-stamp of the data ("AS OF 1 JUL 1950").
    pub as_of: String,
}

/// Which Great Projects are currently on a country's offer board.
pub fn offered_projects<'a>(
    data: &'a ScenarioData,
    clock: &SimClock,
    national: &NationalBalances,
    power: &RegionalPower,
    stat: &EconomyStatic,
    construction: &Construction,
    country: &CountryTag,
) -> Vec<&'a GreatProjectDef> {
    data.projects
        .iter()
        .filter(|p| &p.country == country)
        .filter(|p| !construction.completed_great.contains(&p.id))
        .filter(|p| {
            !construction
                .projects
                .values()
                .any(|q| matches!(&q.kind, ProjectKind::Great(id) if id == &p.id))
        })
        .filter(|p| match &p.offered {
            OfferCondition::AtStart { .. } => true,
            OfferCondition::Date { year, month } => {
                (clock.date.year, clock.date.month) >= (*year, *month)
            }
            OfferCondition::PowerDeficit { after_year } => {
                clock.date.year >= *after_year
                    && project_region(data, stat, p)
                        .and_then(|r| power.by_region.get(&r))
                        .is_some_and(|s| s.factor_permille < 1000)
            }
            OfferCondition::GrainShortfall {
                after_year,
                below_permille,
            } => {
                clock.date.year >= *after_year
                    && national
                        .by_country
                        .get(country)
                        .is_some_and(|b| b.grain_ratio_permille() < *below_permille as u64)
            }
        })
        .collect()
}

/// A Great Project's host region (None = national, e.g. Interstates).
pub fn project_region(
    data: &ScenarioData,
    stat: &EconomyStatic,
    def: &GreatProjectDef,
) -> Option<RegionId> {
    if def.province.is_empty() {
        return None;
    }
    let _ = stat;
    data.province_by_name(&def.country, &def.province)
        .ok()
        .and_then(|id| data.provinces.get(&id).map(|p| p.region))
}

/// Regions majority-held by their 1950 owner (project validation).
pub fn region_held(
    data: &ScenarioData,
    military: &crate::military::Military,
    region: RegionId,
) -> bool {
    let (held, total) =
        data.provinces
            .values()
            .filter(|p| p.region == region)
            .fold((0u64, 0u64), |(h, t), p| {
                let holder = military.owner_of(p.id, &p.owner);
                (h + (holder == p.owner) as u64, t + 1)
            });
    total > 0 && held * 2 > total
}

/// Monthly: project intake and completions, pool cap conversion, zone
/// hygiene. Runs after `update_production` (which accrues the pool and
/// distributes growth) and before `update_snapshots`.
#[allow(clippy::too_many_arguments)]
pub fn update_construction(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    stat: Res<EconomyStatic>,
    power: Res<RegionalPower>,
    balances: Res<NationalBalances>,
    mut regional: ResMut<RegionalIndustry>,
    mut econ: ResMut<Economies>,
    mut agri: ResMut<crate::agriculture::Agriculture>,
    mut nuclear: ResMut<crate::nuclear::NuclearPrograms>,
    military: Res<crate::military::Military>,
    mut tension: ResMut<crate::tension::GlobalTension>,
    mut fired: ResMut<crate::events::FiredEvents>,
    mut construction: ResMut<Construction>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    if !clock.new_month || !regional.initialized {
        return;
    }
    use tuning::*;

    // --- Zone hygiene: cap and existence ---------------------------------
    let zone_fixes: Vec<CountryTag> = construction
        .zones
        .iter()
        .filter(|(_, z)| z.len() > ZONE_CAP)
        .map(|(t, _)| t.clone())
        .collect();
    for t in zone_fixes {
        let z = construction.zones.get_mut(&t).unwrap();
        while z.len() > ZONE_CAP {
            let last = *z.iter().next_back().unwrap();
            z.remove(&last);
        }
    }

    // --- Project intake ---------------------------------------------------
    let ids: Vec<ProjectId> = construction.projects.keys().copied().collect();
    let mut completions: Vec<(ProjectId, Project)> = Vec::new();
    for id in ids {
        let (country, region, cost, progress, monthly_cap) = {
            let p = &construction.projects[&id];
            (
                p.country.clone(),
                p.region,
                p.cost_centi,
                p.progress_centi,
                p.monthly_cap_centi,
            )
        };
        let active = construction
            .projects
            .values()
            .filter(|p| p.country == country)
            .count()
            .max(1) as u64;
        let pool = construction.pool.get(&country).copied().unwrap_or(0);
        let mut draw_cap = (pool / active).min(INTAKE_MAX_CENTI);
        if monthly_cap > 0 {
            // The physical schedule floor: money cannot compress the
            // catalog's minimum build time.
            draw_cap = draw_cap.min(monthly_cap);
        }
        // Never draw more than the project can absorb (the clamp used
        // to destroy up to a month's overshoot).
        draw_cap = draw_cap.min(cost.saturating_sub(progress));
        // The cannot-buy-itself gate: host grid + national materials.
        // National projects (no site) skip the grid gate.
        let power_pm = region
            .and_then(|r| power.by_region.get(&r))
            .map(|s| s.factor_permille)
            .unwrap_or(1000);
        let materials_pm = balances
            .by_country
            .get(&country)
            .map(|b| b.coal_ratio_permille().min(1000))
            .unwrap_or(1000)
            .clamp(500, 1000);
        let held = region.is_none_or(|r| region_held(data, &military, r));
        let intake = if held {
            // Floor at 1 so the last few centi can't strand a project
            // at 99% forever under integer truncation.
            (draw_cap * power_pm / 1000 * materials_pm / 1000).max(draw_cap.min(1))
        } else {
            0 // war severed the site: the project suspends
        };
        let slowed = if !held {
            Some(ConstraintKind::Contested)
        } else if pool == 0 {
            Some(ConstraintKind::Funding)
        } else if intake < draw_cap {
            Some(if power_pm <= materials_pm {
                ConstraintKind::Power
            } else {
                ConstraintKind::Materials
            })
        } else {
            None
        };
        *construction.pool.entry(country.clone()).or_default() = pool.saturating_sub(intake);
        let p = construction.projects.get_mut(&id).unwrap();
        p.progress_centi = (progress + intake).min(cost);
        p.paid_centi += intake;
        p.slowed_by = slowed;
        if p.progress_centi >= cost {
            completions.push((id, p.clone()));
        }
    }

    // --- Completions: step-changes ---------------------------------------
    for (id, p) in completions {
        construction.projects.remove(&id);
        let name = match &p.kind {
            ProjectKind::Great(gid) => data
                .projects
                .iter()
                .find(|g| &g.id == gid)
                .map(|g| g.name.clone())
                .unwrap_or_else(|| gid.clone()),
            other => other.label().to_string(),
        };
        match &p.kind {
            ProjectKind::IndustrialExpansion => {
                if let Some(region) = p.region {
                    *regional.by_region.entry(region).or_default() += EXPANSION_CENTI;
                }
            }
            ProjectKind::PowerStation => {
                if let Some(region) = p.region {
                    let demand = power.by_region.get(&region).map(|s| s.demand).unwrap_or(0);
                    let capacity =
                        (demand * STATION_DEMAND_PERMILLE / 1000).max(STATION_MIN_CAPACITY);
                    *construction.built_power.entry(region).or_default() += capacity;
                }
            }
            ProjectKind::AgriMechanization => {
                *agri.bonus_permille.entry(p.country.clone()).or_default() += MECH_YIELD_PERMILLE;
            }
            ProjectKind::Great(gid) => {
                construction.completed_great.insert(gid.clone());
                if let Some(def) = data.projects.iter().find(|g| &g.id == gid) {
                    match &def.payload {
                        ProjectPayload::Power { capacity } => {
                            if let Some(region) = p.region {
                                // MW -> grid units scale.
                                *construction.built_power.entry(region).or_default() +=
                                    capacity / 10;
                            }
                        }
                        ProjectPayload::Industry { centi } => {
                            match p.region {
                                Some(region) => {
                                    *regional.by_region.entry(region).or_default() += centi;
                                }
                                // National payload (Interstates): the
                                // whole country's regions share it.
                                None => distribute_proportional(
                                    &stat,
                                    &mut regional,
                                    &p.country,
                                    *centi as i64,
                                ),
                            }
                        }
                        ProjectPayload::AgriYield { permille } => {
                            *agri.bonus_permille.entry(p.country.clone()).or_default() += permille;
                        }
                        ProjectPayload::Enrichment { levels } => {
                            if let Some(prog) = nuclear.programs.get_mut(&p.country) {
                                prog.enrichment_level += levels;
                            }
                        }
                        ProjectPayload::Prestige { tension_relief } => {
                            tension.apply(*tension_relief);
                        }
                    }
                    fired.notices.push((
                        "GREAT PROJECT COMPLETE".into(),
                        format!("{} -- {}", def.name, def.blurb),
                    ));
                }
            }
        }
        // Update the country cache after step changes.
        let sums = per_country_sums(&stat, &regional);
        if let Some(st) = econ.industry.get_mut(&p.country) {
            st.actual_centi = sums.get(&p.country).copied().unwrap_or(st.actual_centi);
        }
        let where_label = match p.region {
            Some(r) => region_name(data, r),
            None => "NATIONAL".to_string(),
        };
        construction.log_line(clock.tick, format!("{name} COMPLETE ({where_label})"));
    }

    // --- Pool cap: surplus auto-converts to growth (AI guardrail) --------
    let over: Vec<(CountryTag, u64)> = construction
        .pool
        .iter()
        .filter(|(_, v)| **v > POOL_CAP_CENTI)
        .map(|(t, v)| (t.clone(), *v - POOL_CAP_CENTI))
        .collect();
    for (tag, surplus) in over {
        *construction.pool.get_mut(&tag).unwrap() -= surplus;
        distribute_proportional(&stat, &mut regional, &tag, surplus as i64);
        if let Some(st) = econ.industry.get_mut(&tag) {
            st.actual_centi += surplus;
        }
    }
}

pub fn region_name(data: &ScenarioData, region: RegionId) -> String {
    data.regions
        .get(&region)
        .map(|r| r.name.to_uppercase())
        .unwrap_or_else(|| format!("REGION {}", region.0))
}

/// Distribute a signed centi delta across a country's regions
/// proportionally to current industry (uniform when zero).
pub fn distribute_proportional(
    stat: &EconomyStatic,
    regional: &mut RegionalIndustry,
    country: &CountryTag,
    delta_centi: i64,
) {
    let regions: Vec<RegionId> = stat
        .region_owner
        .iter()
        .filter(|(_, owner)| *owner == country)
        .map(|(r, _)| *r)
        .collect();
    if regions.is_empty() || delta_centi == 0 {
        return;
    }
    let total: u64 = regions
        .iter()
        .map(|r| regional.by_region.get(r).copied().unwrap_or(0))
        .sum();
    let mut remaining = delta_centi;
    for (i, r) in regions.iter().enumerate() {
        let share = if total == 0 {
            delta_centi / regions.len() as i64
        } else {
            delta_centi * regional.by_region.get(r).copied().unwrap_or(0) as i64 / total as i64
        };
        let share = if i == regions.len() - 1 {
            remaining
        } else {
            share
        };
        remaining -= share;
        let entry = regional.by_region.entry(*r).or_default();
        *entry = entry.saturating_add_signed(share);
    }
}

pub fn per_country_sums(
    stat: &EconomyStatic,
    regional: &RegionalIndustry,
) -> BTreeMap<CountryTag, u64> {
    let mut out: BTreeMap<CountryTag, u64> = BTreeMap::new();
    for (region, owner) in &stat.region_owner {
        *out.entry(owner.clone()).or_default() +=
            regional.by_region.get(region).copied().unwrap_or(0);
    }
    out
}

/// Monthly snapshots + the wire, last in the Economy chain. Derived
/// display state: never read by sim logic.
#[allow(clippy::too_many_arguments)]
pub fn update_snapshots(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    stat: Res<EconomyStatic>,
    power: Res<RegionalPower>,
    balances: Res<NationalBalances>,
    regional: Res<RegionalIndustry>,
    demo: Res<crate::demography::Demographics>,
    econ: Res<Economies>,
    construction: Res<Construction>,
    mut snaps: ResMut<RegionSnapshots>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    if !clock.new_month || !regional.initialized {
        return;
    }
    use tuning::*;

    let mut region_pop: BTreeMap<RegionId, u64> = BTreeMap::new();
    let mut region_urban: BTreeMap<RegionId, u64> = BTreeMap::new();
    for (id, c) in &demo.provinces {
        if let Some(p) = data.provinces.get(id) {
            *region_pop.entry(p.region).or_default() += c.total();
            *region_urban.entry(p.region).or_default() += c.urban + c.educated;
        }
    }

    let mut new_wire: BTreeMap<CountryTag, Vec<(u64, String)>> = BTreeMap::new();
    let mut by_region: BTreeMap<RegionId, RegionSnapshot> = BTreeMap::new();
    for (region, owner) in &stat.region_owner {
        let industry = regional.by_region.get(region).copied().unwrap_or(0);
        let ps = power.by_region.get(region).copied().unwrap_or_default();
        let materials_pm = balances
            .by_country
            .get(owner)
            .map(|b| b.coal_ratio_permille().min(1000))
            .unwrap_or(1000);
        let urban = region_urban.get(region).copied().unwrap_or(0);
        let labor_pm = if industry == 0 {
            1000
        } else {
            (urban * 1000 / (industry * LABOR_URBAN_PER_CENTI).max(1)).min(1000)
        };
        let power_pm = if ps.demand == 0 {
            1000
        } else {
            ps.factor_permille
        };
        let (constraint, limit) = [
            (ConstraintKind::Power, power_pm),
            (ConstraintKind::Materials, materials_pm),
            (ConstraintKind::Labor, labor_pm),
        ]
        .into_iter()
        .min_by_key(|(_, v)| *v)
        .unwrap();
        let (constraint, severity) = if limit >= SEVERITY_STRAINED {
            (ConstraintKind::Healthy, Severity::Healthy)
        } else if limit >= SEVERITY_CRITICAL {
            (constraint, Severity::Strained)
        } else {
            (constraint, Severity::Critical)
        };
        let pop = region_pop.get(region).copied().unwrap_or(0);
        let prev = snaps.by_region.get(region);
        let pop_trend = prev
            .map(|p| {
                if p.pop == 0 {
                    0
                } else {
                    (pop as i64 - p.pop as i64) * 1000 / p.pop as i64
                }
            })
            .unwrap_or(0);
        // Reported figure: the country's national padding ratio applied
        // proportionally (per-region falsification drift is the first
        // fast-follow).
        let reported = econ
            .industry
            .get(owner)
            .map(|st| {
                (industry * st.reported_centi)
                    .checked_div(st.actual_centi)
                    .unwrap_or(industry)
            })
            .unwrap_or(industry);

        // Wire deltas.
        if let Some(prev) = prev {
            let owner_wire = new_wire.entry(owner.clone()).or_default();
            if prev.severity < severity {
                owner_wire.push((
                    3 - severity as u64,
                    format!(
                        "{}: NOW {} ({}%)",
                        region_name(data, *region),
                        constraint.label(),
                        limit / 10
                    ),
                ));
            }
            if prev.power_permille >= 1000 && power_pm < 1000 {
                owner_wire.push((
                    1,
                    format!(
                        "{}: GRID DEFICIT ONSET -- INDUSTRY THROTTLED {}%",
                        region_name(data, *region),
                        (1000 - power_pm) / 10
                    ),
                ));
            }
        }
        by_region.insert(
            *region,
            RegionSnapshot {
                pop,
                pop_trend_permille: pop_trend,
                industry_centi: industry,
                reported_centi: reported,
                power_generation: ps.generation,
                power_demand: ps.demand,
                power_permille: power_pm,
                constraint,
                severity,
                private_last_centi: construction.attribution.get(region).copied().unwrap_or(0),
            },
        );
    }
    // Project lines.
    for p in construction.projects.values() {
        let owner_wire = new_wire.entry(p.country.clone()).or_default();
        let pct = p.progress_centi * 100 / p.cost_centi.max(1);
        match p.slowed_by {
            Some(c) => owner_wire.push((
                2,
                format!("{} {}% -- SLOWED ({})", p.kind.label(), pct, c.label()),
            )),
            None => owner_wire.push((4, format!("{} {}% -- ON SCHEDULE", p.kind.label(), pct))),
        }
    }
    snaps.wire = new_wire
        .into_iter()
        .map(|(tag, mut lines)| {
            lines.sort();
            (
                tag,
                lines.into_iter().take(WIRE_LINES).map(|(_, l)| l).collect(),
            )
        })
        .collect();
    snaps.by_region = by_region;
    snaps.as_of = format!(
        "AS OF 1 {} {}",
        month_name(clock.date.month),
        clock.date.year
    );
}

fn month_name(m: u8) -> &'static str {
    [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ][(m as usize - 1).min(11)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::GameDate;
    use crate::command::SimCommand;
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

    fn push(app: &mut App, cmd: SimCommand) {
        app.world_mut()
            .resource_mut::<crate::command::PendingCommands>()
            .push(cmd);
    }

    fn region_of(app: &App, tag: &str) -> RegionId {
        let stat = app.world().resource::<EconomyStatic>();
        *stat
            .region_owner
            .iter()
            .find(|(_, o)| o.0 == tag)
            .map(|(r, _)| r)
            .expect("country has a region")
    }

    #[test]
    fn the_thaw_preserves_country_totals() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 95); // three monthly passes
        let world = app.world();
        let stat = world.resource::<EconomyStatic>();
        let regional = world.resource::<RegionalIndustry>();
        assert!(regional.initialized, "thaw happened");
        let econ = world.resource::<Economies>();
        let sums = per_country_sums(stat, regional);
        for (tag, st) in &econ.industry {
            assert_eq!(
                sums.get(tag).copied().unwrap_or(0),
                st.actual_centi,
                "country scalar is the regional sum for {}",
                tag.0
            );
        }
        // The USA still grows.
        let usa = econ.industry[&CountryTag("USA".into())].actual_centi;
        assert!(usa > 9_000, "US industry alive: {usa}");
    }

    #[test]
    fn planned_projects_complete_as_step_changes() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 40);
        let sov = CountryTag("SOV".into());
        // Pick the healthiest SOV grid — building in a browned-out
        // region is SUPPOSED to crawl (that's the mechanic).
        let region = {
            let world = app.world();
            let stat = world.resource::<EconomyStatic>();
            let power = world.resource::<RegionalPower>();
            *stat
                .region_owner
                .iter()
                .filter(|(_, o)| o.0 == "SOV")
                .max_by_key(|(r, _)| {
                    (
                        power
                            .by_region
                            .get(r)
                            .map(|s| s.factor_permille)
                            .unwrap_or(0),
                        std::cmp::Reverse(r.0),
                    )
                })
                .map(|(r, _)| r)
                .expect("SOV has regions")
        };
        let before = app
            .world()
            .resource::<RegionalIndustry>()
            .by_region
            .get(&region)
            .copied()
            .unwrap_or(0);
        {
            let mut c = app.world_mut().resource_mut::<Construction>();
            c.pool.insert(sov.clone(), 2000);
        }
        push(
            &mut app,
            SimCommand::StartProject {
                country: sov.clone(),
                region,
                kind: ProjectKind::IndustrialExpansion,
            },
        );
        run_ticks(&mut app, 1);
        assert_eq!(
            app.world().resource::<Construction>().projects.len(),
            1,
            "project started"
        );
        run_ticks(&mut app, 24 * 30 * 12);
        let world = app.world();
        let c = world.resource::<Construction>();
        assert!(
            c.projects.is_empty(),
            "project completed within its window: {:?}",
            c.projects.values().next()
        );
        let after = world
            .resource::<RegionalIndustry>()
            .by_region
            .get(&region)
            .copied()
            .unwrap_or(0);
        assert!(
            after >= before + tuning::EXPANSION_CENTI / 2,
            "step change landed (some depreciation allowed): {before} -> {after}"
        );
    }

    #[test]
    fn market_economies_cannot_place_industry_but_can_zone() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 40);
        let usa = CountryTag("USA".into());
        let region = region_of(&app, "USA");
        {
            let mut c = app.world_mut().resource_mut::<Construction>();
            c.pool.insert(usa.clone(), 2000);
        }
        push(
            &mut app,
            SimCommand::StartProject {
                country: usa.clone(),
                region,
                kind: ProjectKind::IndustrialExpansion,
            },
        );
        push(
            &mut app,
            SimCommand::SetDevelopmentZone {
                country: usa.clone(),
                region,
                on: true,
            },
        );
        // And the planner cannot zone.
        let sov = CountryTag("SOV".into());
        let sov_region = region_of(&app, "SOV");
        push(
            &mut app,
            SimCommand::SetDevelopmentZone {
                country: sov.clone(),
                region: sov_region,
                on: true,
            },
        );
        run_ticks(&mut app, 1);
        let c = app.world().resource::<Construction>();
        assert!(
            c.projects.is_empty(),
            "industry placement belongs to firms in a market economy"
        );
        assert!(
            c.zones.get(&usa).is_some_and(|z| z.contains(&region)),
            "the market player zones instead"
        );
        assert!(
            c.zones.get(&sov).is_none_or(|z| z.is_empty()),
            "Gosplan does not beg firms"
        );
    }

    #[test]
    fn the_allocator_attributes_private_investment() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 65); // two monthly passes
        let world = app.world();
        let stat = world.resource::<EconomyStatic>();
        let c = world.resource::<Construction>();
        let usa_attr: u64 = c
            .attribution
            .iter()
            .filter(|(r, _)| stat.region_owner.get(r).is_some_and(|o| o.0 == "USA"))
            .map(|(_, v)| *v)
            .sum();
        assert!(usa_attr > 0, "capital flowed somewhere attributable");
    }

    #[test]
    fn volga_don_is_inherited_and_finishes() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 40);
        let sov = CountryTag("SOV".into());
        {
            let mut c = app.world_mut().resource_mut::<Construction>();
            c.pool.insert(sov.clone(), 4000);
        }
        push(
            &mut app,
            SimCommand::StartProject {
                country: sov.clone(),
                region: RegionId(0), // catalog resolves the real site
                kind: ProjectKind::Great("volga-don".into()),
            },
        );
        run_ticks(&mut app, 1);
        {
            let c = app.world().resource::<Construction>();
            let p = c
                .projects
                .values()
                .find(|p| matches!(&p.kind, ProjectKind::Great(id) if id == "volga-don"))
                .expect("the canal is on the books");
            assert!(
                p.progress_centi * 1000 / p.cost_centi >= 500,
                "inherited at its historical progress"
            );
        }
        run_ticks(&mut app, 24 * 30 * 30);
        let world = app.world();
        let c = world.resource::<Construction>();
        assert!(
            c.completed_great.contains("volga-don"),
            "the canal opens on a historical schedule (real: June 1952): {:?}",
            c.projects.values().next()
        );
        let fired = world.resource::<crate::events::FiredEvents>();
        assert!(
            fired
                .notices
                .iter()
                .any(|(t, _)| t.contains("GREAT PROJECT")),
            "completion is a ceremony"
        );
    }

    #[test]
    fn snapshots_stamp_constraints_monthly() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 35);
        let snaps = app.world().resource::<RegionSnapshots>();
        assert!(!snaps.by_region.is_empty(), "regions snapshotted");
        assert!(
            snaps.as_of.contains("1950"),
            "date-stamped: {}",
            snaps.as_of
        );
        assert!(
            snaps
                .by_region
                .values()
                .any(|s| s.constraint != ConstraintKind::Healthy),
            "somewhere in the 1950 world a constraint binds"
        );
    }
}
