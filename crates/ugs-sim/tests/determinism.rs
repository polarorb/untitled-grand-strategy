//! Divergence test: two sims with the same seed and command stream must
//! stay bit-identical over a long horizon; a different seed must diverge.
//!
//! Ticks are INTERLEAVED between the apps to catch cross-app static/global
//! state leaks that sequential runs would miss. Every sim state resource
//! must be included in `snapshot` — extend it when adding systems.

use bevy_app::App;
use std::sync::Arc;
use ugs_sim::calendar::GameDate;
use ugs_sim::demography::{Demographics, SimScenario};
use ugs_sim::command::{PendingCommands, SimCommand};
use ugs_sim::rng::SimRng;
use ugs_sim::tension::GlobalTension;
use ugs_sim::{run_ticks, SimClock, SimPlugin};

fn make_app(seed: u64) -> App {
    let mut app = App::new();
    app.add_plugins(SimPlugin {
        start_date: GameDate::new(1950, 1, 1, 0),
        seed,
    });
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/data/scenario/1950");
    let data = ugs_data::ScenarioData::load(&dir).expect("scenario");
    app.insert_resource(SimScenario(Arc::new(data)));
    app
}

/// One serialized frame of all sim state. Add every new state resource here.
fn snapshot(app: &App) -> String {
    let world = app.world();
    format!(
        "{:?}|{:?}|{:?}|demo:{:x}",
        world.resource::<SimClock>(),
        world.resource::<GlobalTension>(),
        // RNG state matters: identical outputs with diverged RNG would
        // desync later. Debug output includes the internal state.
        world.resource::<SimRng>(),
        world.resource::<Demographics>().digest(),
    )
}

/// Scripted command stream: (tick, command). Applied identically to both
/// apps, exercising the command path under divergence checking.
fn scripted_commands(tick: u64) -> Vec<SimCommand> {
    match tick {
        240 => vec![SimCommand::DebugAdjustTension(150)],
        1000 => vec![
            SimCommand::DebugAdjustTension(-40),
            SimCommand::DebugAdjustTension(300),
        ],
        1720 => vec![SimCommand::DebugAdjustTension(-500)],
        _ => vec![],
    }
}

#[test]
fn identical_seeds_and_commands_stay_bit_identical() {
    let mut a = make_app(1950);
    let mut b = make_app(1950);
    // 90 in-game days, interleaved, comparing at every day boundary.
    for tick in 0..(90 * 24) {
        for app in [&mut a, &mut b] {
            let mut pending = app.world_mut().resource_mut::<PendingCommands>();
            for cmd in scripted_commands(tick) {
                pending.push(cmd);
            }
            run_ticks(app, 1);
        }
        let day_boundary = a.world().resource::<SimClock>().new_day;
        if day_boundary {
            let (sa, sb) = (snapshot(&a), snapshot(&b));
            assert_eq!(
                sa, sb,
                "sims diverged at tick {tick} ({})",
                a.world().resource::<SimClock>().date
            );
        }
    }
    assert_eq!(snapshot(&a), snapshot(&b));
}

#[test]
fn different_seeds_do_diverge() {
    // Guards against snapshot() comparing constants: RNG state must differ
    // by seed even before any system consumes randomness.
    let a = make_app(1950);
    let b = make_app(1951);
    assert_ne!(snapshot(&a), snapshot(&b));
}
