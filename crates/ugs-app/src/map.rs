//! The in-game map screen: province rendering, camera, selection, sim
//! driving, and the in-game HUD. Active only in `AppState::InGame`.

use std::collections::BTreeMap;

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use ugs_data::{CountryTag, ProvinceId, ScenarioData, Terrain};
use ugs_sim::{
    command::{PendingCommands, SimCommand},
    tension::GlobalTension,
    SimClock,
};

use crate::{AppState, GameSpeed, PlayerNation, World1950};

/// Projected province outlines + bounding boxes, for click hit-testing and
/// drawing the selection outline.
#[derive(Resource, Default)]
pub struct WorldGeometry {
    pub rings: BTreeMap<u32, Vec<Vec<Vec2>>>,
    pub bboxes: BTreeMap<u32, (Vec2, Vec2)>,
}

#[derive(Resource, Default)]
struct Selected(Option<ProvinceId>);

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
enum MapMode {
    #[default]
    Political,
    Terrain,
}

/// Marker for spawned map layer entities (fill + borders).
#[derive(Component)]
struct MapLayer;

/// The shared fill mesh and each province's vertex range within it, for
/// in-place recoloring (map modes, future ownership changes).
#[derive(Resource)]
struct MapFill {
    mesh: Handle<Mesh>,
    /// province id -> (first vertex, vertex count)
    ranges: BTreeMap<u32, (usize, usize)>,
    total_vertices: usize,
}

#[derive(Component)]
struct TopBarText;

#[derive(Component)]
struct SelectionText;

/// Equirectangular degrees -> world units, centered on Korea for now.
pub fn project(lon: f32, lat: f32) -> Vec2 {
    Vec2::new((lon - 127.3) * 60.0, (lat - 37.5) * 60.0)
}

/// One full world circumference in world units. The map wraps east-west:
/// the world is rendered at offsets {-WRAP, 0, +WRAP} and the camera x is
/// wrapped modulo this, so panning across the Pacific is seamless.
pub const WORLD_WRAP: f32 = 360.0 * 60.0;

fn canonical_west() -> f32 {
    project(-180.0, 0.0).x
}

/// Wrap an x coordinate into the canonical copy's range.
fn wrap_x(x: f32) -> f32 {
    let west = canonical_west();
    (x - west).rem_euclid(WORLD_WRAP) + west
}

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selected>();
        // Dev shortcut: UGS_MAPMODE=terrain boots in terrain mode.
        app.insert_resource(match std::env::var("UGS_MAPMODE").as_deref() {
            Ok("terrain") => MapMode::Terrain,
            _ => MapMode::Political,
        });
        // The map underlies both the nation-select screen and the game.
        app.add_systems(OnEnter(AppState::NationSelect), (spawn_map, overview_camera));
        app.add_systems(
            OnEnter(AppState::InGame),
            (spawn_map, spawn_hud, focus_player_camera).chain(),
        );
        // Browsing controls in nation select (suppressed while over UI so
        // wheel scrolls panels, not the camera).
        app.add_systems(
            Update,
            camera_controls
                .run_if(in_state(AppState::NationSelect))
                .run_if(not(crate::menu::ui_hovered)),
        );
        app.add_systems(
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
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Whole-world framing for the nation-select screen.
fn overview_camera(mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>) {
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let center = project(15.0, 22.0);
    transform.translation = center.extend(transform.translation.z);
    if let Projection::Orthographic(ortho) = &mut *projection {
        ortho.scale = 11.0;
    }
}

/// Zoom to the player's capital when the campaign starts.
fn focus_player_camera(
    player: Option<Res<PlayerNation>>,
    world: Res<World1950>,
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Some(player) = player else { return };
    let Some(country) = world.0.countries.get(&player.0) else {
        return;
    };
    let Some(capital) = world.0.provinces.get(&country.capital) else {
        return;
    };
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let center = project(capital.center.0, capital.center.1);
    transform.translation = center.extend(transform.translation.z);
    if let Projection::Orthographic(ortho) = &mut *projection {
        ortho.scale = 2.5;
    }
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

/// Flat quad-strip mesh for a set of polylines (miterless — fine at these
/// widths). `z` orders the layer; color is baked per-vertex.
fn build_line_mesh(polylines: &[Vec<Vec2>], width: f32, color: [f32; 4], z: f32) -> Mesh {
    let half = width / 2.0;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for line in polylines {
        for seg in line.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let dir = b - a;
            if dir.length_squared() < 1e-9 {
                continue;
            }
            let n = dir.normalize().perp() * half;
            let base = positions.len() as u32;
            positions.extend([
                [a.x + n.x, a.y + n.y, z],
                [a.x - n.x, a.y - n.y, z],
                [b.x + n.x, b.y + n.y, z],
                [b.x - n.x, b.y - n.y, z],
            ]);
            indices.extend([base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        }
    }
    let count = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![color; count])
        .with_inserted_indices(Indices::U32(indices))
}

/// Build the whole political map as ONE vertex-colored mesh (a draw call
/// per wrap copy instead of thousands of entities), plus faint province
/// outlines and emphasized country borders.
fn spawn_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    world: Res<World1950>,
    geometry: Res<WorldGeometry>,
    existing: Query<(), With<MapLayer>>,
) {
    if !existing.is_empty() {
        return; // map already spawned
    }
    let data = &world.0;

    // --- Fill mesh with per-province vertex colors -----------------------
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut ranges = BTreeMap::new();
    for (id, rings) in &geometry.rings {
        let Some(p) = data.provinces.get(&ProvinceId(*id)) else {
            continue;
        };
        let color = owner_color(data, &p.owner, *id).to_linear().to_f32_array();
        let start = positions.len();
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
            colors.extend(std::iter::repeat_n(color, ring.len()));
            indices.extend(tris.iter().map(|&i| base + i as u32));
        }
        ranges.insert(*id, (start, positions.len() - start));
    }
    let total_vertices = positions.len();
    let fill_mesh = meshes.add(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
            .with_inserted_indices(Indices::U32(indices)),
    );

    // --- Border layers ---------------------------------------------------
    // Province outlines: faint, from the simplified rings (double-drawn on
    // shared borders, which just deepens them slightly).
    let outline_lines: Vec<Vec<Vec2>> = geometry
        .rings
        .values()
        .flatten()
        .filter(|r| r.len() >= 2)
        .map(|r| {
            let mut line = r.clone();
            line.push(r[0]);
            line
        })
        .collect();
    let outline_mesh = meshes.add(build_line_mesh(
        &outline_lines,
        0.7,
        [0.02, 0.025, 0.03, 0.35],
        1.0,
    ));

    // Country borders: precise polylines from mapgen's raw topology.
    let border_lines: Vec<Vec<Vec2>> =
        std::fs::read_to_string("assets/map/country_borders.ron")
            .ok()
            .and_then(|text| ron::from_str::<Vec<Vec<(f32, f32)>>>(&text).ok())
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line.iter().map(|&(lon, lat)| project(lon, lat)).collect())
                    .collect()
            })
            .unwrap_or_default();
    let border_mesh = meshes.add(build_line_mesh(
        &border_lines,
        2.2,
        [0.015, 0.018, 0.022, 0.9],
        2.0,
    ));

    let white = materials.add(Color::WHITE);
    for offset in [-WORLD_WRAP, 0.0, WORLD_WRAP] {
        for mesh in [&fill_mesh, &outline_mesh, &border_mesh] {
            commands.spawn((
                MapLayer,
                Mesh2d(mesh.clone()),
                MeshMaterial2d(white.clone()),
                Transform::from_xyz(offset, 0.0, 0.0),
            ));
        }
    }

    commands.insert_resource(MapFill {
        mesh: fill_mesh,
        ranges,
        total_vertices,
    });
}

fn spawn_hud(mut commands: Commands, fonts: Res<crate::Fonts>, existing: Query<(), With<TopBarText>>) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        TopBarText,
        Text::new(""),
        crate::font(&fonts.body_medium, 15.0),
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
            "Click a province. Space: pause - 1-5: speed - M: map mode - WASD/drag: pan - scroll: zoom - T/G: tension +/-",
        ),
        crate::font(&fonts.body, 13.0),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(8.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

/// The province under the cursor, if any (shared by in-game selection and
/// the nation-select screen).
pub fn cursor_province(
    windows: &Query<&Window>,
    camera: &Query<(&Camera, &GlobalTransform)>,
    geometry: &WorldGeometry,
) -> Option<ProvinceId> {
    let window = windows.single().ok()?;
    let cursor = window.cursor_position()?;
    let (camera, cam_transform) = camera.single().ok()?;
    let mut world_pos = camera.viewport_to_world_2d(cam_transform, cursor).ok()?;
    // Clicks on an offset copy hit-test against the canonical geometry.
    world_pos.x = wrap_x(world_pos.x);
    geometry
        .bboxes
        .iter()
        .filter(|(_, (lo, hi))| {
            world_pos.x >= lo.x && world_pos.x <= hi.x && world_pos.y >= lo.y && world_pos.y <= hi.y
        })
        .find(|(id, _)| point_in_rings(world_pos, &geometry.rings[id]))
        .map(|(id, _)| ProvinceId(*id))
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
    windows: Query<&Window>,
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = &mut *projection else {
        return;
    };
    // Never let the viewport span more than one world circumference, or
    // the wrap illusion breaks at the edges of the three copies.
    let max_scale = windows
        .single()
        .map(|w| (WORLD_WRAP / w.width().max(1.0)).min(40.0))
        .unwrap_or(12.0);
    for ev in wheel.read() {
        let step = if ev.y > 0.0 { 0.9 } else { 1.1 };
        ortho.scale = (ortho.scale * step).clamp(0.05, max_scale);
    }
    ortho.scale = ortho.scale.min(max_scale);
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
    // East-west wrap: crossing the antimeridian teleports the camera one
    // world-width over; the offset copies make it seamless on screen.
    transform.translation.x = wrap_x(transform.translation.x);
}

/// Recolor the shared fill mesh's vertex colors when the map mode changes.
fn apply_map_mode(
    mode: Res<MapMode>,
    world: Res<World1950>,
    fill: Option<Res<MapFill>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !mode.is_changed() {
        return;
    }
    let Some(fill) = fill else { return };
    let Some(mut mesh) = meshes.get_mut(&fill.mesh) else {
        return;
    };
    let mut colors = vec![[0.5, 0.5, 0.5, 1.0]; fill.total_vertices];
    for (id, (start, len)) in &fill.ranges {
        let Some(p) = world.0.provinces.get(&ProvinceId(*id)) else {
            continue;
        };
        let color = match *mode {
            MapMode::Political => owner_color(&world.0, &p.owner, *id),
            MapMode::Terrain => terrain_color(p.terrain, *id),
        }
        .to_linear()
        .to_f32_array();
        colors[*start..start + len].fill(color);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
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
    selected.0 = cursor_province(&windows, &camera, &geometry);
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
            for offset in [-WORLD_WRAP, 0.0, WORLD_WRAP] {
                let mut pts: Vec<Vec2> =
                    ring.iter().map(|v| Vec2::new(v.x + offset, v.y)).collect();
                pts.push(pts[0]);
                gizmos.linestrip_2d(pts, Color::srgb(0.98, 0.92, 0.45));
            }
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
    player: Option<Res<PlayerNation>>,
    mut top: Query<&mut Text, (With<TopBarText>, Without<SelectionText>)>,
    mut bottom: Query<&mut Text, (With<SelectionText>, Without<TopBarText>)>,
) {
    let state = if speed.paused {
        "PAUSED".to_string()
    } else {
        format!("speed {}", speed.level)
    };
    let playing = player
        .map(|p| {
            world
                .0
                .countries
                .get(&p.0)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| p.0 .0.clone())
        })
        .unwrap_or_else(|| "Observer".into());
    for mut text in &mut top {
        text.0 = format!(
            "{}  [{}]    {}    Tension: {:.1} ({})    Map: {:?}",
            clock.date,
            state,
            playing,
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
                text.0 = format!("{} — {} - {:?} - pop {}", p.name, owner, p.terrain, pop);
            }
        }
    }
}
