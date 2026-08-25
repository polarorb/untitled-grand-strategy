//! Presentation layer: window, rendering, input, UI. This binary owns real
//! time; the sim only ever sees "advance N ticks". Nothing in ugs-sim may
//! depend on anything in this crate.

use bevy::prelude::*;
use ugs_sim::{calendar::GameDate, run_ticks, SimClock, SimPlugin};

/// Game speed control. `ticks_per_second` is in-game hours per real second.
#[derive(Resource, Debug)]
struct GameSpeed {
    paused: bool,
    level: u8,
    accumulator: f32,
}

impl GameSpeed {
    fn ticks_per_second(&self) -> f32 {
        match self.level {
            1 => 1.0,
            2 => 4.0,
            3 => 12.0,
            4 => 48.0,
            _ => 168.0, // speed 5: a week per second
        }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Untitled Grand Strategy — 1950".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 1950,
        })
        .insert_resource(GameSpeed {
            paused: true,
            level: 1,
            accumulator: 0.0,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_speed_input, drive_sim, update_clock_text).chain())
        .run();
}

#[derive(Component)]
struct ClockText;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((
        ClockText,
        Text::new(""),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(12.0),
            ..default()
        },
    ));
}

fn handle_speed_input(keys: Res<ButtonInput<KeyCode>>, mut speed: ResMut<GameSpeed>) {
    if keys.just_pressed(KeyCode::Space) {
        speed.paused = !speed.paused;
    }
    for (key, level) in [
        (KeyCode::Digit1, 1),
        (KeyCode::Digit2, 2),
        (KeyCode::Digit3, 3),
        (KeyCode::Digit4, 4),
        (KeyCode::Digit5, 5),
    ] {
        if keys.just_pressed(key) {
            speed.level = level;
        }
    }
}

/// Accumulate real time and convert it into whole sim ticks. Capped per
/// frame so a long hitch can't trigger a runaway catch-up spiral.
fn drive_sim(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs();
    let ticks = {
        let mut speed = world.resource_mut::<GameSpeed>();
        if speed.paused {
            speed.accumulator = 0.0;
            0
        } else {
            speed.accumulator += delta * speed.ticks_per_second();
            let whole = speed.accumulator.floor().min(500.0);
            speed.accumulator -= whole;
            whole as u64
        }
    };
    if ticks > 0 {
        for _ in 0..ticks {
            world.run_schedule(ugs_sim::SimTick);
        }
    }
    let _ = run_ticks; // shared entry point used by tests/harnesses
}

fn update_clock_text(
    clock: Res<SimClock>,
    speed: Res<GameSpeed>,
    mut query: Query<&mut Text, With<ClockText>>,
) {
    for mut text in &mut query {
        let state = if speed.paused {
            "PAUSED".to_string()
        } else {
            format!("speed {}", speed.level)
        };
        text.0 = format!("{}  [{}]", clock.date, state);
    }
}
