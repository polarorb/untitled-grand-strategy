//! Main menu and the choose-a-nation screen.
//!
//! Nation select is map-first: the political world map renders underneath,
//! clicking any province opens a closeable dossier overlay for its owner.
//! Nation metadata comes from `ScenarioData::nations_meta`; flags and
//! portraits are loose files at `assets/flags/<TAG>.*` /
//! `assets/leaders/<TAG>.*` and every image is optional — the screen
//! degrades to text when an asset is missing.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use ugs_data::CountryTag;

use crate::map::{self, WorldGeometry};
use crate::{font, AppState, Fonts, PlayerNation, World1950};

const PANEL_BG: Color = Color::srgba(0.07, 0.09, 0.12, 0.96);
const PANEL_BG_LIGHT: Color = Color::srgba(0.14, 0.17, 0.21, 0.95);
const ACCENT: Color = Color::srgb(0.83, 0.69, 0.36); // brass
const TEXT_DIM: Color = Color::srgb(0.62, 0.66, 0.70);
const TEXT_MAIN: Color = Color::srgb(0.88, 0.89, 0.90);

/// Nation highlighted in the select screen (not yet confirmed with Play).
#[derive(Resource, Default)]
struct NationChoice(Option<CountryTag>);

/// Whether the dossier overlay is showing.
#[derive(Resource, Default)]
struct InfoOpen(bool);

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct SelectRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum MenuButton {
    NewGame,
    Quit,
    Back,
    Play,
    CloseInfo,
}

/// Any clickable element that highlights a nation.
#[derive(Component)]
struct NationRow(CountryTag);

#[derive(Component)]
struct DetailsPanel;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NationChoice>();
        app.init_resource::<InfoOpen>();
        app.add_systems(OnEnter(AppState::MainMenu), spawn_main_menu);
        app.add_systems(OnExit(AppState::MainMenu), despawn_all::<MenuRoot>);
        app.add_systems(OnEnter(AppState::NationSelect), spawn_nation_select);
        app.add_systems(OnExit(AppState::NationSelect), despawn_all::<SelectRoot>);
        app.add_systems(
            Update,
            (menu_buttons, button_hover).run_if(not(in_state(AppState::InGame))),
        );
        app.add_systems(
            Update,
            (
                nation_map_click,
                nation_row_clicks,
                refresh_details,
                apply_info_visibility,
                draw_choice_outline,
                scroll_panel,
            )
                .run_if(in_state(AppState::NationSelect)),
        );
    }
}

/// True while the cursor is over any interactive UI node — used to keep
/// map clicks and camera zoom from firing through panels.
pub fn ui_hovered(nodes: Query<&Interaction>) -> bool {
    nodes.iter().any(|i| *i != Interaction::None)
}

fn despawn_all<M: Component>(mut commands: Commands, roots: Query<Entity, With<M>>) {
    for e in &roots {
        commands.entity(e).despawn();
    }
}

fn flag_path(tag: &CountryTag) -> Option<String> {
    let p = format!("flags/{}.png", tag.0);
    std::path::Path::new("assets").join(&p).exists().then_some(p)
}

fn leader_path(tag: &CountryTag) -> Option<String> {
    for ext in ["png", "jpg", "jpeg"] {
        let p = format!("leaders/{}.{}", tag.0, ext);
        if std::path::Path::new("assets").join(&p).exists() {
            return Some(p);
        }
    }
    None
}

// --- Main menu -----------------------------------------------------------

fn spawn_main_menu(mut commands: Commands, assets: Res<AssetServer>, fonts: Res<Fonts>) {
    let mut root = commands.spawn((
        MenuRoot,
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            padding: UiRect::all(Val::Px(80.0)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.05, 0.07, 0.09)),
    ));
    root.with_children(|root| {
        if let Some(bg) = ["ui/menu_bg.jpg", "ui/menu_bg.png"]
            .iter()
            .find(|p| std::path::Path::new("assets").join(p).exists())
        {
            root.spawn((
                ImageNode::new(assets.load(bg.to_string())),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
            ));
        }
        // Soft left-to-right fade so text reads over the art without a
        // hard panel edge cutting across the painting.
        let ink = |a: f32| Color::srgba(0.03, 0.045, 0.06, a);
        let stop = |pct: f32, a: f32| bevy::ui::ColorStop {
            color: ink(a),
            point: Val::Percent(pct),
            hint: 0.5,
        };
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            bevy::ui::BackgroundGradient(vec![bevy::ui::Gradient::Linear(
                bevy::ui::LinearGradient {
                    angle: bevy::ui::LinearGradient::TO_RIGHT,
                    stops: vec![
                        stop(0.0, 0.94),
                        stop(28.0, 0.80),
                        stop(46.0, 0.45),
                        stop(66.0, 0.10),
                        stop(80.0, 0.0),
                    ],
                    ..default()
                },
            )]),
        ));
        root.spawn((
            Text::new("UNTITLED GRAND STRATEGY"),
            font(&fonts.display, 48.0),
            TextColor(TEXT_MAIN),
        ));
        root.spawn((
            Text::new("THE COLDEST WINTER - JANUARY 1950"),
            font(&fonts.body_medium, 17.0),
            TextColor(ACCENT),
            Node {
                margin: UiRect::bottom(Val::Px(48.0)),
                ..default()
            },
        ));
        for (label, action) in [("NEW GAME", MenuButton::NewGame), ("QUIT", MenuButton::Quit)] {
            root.spawn((
                Button,
                action,
                Node {
                    width: Val::Px(280.0),
                    padding: UiRect::axes(Val::Px(24.0), Val::Px(12.0)),
                    margin: UiRect::bottom(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG_LIGHT),
            ))
            .with_children(|b| {
                b.spawn((Text::new(label), font(&fonts.display, 19.0), TextColor(TEXT_MAIN)));
            });
        }
    });
}

// --- Nation select: map-first with a dossier overlay ---------------------

fn spawn_nation_select(
    mut commands: Commands,
    assets: Res<AssetServer>,
    world: Res<World1950>,
    fonts: Res<Fonts>,
    mut open: ResMut<InfoOpen>,
) {
    open.0 = false;
    let data = &world.0;

    let interesting: Vec<CountryTag> = data
        .nations_meta
        .values()
        .filter(|m| m.interesting)
        .map(|m| m.tag.clone())
        .collect();

    commands
        .spawn((
            SelectRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
        ))
        .with_children(|root| {
            // Header bar (interactive so clicks don't fall through).
            root.spawn((
                Interaction::default(),
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|h| {
                h.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|t| {
                    t.spawn((
                        Text::new("CHOOSE A NATION"),
                        font(&fonts.display, 24.0),
                        TextColor(TEXT_MAIN),
                    ));
                    t.spawn((
                        Text::new("January 1, 1950 - click a province to open its dossier"),
                        font(&fonts.body, 14.0),
                        TextColor(TEXT_DIM),
                    ));
                });
                h.spawn((
                    Button,
                    MenuButton::Back,
                    Node {
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG_LIGHT),
                ))
                .with_children(|b| {
                    b.spawn((Text::new("BACK"), font(&fonts.display, 15.0), TextColor(TEXT_DIM)));
                });
            });

            // Dossier overlay (hidden until a nation is picked). Children
            // are rebuilt by refresh_details.
            root.spawn((
                DetailsPanel,
                Interaction::default(),
                bevy::ui::ScrollPosition::default(),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(76.0),
                    right: Val::Px(16.0),
                    bottom: Val::Px(120.0),
                    width: Val::Px(640.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(20.0)),
                    row_gap: Val::Px(10.0),
                    overflow: Overflow::scroll_y(),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ));

            // Interesting picks bar.
            root.spawn((
                Interaction::default(),
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|bottom| {
                bottom.spawn((
                    Text::new("INTERESTING NATIONS"),
                    font(&fonts.body_medium, 12.0),
                    TextColor(ACCENT),
                ));
                bottom
                    .spawn(Node {
                        column_gap: Val::Px(8.0),
                        flex_wrap: FlexWrap::Wrap,
                        row_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|row| {
                        for tag in &interesting {
                            let name = data
                                .nations_meta
                                .get(tag)
                                .map(|m| m.display_name.clone())
                                .unwrap_or_else(|| tag.0.clone());
                            row.spawn((
                                Button,
                                NationRow(tag.clone()),
                                Node {
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(8.0),
                                    padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                                    ..default()
                                },
                                BackgroundColor(PANEL_BG_LIGHT),
                            ))
                            .with_children(|b| {
                                if let Some(p) = flag_path(tag) {
                                    b.spawn((
                                        ImageNode::new(assets.load(p)),
                                        Node {
                                            width: Val::Px(36.0),
                                            height: Val::Px(23.0),
                                            ..default()
                                        },
                                    ));
                                }
                                b.spawn((
                                    Text::new(name),
                                    font(&fonts.body, 13.0),
                                    TextColor(TEXT_MAIN),
                                ));
                            });
                        }
                    });
            });
        });
}

/// Left-click on the map: open the owner's dossier, or close it when
/// clicking open water.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn nation_map_click(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    geometry: Res<WorldGeometry>,
    world: Res<World1950>,
    hovered: Query<&Interaction>,
    mut choice: ResMut<NationChoice>,
    mut open: ResMut<InfoOpen>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    if hovered.iter().any(|i| *i != Interaction::None) {
        return;
    }
    match map::cursor_province(&windows, &camera, &geometry) {
        Some(id) => {
            if let Some(p) = world.0.provinces.get(&id) {
                choice.0 = Some(p.owner.clone());
                open.0 = true;
            }
        }
        None => open.0 = false,
    }
}

/// Show/hide the dossier without rebuilding it.
fn apply_info_visibility(
    open: Res<InfoOpen>,
    mut panel: Query<&mut Node, With<DetailsPanel>>,
) {
    if !open.is_changed() {
        return;
    }
    for mut node in &mut panel {
        node.display = if open.0 { Display::Flex } else { Display::None };
    }
}

/// Outline every province of the highlighted nation.
fn draw_choice_outline(
    choice: Res<NationChoice>,
    open: Res<InfoOpen>,
    world: Res<World1950>,
    geometry: Res<WorldGeometry>,
    mut gizmos: Gizmos,
) {
    if !open.0 {
        return;
    }
    let Some(tag) = &choice.0 else { return };
    let color = Color::srgb(0.98, 0.92, 0.45);
    for (id, p) in &world.0.provinces {
        if &p.owner != tag {
            continue;
        }
        let Some(rings) = geometry.rings.get(&id.0) else {
            continue;
        };
        for ring in rings {
            if ring.len() >= 2 {
                for offset in [-map::WORLD_WRAP, 0.0, map::WORLD_WRAP] {
                    let mut pts: Vec<Vec2> =
                        ring.iter().map(|v| Vec2::new(v.x + offset, v.y)).collect();
                    pts.push(pts[0]);
                    gizmos.linestrip_2d(pts, color);
                }
            }
        }
    }
}

/// Rebuild the dossier contents whenever the highlighted nation changes.
fn refresh_details(
    mut commands: Commands,
    choice: Res<NationChoice>,
    world: Res<World1950>,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    panel: Query<Entity, With<DetailsPanel>>,
) {
    if !choice.is_changed() {
        return;
    }
    let Ok(panel) = panel.single() else { return };
    let Some(tag) = choice.0.clone() else { return };
    let data = &world.0;
    let Some(country) = data.countries.get(&tag) else {
        return;
    };
    let meta = data.nations_meta.get(&tag);

    let display_name = meta
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| country.name.clone());
    let population_k: u64 = data
        .provinces
        .values()
        .filter(|p| p.owner == tag)
        .map(|p| p.population_k as u64)
        .sum();
    let province_count = data.provinces.values().filter(|p| p.owner == tag).count();
    let capital = data
        .provinces
        .get(&country.capital)
        .map(|p| p.name.clone())
        .unwrap_or_default();

    commands.entity(panel).despawn_related::<Children>();
    commands.entity(panel).with_children(|d| {
        // Title row with close button.
        d.spawn(Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::FlexStart,
            column_gap: Val::Px(12.0),
            ..default()
        })
        .with_children(|top| {
            top.spawn((
                Text::new(display_name),
                font(&fonts.display, 28.0),
                TextColor(TEXT_MAIN),
            ));
            top.spawn((
                Button,
                MenuButton::CloseInfo,
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(PANEL_BG_LIGHT),
            ))
            .with_children(|b| {
                b.spawn((Text::new("X"), font(&fonts.body_medium, 16.0), TextColor(TEXT_DIM)));
            });
        });

        // Flag + government/leader + portrait.
        d.spawn(Node {
            column_gap: Val::Px(16.0),
            align_items: AlignItems::FlexStart,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        })
        .with_children(|h| {
            h.spawn(Node {
                column_gap: Val::Px(16.0),
                align_items: AlignItems::FlexStart,
                ..default()
            })
            .with_children(|left| {
                if let Some(p) = flag_path(&tag) {
                    left.spawn((
                        ImageNode::new(assets.load(p)),
                        Node {
                            width: Val::Px(132.0),
                            height: Val::Px(84.0),
                            flex_shrink: 0.0,
                            ..default()
                        },
                    ));
                }
                left.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|t| {
                    if let Some(m) = meta {
                        t.spawn((
                            Text::new(m.government.clone()),
                            font(&fonts.body_medium, 16.0),
                            TextColor(ACCENT),
                        ));
                        t.spawn((
                            Text::new(m.leader_title.clone()),
                            font(&fonts.body, 13.0),
                            TextColor(TEXT_DIM),
                        ));
                        t.spawn((
                            Text::new(m.leader_name.clone()),
                            font(&fonts.body_medium, 18.0),
                            TextColor(TEXT_MAIN),
                        ));
                    }
                });
            });
            if let Some(p) = leader_path(&tag) {
                h.spawn((
                    ImageNode::new(assets.load(p)),
                    Node {
                        width: Val::Px(100.0),
                        height: Val::Px(128.0),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ));
            }
        });

        // Stats strip.
        let alignment = match country.alignment {
            ugs_data::Alignment::WesternBloc => "Western".to_string(),
            ugs_data::Alignment::EasternBloc => "Eastern".to_string(),
            ugs_data::Alignment::NonAligned => "Non-Aligned".to_string(),
        };
        let stats = [
            ("POPULATION", format!("{:.1}M", population_k as f64 / 1000.0)),
            ("INDUSTRY", country.industry.to_string()),
            ("STABILITY", format!("{}%", country.stability)),
            ("PROVINCES", province_count.to_string()),
            ("CAPITAL", capital),
            ("BLOC", alignment),
            (
                "NUCLEAR",
                if country.nuclear_power { "YES".into() } else { "no".into() },
            ),
        ];
        d.spawn(Node {
            column_gap: Val::Px(22.0),
            flex_wrap: FlexWrap::Wrap,
            row_gap: Val::Px(6.0),
            padding: UiRect::axes(Val::Px(0.0), Val::Px(4.0)),
            ..default()
        })
        .with_children(|s| {
            for (label, value) in stats {
                s.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|col| {
                    col.spawn((Text::new(label), font(&fonts.body_medium, 11.0), TextColor(TEXT_DIM)));
                    col.spawn((Text::new(value), font(&fonts.body, 17.0), TextColor(TEXT_MAIN)));
                });
            }
        });

        // Situation dossier (typewriter).
        if let Some(m) = meta {
            d.spawn((
                Text::new(m.situation.clone()),
                font(&fonts.mono, 13.5),
                TextColor(TEXT_MAIN),
            ));
            d.spawn((
                Text::new(format!("\"{}\"", m.hook)),
                font(&fonts.mono_bold, 13.5),
                TextColor(ACCENT),
            ));
        } else {
            d.spawn((
                Text::new("No dossier compiled for this nation yet."),
                font(&fonts.mono, 13.5),
                TextColor(TEXT_DIM),
            ));
        }

        d.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        d.spawn((
            Button,
            MenuButton::Play,
            Node {
                align_self: AlignSelf::FlexEnd,
                padding: UiRect::axes(Val::Px(38.0), Val::Px(12.0)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::srgb(0.55, 0.44, 0.18)),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new("PLAY"),
                font(&fonts.display, 20.0),
                TextColor(Color::srgb(0.98, 0.95, 0.88)),
            ));
        });
    });
}

// --- Interactions --------------------------------------------------------

/// Interesting-picks buttons: highlight, open, and fly the camera there.
fn nation_row_clicks(
    rows: Query<(&Interaction, &NationRow), Changed<Interaction>>,
    world: Res<World1950>,
    mut choice: ResMut<NationChoice>,
    mut open: ResMut<InfoOpen>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    for (interaction, row) in &rows {
        if *interaction != Interaction::Pressed {
            continue;
        }
        choice.0 = Some(row.0.clone());
        open.0 = true;
        if let Some(capital) = world
            .0
            .countries
            .get(&row.0)
            .and_then(|c| world.0.provinces.get(&c.capital))
        {
            if let Ok(mut transform) = camera.single_mut() {
                let center = map::project(capital.center.0, capital.center.1);
                transform.translation.x = center.x;
                transform.translation.y = center.y;
            }
        }
    }
}

fn menu_buttons(
    buttons: Query<(&Interaction, &MenuButton), Changed<Interaction>>,
    choice: Res<NationChoice>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut open: ResMut<InfoOpen>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            MenuButton::NewGame => next.set(AppState::NationSelect),
            MenuButton::Quit => {
                exit.write(AppExit::Success);
            }
            MenuButton::Back => next.set(AppState::MainMenu),
            MenuButton::CloseInfo => open.0 = false,
            MenuButton::Play => {
                if let Some(tag) = choice.0.clone() {
                    commands.insert_resource(PlayerNation(tag));
                    next.set(AppState::InGame);
                }
            }
        }
    }
}

/// Hover feedback on any button.
#[allow(clippy::type_complexity)] // Bevy query tuples are what they are
fn button_hover(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor, Option<&MenuButton>),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, mut bg, menu_button) in &mut buttons {
        let is_play = menu_button == Some(&MenuButton::Play);
        bg.0 = match (interaction, is_play) {
            (Interaction::Hovered, true) => Color::srgb(0.68, 0.55, 0.24),
            (Interaction::Pressed, true) | (Interaction::None, true) => {
                Color::srgb(0.55, 0.44, 0.18)
            }
            (Interaction::Hovered, false) => Color::srgba(0.22, 0.26, 0.32, 0.95),
            (Interaction::Pressed, false) => Color::srgba(0.30, 0.34, 0.40, 0.95),
            (Interaction::None, false) => PANEL_BG_LIGHT,
        };
    }
}

/// Mouse-wheel scrolling for the dossier while hovering it (the camera
/// zoom is suppressed over UI, so the wheel is free here).
fn scroll_panel(
    mut wheel: MessageReader<MouseWheel>,
    mut panel: Query<(&Interaction, &mut ScrollPosition), With<DetailsPanel>>,
) {
    let delta: f32 = wheel.read().map(|e| e.y).sum();
    if delta == 0.0 {
        return;
    }
    for (interaction, mut pos) in &mut panel {
        if *interaction != Interaction::None {
            pos.y = (pos.y - delta * 36.0).max(0.0);
        }
    }
}
