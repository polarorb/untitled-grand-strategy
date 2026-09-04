//! Save/load as command-log replay.
//!
//! A campaign is fully determined by (start date, seed, scenario data,
//! command log) — the property the determinism suite guards. A save is
//! therefore tiny: the log plus the target tick. Loading resets the sim
//! to its initial state and replays every tick, re-applying each command
//! on the tick it originally executed. This same machinery is a replay
//! system and the future multiplayer-lockstep join path.

use bevy_ecs::prelude::*;
use bevy_ecs::world::World;
use serde::{Deserialize, Serialize};

use crate::calendar::GameDate;
use crate::command::{PendingCommands, SimCommand};
use crate::rng::SimRng;
use crate::tension::GlobalTension;
use crate::SimClock;

/// The campaign seed, kept for saving.
#[derive(Resource, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SimSeed(pub u64);

/// Every command ever applied, with the tick it executed on.
#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct CommandLog(pub Vec<(u64, SimCommand)>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveGame {
    pub seed: u64,
    pub start_date: GameDate,
    pub current_tick: u64,
    /// Player nation tag, if any (presentation-layer concern, carried
    /// through the save for convenience).
    pub player: Option<String>,
    pub log: Vec<(u64, SimCommand)>,
}

impl SaveGame {
    pub fn capture(world: &World, player: Option<String>) -> Self {
        let clock = world.resource::<SimClock>();
        Self {
            seed: world.resource::<SimSeed>().0,
            start_date: world.resource::<StartDate>().0,
            current_tick: clock.tick,
            player,
            log: world.resource::<CommandLog>().0.clone(),
        }
    }
}

/// The campaign start date, kept for saving/reset.
#[derive(Resource, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StartDate(pub GameDate);

/// Reset every sim state resource to campaign start. The scenario
/// (`SimScenario`) is left untouched — it is immutable input.
pub fn reset_sim(world: &mut World, start_date: GameDate, seed: u64) {
    world.insert_resource(SimClock::at_start(start_date));
    world.insert_resource(SimRng::seeded(seed));
    world.insert_resource(SimSeed(seed));
    world.insert_resource(StartDate(start_date));
    world.insert_resource(GlobalTension::new(crate::tension::tuning::START_1950));
    world.insert_resource(PendingCommands::default());
    world.insert_resource(CommandLog::default());
    world.insert_resource(crate::demography::Demographics::default());
    world.insert_resource(crate::demography::LivingStandards::default());
    world.insert_resource(crate::economy::EconomyStatic::default());
    world.insert_resource(crate::economy::NationalBalances::default());
    world.insert_resource(crate::economy::RegionalPower::default());
    world.insert_resource(crate::planning::Economies::default());
    world.insert_resource(crate::agriculture::Agriculture::default());
    world.insert_resource(crate::military::Military::default());
    world.insert_resource(crate::military::PlayerCountry::default());
    world.insert_resource(crate::events::FiredEvents::default());
    world.insert_resource(crate::nuclear::NuclearPrograms::default());
    world.insert_resource(crate::deterrence::Deterrence::default());
    world.insert_resource(crate::crisis::Crises::default());
    world.remove_resource::<crate::crisis::GameOver>();
    world.insert_resource(crate::intel::Intel::default());
    world.insert_resource(crate::influence::Influence::default());
    world.insert_resource(crate::score::Ledger::default());
    world.insert_resource(crate::settlement::Settlements::default());
    world.insert_resource(crate::construction::RegionalIndustry::default());
    world.insert_resource(crate::construction::Construction::default());
    world.insert_resource(crate::construction::RegionSnapshots::default());
}

/// Rebuild the world from a save: reset, then replay every tick with the
/// logged commands re-flushed at their original tick boundaries. A log
/// entry `(T, cmd)` means "applied after tick T completed" (commands may
/// be issued while paused), so each boundary flushes its commands before
/// the next tick runs — including the final boundary, for commands
/// issued while paused right before the save.
pub fn load_save(world: &mut World, save: &SaveGame) {
    reset_sim(world, save.start_date, save.seed);
    let mut next_cmd = 0usize;
    loop {
        let tick = world.resource::<SimClock>().tick;
        while next_cmd < save.log.len() && save.log[next_cmd].0 == tick {
            world
                .resource_mut::<PendingCommands>()
                .push(save.log[next_cmd].1.clone());
            next_cmd += 1;
        }
        crate::flush_commands(world);
        if tick >= save.current_tick {
            break;
        }
        world.run_schedule(crate::SimTick);
    }
    // The replay regenerates every teletype notice of the intervening
    // years; a loaded game should not re-read them. Notices are derived
    // narrative (not digested), so clearing them changes no state that
    // matters. Live decisions stay.
    world
        .resource_mut::<crate::events::FiredEvents>()
        .notices
        .clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agriculture::Quota;
    use crate::demography::{Demographics, SimScenario};
    use crate::planning::Economies;
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
            seed: 99,
        });
        app.insert_resource(SimScenario(Arc::new(data)));
        app
    }

    fn digest(app: &App) -> String {
        let world = app.world();
        format!(
            "{:?}|{:x}|{:x}|{:x}|{:x}|{:x}|{:x}|{:x}|{:x}|{:x}|{:x}|{:x}|{:x}",
            world.resource::<SimClock>(),
            world.resource::<Demographics>().digest(),
            world.resource::<Economies>().digest(),
            world.resource::<crate::agriculture::Agriculture>().digest(),
            world.resource::<crate::military::Military>().digest(),
            world.resource::<crate::settlement::Settlements>().digest(),
            world
                .resource::<crate::construction::RegionalIndustry>()
                .digest(),
            world
                .resource::<crate::construction::Construction>()
                .digest(),
            world.resource::<crate::economy::EconomyStatic>().digest(),
            world.resource::<crate::events::FiredEvents>().digest(),
            world.resource::<crate::intel::Intel>().digest(),
            world.resource::<crate::influence::Influence>().digest(),
            world.resource::<crate::score::Ledger>().digest(),
        )
    }

    #[test]
    fn save_and_replay_reproduces_state_exactly() {
        let mut original = app_with_scenario();
        run_ticks(&mut original, 24 * 45);
        original.world_mut().resource_mut::<PendingCommands>().push(
            SimCommand::SetPlannedAllocation {
                country: CountryTag("SOV".into()),
                consumer: 250,
                investment: 550,
                military: 200,
            },
        );
        run_ticks(&mut original, 24 * 100);
        original
            .world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetAgriPolicy {
                country: CountryTag("SOV".into()),
                collectivized: true,
                quota: Quota::High,
            });
        run_ticks(&mut original, 24 * 400);

        let save = SaveGame::capture(original.world(), Some("SOV".into()));
        assert_eq!(save.log.len(), 2, "both commands logged");
        let expected = digest(&original);

        let mut restored = app_with_scenario();
        load_save(restored.world_mut(), &save);
        assert_eq!(digest(&restored), expected, "replay diverged from original");

        // And the two worlds keep agreeing when run further.
        run_ticks(&mut original, 24 * 60);
        run_ticks(&mut restored, 24 * 60);
        assert_eq!(digest(&restored), expected_after(&original));
        fn expected_after(app: &App) -> String {
            digest(app)
        }
    }

    #[test]
    fn paused_commands_apply_immediately_and_replay() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 10);
        // "Paused": no tick runs — the boundary flush alone must apply
        // the command (this is what makes theater painting instant).
        let before = app.world().resource::<GlobalTension>().value();
        let tick_before = app.world().resource::<SimClock>().tick;
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::DebugAdjustTension(100));
        crate::flush_commands(app.world_mut());
        assert_eq!(
            app.world().resource::<SimClock>().tick,
            tick_before,
            "no time passed"
        );
        assert_eq!(
            app.world().resource::<GlobalTension>().value(),
            before + 100,
            "command applied without a tick"
        );
        // And the flush boundary replays exactly, including a command
        // issued while paused right before the save.
        run_ticks(&mut app, 24 * 5);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::DebugAdjustTension(-40));
        crate::flush_commands(app.world_mut());
        let save = SaveGame::capture(app.world(), None);
        let expected = digest(&app);
        let mut restored = app_with_scenario();
        load_save(restored.world_mut(), &save);
        assert_eq!(digest(&restored), expected, "paused-flush replay diverged");
        assert_eq!(
            restored.world().resource::<GlobalTension>().value(),
            app.world().resource::<GlobalTension>().value(),
        );
    }

    #[test]
    fn save_roundtrips_through_ron() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 40);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::DebugAdjustTension(100));
        run_ticks(&mut app, 24 * 10);
        let save = SaveGame::capture(app.world(), None);
        let text = ron::to_string(&save).unwrap();
        let parsed: SaveGame = ron::from_str(&text).unwrap();
        assert_eq!(parsed.current_tick, save.current_tick);
        assert_eq!(parsed.log, save.log);
    }
}
