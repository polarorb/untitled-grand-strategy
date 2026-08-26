//! Nuclear weapons programs — phase 1 of the escalation pillar
//! (docs/design/systems/escalation.md). The program is an industrial
//! project: uranium feed and grid electricity in, fissile grams out,
//! warheads assembled from fissile. Acquisition milestones move the
//! world; the bomb itself is never a unit order.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{CountryTag, RegionId};

use crate::demography::SimScenario;
use crate::economy::{EconomyStatic, NationalBalances, RegionalPower};
use crate::events::FiredEvents;
use crate::tension::GlobalTension;
use crate::SimClock;

pub mod tuning {
    /// Grams of fissile material per warhead core.
    pub const FISSILE_G_PER_WARHEAD: u64 = 6_000;
    /// A first device is tested only with a safety margin banked.
    pub const TEST_DEVICE_BANK_G: u64 = 12_000;
    /// Monthly fissile output per facility level (grams), before
    /// modifiers. One level ~ an Oak Ridge / Mayak scale increment.
    pub const ENRICH_G_PER_LEVEL: u64 = 16_000;
    pub const REACTOR_G_PER_LEVEL: u64 = 15_000;
    /// Uranium feed: grams of fissile per economy stock unit (deposit
    /// size points/month — a superpower program needs imports too).
    pub const FISSILE_G_PER_URANIUM_UNIT: u64 = 5_000;
    /// Open-market ore purchases (Congo, Canada) per program per month.
    pub const URANIUM_IMPORT_TRICKLE: u64 = 40;
    /// Warheads assembled per month (assembly teams are the bottleneck
    /// until sealed-pit designs).
    pub const ASSEMBLED_PER_MONTH: u32 = 6;
    /// Scientist quality: speed = 800 + skill * 8 (permille).
    pub const SPEED_BASE_PERMILLE: u32 = 800;
    pub const SPEED_PER_SCIENTIST: u32 = 8;
    /// Program posture speed (permille) and monthly exposure accrual.
    pub const COVERT_SPEED: u32 = 700;
    pub const CRASH_SPEED: u32 = 1400;
    pub const EXPOSURE_COVERT: u32 = 4;
    pub const EXPOSURE_STANDARD: u32 = 10;
    pub const EXPOSURE_CRASH: u32 = 25;
    pub const EXPOSURE_PER_FACILITY_LEVEL: u32 = 3;
    /// Facility construction times (months).
    pub const REACTOR_BUILD_MONTHS: u32 = 24;
    pub const ENRICH_BUILD_MONTHS: u32 = 30;
    /// Thermonuclear lead time from authorization (months, divided by
    /// the program's speed factor — Ivy Mike Nov 1952 for the US).
    pub const THERMO_BASE_MONTHS: u64 = 34;
    /// Grid electricity demand per facility level, in regional power
    /// units (region demand runs ~industry x 10).
    pub const POWER_PER_ENRICH_LEVEL: u64 = 40;
    pub const POWER_PER_REACTOR_LEVEL: u64 = 8;
    /// World reaction (tension permille of the 0-100 display scale x10).
    pub const FIRST_TEST_TENSION: i32 = 60;
    pub const THERMO_TENSION: i32 = 80;
    /// Alert levels 0-3: tension on raising (per level) and a monthly
    /// carrying cost in tension while at 2+.
    pub const ALERT_RAISE_TENSION: i32 = 40;
    pub const ALERT_CARRY_TENSION: i32 = 5;
    /// Incident-hazard multiplier by the HIGHER of the two alert levels.
    pub const ALERT_HAZARD_MULT: [u32; 4] = [1, 2, 3, 4];
    /// Commander-request trigger: allied provinces lost to the enemy.
    pub const USE_REQUEST_PROVINCES: usize = 4;
    /// Months between commander requests.
    pub const USE_REQUEST_COOLDOWN_MONTHS: u64 = 8;
    /// Battlefield use: tension, casualties fraction (permille of the
    /// target province's population), enemy strength divisor.
    pub const TACTICAL_USE_TENSION: i32 = 350;
    pub const DEMONSTRATION_TENSION: i32 = 150;
    pub const TACTICAL_DEATH_PERMILLE: u64 = 300;
    pub const TACTICAL_STRENGTH_DIVISOR: u64 = 5;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Stage {
    /// Program founded; no producing facilities yet.
    Founded,
    /// Fissile material accumulating.
    Producing,
    /// First device detonated — a nuclear power.
    Tested,
    /// Thermonuclear weapons demonstrated.
    Thermonuclear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Route {
    Plutonium,
    Heu,
    Both,
}

impl Route {
    pub fn parse(s: &str) -> Self {
        match s {
            "Heu" => Route::Heu,
            "Both" => Route::Both,
            _ => Route::Plutonium,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgramPosture {
    Covert,
    Standard,
    Crash,
}

impl ProgramPosture {
    pub fn parse(s: &str) -> Self {
        match s {
            "Covert" => ProgramPosture::Covert,
            "Crash" => ProgramPosture::Crash,
            _ => ProgramPosture::Standard,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FacilityKind {
    Reactor,
    Enrichment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Program {
    pub stage: Stage,
    pub route: Route,
    pub posture: ProgramPosture,
    /// Banked fissile material, grams.
    pub fissile_g: u64,
    /// Total warhead cores produced (the number history counts).
    pub stockpile: u32,
    /// Weapons actually assembled and usable.
    pub assembled: u32,
    pub enrichment_level: u32,
    pub reactor_level: u32,
    /// Facilities under construction: (kind, months remaining).
    pub building: Vec<(FacilityKind, u32)>,
    /// Aggregate scientist skill.
    pub scientists: u32,
    /// Multiplicative event modifier (espionage, purges), permille.
    pub speed_mod_permille: u32,
    /// How much rivals can see, 0-1000. Drives estimate quality.
    pub exposure_permille: u32,
    /// The regional grid this program's plants draw on.
    pub site_region: Option<RegionId>,
    pub thermonuclear_authorized: bool,
    /// Calendar month index (year*12+month) of authorization.
    pub authorized_month: Option<i64>,
    /// Nuclear-capable bombers and combat radius.
    pub bombers: u32,
    pub bomber_range_km: u32,
    /// Extra reach on one-way missions.
    pub one_way_extra_km: u32,
    /// Whose territory hosts this nation's strike forces.
    pub basing_rights: Vec<CountryTag>,
    /// Bomber-generation improvement, km/year.
    pub range_growth_km_per_year: u32,
    /// Parade deception: inflate rival estimates of this arsenal.
    pub deception: bool,
    /// Strategic-forces alert level 0-3 (Peacetime / Increased /
    /// Airborne / Maximum). Costly to hold, visible to the enemy.
    pub alert: u8,
}

impl Program {
    fn new(route: Route) -> Self {
        Program {
            stage: Stage::Founded,
            route,
            posture: ProgramPosture::Standard,
            fissile_g: 0,
            stockpile: 0,
            assembled: 0,
            enrichment_level: 0,
            reactor_level: 0,
            building: Vec::new(),
            scientists: 8,
            speed_mod_permille: 1000,
            exposure_permille: 0,
            site_region: None,
            thermonuclear_authorized: false,
            authorized_month: None,
            bombers: 0,
            bomber_range_km: 0,
            one_way_extra_km: 0,
            basing_rights: Vec::new(),
            range_growth_km_per_year: 0,
            deception: false,
            alert: 0,
        }
    }

    /// Speed factor from scientists, posture, and event modifiers.
    pub fn speed_permille(&self) -> u64 {
        use tuning::*;
        let sci = (SPEED_BASE_PERMILLE + self.scientists * SPEED_PER_SCIENTIST) as u64;
        let posture = match self.posture {
            ProgramPosture::Covert => COVERT_SPEED,
            ProgramPosture::Standard => 1000,
            ProgramPosture::Crash => CRASH_SPEED,
        } as u64;
        sci * posture / 1000 * self.speed_mod_permille as u64 / 1000
    }

    /// Base monthly fissile output before grid/uranium modifiers, grams.
    pub fn base_production_g(&self) -> u64 {
        use tuning::*;
        (self.enrichment_level as u64 * ENRICH_G_PER_LEVEL
            + self.reactor_level as u64 * REACTOR_G_PER_LEVEL)
            * self.speed_permille()
            / 1000
    }
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct NuclearPrograms {
    pub programs: BTreeMap<CountryTag, Program>,
    /// One-way global flag: once a nuclear weapon is used in anger,
    /// every later use is cheaper for everyone (Tannenwald's taboo).
    pub taboo_broken: bool,
    /// Commander-request bookkeeping (the MacArthur chain).
    pub use_request_seq: u32,
    /// Calendar month index of the last request (0 = never).
    pub last_request_month: i64,
    use_resolved_cursor: usize,
    seeded: bool,
}

impl NuclearPrograms {
    pub fn found(&mut self, country: CountryTag, route: Route) {
        self.programs
            .entry(country)
            .or_insert_with(|| Program::new(route));
    }

    pub fn authorize_thermonuclear(&mut self, country: &CountryTag, month_index: i64) {
        if let Some(p) = self.programs.get_mut(country) {
            if !p.thermonuclear_authorized {
                p.thermonuclear_authorized = true;
                p.authorized_month = Some(month_index);
            }
        }
    }

    pub fn adjust_speed(&mut self, country: &CountryTag, permille: u32) {
        if let Some(p) = self.programs.get_mut(country) {
            p.speed_mod_permille = p.speed_mod_permille * permille / 1000;
        }
    }

    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (tag, p) in &self.programs {
            for b in tag.0.bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
            for v in [
                p.stage as u64,
                p.fissile_g,
                p.stockpile as u64,
                p.assembled as u64,
                p.exposure_permille as u64,
                p.enrichment_level as u64 + p.reactor_level as u64,
                p.alert as u64 + if self.taboo_broken { 100 } else { 0 },
            ] {
                h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// Calendar month index — leap-safe cadence arithmetic (the tick/720
/// shortcut drifts across leap years like 1952).
fn month_index(clock: &SimClock) -> i64 {
    clock.date.year as i64 * 12 + clock.date.month as i64
}

/// Seed on first tick; advance monthly. Runs in Politics after events.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
pub fn update_nuclear(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    stat: Res<EconomyStatic>,
    power: Res<RegionalPower>,
    mut balances: ResMut<NationalBalances>,
    mut programs: ResMut<NuclearPrograms>,
    mut tension: ResMut<GlobalTension>,
    mut fired: ResMut<FiredEvents>,
) {
    use tuning::*;
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;

    if !programs.seeded {
        programs.seeded = true;
        for seed in &data.nuclear {
            let mut p = Program::new(Route::parse(&seed.route));
            p.stage = match seed.stage.as_str() {
                "Tested" => Stage::Tested,
                "Producing" => Stage::Producing,
                _ => Stage::Founded,
            };
            p.stockpile = seed.stockpile;
            p.assembled = seed.assembled.min(seed.stockpile);
            p.enrichment_level = seed.enrichment_level;
            p.reactor_level = seed.reactor_level;
            p.scientists = seed.scientists;
            if seed.building_reactor_months > 0 {
                p.building
                    .push((FacilityKind::Reactor, seed.building_reactor_months));
            }
            if seed.thermonuclear_authorized {
                p.thermonuclear_authorized = true;
                p.authorized_month = Some(month_index(&clock));
            }
            p.bombers = seed.bombers;
            p.bomber_range_km = seed.bomber_range_km;
            p.one_way_extra_km = seed.one_way_extra_km;
            p.basing_rights = seed.basing_rights.clone();
            p.range_growth_km_per_year = seed.range_growth_km_per_year;
            // Programs that already tested are known to the world.
            p.exposure_permille = match p.stage {
                Stage::Tested | Stage::Thermonuclear => 750,
                Stage::Producing => 300,
                _ => 100,
            };
            programs.programs.entry(seed.country.clone()).or_insert(p);
        }
        return;
    }

    if !clock.new_month {
        return;
    }

    // Iterate deterministically; collect world-facing announcements.
    let tags: Vec<CountryTag> = programs.programs.keys().cloned().collect();
    let mut announcements: Vec<(String, String, i32)> = Vec::new();
    for tag in tags {
        // Site the program in its nation's most industrial region.
        let site = {
            let p = &programs.programs[&tag];
            p.site_region.or_else(|| {
                stat.region_industry
                    .iter()
                    .filter(|(r, _)| stat.region_owner.get(r) == Some(&tag))
                    .max_by_key(|(r, ind)| (**ind, std::cmp::Reverse(r.0)))
                    .map(|(r, _)| *r)
            })
        };
        let grid_factor = site
            .and_then(|r| power.by_region.get(&r))
            .map(|s| s.factor_permille)
            .unwrap_or(1000);

        let p = programs.programs.get_mut(&tag).unwrap();
        p.site_region = site;

        // Construction advances.
        let mut completed: Vec<FacilityKind> = Vec::new();
        for (kind, months) in p.building.iter_mut() {
            *months = months.saturating_sub(1);
            if *months == 0 {
                completed.push(*kind);
            }
        }
        p.building.retain(|(_, m)| *m > 0);
        for kind in completed {
            match kind {
                FacilityKind::Reactor => p.reactor_level += 1,
                FacilityKind::Enrichment => p.enrichment_level += 1,
            }
            if p.stage == Stage::Founded {
                p.stage = Stage::Producing;
            }
        }

        // Fissile production: base x grid, capped by uranium feed.
        if p.stage >= Stage::Producing && p.enrichment_level + p.reactor_level > 0 {
            let base = p.base_production_g();
            let gridded = base * grid_factor / 1000;
            let bal = balances.by_country.entry(tag.clone()).or_default();
            bal.uranium_stock += URANIUM_IMPORT_TRICKLE;
            let need_units = gridded.div_ceil(FISSILE_G_PER_URANIUM_UNIT).max(1);
            let feed_factor = (bal.uranium_stock * 1000 / need_units).min(1000);
            let produced = gridded * feed_factor / 1000;
            let consumed = produced.div_ceil(FISSILE_G_PER_URANIUM_UNIT);
            bal.uranium_stock = bal.uranium_stock.saturating_sub(consumed);
            p.fissile_g += produced;
        }

        // First test: fires once the bank holds a device plus margin.
        if p.stage == Stage::Producing && p.fissile_g >= TEST_DEVICE_BANK_G {
            p.fissile_g -= FISSILE_G_PER_WARHEAD;
            p.stage = Stage::Tested;
            p.exposure_permille = p.exposure_permille.max(750);
            announcements.push((
                "ANOMALOUS RADIOACTIVITY DETECTED".into(),
                format!(
                    "LONG-RANGE SAMPLING FLIGHTS REPORT FISSION PRODUCTS IN THE UPPER ATMOSPHERE. ANALYSIS INDICATES A NUCLEAR DETONATION WITHIN THE TERRITORY OF {}. THE MONOPOLY, WHERE IT EXISTED, IS OVER.",
                    tag.0
                ),
                FIRST_TEST_TENSION,
            ));
        }

        // Warhead conversion + assembly.
        while p.fissile_g >= FISSILE_G_PER_WARHEAD && p.stage >= Stage::Tested {
            p.fissile_g -= FISSILE_G_PER_WARHEAD;
            p.stockpile += 1;
        }
        if p.stage >= Stage::Tested && p.assembled < p.stockpile {
            p.assembled = (p.assembled + ASSEMBLED_PER_MONTH).min(p.stockpile);
        }

        // Thermonuclear follow-on.
        if p.stage == Stage::Tested && p.thermonuclear_authorized {
            if let Some(start) = p.authorized_month {
                let months_needed = (THERMO_BASE_MONTHS * 1000 / p.speed_permille().max(1)) as i64;
                if month_index(&clock) - start >= months_needed {
                    p.stage = Stage::Thermonuclear;
                    announcements.push((
                        "THERMONUCLEAR DETONATION CONFIRMED".into(),
                        format!(
                            "SEISMIC AND ATMOSPHERIC EVIDENCE CONFIRMS A DETONATION IN THE MEGATON RANGE BY {}. THE FISSION BOMB IS NOW THE SMALL ONE.",
                            tag.0
                        ),
                        THERMO_TENSION,
                    ));
                }
            }
        }

        // Holding high alert is itself a signal the world reads.
        if p.alert >= 2 {
            announcements.push((String::new(), String::new(), ALERT_CARRY_TENSION));
        }
        // Exposure accrual.
        let accrual = match p.posture {
            ProgramPosture::Covert => EXPOSURE_COVERT,
            ProgramPosture::Standard => EXPOSURE_STANDARD,
            ProgramPosture::Crash => EXPOSURE_CRASH,
        } + (p.enrichment_level + p.reactor_level) * EXPOSURE_PER_FACILITY_LEVEL;
        p.exposure_permille = (p.exposure_permille + accrual).min(1000);
    }

    for (title, body, tension_delta) in announcements {
        tension.apply(tension_delta);
        if !title.is_empty() {
            fired.notices.push((title, body));
        }
    }
}

/// Command handlers (called from apply_commands).
pub fn expand_facility(programs: &mut NuclearPrograms, country: &CountryTag, kind: &str) {
    use tuning::*;
    let Some(p) = programs.programs.get_mut(country) else {
        return;
    };
    let (kind, months) = match kind {
        "Enrichment" => (FacilityKind::Enrichment, ENRICH_BUILD_MONTHS),
        _ => (FacilityKind::Reactor, REACTOR_BUILD_MONTHS),
    };
    p.building.push((kind, months));
}

pub fn set_posture(programs: &mut NuclearPrograms, country: &CountryTag, posture: &str) {
    if let Some(p) = programs.programs.get_mut(country) {
        p.posture = ProgramPosture::parse(posture);
    }
}

/// The MacArthur chain: when the player's side is losing a war and the
/// bomb is available, the theater commander ASKS. Refusing holds the
/// taboo; a demonstration warns; battlefield use wins the province and
/// breaks the world's one-way flag. Runs after the crisis system.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
pub fn update_nuclear_use(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    player: Res<crate::military::PlayerCountry>,
    mut programs: ResMut<NuclearPrograms>,
    mut military: ResMut<crate::military::Military>,
    mut demo: ResMut<crate::demography::Demographics>,
    mut crises: ResMut<crate::crisis::Crises>,
    mut fired: ResMut<FiredEvents>,
    mut tension: ResMut<GlobalTension>,
) {
    use tuning::*;
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    let Some(me) = player.0.clone() else { return };

    // --- Consume answered requests -------------------------------------
    let new_resolved: Vec<(String, u8)> = fired
        .resolved
        .iter()
        .skip(programs.use_resolved_cursor)
        .filter(|(id, _)| id.starts_with("nuke-use-"))
        .cloned()
        .collect();
    programs.use_resolved_cursor = fired.resolved.len();
    for (_, option) in new_resolved {
        match option {
            1 => {
                // Demonstration shot over open water.
                tension.apply(DEMONSTRATION_TENSION);
                if let Some(p) = programs.programs.get_mut(&me) {
                    p.exposure_permille = 1000;
                }
                fired.notices.push((
                    "DEMONSTRATION SHOT".into(),
                    "AN ATOMIC DEVICE IS DETONATED OVER OPEN WATER, ANNOUNCED IN ADVANCE, VISIBLE FOR TWO HUNDRED MILES. THE MESSAGE REQUIRES NO TRANSLATION. THE TABOO, TECHNICALLY, HOLDS.".into(),
                ));
            }
            2 => {
                // Battlefield use: find the densest enemy-held province.
                let enemies: Vec<CountryTag> = military
                    .wars
                    .iter()
                    .filter_map(|(a, b)| {
                        if a == &me {
                            Some(b.clone())
                        } else if b == &me {
                            Some(a.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                let mut counts: BTreeMap<ugs_data::ProvinceId, usize> = BTreeMap::new();
                for f in military.formations.values() {
                    if enemies.contains(&f.owner) {
                        *counts.entry(f.location).or_default() += 1;
                    }
                }
                let Some((target, _)) = counts
                    .iter()
                    .max_by_key(|(id, n)| (**n, std::cmp::Reverse(id.0)))
                    .map(|(id, n)| (*id, *n))
                else {
                    continue;
                };
                let Some(prov) = data.provinces.get(&target) else {
                    continue;
                };
                // The weapon works. That is the trap.
                let mut killed_strength = 0u64;
                for f in military.formations.values_mut() {
                    if f.location == target && enemies.contains(&f.owner) {
                        let lost = f.strength - f.strength / TACTICAL_STRENGTH_DIVISOR;
                        killed_strength += lost;
                        f.strength /= TACTICAL_STRENGTH_DIVISOR;
                        f.cohesion = f.cohesion.min(50);
                    }
                }
                let civilian_dead = demo
                    .provinces
                    .get_mut(&target)
                    .map(|c| {
                        let dead = c.total() * TACTICAL_DEATH_PERMILLE / 1000;
                        c.rural -= c.rural * TACTICAL_DEATH_PERMILLE / 1000;
                        c.urban -= c.urban * TACTICAL_DEATH_PERMILLE / 1000;
                        c.educated -= c.educated * TACTICAL_DEATH_PERMILLE / 1000;
                        dead
                    })
                    .unwrap_or(0);
                if let Some(p) = programs.programs.get_mut(&me) {
                    p.assembled = p.assembled.saturating_sub(1);
                    p.stockpile = p.stockpile.saturating_sub(1);
                }
                programs.taboo_broken = true;
                tension.apply(TACTICAL_USE_TENSION);
                military.log(
                    clock.tick,
                    format!("ATOMIC WEAPON EMPLOYED AT {}", prov.name.to_uppercase()),
                );
                fired.notices.push((
                    "ATOMIC WEAPON EMPLOYED".into(),
                    format!(
                        "A NUCLEAR DEVICE HAS BEEN USED AGAINST TROOP CONCENTRATIONS AT {}. ENEMY FORMATIONS DESTROYED IN PLACE ({} MEN). CIVILIAN DEAD EST {}. THE TABOO IS BROKEN -- FOR EVERYONE, FOREVER. EVERY LATER CRISIS NOW CARRIES THIS RUNG.",
                        prov.name.to_uppercase(),
                        killed_strength * crate::military::tuning::MEN_PER_STRENGTH_POINT,
                        civilian_dead,
                    ),
                ));
                // The enemy's patron — the nuclear power of its bloc —
                // answers with an ultimatum crisis.
                let enemy_alignment = enemies.first().map(|e| military.alignment_of(data, e));
                let patron = programs
                    .programs
                    .keys()
                    .find(|t| {
                        **t != me
                            && !enemies.contains(t)
                            && Some(military.alignment_of(data, t)) == enemy_alignment
                    })
                    .cloned();
                if let Some(patron) = patron {
                    crises.spawn_ultimatum(
                        &clock,
                        patron.clone(),
                        me.clone(),
                        "NUCLEAR USE IN THEATER",
                        &mut fired,
                        programs.taboo_broken,
                    );
                }
            }
            _ => {
                fired.notices.push((
                    "RELEASE REFUSED".into(),
                    "THE COMMANDER'S REQUEST FOR ATOMIC RELEASE IS DENIED. THE WAR STAYS CONVENTIONAL. THE TABOO HOLDS -- AND EVERY YEAR IT HOLDS, IT GROWS STRONGER.".into(),
                ));
            }
        }
    }

    // --- Monthly trigger check -----------------------------------------
    if !clock.new_month {
        return;
    }
    let Some(prog) = programs.programs.get(&me) else {
        return;
    };
    if prog.stage < Stage::Tested || prog.assembled == 0 {
        return;
    }
    if programs.last_request_month > 0
        && month_index(&clock) - programs.last_request_month < USE_REQUEST_COOLDOWN_MONTHS as i64
    {
        return;
    }
    let enemies: Vec<CountryTag> = military
        .wars
        .iter()
        .filter_map(|(a, b)| {
            if a == &me {
                Some(b.clone())
            } else if b == &me {
                Some(a.clone())
            } else {
                None
            }
        })
        .collect();
    if enemies.is_empty() {
        return;
    }
    // Our side is losing when enemies hold this many friendly provinces.
    let lost = data
        .provinces
        .values()
        .filter(|p| {
            let holder = military.owner_of(p.id, &p.owner);
            enemies.contains(&holder)
                && p.owner != holder
                && (p.owner == me || military.at_war(&p.owner, &holder))
        })
        .count();
    if lost < USE_REQUEST_PROVINCES {
        return;
    }
    programs.last_request_month = month_index(&clock);
    programs.use_request_seq += 1;
    let id = format!("nuke-use-{}", programs.use_request_seq);
    fired.dynamic.push(crate::events::DynamicChoice {
        id,
        title: "COMMANDER REQUESTS ATOMIC RELEASE".into(),
        body: format!(
            "THEATER COMMAND REPORTS THE FRONT CANNOT BE HELD BY CONVENTIONAL MEANS. FORMAL REQUEST SUBMITTED FOR DISCRETIONARY ATOMIC AUTHORITY AGAINST TROOP CONCENTRATIONS. {} PROVINCES LOST. THE WEAPON WOULD WORK. THAT IS NOT THE QUESTION.",
            lost
        ),
        country: me.clone(),
        options: vec![
            "REFUSE RELEASE -- THE TABOO HOLDS".into(),
            "AUTHORIZE DEMONSTRATION SHOT".into(),
            "AUTHORIZE BATTLEFIELD USE".into(),
        ],
        deadline_tick: clock.tick + 96,
    });
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
            seed: 1950,
        });
        app.insert_resource(crate::demography::SimScenario(Arc::new(data)));
        app
    }

    #[test]
    fn programs_seed_the_1950_balance() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 2);
        let nukes = app.world().resource::<NuclearPrograms>();
        let usa = &nukes.programs[&CountryTag("USA".into())];
        let sov = &nukes.programs[&CountryTag("SOV".into())];
        assert_eq!(usa.stockpile, 299, "US stockpile at start");
        assert_eq!(sov.stockpile, 5, "Soviet stockpile at start");
        assert!(usa.assembled < usa.stockpile, "cores in civilian custody");
        assert_eq!(usa.stage, Stage::Tested);
        assert!(
            nukes.programs[&CountryTag("GBR".into())].stage == Stage::Producing,
            "UK mid-program"
        );
    }

    #[test]
    fn arsenals_grow_on_historical_curves() {
        let mut app = app_with_scenario();
        // Two years: through 1951.
        run_ticks(&mut app, 24 * 730);
        let nukes = app.world().resource::<NuclearPrograms>();
        let usa = &nukes.programs[&CountryTag("USA".into())];
        let sov = &nukes.programs[&CountryTag("SOV".into())];
        // Historical anchors: US ~438 (end 1951), ~841 (end 1952);
        // USSR ~25-50. Grid strain throttles the US complex realistically.
        assert!(
            (400..1100).contains(&usa.stockpile),
            "US stockpile after 2y: {}",
            usa.stockpile
        );
        assert!(
            (10..120).contains(&sov.stockpile),
            "Soviet stockpile after 2y: {}",
            sov.stockpile
        );
        assert!(
            usa.stockpile > sov.stockpile * 8,
            "the gap stays wide early"
        );
        assert!(usa.assembled > 100, "assembly proceeds monthly");
    }

    #[test]
    fn the_macarthur_moment_tempts_and_costs() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 2);
        app.world_mut()
            .resource_mut::<crate::command::PendingCommands>()
            .push(crate::command::SimCommand::SetPlayerCountry {
                country: Some(CountryTag("USA".into())),
            });
        // Through the invasion and the summer retreat: the KPA takes
        // ROK provinces, the commander asks for the bomb.
        run_ticks(&mut app, 24 * 245);
        let fired = app.world().resource::<FiredEvents>();
        let request = fired
            .dynamic
            .iter()
            .find(|d| d.id.starts_with("nuke-use-"))
            .cloned();
        let request = request.unwrap_or_else(|| {
            panic!(
                "commander should have requested release; dynamic={:?} notices={:?}",
                fired.dynamic.iter().map(|d| &d.id).collect::<Vec<_>>(),
                fired.notices.iter().map(|(t, _)| t).collect::<Vec<_>>()
            )
        });
        assert_eq!(request.options.len(), 3, "refuse / demonstrate / use");

        // Authorize battlefield use. The weapon works. That is the trap.
        let tension_before = app.world().resource::<GlobalTension>().value();
        app.world_mut()
            .resource_mut::<crate::command::PendingCommands>()
            .push(crate::command::SimCommand::ResolveEvent {
                id: request.id.clone(),
                option: 2,
            });
        run_ticks(&mut app, 3);
        let nukes = app.world().resource::<NuclearPrograms>();
        assert!(nukes.taboo_broken, "the taboo is broken for everyone");
        assert!(
            app.world().resource::<GlobalTension>().value() >= tension_before + 300,
            "tension explodes"
        );
        let fired = app.world().resource::<FiredEvents>();
        assert!(
            fired
                .notices
                .iter()
                .any(|(t, _)| t.contains("ATOMIC WEAPON EMPLOYED")),
            "the wire reports the strike"
        );
        // The patron answers: an ultimatum crisis opens at the nuclear rung.
        let crises = app.world().resource::<crate::crisis::Crises>();
        assert!(
            crises.active.iter().any(|c| c.rung == 6),
            "patron ultimatum at rung 6: {:?}",
            crises.active
        );
    }

    /// The full failure chain: tactical use, patron ultimatum, one more
    /// rung each — and the campaign ends, attributed.
    #[test]
    fn the_general_exchange_ends_the_campaign() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 2);
        app.world_mut()
            .resource_mut::<crate::command::PendingCommands>()
            .push(crate::command::SimCommand::SetPlayerCountry {
                country: Some(CountryTag("USA".into())),
            });
        run_ticks(&mut app, 24 * 245);
        let request_id = {
            let fired = app.world().resource::<FiredEvents>();
            fired
                .dynamic
                .iter()
                .find(|d| d.id.starts_with("nuke-use-"))
                .expect("commander request")
                .id
                .clone()
        };
        app.world_mut()
            .resource_mut::<crate::command::PendingCommands>()
            .push(crate::command::SimCommand::ResolveEvent {
                id: request_id,
                option: 2,
            });
        run_ticks(&mut app, 3);
        // A hardliner Kremlin: the ultimatum will be pressed home.
        app.world_mut()
            .resource_mut::<crate::crisis::Crises>()
            .resolve
            .insert(CountryTag("SOV".into()), 99);
        let ultimatum_id = {
            let fired = app.world().resource::<FiredEvents>();
            fired
                .dynamic
                .iter()
                .find(|d| d.id.ends_with("-r6") && d.country.0 == "USA")
                .expect("ultimatum decision for the player")
                .id
                .clone()
        };
        // The player presses on across firebreak B...
        app.world_mut()
            .resource_mut::<crate::command::PendingCommands>()
            .push(crate::command::SimCommand::ResolveEvent {
                id: ultimatum_id,
                option: 1,
            });
        // ...and the hardliners answer with the last rung.
        run_ticks(&mut app, 24 * 6);
        let go = app.world().get_resource::<crate::crisis::GameOver>();
        let go = go.expect("the general exchange ends the campaign");
        assert!(
            go.dead > 10_000_000,
            "megadeaths from real demography: {}",
            go.dead
        );
        assert!(!go.cities.is_empty(), "the final wire has city names");
        assert_eq!(
            go.initiator.0, "SOV",
            "attribution: who crossed the last rung"
        );
    }

    #[test]
    fn britain_gets_the_bomb_around_1952() {
        let mut app = app_with_scenario();
        // Through 1953: Windscale completes ~late 1951, device banks up.
        run_ticks(&mut app, 24 * 1460);
        let nukes = app.world().resource::<NuclearPrograms>();
        let gbr = &nukes.programs[&CountryTag("GBR".into())];
        assert!(
            gbr.stage >= Stage::Tested,
            "UK should have tested by end 1953 (stage {:?}, fissile {} g)",
            gbr.stage,
            gbr.fissile_g
        );
        let fired = app.world().resource::<FiredEvents>();
        assert!(
            fired
                .notices
                .iter()
                .any(|(t, b)| t.contains("RADIOACTIVITY") && b.contains("GBR")),
            "the world detects the British test"
        );
    }
}
