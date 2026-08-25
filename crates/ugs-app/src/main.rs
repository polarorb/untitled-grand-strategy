//! Presentation layer: window, rendering, input, UI. This binary owns real
//! time; the sim only ever sees "advance N ticks". Nothing in ugs-sim may
//! depend on anything in this crate. All sim mutations go through
//! `PendingCommands` — never write sim resources directly from here.

use std::collections::BTreeMap;
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use ugs_data::{CountryTag, ProvinceId, ScenarioData, Terrain};
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

/// Projected province outlines + bounding boxes, for click hit-testing and
/// drawing the selection outline.
#[derive(Resource, Default)]
struct WorldGeometry {
    rings: BTreeMap<u32, Vec<Vec<Vec2>>>,
    bboxes: BTreeMap<u32, (Vec2, Vec2)>,
}

#[derive(Resource, Default)]
struct Selected(Option<ProvinceId>);

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
enum MapMode {
    #[default]
    Political,
    Terrain,
}

#[derive(Component)]
struct ProvinceMarker {
    id: ProvinceId,
}

#[derive(Component)]
struct TopBarText;

#[derive(Component)]
struct SelectionText;

/// Equirectangular degrees -> world units, centered on Korea for now.
fn project(lon: f32, lat: f32) -> Vec2 {
    Vec2::new((lon - 127.3) * 60.0, (lat - 37.5) * 60.0)
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
        .insert_resource(ClearColor(Color::srgb(0.09, 0.12, 0.16))) // ocean
        .init_resource::<Selected>()
        .init_resource::<MapMode>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                handle_input,
                camera_controls,
                drive_sim,
                apply_map_mode,
                select_province,
                draw_selection_outline,
                update_ui_text,
            )
                .chain(),
        )
        .run();
}

/// Per-province lightness wobble in lieu of border lines (cheap, replaced
/// by real borders later).
fn wobbled(rgb: (u8, u8, u8), province_id: u32) -> Color {
    let (r, g, b) = rgb;
    let wobble = (((province_id.wrapping_mul(2654435761)) >> 8) % 13) as f32 * 0.012 - 0.07;
    Color::srgb(
        (r as f32 / 255.0 * (1.0 + wobble)).clamp(0.0, 1.0),
        (g as f32 / 255.0 * (1.0 + wobble)).clamp(0.0, 1.0),
        (b as f32 / 255.0 * (1.0 + wobble)).clamp(0.0, 1.0),
    )
}

/// National color from data.
fn owner_color(data: &ScenarioData, tag: &CountryTag, province_id: u32) -> Color {
    let rgb = data
        .countries
        .get(tag)
        .map(|c| c.color)
        .unwrap_or((128, 128, 128));
    wobbled(rgb, province_id)
}

/// Atlas-style terrain palette.
fn terrain_color(terrain: Terrain, province_id: u32) -> Color {
    let rgb = match terrain {
        Terrain::Plains => (163, 168, 118),
        Terrain::Forest => (86, 118, 80),
        Terrain::Hills => (172, 144, 96),
        Terrain::Mountain => (128, 116, 106),
        Terrain::Desert => (216, 192, 134),
        Terrain::Jungle => (58, 100, 64),
        Terrain::Urban => (110, 106, 118),
        Terrain::Marsh => (108, 138, 120),
        Terrain::Tundra => (182, 192, 188),
    };
    wobbled(rgb, province_id)
}

fn build_province_mesh(rings: &[Vec<Vec2>]) -> Option<Mesh> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for ring in rings {
        if ring.len() < 3 {
            continue;
        }
        let flat: Vec<f64> = ring.iter().flat_map(|v| [v.x as f64, v.y as f64]).collect();
        let Ok(tris) = earcutr::earcut(&flat, &[], 2) else {
            continue;
        };
        let base = positions.len() as u32;
        positions.extend(ring.iter().map(|v| [v.x, v.y, 0.0]));
        indices.extend(tris.iter().map(|&i| base + i as u32));
    }
    if indices.is_empty() {
        return None;
    }
    Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_indices(Indices::U32(indices)),
    )
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2d);

    let data = ScenarioData::load(Path::new("assets/data/scenario/1950"))
        .expect("failed to load 1950 scenario (run from the workspace root)");
    let geo_text = std::fs::read_to_string("assets/map/world.geo.ron")
        .expect("missing assets/map/world.geo.ron (run: cargo run -p mapgen --release)");
    let raw_geo: BTreeMap<u32, Vec<Vec<(f32, f32)>>> =
        ron::from_str(&geo_text).expect("bad world.geo.ron");

    let mut geometry = WorldGeometry::default();
    // One material per country, shared by its provinces (per-province wobble
    // sacrificed for batching would be better; keep per-province for now).
    for (id, rings) in &raw_geo {
        let projected: Vec<Vec<Vec2>> = rings
            .iter()
            .map(|ring| ring.iter().map(|&(lon, lat)| project(lon, lat)).collect())
            .collect();
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for v in projected.iter().flatten() {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        let Some(p) = data.provinces.get(&ProvinceId(*id)) else {
            continue;
        };
        if let Some(mesh) = build_province_mesh(&projected) {
            commands.spawn((
                ProvinceMarker { id: ProvinceId(*id) },
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(materials.add(owner_color(&data, &p.owner, *id))),
                Transform::IDENTITY,
            ));
        }
        geometry.bboxes.insert(*id, (lo, hi));
        geometry.rings.insert(*id, projected);
    }

    commands.insert_resource(geometry);
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
        Text::new(
            "Click a province. Space: pause · 1-5: speed · M: map mode · WASD/drag: pan · scroll: zoom · T/G: tension +/-",
        ),
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
    mut mode: ResMut<MapMode>,
) {
    if keys.just_pressed(KeyCode::Space) {
        speed.paused = !speed.paused;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        *mode = match *mode {
            MapMode::Political => MapMode::Terrain,
            MapMode::Terrain => MapMode::Political,
        };
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

fn camera_controls(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    mut motion: MessageReader<MouseMotion>,
    time: Res<Time>,
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = &mut *projection else {
        return;
    };
    for ev in wheel.read() {
        let step = if ev.y > 0.0 { 0.9 } else { 1.1 };
        ortho.scale = (ortho.scale * step).clamp(0.05, 40.0);
    }
    let mut pan = Vec2::ZERO;
    let pan_speed = 600.0 * ortho.scale * time.delta_secs();
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        pan.y += pan_speed;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        pan.y -= pan_speed;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        pan.x -= pan_speed;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        pan.x += pan_speed;
    }
    if buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Middle) {
        for ev in motion.read() {
            pan.x -= ev.delta.x * ortho.scale;
            pan.y += ev.delta.y * ortho.scale;
        }
    } else {
        motion.clear();
    }
    transform.translation += pan.extend(0.0);
}

/// Recolor province materials when the map mode changes. `Res::is_changed`
/// is true on the first frame too, which harmlessly repaints the colors
/// that setup already applied.
fn apply_map_mode(
    mode: Res<MapMode>,
    world: Res<World1950>,
    provinces: Query<(&ProvinceMarker, &MeshMaterial2d<ColorMaterial>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    if !mode.is_changed() {
        return;
    }
    for (marker, material) in &provinces {
        let Some(p) = world.0.provinces.get(&marker.id) else {
            continue;
        };
        let color = match *mode {
            MapMode::Political => owner_color(&world.0, &p.owner, marker.id.0),
            MapMode::Terrain => terrain_color(p.terrain, marker.id.0),
        };
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.color = color;
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
    for _ in 0..ticks {
        world.run_schedule(ugs_sim::SimTick);
    }
}

fn point_in_rings(point: Vec2, rings: &[Vec<Vec2>]) -> bool {
    // Even-odd ray cast across all rings of the province.
    let mut inside = false;
    for ring in rings {
        let n = ring.len();
        let mut j = n - 1;
        for i in 0..n {
            let (a, b) = (ring[i], ring[j]);
            if (a.y > point.y) != (b.y > point.y)
                && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
            {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

fn select_province(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    geometry: Res<WorldGeometry>,
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
    selected.0 = geometry
        .bboxes
        .iter()
        .filter(|(_, (lo, hi))| {
            world_pos.x >= lo.x && world_pos.x <= hi.x && world_pos.y >= lo.y && world_pos.y <= hi.y
        })
        .find(|(id, _)| point_in_rings(world_pos, &geometry.rings[id]))
        .map(|(id, _)| ProvinceId(*id));
}

fn draw_selection_outline(
    selected: Res<Selected>,
    geometry: Res<WorldGeometry>,
    mut gizmos: Gizmos,
) {
    let Some(ProvinceId(id)) = selected.0 else {
        return;
    };
    let Some(rings) = geometry.rings.get(&id) else {
        return;
    };
    for ring in rings {
        if ring.len() >= 2 {
            let mut pts = ring.clone();
            pts.push(ring[0]);
            gizmos.linestrip_2d(pts, Color::srgb(0.98, 0.92, 0.45));
        }
    }
}

#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn update_ui_text(
    clock: Res<SimClock>,
    speed: Res<GameSpeed>,
    tension: Res<GlobalTension>,
    mode: Res<MapMode>,
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
            "{}  [{}]    Tension: {:.1} ({})    Map: {:?}",
            clock.date,
            state,
            tension.displayed(),
            tension.band(),
            *mode,
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
                let pop = if p.population_k >= 1000 {
                    format!("{:.1}M", p.population_k as f32 / 1000.0)
                } else {
                    format!("{}k", p.population_k)
                };
                text.0 = format!("{} — {} · {:?} · pop {}", p.name, owner, p.terrain, pop);
            }
        }
    }
}
