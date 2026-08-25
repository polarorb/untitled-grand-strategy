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

use crate::tension::GlobalTension;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimCommand {
    /// Debug/cheat: adjust global tension by internal tenths.
    DebugAdjustTension(i32),
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
    mut pending: ResMut<PendingCommands>,
    mut tension: ResMut<GlobalTension>,
) {
    for command in pending.queue.drain(..) {
        match command {
            SimCommand::DebugAdjustTension(delta) => tension.apply(delta),
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
