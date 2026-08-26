//! Shared UI widgets: toggles that show their state, radio segments,
//! and hover tooltips. Every stateful control must read at a glance —
//! a toggle that looks like a plain button is a bug.

use bevy::prelude::*;

use crate::{font, Fonts};

/// Hover tooltip text. Attach to anything with an `Interaction`
/// component; the tooltip system shows it near the cursor after a
/// short hover delay.
#[derive(Component, Clone)]
pub struct Tooltip(pub String);

impl Tooltip {
    pub fn of(text: impl Into<String>) -> Self {
        Tooltip(text.into())
    }
}

#[derive(Component)]
struct TooltipPanel;

#[derive(Resource, Default)]
struct HoverState {
    entity: Option<Entity>,
    secs: f32,
    shown: bool,
}

const TIP_DELAY_SECS: f32 = 0.35;
const TIP_WIDTH: f32 = 290.0;

pub const TOGGLE_ON_BG: Color = Color::srgb(0.55, 0.44, 0.18);
pub const TOGGLE_OFF_BG: Color = Color::srgba(0.14, 0.17, 0.21, 0.95);
pub const TOGGLE_ON_MARK: Color = Color::srgb(0.98, 0.88, 0.55);
pub const TOGGLE_OFF_MARK: Color = Color::srgb(0.35, 0.40, 0.46);
/// Restrictive/dangerous on-state (ROE bans and the like).
pub const TOGGLE_RED_BG: Color = Color::srgb(0.5, 0.22, 0.18);
pub const TOGGLE_RED_MARK: Color = Color::srgb(0.95, 0.55, 0.45);

/// An on/off control that SHOWS its state: `[■] LABEL` lit when on,
/// `[ ] LABEL` dark when off. `red` uses the restrictive palette.
#[allow(clippy::too_many_arguments)]
pub fn toggle<B: Bundle>(
    parent: &mut ChildSpawnerCommands,
    tag: B,
    label: &str,
    on: bool,
    red: bool,
    fonts: &Fonts,
    size: f32,
    tip: &str,
) {
    let (bg, mark) = match (on, red) {
        (true, false) => (TOGGLE_ON_BG, TOGGLE_ON_MARK),
        (true, true) => (TOGGLE_RED_BG, TOGGLE_RED_MARK),
        (false, _) => (TOGGLE_OFF_BG, TOGGLE_OFF_MARK),
    };
    parent
        .spawn((
            Button,
            tag,
            Tooltip::of(tip),
            Node {
                padding: UiRect::axes(Val::Px(7.0), Val::Px(3.0)),
                column_gap: Val::Px(5.0),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(bg),
        ))
        .with_children(|b| {
            // The state indicator: solid when on, hollow when off.
            b.spawn((
                Node {
                    width: Val::Px(size * 0.7),
                    height: Val::Px(size * 0.7),
                    padding: UiRect::all(Val::Px(1.5)),
                    ..default()
                },
                BackgroundColor(mark),
            ))
            .with_children(|inner| {
                if !on {
                    inner.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(TOGGLE_OFF_BG),
                    ));
                }
            });
            b.spawn((
                Text::new(label),
                font(&fonts.mono, size),
                TextColor(Color::srgb(0.88, 0.89, 0.90)),
            ));
        });
}

/// One option of a radio group: lit background when selected, no
/// indicator square — exactly one segment in a row should be lit.
#[allow(clippy::too_many_arguments)]
pub fn segment<B: Bundle>(
    parent: &mut ChildSpawnerCommands,
    tag: B,
    label: &str,
    selected: bool,
    fonts: &Fonts,
    size: f32,
    tip: &str,
) {
    parent
        .spawn((
            Button,
            tag,
            Tooltip::of(tip),
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(if selected {
                TOGGLE_ON_BG
            } else {
                TOGGLE_OFF_BG
            }),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                font(&fonts.mono, size),
                TextColor(if selected {
                    Color::srgb(0.98, 0.95, 0.88)
                } else {
                    Color::srgb(0.70, 0.73, 0.76)
                }),
            ));
        });
}

/// A read-only info node that only exists to carry a tooltip: adds
/// `Interaction` so hovering works on plain text rows.
pub fn tipped_text(
    parent: &mut ChildSpawnerCommands,
    text: String,
    fonts: &Fonts,
    size: f32,
    color: Color,
    tip: &str,
) {
    parent.spawn((
        Text::new(text),
        font(&fonts.mono, size),
        TextColor(color),
        Interaction::default(),
        Tooltip::of(tip),
    ));
}

pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoverState>();
        app.add_systems(Update, tooltip_system);
    }
}

/// Show the hovered widget's tooltip near the cursor after a short
/// delay; despawn it the moment the hover ends or moves.
fn tooltip_system(
    mut commands: Commands,
    time: Res<Time>,
    fonts: Res<Fonts>,
    windows: Query<&Window>,
    tips: Query<(Entity, &Interaction, &Tooltip)>,
    panels: Query<Entity, With<TooltipPanel>>,
    mut hover: ResMut<HoverState>,
) {
    let hovered = tips
        .iter()
        .find(|(_, interaction, _)| **interaction != Interaction::None);
    let Some((entity, _, tip)) = hovered else {
        if hover.entity.is_some() {
            *hover = HoverState::default();
            for e in &panels {
                commands.entity(e).despawn();
            }
        }
        return;
    };
    if hover.entity != Some(entity) {
        *hover = HoverState {
            entity: Some(entity),
            secs: 0.0,
            shown: false,
        };
        for e in &panels {
            commands.entity(e).despawn();
        }
    }
    hover.secs += time.delta_secs();
    if hover.secs < TIP_DELAY_SECS || hover.shown {
        return;
    }
    hover.shown = true;
    let Ok(window) = windows.single() else { return };
    let cursor = window.cursor_position().unwrap_or(Vec2::new(40.0, 40.0));
    // Keep the panel on-screen: flip left of the cursor near the right
    // edge, clamp above the bottom.
    let x = if cursor.x + TIP_WIDTH + 26.0 > window.width() {
        (cursor.x - TIP_WIDTH - 12.0).max(4.0)
    } else {
        cursor.x + 14.0
    };
    let y = (cursor.y + 18.0).min(window.height() - 140.0);
    commands
        .spawn((
            TooltipPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(x),
                top: Val::Px(y),
                max_width: Val::Px(TIP_WIDTH),
                padding: UiRect::all(Val::Px(9.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.07, 0.10, 0.97)),
            GlobalZIndex(60),
        ))
        .with_children(|t| {
            t.spawn((
                Text::new(tip.0.clone()),
                font(&fonts.mono, 11.0),
                TextColor(Color::srgb(0.86, 0.87, 0.88)),
            ));
        });
}
