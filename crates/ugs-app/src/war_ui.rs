//! War presentation: teletype event popups, formation markers on the
//! map, and occupation-driven map repainting hooks.

use bevy::prelude::*;
use ugs_sim::command::{PendingCommands, SimCommand};
use ugs_sim::events::FiredEvents;
use ugs_sim::military::Military;

use crate::map::project;
use crate::{font, AppState, Fonts, GameSpeed, PlayerNation, World1950};

const PANEL_BG: Color = Color::srgba(0.07, 0.09, 0.12, 0.97);
const ACCENT: Color = Color::srgb(0.83, 0.69, 0.36);
const MAIN: Color = Color::srgb(0.88, 0.89, 0.90);

#[derive(Component)]
struct EventModal;

#[derive(Component)]
struct DismissButton;

/// A choice button: resolves the pending event with this option.
#[derive(Component)]
struct ChoiceButton {
    event_id: String,
    option: u8,
}

#[derive(Component)]
struct FormationMarker;

pub struct WarUiPlugin;

impl Plugin for WarUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                show_event_popups,
                dismiss_popup,
                choice_buttons,
                sync_formation_markers,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Pop a teletype modal for each newly fired event; pause the game.
/// Player-country choice events show option buttons instead of a
/// dismiss; other pending decisions show as news awaiting the decider.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn show_event_popups(
    mut commands: Commands,
    fired: Res<FiredEvents>,
    world: Res<World1950>,
    fonts: Res<Fonts>,
    player: Option<Res<PlayerNation>>,
    mut speed: ResMut<GameSpeed>,
    mut seen: Local<usize>,
    existing: Query<(), With<EventModal>>,
) {
    if fired.fired.len() <= *seen || !existing.is_empty() {
        return;
    }
    let id = &fired.fired[*seen];
    *seen += 1;
    let Some(event) = world.0.events.iter().find(|e| &e.id == id) else {
        return;
    };
    let is_player_choice = fired.is_pending(&event.id)
        && event.country.is_some()
        && player.as_ref().map(|p| Some(&p.0) == event.country.as_ref()).unwrap_or(false);
    speed.paused = true;
    commands
        .spawn((
            EventModal,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(20.0),
                margin: UiRect::left(Val::Px(-260.0)),
                width: Val::Px(520.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                padding: UiRect::all(Val::Px(22.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|m| {
            m.spawn((
                Text::new("*** FLASH TRAFFIC ***"),
                font(&fonts.mono_bold, 13.0),
                TextColor(ACCENT),
            ));
            m.spawn((
                Text::new(event.title.clone()),
                font(&fonts.display, 22.0),
                TextColor(MAIN),
            ));
            m.spawn((
                Text::new(event.body.clone()),
                font(&fonts.mono, 13.5),
                TextColor(MAIN),
            ));
            if is_player_choice {
                for (i, option) in event.options.iter().enumerate() {
                    m.spawn((
                        Button,
                        ChoiceButton {
                            event_id: event.id.clone(),
                            option: i as u8,
                        },
                        Node {
                            padding: UiRect::axes(Val::Px(20.0), Val::Px(9.0)),
                            ..default()
                        },
                        BackgroundColor(if i == 0 {
                            Color::srgb(0.55, 0.44, 0.18)
                        } else {
                            Color::srgba(0.14, 0.17, 0.21, 0.95)
                        }),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(option.label.clone()),
                            font(&fonts.display, 15.0),
                            TextColor(Color::srgb(0.98, 0.95, 0.88)),
                        ));
                    });
                }
            } else {
                if fired.is_pending(&event.id) {
                    let decider = event
                        .country
                        .as_ref()
                        .map(|c| c.0.clone())
                        .unwrap_or_default();
                    m.spawn((
                        Text::new(format!("DECISION RESTS WITH {decider}")),
                        font(&fonts.mono, 12.0),
                        TextColor(Color::srgb(0.62, 0.66, 0.70)),
                    ));
                }
                m.spawn((
                    Button,
                    DismissButton,
                    Node {
                        align_self: AlignSelf::FlexEnd,
                        padding: UiRect::axes(Val::Px(24.0), Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.55, 0.44, 0.18)),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("ACKNOWLEDGE"),
                        font(&fonts.display, 15.0),
                        TextColor(Color::srgb(0.98, 0.95, 0.88)),
                    ));
                });
            }
        });
}

/// Choice buttons resolve the event through the command queue (so the
/// decision is part of the save/replay log) and close the modal.
fn choice_buttons(
    mut commands: Commands,
    buttons: Query<(&Interaction, &ChoiceButton), Changed<Interaction>>,
    mut pending: ResMut<PendingCommands>,
    modal: Query<Entity, With<EventModal>>,
) {
    for (interaction, choice) in &buttons {
        if *interaction == Interaction::Pressed {
            pending.push(SimCommand::ResolveEvent {
                id: choice.event_id.clone(),
                option: choice.option,
            });
            for e in &modal {
                commands.entity(e).despawn();
            }
        }
    }
}

fn dismiss_popup(
    mut commands: Commands,
    buttons: Query<&Interaction, (Changed<Interaction>, With<DismissButton>)>,
    modal: Query<Entity, With<EventModal>>,
) {
    for interaction in &buttons {
        if *interaction == Interaction::Pressed {
            for e in &modal {
                commands.entity(e).despawn();
            }
        }
    }
}

/// Rebuild division counters when armies move (cheap: tens of markers).
fn sync_formation_markers(
    mut commands: Commands,
    military: Res<Military>,
    world: Res<World1950>,
    fonts: Res<Fonts>,
    mut last_hash: Local<u64>,
    markers: Query<Entity, With<FormationMarker>>,
) {
    if !military.is_changed() {
        return;
    }
    // Only rebuild when the position/ownership picture changes.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for f in military.formations.values() {
        for v in [f.location.0 as u64, f.owner.0.bytes().map(u64::from).sum()] {
            hash = (hash ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    if hash == *last_hash {
        return;
    }
    *last_hash = hash;

    for e in &markers {
        commands.entity(e).despawn();
    }
    // Group counts per (province, owner).
    let mut counts: std::collections::BTreeMap<(u32, String), u32> = Default::default();
    for f in military.formations.values() {
        *counts.entry((f.location.0, f.owner.0.clone())).or_default() += 1;
    }
    for ((province, owner), count) in counts {
        let Some(p) = world.0.provinces.get(&ugs_data::ProvinceId(province)) else {
            continue;
        };
        let pos = project(p.center.0, p.center.1);
        let rgb = world
            .0
            .countries
            .get(&ugs_data::CountryTag(owner.clone()))
            .map(|c| c.color)
            .unwrap_or((128, 128, 128));
        let color = Color::srgb(
            rgb.0 as f32 / 255.0 * 0.6,
            rgb.1 as f32 / 255.0 * 0.6,
            rgb.2 as f32 / 255.0 * 0.6,
        );
        commands.spawn((
            FormationMarker,
            Sprite::from_color(color, Vec2::new(16.0, 11.0)),
            Transform::from_translation(pos.extend(3.0)),
        ));
        commands.spawn((
            FormationMarker,
            Text2d::new(format!("{count}")),
            font(&fonts.body_medium, 12.0),
            TextColor(Color::WHITE),
            Transform::from_translation(pos.extend(3.1)),
        ));
    }
}
