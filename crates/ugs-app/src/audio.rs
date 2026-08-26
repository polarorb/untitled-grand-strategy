//! Audio v1.5: menu music, UI clicks, and a light tension-driven music
//! pass on the campaign map, per the researched direction
//! (docs/design/systems/audio.md). Whole-track switching with hysteresis
//! and crossfades — the synced stem mixer (bevy_kira_audio) comes later.
//!
//! The design's sound: near-silence IS the Calm state, the room thickens
//! toward Crisis, and Brink strips the music back to a bare synthesized
//! pulse. Pause doesn't stop the music — it ducks, like held breath.

use bevy::audio::{AudioSinkPlayback, PlaybackSettings, Volume};
use bevy::prelude::*;
use ugs_sim::tension::{GlobalTension, TensionBand};

use crate::{AppState, GameSpeed};

mod tuning {
    /// Target linear volumes per band — Calm is barely there.
    pub const CALM_VOLUME: f32 = 0.16;
    pub const CRISIS_VOLUME: f32 = 0.26;
    pub const BRINK_VOLUME: f32 = 0.30;
    /// Crossfade rate, linear volume per second (~3s full swap).
    pub const FADE_PER_SEC: f32 = 0.12;
    /// A band change must hold this long (real seconds) before the
    /// music follows — boundary jitter must not flap the mix.
    pub const HYSTERESIS_SECS: f32 = 8.0;
    /// Pause ducks music to this multiplier (~ -6 dB).
    pub const PAUSE_DUCK: f32 = 0.5;
}

/// Preloaded handles so the first click isn't delayed by asset IO.
#[derive(Resource)]
pub struct AudioHandles {
    menu_music: Handle<AudioSource>,
    calm_music: Handle<AudioSource>,
    crisis_music: Handle<AudioSource>,
    /// Synthesized in-house (tools/audio/synth_sfx.py) — at Brink the
    /// music strips back to a bare pulse.
    brink_music: Handle<AudioSource>,
    click: Handle<AudioSource>,
    /// Synthesized in-house (tools/audio/synth_sfx.py).
    pub teletype: Handle<AudioSource>,
    pub alert: Handle<AudioSource>,
}

impl FromWorld for AudioHandles {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self {
            menu_music: assets.load("audio/music/dark_ambient.mp3"),
            calm_music: assets.load("audio/music/cold_journey.mp3"),
            crisis_music: assets.load("audio/music/tension.mp3"),
            brink_music: assets.load("audio/music/brink_pulse.wav"),
            click: assets.load("audio/ui/click_001.ogg"),
            teletype: assets.load("audio/ui/teletype.wav"),
            alert: assets.load("audio/ui/alert.wav"),
        }
    }
}

#[derive(Component)]
struct MenuMusic;

#[derive(Component)]
struct GameMusic;

/// Manual volume envelope on a music entity: lerped toward `target`
/// each frame (times the pause duck); `fade_out` despawns at zero.
#[derive(Component)]
struct MusicFade {
    current: f32,
    target: f32,
    fade_out: bool,
}

/// Which track the campaign map wants per tension band. Wary shares
/// Calm's track — the room only wakes at Crisis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MusicBand {
    Calm,
    Crisis,
    Brink,
}

impl MusicBand {
    fn of(band: TensionBand) -> Self {
        match band {
            TensionBand::Calm | TensionBand::Wary => MusicBand::Calm,
            TensionBand::Crisis => MusicBand::Crisis,
            TensionBand::Brink => MusicBand::Brink,
        }
    }
    fn volume(self) -> f32 {
        match self {
            MusicBand::Calm => tuning::CALM_VOLUME,
            MusicBand::Crisis => tuning::CRISIS_VOLUME,
            MusicBand::Brink => tuning::BRINK_VOLUME,
        }
    }
}

/// Hysteresis state for the campaign music.
#[derive(Resource, Default)]
struct MusicState {
    playing: Option<MusicBand>,
    /// A differing band and how long it has held, in real seconds.
    pending: Option<(MusicBand, f32)>,
}

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioHandles>();
        app.init_resource::<MusicState>();
        app.add_systems(OnEnter(AppState::MainMenu), start_menu_music);
        app.add_systems(OnEnter(AppState::InGame), stop_menu_music);
        app.add_systems(OnExit(AppState::InGame), stop_game_music);
        app.add_systems(Update, update_game_music.run_if(in_state(AppState::InGame)));
        app.add_systems(Update, (apply_music_fades, ui_click_sfx));
    }
}

fn start_menu_music(
    mut commands: Commands,
    handles: Res<AudioHandles>,
    existing: Query<(), With<MenuMusic>>,
) {
    if !existing.is_empty() {
        return; // already playing (came back from nation select)
    }
    commands.spawn((
        MenuMusic,
        AudioPlayer::new(handles.menu_music.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.4)),
    ));
}

fn stop_menu_music(mut commands: Commands, music: Query<Entity, With<MenuMusic>>) {
    for e in &music {
        commands.entity(e).despawn();
    }
}

fn stop_game_music(
    mut commands: Commands,
    mut state: ResMut<MusicState>,
    music: Query<Entity, With<GameMusic>>,
) {
    *state = MusicState::default();
    for e in &music {
        commands.entity(e).despawn();
    }
}

/// Follow the tension band with hysteresis: the first track starts
/// immediately, later switches only after the band has held for
/// HYSTERESIS_SECS. Old tracks fade out, new ones fade in from zero.
fn update_game_music(
    mut commands: Commands,
    time: Res<Time>,
    tension: Res<GlobalTension>,
    handles: Res<AudioHandles>,
    mut state: ResMut<MusicState>,
    mut playing: Query<&mut MusicFade, With<GameMusic>>,
) {
    let want = MusicBand::of(tension.band());
    let switch = match state.playing {
        None => true, // campaign start: no hysteresis on the first track
        Some(current) if current == want => {
            state.pending = None;
            false
        }
        Some(_) => match &mut state.pending {
            Some((band, held)) if *band == want => {
                *held += time.delta_secs();
                *held >= tuning::HYSTERESIS_SECS
            }
            _ => {
                state.pending = Some((want, 0.0));
                false
            }
        },
    };
    if !switch {
        return;
    }
    state.playing = Some(want);
    state.pending = None;
    for mut fade in &mut playing {
        fade.fade_out = true;
        fade.target = 0.0;
    }
    let track = match want {
        MusicBand::Calm => handles.calm_music.clone(),
        MusicBand::Crisis => handles.crisis_music.clone(),
        MusicBand::Brink => handles.brink_music.clone(),
    };
    commands.spawn((
        GameMusic,
        AudioPlayer::new(track),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
        MusicFade {
            current: 0.0,
            target: want.volume(),
            fade_out: false,
        },
    ));
}

/// Drive every music fade envelope; pausing ducks the campaign music
/// instead of stopping it. (The AudioSink appears a frame or two after
/// spawn, once playback starts — the query simply skips until then.)
fn apply_music_fades(
    mut commands: Commands,
    time: Res<Time>,
    speed: Option<Res<GameSpeed>>,
    mut music: Query<(Entity, &mut MusicFade, &mut AudioSink), With<GameMusic>>,
) {
    let duck = if speed.is_some_and(|s| s.paused) {
        tuning::PAUSE_DUCK
    } else {
        1.0
    };
    for (entity, mut fade, mut sink) in &mut music {
        let target = fade.target * if fade.fade_out { 1.0 } else { duck };
        let step = tuning::FADE_PER_SEC * time.delta_secs();
        fade.current = if fade.current < target {
            (fade.current + step).min(target)
        } else {
            (fade.current - step).max(target)
        };
        sink.set_volume(Volume::Linear(fade.current));
        if fade.fade_out && fade.current <= 0.001 {
            commands.entity(entity).despawn();
        }
    }
}

/// One click sound for any button press, anywhere.
fn ui_click_sfx(
    mut commands: Commands,
    handles: Res<AudioHandles>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<Button>)>,
) {
    for interaction in &buttons {
        if *interaction == Interaction::Pressed {
            commands.spawn((
                AudioPlayer::new(handles.click.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.55)),
            ));
            break; // one sound per frame is plenty
        }
    }
}
