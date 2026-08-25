//! Audio v1: menu music and UI clicks, per the researched direction
//! (docs/design/systems/audio.md). The war room itself stays silent for
//! now — deliberately: near-silence is the Calm state, and the
//! tension-driven stem mixer arrives with bevy_kira_audio later.

use bevy::audio::{PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::AppState;

/// Preloaded handles so the first click isn't delayed by asset IO.
#[derive(Resource)]
struct AudioHandles {
    menu_music: Handle<AudioSource>,
    click: Handle<AudioSource>,
}

impl FromWorld for AudioHandles {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self {
            menu_music: assets.load("audio/music/dark_ambient.mp3"),
            click: assets.load("audio/ui/click_001.ogg"),
        }
    }
}

#[derive(Component)]
struct MenuMusic;

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioHandles>();
        app.add_systems(OnEnter(AppState::MainMenu), start_menu_music);
        // The campaign map is a quiet war room until the stem mixer lands.
        app.add_systems(OnEnter(AppState::InGame), stop_menu_music);
        app.add_systems(Update, ui_click_sfx);
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
