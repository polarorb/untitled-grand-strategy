//! The command queue: the ONLY doorway through which the outside world
//! (player input, AI decisions, future network peers) mutates sim state.
//!
//! Commands are queued between ticks and applied in insertion order at the
//! start of the next tick (`TickSet::Commands`). Because the applied
//! command sequence plus the seed fully determines a campaign, this is also
//! the future save/replay/multiplayer format — every command must stay
//! serializable and self-contained.

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::CountryTag;

use crate::agriculture::{self, Agriculture, Quota};
use crate::demography::SimScenario;
use crate::events::{self, FiredEvents};
use crate::military::{
    self, Archetype, FormationId, Military, PlayerCountry, Posture, Readiness, TheaterId,
    TheaterPosture,
};
use crate::planning::{self, Economies, Procurement};
use crate::savegame::CommandLog;
use crate::tension::GlobalTension;
use crate::SimClock;
use ugs_data::ProvinceId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimCommand {
    /// Debug/cheat: adjust global tension by internal tenths.
    DebugAdjustTension(i32),
    /// Planned economies: set output quotas (permille, must sum to 1000).
    SetPlannedAllocation {
        country: CountryTag,
        consumer: u16,
        investment: u16,
        military: u16,
    },
    /// Market economies: set the policy levers.
    SetMarketPolicy {
        country: CountryTag,
        interest_bp: u16,
        tax_permille: u16,
        procurement: Procurement,
    },
    /// Planned economies: agricultural organization and procurement.
    SetAgriPolicy {
        country: CountryTag,
        collectivized: bool,
        quota: Quota,
    },
    /// Set a country's military posture toward an enemy it is at war with.
    /// (Drives the AI/default auto-theater; player theaters override it.)
    SetPosture {
        country: CountryTag,
        enemy: CountryTag,
        posture: Posture,
    },
    /// Raise green divisions from the military stockpile
    /// (military-command.md). Home must be own or co-belligerent soil.
    RaiseFormation {
        country: CountryTag,
        archetype: Archetype,
        home: ProvinceId,
        count: u8,
    },
    /// Disband a division, returning most of its men to the pool.
    DisbandFormation {
        country: CountryTag,
        id: FormationId,
    },
    /// Active <-> Reserve. Activation takes MOBILIZE_DAYS and is a
    /// public signal (tension at peace).
    SetReadiness {
        country: CountryTag,
        id: FormationId,
        active: bool,
    },
    /// Create an empty player theater.
    CreateTheater { country: CountryTag, name: String },
    /// Paint one province into (or out of) a theater. Exclusive within
    /// a country: adding removes it from that country's other theaters.
    PaintTheater {
        country: CountryTag,
        id: TheaterId,
        province: ProvinceId,
        add: bool,
    },
    /// Delete a theater; its formations go unassigned (walk home).
    DeleteTheater { country: CountryTag, id: TheaterId },
    /// Assign a formation to a theater (None = unassign).
    AssignTheater {
        country: CountryTag,
        formation: FormationId,
        theater: Option<TheaterId>,
    },
    SetTheaterPosture {
        country: CountryTag,
        id: TheaterId,
        posture: TheaterPosture,
    },
    /// Advance axes, <= MAX_OBJECTIVES, none on forbidden soil.
    SetTheaterObjectives {
        country: CountryTag,
        id: TheaterId,
        objectives: Vec<ProvinceId>,
    },
    /// Share of committed divisions held at the theater rear.
    SetTheaterEchelon {
        country: CountryTag,
        id: TheaterId,
        permille: u16,
    },
    /// ROE: toggle a country whose soil this theater may never enter.
    SetTheaterRoe {
        country: CountryTag,
        id: TheaterId,
        tag: CountryTag,
        forbidden: bool,
    },
    /// Resolve a pending choice event with the given option index.
    ResolveEvent { id: String, option: u8 },
    /// Identify the human player's country (part of the replay log so
    /// armistice AI knows who NOT to auto-decide for).
    SetPlayerCountry { country: Option<CountryTag> },
    /// Offer (or retract) an armistice to an enemy.
    SetArmisticeOffer {
        country: CountryTag,
        enemy: CountryTag,
        offer: bool,
    },
    /// Found a national nuclear weapons program.
    FoundNuclearProgram { country: CountryTag, route: String },
    /// Set a nuclear program's secrecy/speed posture.
    SetProgramPosture {
        country: CountryTag,
        posture: String,
    },
    /// Queue construction of a fissile-production facility.
    ExpandNuclearFacility { country: CountryTag, kind: String },
    /// Parade deception: inflate rival estimates of this arsenal.
    /// Works — and provokes (the bomber gap drove real procurement).
    SetParadeDeception { country: CountryTag, on: bool },
    /// Strategic-forces alert level 0-3.
    SetAlertLevel { country: CountryTag, level: u8 },
    /// Fund a collection network against a target (owner = player).
    SetNetworkFunding { target: CountryTag, level: u8 },
    /// Set the player's counterintelligence funding level 0-3.
    SetCounterintel { level: u8 },
    /// Launch a covert operation against a target (owner = player).
    LaunchOperation {
        target: CountryTag,
        kind: crate::intel::OpKind,
    },
}

/// Commands queued for the next tick. The presentation layer pushes;
/// `apply_commands` drains. Never mutated mid-tick from outside the sim.
#[derive(Resource, Debug, Default)]
pub struct PendingCommands {
    queue: Vec<SimCommand>,
}

impl PendingCommands {
    pub fn push(&mut self, command: SimCommand) {
        self.queue.push(command);
    }
}

#[allow(clippy::too_many_arguments)] // the command hub touches every domain
pub fn apply_commands(
    clock: Res<SimClock>,
    mut pending: ResMut<PendingCommands>,
    mut log: ResMut<CommandLog>,
    mut tension: ResMut<GlobalTension>,
    mut econ: ResMut<Economies>,
    mut agri: ResMut<Agriculture>,
    mut military: ResMut<Military>,
    mut fired: ResMut<FiredEvents>,
    mut player: ResMut<PlayerCountry>,
    mut nuclear: ResMut<crate::nuclear::NuclearPrograms>,
    mut intel: ResMut<crate::intel::Intel>,
    deterrence: Res<crate::deterrence::Deterrence>,
    scenario: Option<Res<SimScenario>>,
) {
    for command in pending.queue.drain(..) {
        log.0.push((clock.tick, command.clone()));
        match command {
            SimCommand::DebugAdjustTension(delta) => tension.apply(delta),
            SimCommand::SetPlannedAllocation {
                country,
                consumer,
                investment,
                military,
            } => planning::set_planned_allocation(
                &mut econ, &country, consumer, investment, military,
            ),
            SimCommand::SetMarketPolicy {
                country,
                interest_bp,
                tax_permille,
                procurement,
            } => planning::set_market_policy(
                &mut econ,
                &country,
                interest_bp,
                tax_permille,
                procurement,
            ),
            SimCommand::SetAgriPolicy {
                country,
                collectivized,
                quota,
            } => agriculture::set_agri_policy(&mut agri, &econ, &country, collectivized, quota),
            SimCommand::SetPosture {
                country,
                enemy,
                posture,
            } => {
                if military.at_war(&country, &enemy) {
                    military.postures.insert((country, enemy), posture);
                }
            }
            SimCommand::SetPlayerCountry { country } => {
                player.0 = country;
            }
            SimCommand::RaiseFormation {
                country,
                archetype,
                home,
                count,
            } => {
                if let Some(scenario) = &scenario {
                    let data = &scenario.0;
                    if military.may_operate(data, &country, home) {
                        for _ in 0..count.min(5) {
                            if !military::raise_division(
                                data,
                                &mut military,
                                &mut econ,
                                &mut tension,
                                clock.tick,
                                country.clone(),
                                archetype,
                                home,
                            ) {
                                break;
                            }
                        }
                    }
                }
            }
            SimCommand::DisbandFormation { country, id } => {
                if military
                    .formations
                    .get(&id)
                    .is_some_and(|f| f.owner == country)
                {
                    military.disband(id);
                }
            }
            SimCommand::SetReadiness {
                country,
                id,
                active,
            } => {
                let at_war = military
                    .wars
                    .iter()
                    .any(|(a, b)| a == &country || b == &country);
                let Some(f) = military.formations.get_mut(&id) else {
                    continue;
                };
                if f.owner != country {
                    continue;
                }
                match (f.readiness, active) {
                    (Readiness::Reserve, true) => {
                        f.readiness = Readiness::Mobilizing {
                            days_left: military::tuning::MOBILIZE_DAYS,
                        };
                        let name = f.name.clone();
                        if !at_war {
                            tension.apply(military::tuning::MOBILIZATION_TENSION);
                        }
                        military.log(clock.tick, format!("{name} MOBILIZING"));
                    }
                    (Readiness::Active | Readiness::Mobilizing { .. }, false) => {
                        f.readiness = Readiness::Reserve;
                        f.cohesion = f.cohesion.min(military::tuning::STAND_DOWN_COHESION);
                        f.slot = None;
                    }
                    _ => {}
                }
            }
            SimCommand::CreateTheater { country, name } => {
                let mine = military
                    .theaters
                    .values()
                    .filter(|t| t.owner == country)
                    .count();
                if mine < military::tuning::MAX_THEATERS {
                    military.create_theater(country, name, false);
                }
            }
            SimCommand::PaintTheater {
                country,
                id,
                province,
                add,
            } => {
                let Some(scenario) = &scenario else { continue };
                let owned = military
                    .theaters
                    .get(&id)
                    .is_some_and(|t| t.owner == country);
                if !owned {
                    continue;
                }
                if add {
                    if !military.may_operate(&scenario.0, &country, province) {
                        continue;
                    }
                    // Exclusive within the country.
                    for (tid, t) in military.theaters.iter_mut() {
                        if t.owner == country && *tid != id {
                            t.provinces.remove(&province);
                        }
                    }
                    let t = military.theaters.get_mut(&id).unwrap();
                    t.provinces.insert(province);
                    t.auto = false;
                } else {
                    let t = military.theaters.get_mut(&id).unwrap();
                    t.provinces.remove(&province);
                    t.auto = false;
                }
            }
            SimCommand::DeleteTheater { country, id } => {
                if military
                    .theaters
                    .get(&id)
                    .is_some_and(|t| t.owner == country)
                {
                    military.theaters.remove(&id);
                    for f in military.formations.values_mut() {
                        if f.theater == Some(id) {
                            f.theater = None;
                            f.slot = None;
                        }
                    }
                }
            }
            SimCommand::AssignTheater {
                country,
                formation,
                theater,
            } => {
                let target_ok = match theater {
                    None => true,
                    Some(t) => military
                        .theaters
                        .get(&t)
                        .is_some_and(|th| th.owner == country),
                };
                if !target_ok {
                    continue;
                }
                if let Some(f) = military.formations.get_mut(&formation) {
                    if f.owner == country {
                        f.theater = theater;
                        f.slot = None;
                    }
                }
            }
            SimCommand::SetTheaterPosture {
                country,
                id,
                posture,
            } => {
                if let Some(t) = military.theaters.get_mut(&id) {
                    if t.owner == country {
                        t.posture = posture;
                        t.auto = false;
                    }
                }
            }
            SimCommand::SetTheaterObjectives {
                country,
                id,
                objectives,
            } => {
                let Some(scenario) = &scenario else { continue };
                let data = &scenario.0;
                if let Some(t) = military.theaters.get_mut(&id) {
                    if t.owner == country {
                        t.objectives = objectives
                            .into_iter()
                            .filter(|o| {
                                data.provinces
                                    .get(o)
                                    .is_some_and(|p| !t.forbidden.contains(&p.owner))
                            })
                            .take(military::tuning::MAX_OBJECTIVES)
                            .collect();
                        t.auto = false;
                    }
                }
            }
            SimCommand::SetTheaterEchelon {
                country,
                id,
                permille,
            } => {
                if let Some(t) = military.theaters.get_mut(&id) {
                    if t.owner == country {
                        t.echelon_permille = permille.min(1000);
                        t.auto = false;
                    }
                }
            }
            SimCommand::SetTheaterRoe {
                country,
                id,
                tag,
                forbidden,
            } => {
                if let Some(t) = military.theaters.get_mut(&id) {
                    if t.owner == country && tag != country {
                        if forbidden {
                            t.forbidden.insert(tag);
                        } else {
                            t.forbidden.remove(&tag);
                        }
                        t.auto = false;
                    }
                }
            }
            SimCommand::SetArmisticeOffer {
                country,
                enemy,
                offer,
            } => {
                if military.at_war(&country, &enemy) {
                    military
                        .armistice_offers
                        .retain(|(c, e)| !(c == &country && e == &enemy));
                    if offer {
                        military.armistice_offers.push((country, enemy));
                    }
                }
            }
            SimCommand::ResolveEvent { id, option } => {
                if let Some(scenario) = &scenario {
                    events::resolve_event(
                        &mut fired,
                        &mut tension,
                        &mut military,
                        &mut nuclear,
                        &deterrence,
                        &mut econ,
                        &scenario.0,
                        clock.date.year as i64 * 12 + clock.date.month as i64,
                        &id,
                        option,
                    );
                }
            }
            SimCommand::FoundNuclearProgram { country, route } => {
                nuclear.found(country, crate::nuclear::Route::parse(&route));
            }
            SimCommand::SetProgramPosture { country, posture } => {
                crate::nuclear::set_posture(&mut nuclear, &country, &posture);
            }
            SimCommand::ExpandNuclearFacility { country, kind } => {
                crate::nuclear::expand_facility(&mut nuclear, &country, &kind);
            }
            SimCommand::SetParadeDeception { country, on } => {
                if let Some(p) = nuclear.programs.get_mut(&country) {
                    if p.deception != on {
                        p.deception = on;
                        if on {
                            tension.apply(10);
                        }
                    }
                }
            }
            SimCommand::SetNetworkFunding { target, level } => {
                if let Some(owner) = player.0.clone() {
                    crate::intel::set_network_funding(&mut intel, owner, target, level);
                }
            }
            SimCommand::SetCounterintel { level } => {
                if let Some(country) = player.0.clone() {
                    crate::intel::set_counterintel(&mut intel, country, level);
                }
            }
            SimCommand::LaunchOperation { target, kind } => {
                if let Some(owner) = player.0.clone() {
                    crate::intel::queue_operation(&mut intel, owner, target, kind);
                }
            }
            SimCommand::SetAlertLevel { country, level } => {
                if let Some(p) = nuclear.programs.get_mut(&country) {
                    let level = level.min(3);
                    if level > p.alert {
                        tension.apply(
                            crate::nuclear::tuning::ALERT_RAISE_TENSION * (level - p.alert) as i32,
                        );
                    }
                    p.alert = level;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{calendar::GameDate, run_ticks, tension::tuning, SimPlugin};
    use bevy_app::App;

    #[test]
    fn commands_apply_on_next_tick_in_order() {
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 1,
        });
        {
            let mut pending = app.world_mut().resource_mut::<PendingCommands>();
            pending.push(SimCommand::DebugAdjustTension(700));
            pending.push(SimCommand::DebugAdjustTension(100)); // clamps at MAX
            pending.push(SimCommand::DebugAdjustTension(-50));
        }
        run_ticks(&mut app, 1);
        let tension = app.world().resource::<GlobalTension>();
        // 300 +700 -> 1000 (clamp), +100 -> 1000, -50 -> 950. Order matters:
        // applying -50 before +100 would end at 1000.
        assert_eq!(tension.value(), tuning::MAX - 50);
        assert!(app.world().resource::<PendingCommands>().queue.is_empty());
    }
}
