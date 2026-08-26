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
use crate::military::{Military, PlayerCountry, Posture};
use crate::planning::{self, Economies, Procurement};
use crate::savegame::CommandLog;
use crate::tension::GlobalTension;
use crate::SimClock;

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
    SetPosture {
        country: CountryTag,
        enemy: CountryTag,
        posture: Posture,
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
