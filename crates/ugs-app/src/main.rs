//! Presentation layer: window, rendering, input, UI. This binary owns real
//! time; the sim only ever sees "advance N ticks". Nothing in ugs-sim may
//! depend on anything in this crate. All sim mutations go through
//! `PendingCommands` — never write sim resources directly from here.

mod atomic_ui;
mod audio;
mod econ_ui;
mod endgame;
mod map;
mod menu;
mod war_ui;

use std::collections::BTreeMap;
use std::path::Path;

use bevy::prelude::*;
use ugs_data::{CountryTag, ScenarioData};
use ugs_sim::{calendar::GameDate, SimPlugin};

use map::WorldGeometry;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    NationSelect,
    InGame,
}

/// The nation the player controls; inserted when Play is pressed.
#[derive(Resource, Debug, Clone)]
pub struct PlayerNation(pub CountryTag);

/// Game typefaces (all SIL OFL, shipped in assets/fonts):
/// Oswald for display/headers, Jost (Futura-lineage geometric sans) for
/// UI, Courier Prime for dossier/typewriter flavor text.
#[derive(Resource)]
pub struct Fonts {
    pub display: Handle<Font>,
    pub body: Handle<Font>,
    pub body_medium: Handle<Font>,
    pub mono: Handle<Font>,
    pub mono_bold: Handle<Font>,
}

impl FromWorld for Fonts {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self {
            display: assets.load("fonts/Oswald-500.ttf"),
            body: assets.load("fonts/Jost-400.ttf"),
            body_medium: assets.load("fonts/Jost-500.ttf"),
            mono: assets.load("fonts/CourierPrime-400.ttf"),
            mono_bold: assets.load("fonts/CourierPrime-700.ttf"),
        }
    }
}

/// A `TextFont` from a handle + size.
pub fn font(handle: &Handle<Font>, size: f32) -> TextFont {
    TextFont {
        font: bevy::text::FontSource::Handle(handle.clone()),
        font_size: bevy::text::FontSize::Px(size),
        ..default()
    }
}

/// Loaded scenario, kept for UI lookups (names, owners, nation meta).
#[derive(Resource)]
pub struct World1950(pub ScenarioData);

/// Game speed control. `ticks_per_second` is in-game hours per real second.
#[derive(Resource, Debug)]
pub struct GameSpeed {
    pub paused: bool,
    pub level: u8,
    pub accumulator: f32,
}

impl GameSpeed {
    pub fn ticks_per_second(&self) -> f32 {
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
    // Load world data before the app starts: screens may need it in their
    // very first OnEnter, which can run before Startup commands apply.
    let (world, geometry) = load_world();
    let scenario = std::sync::Arc::new(world.0.clone());
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Untitled Grand Strategy — 1950".into(),
                    position: WindowPosition::At(IVec2::new(0, 40)),
                    resolution: (1440u32, 810u32).into(),
                    ..default()
                }),
                ..default()
            })
            // Everything else (scenario RON, geometry) loads relative to
            // the working directory; make the asset server match instead
            // of rooting at the executable's folder.
            .set(AssetPlugin {
                file_path: std::env::current_dir()
                    .map(|d| d.join("assets").to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "assets".into()),
                ..default()
            }),
    )
    .add_plugins(SimPlugin {
        start_date: GameDate::new(1950, 1, 1, 0),
        seed: 1950,
    })
    .init_resource::<Fonts>()
    .init_state::<AppState>()
    // Dev shortcut: UGS_SCREEN=select|game boots straight to a screen.
    .insert_state(match std::env::var("UGS_SCREEN").as_deref() {
        Ok("select") => AppState::NationSelect,
        Ok("game") => AppState::InGame,
        _ => AppState::MainMenu,
    })
    // Dev shortcut: UGS_SPEED=1..5 boots unpaused at that speed.
    .insert_resource(
        match std::env::var("UGS_SPEED")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
        {
            Some(level) => GameSpeed {
                paused: false,
                level: level.clamp(1, 5),
                accumulator: 0.0,
            },
            None => GameSpeed {
                paused: true,
                level: 1,
                accumulator: 0.0,
            },
        },
    )
    .insert_resource(ClearColor(Color::srgb(0.09, 0.12, 0.16))) // ocean
    .insert_resource(world)
    .insert_resource(geometry)
    .insert_resource(ugs_sim::demography::SimScenario(scenario))
    .add_systems(Update, dev_auto_screenshot)
    .add_plugins((
        menu::MenuPlugin,
        map::MapPlugin,
        econ_ui::EconUiPlugin,
        audio::GameAudioPlugin,
        war_ui::WarUiPlugin,
        atomic_ui::AtomicUiPlugin,
        endgame::EndgamePlugin,
    ));
    // Spawn the camera before the first state transition: initial OnEnter
    // systems (screen framing) run before Startup would.
    app.world_mut().spawn(Camera2d);
    // Dev shortcut: UGS_NATION=TAG plays that nation (with UGS_SCREEN=game).
    if let Ok(tag) = std::env::var("UGS_NATION") {
        app.world_mut()
            .insert_resource(PlayerNation(CountryTag(tag.to_uppercase())));
    }
    app.run();
}

/// Dev aid: UGS_SHOT=<path.png> saves a screenshot of the game window a few
/// seconds after boot (pairs with UGS_SCREEN to verify screens headlessly).
fn dev_auto_screenshot(mut frames: Local<u32>, mut done: Local<bool>, mut commands: Commands) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    if *done {
        return;
    }
    let Ok(path) = std::env::var("UGS_SHOT") else {
        *done = true;
        return;
    };
    *frames += 1;
    let target: u32 = std::env::var("UGS_SHOT_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    if *frames == target {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        *done = true;
    }
}

/// Load scenario data and project province geometry once at boot.
fn load_world() -> (World1950, WorldGeometry) {
    let data = ScenarioData::load(Path::new("assets/data/scenario/1950"))
        .expect("failed to load 1950 scenario (run from the workspace root)");
    let geo_text = std::fs::read_to_string("assets/map/world.geo.ron")
        .expect("missing assets/map/world.geo.ron (run: cargo run -p mapgen --release)");
    let raw_geo: BTreeMap<u32, Vec<Vec<(f32, f32)>>> =
        ron::from_str(&geo_text).expect("bad world.geo.ron");

    let mut geometry = WorldGeometry::default();
    for (id, rings) in &raw_geo {
        let projected: Vec<Vec<Vec2>> = rings
            .iter()
            .map(|ring| {
                ring.iter()
                    .map(|&(lon, lat)| map::project(lon, lat))
                    .collect()
            })
            .collect();
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for v in projected.iter().flatten() {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        geometry.bboxes.insert(*id, (lo, hi));
        geometry.rings.insert(*id, projected);
    }

    (World1950(data), geometry)
}
