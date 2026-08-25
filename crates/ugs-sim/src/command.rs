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
use crate::planning::{self, Economies, Procurement};
use crate::military::{Military, Posture};
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

pub fn apply_commands(
    clock: Res<SimClock>,
    mut pending: ResMut<PendingCommands>,
    mut log: ResMut<CommandLog>,
    mut tension: ResMut<GlobalTension>,
    mut econ: ResMut<Economies>,
    mut agri: ResMut<Agriculture>,
    mut military: ResMut<Military>,
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
                &mut econ, &country, interest_bp, tax_permille, procurement,
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
