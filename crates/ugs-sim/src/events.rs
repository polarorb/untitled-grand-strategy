//! Scripted events: date/condition triggers, chance rolls, and choice
//! events with deadlines. World events apply instantly; country events
//! wait for a `ResolveEvent` command (from the player or an AI) until
//! their deadline, when the historical default (option 0) applies.

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
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
}

impl FiredEvents {
    pub fn is_pending(&self, id: &str) -> bool {
        self.pending.iter().any(|p| p.id == id)
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
    data: &ScenarioData,
) -> bool {
    match &event.trigger {
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
                    &mut notices,
                    clock.date.year as i64 * 12 + clock.date.month as i64,
                    clock.tick,
                );
                fired.notices.extend(notices);
            }
        }
    }

    // Fire new events. Chance-gated triggers roll once per day.
    for event in &data.events {
        if fired.fired.iter().any(|id| id == &event.id) {
            continue;
        }
        if !trigger_met(event, &clock, &fired, &military, data) {
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
            assert!(military.at_war(&CountryTag("PRC".into()), &CountryTag("USA".into())));
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
