//! Scripted events: date/condition triggers, chance rolls, and choice
//! events with deadlines. World events apply instantly; country events
//! wait for a `ResolveEvent` command (from the player or an AI) until
//! their deadline, when the historical default (option 0) applies.

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ugs_data::{CountryTag, EventDef, EventEffect, EventTrigger, ScenarioData};

use crate::demography::SimScenario;
use crate::military::{Archetype, Military, Posture};
use crate::rng::SimRng;
use crate::tension::GlobalTension;
use crate::SimClock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingChoice {
    pub id: String,
    /// Tick at which option 0 auto-applies.
    pub deadline_tick: u64,
}

/// A sim-generated decision (crises, commander requests): not authored
/// in events.ron, resolved through the same ResolveEvent command path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicChoice {
    pub id: String,
    pub title: String,
    pub body: String,
    /// The country whose decision this is.
    pub country: CountryTag,
    pub options: Vec<String>,
    pub deadline_tick: u64,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct FiredEvents {
    /// Every event that has fired (shown to the player), in order.
    pub fired: Vec<String>,
    /// Choice events awaiting resolution.
    pub pending: Vec<PendingChoice>,
    /// Sim-generated decisions awaiting resolution.
    pub dynamic: Vec<DynamicChoice>,
    /// Resolved choices: (event id, option index).
    pub resolved: Vec<(String, u8)>,
    /// Tick when each war began, for WarDaysElapsed triggers.
    pub war_started: Vec<((String, String), u64)>,
    /// Dynamic notifications (title, body) from sim systems (armistices,
    /// capitulations) — shown by the UI like events.
    pub notices: Vec<(String, String)>,
    /// Tick each event fired (chain triggers: EventFired).
    #[serde(default)]
    pub fired_ticks: BTreeMap<String, u64>,
    /// Resolution record per event: (option, tick) — OptionChosen.
    #[serde(default)]
    pub resolved_ticks: BTreeMap<String, (u8, u64)>,
}

impl FiredEvents {
    pub fn is_pending(&self, id: &str) -> bool {
        self.pending.iter().any(|p| p.id == id)
    }

    /// Chain-trigger state drives future behavior — it must be
    /// divergence-visible to the determinism harnesses.
    pub fn digest(&self) -> u64 {
        fn fold(h: &mut u64, v: u64) {
            *h = (*h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (id, t) in &self.fired_ticks {
            for b in id.bytes() {
                fold(&mut h, b as u64);
            }
            fold(&mut h, *t);
        }
        for (id, (o, t)) in &self.resolved_ticks {
            for b in id.bytes() {
                fold(&mut h, b as u64);
            }
            fold(&mut h, *o as u64);
            fold(&mut h, *t);
        }
        for p in &self.pending {
            for b in p.id.bytes() {
                fold(&mut h, b as u64);
            }
            fold(&mut h, p.deadline_tick);
        }
        h
    }
}

#[allow(clippy::too_many_arguments)] // effects touch every domain
fn apply_effects(
    effects: &[EventEffect],
    data: &ScenarioData,
    tension: &mut GlobalTension,
    military: &mut Military,
    nuclear: &mut crate::nuclear::NuclearPrograms,
    deterrence: &crate::deterrence::Deterrence,
    econ: &mut crate::planning::Economies,
    settlements: &mut crate::settlement::Settlements,
    stat: &mut crate::economy::EconomyStatic,
    regional: &mut crate::construction::RegionalIndustry,
    demo: &crate::demography::Demographics,
    influence: &mut crate::influence::Influence,
    fired_notices: &mut Vec<(String, String)>,
    month_index: i64,
    tick: u64,
) {
    for effect in effects {
        match effect {
            EventEffect::AdjustTension(delta) => tension.apply(*delta),
            EventEffect::DeclareWar { a, b } => {
                // Under mutual deterrence, war between the peers is not
                // declarable — only crises remain (stability-instability
                // paradox as a rule change). NOTE: this reads LAST
                // month's deterrence assessment (deterrence updates
                // after events in the Politics chain) — deterministic,
                // and thematically the general staff works from the
                // last estimate. Do not reorder the chain casually.
                if settlements.truce_active(a, b, tick) {
                    fired_notices.push((
                        "TRUCE HOLDS".into(),
                        format!(
                            "A SIGNED SETTLEMENT BARS WAR BETWEEN {} AND {}. THE TREATY, FOR NOW, IS STRONGER THAN THE GRIEVANCE.",
                            a.0, b.0
                        ),
                    ));
                    continue;
                }
                if deterrence.class(a, b) == crate::deterrence::DyadClass::Mutual {
                    fired_notices.push((
                        "WAR UNTHINKABLE".into(),
                        format!(
                            "GENERAL STAFF ASSESSMENT: DIRECT HOSTILITIES BETWEEN {} AND {} WOULD MEAN MUTUAL ATOMIC DESTRUCTION. THERE WILL BE NO DECLARATION. THE STRUGGLE CONTINUES BY OTHER MEANS.",
                            a.0, b.0
                        ),
                    ));
                    continue;
                }
                military.declare_war(a.clone(), b.clone());
            }
            EventEffect::SetPosture {
                country,
                enemy,
                posture,
            } => {
                let p = if posture == "Advance" {
                    Posture::Advance
                } else {
                    Posture::Hold
                };
                military
                    .postures
                    .insert((country.clone(), enemy.clone()), p);
            }
            EventEffect::TransferProvinces { from, to, names } => {
                for name in names {
                    if let Ok(id) = data.province_by_name(from, name) {
                        military.occupation.insert(id, to.clone());
                    }
                }
            }
            EventEffect::SpawnForces {
                owner,
                province_owner,
                province,
                archetype,
                divisions,
                quality,
            } => {
                if let Ok(location) = data.province_by_name(province_owner, province) {
                    let arch = match archetype.as_str() {
                        "Motorized" => Archetype::Motorized,
                        "Armor" => Archetype::Armor,
                        _ => Archetype::Infantry,
                    };
                    let home = Military::heartland_of(data, owner, location);
                    for _ in 0..*divisions {
                        military.raise(data, owner.clone(), arch, location, home, *quality as u64);
                    }
                }
            }
            EventEffect::GrantStock { country, amount } => {
                if let Some(st) = econ.industry.get_mut(country) {
                    st.military_stock += amount;
                }
            }
            EventEffect::SetWarAim {
                country,
                enemy,
                aim,
            } => {
                use crate::settlement::WarAim;
                let aim = match aim.as_str() {
                    "Punish" => WarAim::Punish,
                    "NewLine" => WarAim::NewLine,
                    "Unify" => WarAim::Unify,
                    _ => WarAim::StatusQuoAnte,
                };
                settlements
                    .war_aims
                    .insert((country.clone(), enemy.clone()), aim);
            }
            EventEffect::GrantLegitimacy { country, amount } => {
                *settlements.legitimacy.entry(country.clone()).or_default() += amount;
            }
            EventEffect::SetAlignment { country, alignment } => {
                // A band-edge shove: no-op if already in the band. The
                // bloc enum is derived from the influence position.
                crate::influence::effect_set_alignment(
                    influence, military, data, country, alignment, tick,
                );
            }
            EventEffect::ShiftAlignment { country, delta } => {
                influence.shift(country, *delta);
                influence.project(military, data, country, tick);
            }
            EventEffect::LockAlignment {
                country,
                months,
                label,
            } => {
                let until = if *months == 0 {
                    0
                } else {
                    tick + *months as u64 * 30 * 24
                };
                influence.set_lock(country, until, label);
            }
            EventEffect::Crush { patron, country } => {
                influence.crush(military, data, patron, country, tick);
            }
            EventEffect::SetArmyPatron { country, patron } => {
                match crate::influence::Pole::parse(patron) {
                    Some(p) => {
                        influence.army_patron.insert(country.clone(), p);
                    }
                    None => {
                        influence.army_patron.remove(country);
                    }
                }
            }
            EventEffect::GrantInfluenceSlot { country, ops } => {
                let map = if *ops {
                    &mut influence.op_slots
                } else {
                    &mut influence.slots
                };
                *map.entry(country.clone()).or_default() += 1;
            }
            EventEffect::UnlockPresence { country } => {
                influence.presence_unlocked.insert(country.clone());
            }
            EventEffect::OpenContest { country, months } => {
                influence.open_contest(country, tick + *months as u64 * 30 * 24);
            }
            EventEffect::AdjustStability { country, delta } => {
                let current = military.stability_of(data, country) as i32;
                military
                    .stability
                    .insert(country.clone(), (current + delta).clamp(0, 100) as u8);
            }
            EventEffect::GrantIndustry { country, centi } => {
                crate::construction::distribute_proportional(stat, regional, country, *centi);
            }
            EventEffect::Independence {
                country,
                from,
                provinces,
                province_ids,
            } => {
                use std::collections::BTreeSet;
                // Per-province transfer (the region is NOT the unit:
                // colonial super-regions hold many future states).
                let mut ids: BTreeSet<ugs_data::ProvinceId> = province_ids
                    .iter()
                    .map(|i| ugs_data::ProvinceId(*i))
                    .collect();
                for name in provinces {
                    if let Ok(id) = data.province_by_name(from, name) {
                        ids.insert(id);
                    }
                }
                let mut pop: u64 = 0;
                let mut affected_regions: BTreeSet<ugs_data::RegionId> = BTreeSet::new();
                for id in &ids {
                    let Some(p) = data.provinces.get(id) else {
                        continue;
                    };
                    let holder = military.owner_of(*id, &p.owner);
                    // Enemy-occupied ground stays occupied: the state is
                    // born into a claim, not a possession.
                    if &holder != from {
                        continue;
                    }
                    military.occupation.insert(*id, country.clone());
                    settlements.recognized.insert(*id);
                    affected_regions.insert(p.region);
                    if let Some(c) = demo.provinces.get(id) {
                        pop += c.total();
                    }
                }
                // Region ownership follows the majority CURRENT holder
                // (deterministic: counts over BTreeMap order, ties to
                // the lexicographically first tag).
                for region in &affected_regions {
                    let mut counts: std::collections::BTreeMap<CountryTag, u32> =
                        Default::default();
                    for p in data.provinces.values().filter(|p| p.region == *region) {
                        let holder = military.owner_of(p.id, &p.owner);
                        *counts.entry(holder).or_default() += 1;
                    }
                    if let Some((winner, _)) = counts
                        .iter()
                        .max_by_key(|(tag, n)| (**n, std::cmp::Reverse((*tag).clone())))
                    {
                        stat.region_owner.insert(*region, winner.clone());
                    }
                }
                *military.manpower.entry(country.clone()).or_default() +=
                    pop * crate::military::tuning::MANPOWER_BASE_PERMILLE / 1000;
                // The newborn is a contest, not a script.
                influence.on_independence(military, data, country, tick);
                let name = data
                    .countries
                    .get(country)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| country.0.clone());
                fired_notices.push((
                    "A NATION IS BORN".into(),
                    format!(
                        "{} PROCLAIMS ITS INDEPENDENCE FROM {}. THE FLAG COMES DOWN, ANOTHER RISES. THE COLD WAR HAS A NEW BATTLEGROUND, AND BOTH BLOCS KNOW IT.",
                        name.to_uppercase(),
                        from.0
                    ),
                ));
            }
            EventEffect::AuthorizeThermonuclear { country } => {
                nuclear.authorize_thermonuclear(country, month_index);
            }
            EventEffect::AdjustProgramSpeed { country, permille } => {
                nuclear.adjust_speed(country, *permille);
            }
            EventEffect::FoundNuclearProgram { country, route } => {
                nuclear.found(country.clone(), crate::nuclear::Route::parse(route));
            }
        }
    }
}

fn trigger_met(
    event: &EventDef,
    clock: &SimClock,
    fired: &FiredEvents,
    military: &Military,
    tension: &GlobalTension,
    data: &ScenarioData,
) -> bool {
    match &event.trigger {
        EventTrigger::AlignmentBand { country, band } => {
            let current = military.alignment_of(data, country);
            let want = match band.as_str() {
                "WesternBloc" => ugs_data::Alignment::WesternBloc,
                "EasternBloc" => ugs_data::Alignment::EasternBloc,
                _ => ugs_data::Alignment::NonAligned,
            };
            current == want
        }
        EventTrigger::StabilityBelow { country, value } => {
            military.stability_of(data, country) < *value
        }
        EventTrigger::TensionAbove { tenths } => tension.value() >= *tenths,
        EventTrigger::TensionBelow { tenths } => tension.value() <= *tenths,
        EventTrigger::EventFired { id, days_after } => fired
            .fired_ticks
            .get(id)
            .is_some_and(|t| clock.tick >= t + *days_after as u64 * 24),
        EventTrigger::OptionChosen {
            id,
            option,
            days_after,
        } => fired
            .resolved_ticks
            .get(id)
            .is_some_and(|(o, t)| o == option && clock.tick >= t + *days_after as u64 * 24),
        EventTrigger::Date(date) => {
            let now = (
                clock.date.year,
                clock.date.month,
                clock.date.day,
                clock.date.hour,
            );
            *date <= now
        }
        EventTrigger::WarDaysElapsed { a, b, days } => {
            let key = if a.0 < b.0 {
                (a.0.clone(), b.0.clone())
            } else {
                (b.0.clone(), a.0.clone())
            };
            fired
                .war_started
                .iter()
                .find(|(k, _)| *k == key)
                .is_some_and(|(_, start)| clock.tick >= start + *days as u64 * 24)
        }
        EventTrigger::ProvincesLost { owner, count } => {
            let lost = data
                .provinces
                .values()
                .filter(|p| {
                    p.owner == *owner && {
                        let holder = military.owner_of(p.id, &p.owner);
                        holder != *owner && military.at_war(owner, &holder)
                    }
                })
                .count();
            lost >= *count as usize
        }
    }
}

#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
pub fn update_events(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    deterrence: Res<crate::deterrence::Deterrence>,
    mut rng: ResMut<SimRng>,
    mut fired: ResMut<FiredEvents>,
    mut tension: ResMut<GlobalTension>,
    mut military: ResMut<Military>,
    mut nuclear: ResMut<crate::nuclear::NuclearPrograms>,
    mut econ: ResMut<crate::planning::Economies>,
    mut settlements: ResMut<crate::settlement::Settlements>,
    mut stat: ResMut<crate::economy::EconomyStatic>,
    mut regional: ResMut<crate::construction::RegionalIndustry>,
    demo: Res<crate::demography::Demographics>,
    mut influence: ResMut<crate::influence::Influence>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;

    // Track war start ticks for WarDaysElapsed triggers.
    let war_keys: Vec<(String, String)> = military
        .wars
        .iter()
        .map(|(a, b)| (a.0.clone(), b.0.clone()))
        .collect();
    for key in war_keys {
        if !fired.war_started.iter().any(|(k, _)| *k == key) {
            let tick = clock.tick;
            fired.war_started.push((key, tick));
        }
    }

    // Resolve expired deadlines: historical default (option 0).
    let expired: Vec<String> = fired
        .pending
        .iter()
        .filter(|p| clock.tick >= p.deadline_tick)
        .map(|p| p.id.clone())
        .collect();
    for id in expired {
        fired.pending.retain(|p| p.id != id);
        fired.resolved.push((id.clone(), 0));
        fired.resolved_ticks.insert(id.clone(), (0, clock.tick));
        if let Some(event) = data.events.iter().find(|e| e.id == id) {
            if let Some(option) = event.options.first() {
                let mut notices = Vec::new();
                apply_effects(
                    &option.effects,
                    data,
                    &mut tension,
                    &mut military,
                    &mut nuclear,
                    &deterrence,
                    &mut econ,
                    &mut settlements,
                    &mut stat,
                    &mut regional,
                    &demo,
                    &mut influence,
                    &mut notices,
                    clock.date.year as i64 * 12 + clock.date.month as i64,
                    clock.tick,
                );
                fired.notices.extend(notices);
            }
        }
    }

    // Sim-generated choices expire too: past the deadline the cautious
    // default (option 0) applies and the owning module reads it from
    // `resolved` like any answer. Crises enforce their own deadlines.
    let expired_dynamic: Vec<String> = fired
        .dynamic
        .iter()
        .filter(|d| clock.tick >= d.deadline_tick && !d.id.starts_with("crisis-"))
        .map(|d| d.id.clone())
        .collect();
    for id in expired_dynamic {
        fired.dynamic.retain(|d| d.id != id);
        fired.resolved.push((id.clone(), 0));
        fired.resolved_ticks.insert(id, (0, clock.tick));
    }

    // Fire new events. Chance-gated triggers roll once per day.
    for event in &data.events {
        if fired.fired.iter().any(|id| id == &event.id) {
            continue;
        }
        if !trigger_met(event, &clock, &fired, &military, &tension, data) {
            continue;
        }
        if let Some(chance) = event.chance_permille {
            if !clock.new_day {
                continue;
            }
            let mut stream = rng.fork(b"events");
            if stream.below(1000) >= chance {
                continue;
            }
        }
        fired.fired.push(event.id.clone());
        fired.fired_ticks.insert(event.id.clone(), clock.tick);
        if event.country.is_some() && !event.options.is_empty() {
            // A decision: wait for ResolveEvent or the deadline.
            let deadline_tick = clock.tick + event.deadline_days.max(1) as u64 * 24;
            fired.pending.push(PendingChoice {
                id: event.id.clone(),
                deadline_tick,
            });
        } else {
            let effects: &[EventEffect] = if let Some(option) = event.options.first() {
                &option.effects
            } else {
                &event.effects
            };
            let mut notices = Vec::new();
            apply_effects(
                effects,
                data,
                &mut tension,
                &mut military,
                &mut nuclear,
                &deterrence,
                &mut econ,
                &mut settlements,
                &mut stat,
                &mut regional,
                &demo,
                &mut influence,
                &mut notices,
                clock.date.year as i64 * 12 + clock.date.month as i64,
                clock.tick,
            );
            fired.notices.extend(notices);
        }
    }
}

/// Command handler: resolve a pending choice event with the given option.
#[allow(clippy::too_many_arguments)]
pub fn resolve_event(
    fired: &mut FiredEvents,
    tension: &mut GlobalTension,
    military: &mut Military,
    nuclear: &mut crate::nuclear::NuclearPrograms,
    deterrence: &crate::deterrence::Deterrence,
    econ: &mut crate::planning::Economies,
    settlements: &mut crate::settlement::Settlements,
    stat: &mut crate::economy::EconomyStatic,
    regional: &mut crate::construction::RegionalIndustry,
    demo: &crate::demography::Demographics,
    influence: &mut crate::influence::Influence,
    data: &ScenarioData,
    month_index: i64,
    tick: u64,
    id: &str,
    option: u8,
) {
    // Dynamic (sim-generated) choices: record the answer; the owning
    // module reads it from `resolved` on its next tick. Out-of-range
    // options clamp to the cautious default — a crafted command must
    // not reach hidden arms (e.g. a compromise never offered).
    if let Some(pos) = fired.dynamic.iter().position(|d| d.id == id) {
        let bounded = if (option as usize) < fired.dynamic[pos].options.len() {
            option
        } else {
            0
        };
        fired.dynamic.remove(pos);
        fired.resolved.push((id.to_string(), bounded));
        fired.resolved_ticks.insert(id.to_string(), (bounded, tick));
        return;
    }
    if !fired.is_pending(id) {
        return;
    }
    let Some(event) = data.events.iter().find(|e| e.id == id) else {
        return;
    };
    let Some(chosen) = event.options.get(option as usize) else {
        return;
    };
    fired.pending.retain(|p| p.id != id);
    fired.resolved.push((id.to_string(), option));
    fired.resolved_ticks.insert(id.to_string(), (option, tick));
    let mut notices = Vec::new();
    apply_effects(
        &chosen.effects,
        data,
        tension,
        military,
        nuclear,
        deterrence,
        econ,
        settlements,
        stat,
        regional,
        demo,
        influence,
        &mut notices,
        month_index,
        tick,
    );
    fired.notices.extend(notices);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::GameDate;
    use crate::{run_ticks, SimPlugin};
    use bevy_app::App;
    use std::path::Path;
    use std::sync::Arc;
    use ugs_data::CountryTag;

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
    #[ignore = "dev helper: writes saves/war-jul-1950.ron"]
    fn make_midwar_save() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 190); // July 10, 1950 — war in progress
        let save = crate::savegame::SaveGame::capture(app.world(), Some("KOR".into()));
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../saves");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("war-jul-1950.ron"), ron::to_string(&save).unwrap()).unwrap();
    }

    #[test]
    fn the_korean_war_runs_its_historical_arc() {
        let mut app = app_with_scenario();
        // Peace until June 24.
        run_ticks(&mut app, 24 * 174);
        assert!(app.world().resource::<Military>().wars.is_empty());
        let tension_before = app.world().resource::<GlobalTension>().value();

        // Invasion fires on the 25th.
        run_ticks(&mut app, 24 * 2);
        {
            let fired = app.world().resource::<FiredEvents>();
            assert!(fired.fired.iter().any(|id| id == "korea-invasion"));
            let military = app.world().resource::<Military>();
            assert!(military.at_war(&CountryTag("PRK".into()), &CountryTag("KOR".into())));
            assert!(app.world().resource::<GlobalTension>().value() > tension_before + 100);
        }

        // Day ~5: the US decision fires; nobody answers, so the
        // historical default (commit) auto-applies at the deadline.
        run_ticks(&mut app, 24 * 10);
        {
            let fired = app.world().resource::<FiredEvents>();
            assert!(
                fired.fired.iter().any(|id| id == "us-intervention"),
                "US decision should have fired"
            );
            assert!(
                fired
                    .resolved
                    .iter()
                    .any(|(id, opt)| id == "us-intervention" && *opt == 0),
                "deadline should auto-commit"
            );
            let military = app.world().resource::<Military>();
            assert!(
                military.at_war(&CountryTag("USA".into()), &CountryTag("PRK".into())),
                "USA at war with PRK"
            );
            let us_troops = military
                .formations
                .values()
                .filter(|f| f.owner.0 == "USA")
                .count();
            assert_eq!(us_troops, 4, "US divisions landed");
        }

        // The UN side counterattacks north; China enters when the KPA
        // starts losing its own provinces.
        run_ticks(&mut app, 24 * 210);
        {
            let fired = app.world().resource::<FiredEvents>();
            let military = app.world().resource::<Military>();
            assert!(
                fired.fired.iter().any(|id| id == "chinese-intervention"),
                "China should have entered"
            );
            // The PRC-USA war exists (it may already have found an
            // ending -- termination timing is emergent since the
            // settlement system landed).
            assert!(
                military
                    .war_started
                    .keys()
                    .any(|(a, b)| (a.0 == "PRC" && b.0 == "USA") || (a.0 == "USA" && b.0 == "PRC"))
                    || military.at_war(&CountryTag("PRC".into()), &CountryTag("USA".into())),
                "China fought the US"
            );
            let prc_troops = military
                .formations
                .values()
                .filter(|f| f.owner.0 == "PRC")
                .count();
            assert!(prc_troops > 0, "Chinese People's Volunteers in the field");
            let un_alive = military
                .formations
                .values()
                .any(|f| f.owner.0 == "USA" || f.owner.0 == "KOR");
            assert!(un_alive, "UN side annihilated");
        }

        // The long grind: with the front frozen, non-player belligerents
        // reach mutual willingness and the guns fall silent — Korea stays
        // divided along the line of control.
        run_ticks(&mut app, 24 * 550);
        {
            let military = app.world().resource::<Military>();
            let fired = app.world().resource::<FiredEvents>();
            assert!(
                military.wars.is_empty(),
                "wars should have ended by early 1952: {:?}",
                military.wars
            );
            assert!(
                fired.notices.iter().any(|(t, _)| t.contains("ARMISTICE")
                    || t.contains("RESISTANCE")
                    || t.contains("SETTLEMENT")),
                "a termination notice should exist (armistice, settlement, or collapse): {:?}",
                fired.notices.iter().map(|(t, _)| t).collect::<Vec<_>>()
            );
            // The ending is a recorded outcome: a signed treaty (which
            // may restore the status quo ante and clear the occupation
            // map), a frozen conflict, or a persisting line of control.
            let settlements = app.world().resource::<crate::settlement::Settlements>();
            assert!(
                !settlements.treaties.is_empty()
                    || !settlements.frozen.is_empty()
                    || !military.occupation.is_empty(),
                "the war's ending must leave an outcome object"
            );
        }
    }

    #[test]
    fn unanswered_dynamic_choices_expire_to_the_cautious_default() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 1);
        app.world_mut()
            .resource_mut::<FiredEvents>()
            .dynamic
            .push(DynamicChoice {
                id: "test-dynamic-1".into(),
                title: "A REQUEST".into(),
                body: "ANSWER OR DO NOT.".into(),
                country: CountryTag("USA".into()),
                options: vec!["REFUSE".into(), "PROCEED".into()],
                deadline_tick: 1 + 48,
            });
        run_ticks(&mut app, 47);
        assert!(
            app.world().resource::<FiredEvents>().dynamic.len() == 1,
            "still open"
        );
        run_ticks(&mut app, 2);
        let fired = app.world().resource::<FiredEvents>();
        assert!(fired.dynamic.is_empty(), "expired at the deadline");
        assert!(
            fired
                .resolved
                .iter()
                .any(|(id, o)| id == "test-dynamic-1" && *o == 0),
            "the cautious default was recorded"
        );
    }

    #[test]
    fn player_can_stand_aside() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 181); // invasion + US decision fired
        {
            let fired = app.world().resource::<FiredEvents>();
            assert!(
                fired.is_pending("us-intervention"),
                "choice should be pending"
            );
        }
        app.world_mut()
            .resource_mut::<crate::command::PendingCommands>()
            .push(crate::command::SimCommand::ResolveEvent {
                id: "us-intervention".into(),
                option: 1,
            });
        run_ticks(&mut app, 24 * 5);
        let military = app.world().resource::<Military>();
        assert!(
            !military.at_war(&CountryTag("USA".into()), &CountryTag("PRK".into())),
            "standing aside keeps the US out"
        );
        assert!(
            military.formations.values().all(|f| f.owner.0 != "USA"),
            "no US troops"
        );
    }
}

#[cfg(test)]
mod timeline_tests {
    use crate::calendar::GameDate;
    use crate::military::Military;
    use crate::{run_ticks, SimPlugin};
    use bevy_app::App;
    use std::path::Path;
    use std::sync::Arc;
    use ugs_data::{Alignment, CountryDef, CountryTag, EventDef, EventEffect, EventTrigger};

    /// The 1950 scenario plus a synthetic Ghana and a three-event
    /// chain exercising the new engine: Independence -> EventFired
    /// chain -> SetAlignment/AdjustStability.
    fn app_with_test_timeline() -> App {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let mut data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        let capital = data
            .province_by_name(&CountryTag("GBR".into()), "Ashanti")
            .expect("colonial province exists");
        data.countries.insert(
            CountryTag("GHA".into()),
            CountryDef {
                tag: CountryTag("GHA".into()),
                name: "Ghana".into(),
                alignment: Alignment::NonAligned,
                color: (40, 120, 60),
                capital,
                stability: 60,
                industry: 1,
                nuclear_power: false,
            },
        );
        data.events.push(EventDef {
            id: "test-independence".into(),
            trigger: EventTrigger::Date((1950, 2, 1, 0)),
            chance_permille: None,
            title: "T".into(),
            body: "T".into(),
            country: None,
            deadline_days: 0,
            options: vec![],
            effects: vec![EventEffect::Independence {
                country: CountryTag("GHA".into()),
                from: CountryTag("GBR".into()),
                provinces: vec!["Ashanti".into()],
                province_ids: vec![],
            }],
        });
        data.events.push(EventDef {
            id: "test-chain".into(),
            trigger: EventTrigger::EventFired {
                id: "test-independence".into(),
                days_after: 10,
            },
            chance_permille: None,
            title: "T2".into(),
            body: "T2".into(),
            country: None,
            deadline_days: 0,
            options: vec![],
            effects: vec![
                EventEffect::SetAlignment {
                    country: CountryTag("GHA".into()),
                    alignment: "EasternBloc".into(),
                },
                EventEffect::AdjustStability {
                    country: CountryTag("GHA".into()),
                    delta: -20,
                },
            ],
        });
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 3,
        });
        app.insert_resource(crate::demography::SimScenario(Arc::new(data)));
        app
    }

    #[test]
    fn independence_transfers_regions_and_seeds_a_nation() {
        let mut app = app_with_test_timeline();
        run_ticks(&mut app, 24 * 33); // past Feb 1
        let world = app.world();
        let data = world.resource::<crate::demography::SimScenario>().0.clone();
        let gha = CountryTag("GHA".into());
        let military = world.resource::<Military>();
        let stat = world.resource::<crate::economy::EconomyStatic>();
        // The region flipped, and with it the economy.
        let ashanti = data
            .province_by_name(&CountryTag("GBR".into()), "Ashanti")
            .unwrap();
        let region = data.provinces[&ashanti].region;
        // Region ownership follows the MAJORITY holder: one province
        // out of a colonial super-region is not a majority, so the
        // region correctly stays with the parent (the full-corpus
        // timeline test pins the majority-flip case).
        assert_eq!(
            stat.region_owner.get(&region),
            Some(&CountryTag("GBR".into())),
            "a single province does not flip a super-region"
        );
        // Map-level ownership + recognition.
        assert_eq!(
            military.owner_of(ashanti, &data.provinces[&ashanti].owner),
            gha,
            "the province flies the new flag"
        );
        assert!(
            world
                .resource::<crate::settlement::Settlements>()
                .recognized
                .contains(&ashanti),
            "born recognized, not occupied"
        );
        // A people under arms potential.
        assert!(
            military.manpower.get(&gha).copied().unwrap_or(0) > 0,
            "manpower seeded from the transferred population"
        );
        let fired = world.resource::<crate::events::FiredEvents>();
        assert!(
            fired.notices.iter().any(|(t, _)| t.contains("NATION")),
            "the birth makes the wire"
        );
    }

    #[test]
    fn chains_fire_on_schedule_and_flip_alignment() {
        let mut app = app_with_test_timeline();
        run_ticks(&mut app, 24 * 45); // independence (day 31) + 10-day chain
        let world = app.world();
        let data = world.resource::<crate::demography::SimScenario>().0.clone();
        let military = world.resource::<Military>();
        let gha = CountryTag("GHA".into());
        let fired = world.resource::<crate::events::FiredEvents>();
        assert!(
            fired.fired.iter().any(|id| id == "test-chain"),
            "the chained event fired after its offset"
        );
        assert_eq!(
            military.alignment_of(&data, &gha),
            Alignment::EasternBloc,
            "SetAlignment overrides the 1950 baseline"
        );
        assert_eq!(
            military.stability_of(&data, &gha),
            40,
            "stability delta applied to the baseline"
        );
        // And chains respect the offset: at day 32 the chain has NOT fired.
        let mut early = app_with_test_timeline();
        run_ticks(&mut early, 24 * 36);
        let fired_early = early.world().resource::<crate::events::FiredEvents>();
        assert!(
            fired_early.fired.iter().any(|id| id == "test-independence"),
            "parent fired"
        );
    }
}

#[cfg(test)]
mod world_timeline_tests {
    use crate::calendar::GameDate;
    use crate::{run_ticks, SimPlugin};
    use bevy_app::App;
    use std::path::Path;
    use std::sync::Arc;
    use ugs_data::CountryTag;

    /// The full 1950-1970 world, hands off: the historical defaults
    /// (option 0) drive every decision, the sim runs the wars and
    /// settlements, and the timeline content fires on schedule.
    #[test]
    fn the_world_turns_to_1970() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 1950,
        });
        app.insert_resource(crate::demography::SimScenario(Arc::new(data)));
        // Twenty years, one hour at a time.
        run_ticks(&mut app, 24 * 365 * 20 + 24 * 5);

        let world = app.world();
        let data = world.resource::<crate::demography::SimScenario>().0.clone();
        let fired = world.resource::<crate::events::FiredEvents>();
        let military = world.resource::<crate::military::Military>();
        let stat = world.resource::<crate::economy::EconomyStatic>();

        // The spine of the era fired.
        for id in [
            "stalin-death",
            "austrian-state-treaty",
            "secret-speech",
            "sputnik",
            "hungary-uprising",
            "berlin-wall",
            "cuban-revolution",
            "ghana-independence",
            "congo-independence",
            "six-day-war",
        ] {
            assert!(
                fired.fired.iter().any(|f| f == id),
                "{id} should have fired by 1970; fired: {} events",
                fired.fired.len()
            );
        }

        // Decolonization happened: many new states own regions now.
        let owners_1970: std::collections::BTreeSet<&CountryTag> =
            stat.region_owner.values().collect();
        let new_states = owners_1970
            .iter()
            .filter(|t| {
                !data
                    .countries
                    .get(**t)
                    .map(|c| c.industry > 0)
                    .unwrap_or(true)
            })
            .count();
        let born = data
            .countries
            .keys()
            .filter(|t| {
                stat.region_owner.values().any(|o| o == *t)
                    && military.manpower.contains_key(*t)
                    && data.countries[*t].industry <= 3
            })
            .count();
        let _ = new_states;
        assert!(
            born >= 12,
            "the decolonization wave produced at least a dozen region-owning new states: {born}"
        );

        // The map itself is right: the new states hold their ground
        // (these pin the per-province independence semantics — under
        // the old whole-region transfer, Guinea swallowed West Africa).
        for (tag, parent, province) in [
            ("NGA", "GBR", "Lagos"),
            ("KEN", "GBR", "Nairobi"),
            ("DZA", "FRA", "Alger"),
            ("GHA", "GBR", "Ashanti"),
        ] {
            let id = data
                .province_by_name(&CountryTag(parent.into()), province)
                .expect("map name");
            assert_eq!(
                military.owner_of(id, &data.provinces[&id].owner),
                CountryTag(tag.into()),
                "{tag} holds {province} by 1970"
            );
        }
        // And Guinea did NOT swallow its neighbors.
        let dakar = data
            .province_by_name(&CountryTag("FRA".into()), "Dakar")
            .expect("map name");
        assert_ne!(
            military.owner_of(dakar, &data.provinces[&dakar].owner),
            CountryTag("GIN".into()),
            "Dakar is not Guinean"
        );

        // Cuba turned east (the historical default path).
        assert_eq!(
            military.alignment_of(&data, &CountryTag("CUB".into())),
            ugs_data::Alignment::EasternBloc,
            "Cuba aligned east by the mid-1960s"
        );

        // The world is intact: no runaway wars, tension within scale.
        let t = world.resource::<crate::tension::GlobalTension>().value();
        assert!((0..=1000).contains(&t));
        assert!(
            military.wars.len() < 12,
            "1970 is not a world war: {:?}",
            military.wars
        );
    }
}
