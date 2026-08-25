//! Presentation layer: window, rendering, input, UI. This binary owns real
//! time; the sim only ever sees "advance N ticks". Nothing in ugs-sim may
//! depend on anything in this crate. All sim mutations go through
//! `PendingCommands` — never write sim resources directly from here.

use std::path::Path;

use bevy::prelude::*;
use ugs_data::{Alignment, CountryTag, ProvinceId, ScenarioData};
use ugs_sim::{
    calendar::GameDate,
    command::{PendingCommands, SimCommand},
    tension::GlobalTension,
    SimClock, SimPlugin,
};

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

/// Loaded scenario, kept for UI lookups (names, owners).
#[derive(Resource)]
struct World1950(ScenarioData);

#[derive(Resource, Default)]
struct Selected(Option<ProvinceId>);

#[derive(Component)]
struct ProvinceMarker {
    id: ProvinceId,
}

#[derive(Component)]
struct SelectionRing;

#[derive(Component)]
struct TopBarText;

#[derive(Component)]
struct SelectionText;

/// Placeholder projection: degrees -> world units, centered on Korea.
/// Replaced by a real projection when the mapgen tool lands.
fn project(center: (f32, f32)) -> Vec2 {
    Vec2::new((center.0 - 127.3) * 60.0, (center.1 - 37.5) * 60.0)
}

const PROVINCE_RADIUS: f32 = 26.0;

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
        .init_resource::<Selected>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                handle_input,
                drive_sim,
                select_province,
                update_selection_ring,
                update_ui_text,
            )
                .chain(),
        )
        .run();
}

fn country_color(data: &ScenarioData, tag: &CountryTag) -> Color {
    match tag.0.as_str() {
        "USA" => Color::srgb(0.20, 0.35, 0.70),
        "KOR" => Color::srgb(0.35, 0.55, 0.85),
        "SOV" => Color::srgb(0.75, 0.15, 0.15),
        "PRC" => Color::srgb(0.85, 0.30, 0.15),
        "PRK" => Color::srgb(0.60, 0.10, 0.20),
        _ => match data.countries.get(tag).map(|c| c.alignment) {
            Some(Alignment::WesternBloc) => Color::srgb(0.3, 0.45, 0.75),
            Some(Alignment::EasternBloc) => Color::srgb(0.7, 0.2, 0.2),
            _ => Color::srgb(0.5, 0.5, 0.45),
        },
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2d);

    let data = ScenarioData::load(Path::new("assets/data/scenario/1950"))
        .expect("failed to load 1950 scenario (run from the workspace root)");

    let hex = meshes.add(RegularPolygon::new(PROVINCE_RADIUS, 6));
    let line = meshes.add(Rectangle::new(1.0, 1.0));
    let link_color = materials.add(Color::srgba(1.0, 1.0, 1.0, 0.15));

    // Adjacency links (draw each pair once), under the hexes.
    for (id, p) in &data.provinces {
        let a = project(p.center);
        for adj in &p.adjacent {
            if adj > id {
                let b = project(data.provinces[adj].center);
                let mid = (a + b) / 2.0;
                let delta = b - a;
                commands.spawn((
                    Mesh2d(line.clone()),
                    MeshMaterial2d(link_color.clone()),
                    Transform::from_translation(mid.extend(-1.0))
                        .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x)))
                        .with_scale(Vec3::new(delta.length(), 3.0, 1.0)),
                ));
            }
        }
    }

    for (id, p) in &data.provinces {
        let pos = project(p.center);
        commands.spawn((
            ProvinceMarker { id: *id },
            Mesh2d(hex.clone()),
            MeshMaterial2d(materials.add(country_color(&data, &p.owner))),
            Transform::from_translation(pos.extend(0.0)),
        ));
        commands.spawn((
            Text2d::new(p.name.clone()),
            TextFont {
                font_size: bevy::text::FontSize::Px(12.0),
                ..default()
            },
            Transform::from_translation(pos.extend(1.0) + Vec3::Y * (PROVINCE_RADIUS + 10.0)),
        ));
    }

    commands.spawn((
        SelectionRing,
        Mesh2d(meshes.add(RegularPolygon::new(PROVINCE_RADIUS + 5.0, 6))),
        MeshMaterial2d(materials.add(Color::srgb(0.95, 0.9, 0.5))),
        Transform::from_xyz(0.0, 0.0, -0.5),
        Visibility::Hidden,
    ));

    commands.insert_resource(World1950(data));

    commands.spawn((
        TopBarText,
        Text::new(""),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
    commands.spawn((
        SelectionText,
        Text::new("Click a province. Space: pause · 1-5: speed · T/G: tension +/- (debug)"),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(8.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut speed: ResMut<GameSpeed>,
    mut pending: ResMut<PendingCommands>,
) {
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
    // Debug tension controls, routed through the command queue like any
    // real player action will be.
    if keys.just_pressed(KeyCode::KeyT) {
        pending.push(SimCommand::DebugAdjustTension(50));
    }
    if keys.just_pressed(KeyCode::KeyG) {
        pending.push(SimCommand::DebugAdjustTension(-50));
    }
}

/// Accumulate real time and convert it into whole sim ticks. Capped per
/// frame so a long hitch can't trigger a runaway catch-up spiral.
fn drive_sim(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs();
    let paused = {
        let mut speed = world.resource_mut::<GameSpeed>();
        if speed.paused {
            speed.accumulator = 0.0;
        }
        speed.paused
    };
    let ticks = if paused {
        // Commands queued while paused (debug keys, future orders) still
        // apply immediately so the UI reflects them: run the command stage
        // by advancing zero ticks is not possible, so apply on next unpause.
        0
    } else {
        let mut speed = world.resource_mut::<GameSpeed>();
        speed.accumulator += delta * speed.ticks_per_second();
        let whole = speed.accumulator.floor().min(500.0);
        speed.accumulator -= whole;
        whole as u64
    };
    for _ in 0..ticks {
        world.run_schedule(ugs_sim::SimTick);
    }
}

fn select_province(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    provinces: Query<(&ProvinceMarker, &Transform)>,
    mut selected: ResMut<Selected>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_transform)) = camera.single() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor) else {
        return;
    };
    selected.0 = provinces
        .iter()
        .map(|(marker, tf)| (marker.id, tf.translation.truncate().distance(world_pos)))
        .filter(|(_, dist)| *dist <= PROVINCE_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id);
}

fn update_selection_ring(
    selected: Res<Selected>,
    provinces: Query<(&ProvinceMarker, &Transform), Without<SelectionRing>>,
    mut ring: Query<(&mut Transform, &mut Visibility), With<SelectionRing>>,
) {
    let Ok((mut tf, mut vis)) = ring.single_mut() else {
        return;
    };
    match selected.0 {
        Some(id) => {
            if let Some((_, ptf)) = provinces.iter().find(|(m, _)| m.id == id) {
                tf.translation = ptf.translation.truncate().extend(-0.5);
                *vis = Visibility::Visible;
            }
        }
        None => *vis = Visibility::Hidden,
    }
}

fn update_ui_text(
    clock: Res<SimClock>,
    speed: Res<GameSpeed>,
    tension: Res<GlobalTension>,
    selected: Res<Selected>,
    world: Res<World1950>,
    mut top: Query<&mut Text, (With<TopBarText>, Without<SelectionText>)>,
    mut bottom: Query<&mut Text, (With<SelectionText>, Without<TopBarText>)>,
) {
    let state = if speed.paused {
        "PAUSED".to_string()
    } else {
        format!("speed {}", speed.level)
    };
    for mut text in &mut top {
        text.0 = format!(
            "{}  [{}]    Tension: {:.1} ({})",
            clock.date,
            state,
            tension.displayed(),
            tension.band(),
        );
    }
    for mut text in &mut bottom {
        if let Some(id) = selected.0 {
            if let Some(p) = world.0.provinces.get(&id) {
                let owner = world
                    .0
                    .countries
                    .get(&p.owner)
                    .map(|c| c.name.as_str())
                    .unwrap_or("?");
                text.0 = format!(
                    "{} — {} · {:?} · pop {}k",
                    p.name, owner, p.terrain, p.population_k
                );
            }
        }
    }
}
