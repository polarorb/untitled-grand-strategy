//! Main menu and the choose-a-nation screen.
//!
//! Nation metadata (leader, government, situation text) comes from
//! `ScenarioData::nations_meta`; flags and portraits are loose files at
//! `assets/flags/<TAG>.*` / `assets/leaders/<TAG>.*` and every image is
//! optional — the screen degrades to text when an asset is missing.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use ugs_data::CountryTag;

use crate::{AppState, PlayerNation, World1950};

const PANEL_BG: Color = Color::srgba(0.08, 0.10, 0.13, 0.92);
const PANEL_BG_LIGHT: Color = Color::srgba(0.14, 0.17, 0.21, 0.95);
const ACCENT: Color = Color::srgb(0.83, 0.69, 0.36); // brass
const TEXT_DIM: Color = Color::srgb(0.62, 0.66, 0.70);
const TEXT_MAIN: Color = Color::srgb(0.88, 0.89, 0.90);

/// Nation highlighted in the select screen (not yet confirmed with Play).
#[derive(Resource, Default)]
struct NationChoice(Option<CountryTag>);

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
}

/// Any clickable element that highlights a nation.
#[derive(Component)]
struct NationRow(CountryTag);

#[derive(Component)]
struct DetailsPanel;

#[derive(Component)]
struct NationList;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NationChoice>();
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
            (nation_row_clicks, refresh_details, scroll_list)
                .run_if(in_state(AppState::NationSelect)),
        );
    }
}

fn despawn_all<M: Component>(mut commands: Commands, roots: Query<Entity, With<M>>) {
    for e in &roots {
        commands.entity(e).despawn();
    }
}

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: bevy::text::FontSize::Px(size),
        ..default()
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

fn spawn_main_menu(mut commands: Commands, assets: Res<AssetServer>) {
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
        // Left-side smoked glass column so text reads over the art.
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(560.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.05, 0.07, 0.78)),
        ));
        root.spawn((
            Text::new("UNTITLED GRAND STRATEGY"),
            font(44.0),
            TextColor(TEXT_MAIN),
        ));
        root.spawn((
            Text::new("The Coldest Winter  -  January 1950"),
            font(19.0),
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
                b.spawn((Text::new(label), font(20.0), TextColor(TEXT_MAIN)));
            });
        }
    });
}

// --- Nation select -------------------------------------------------------

fn spawn_nation_select(
    mut commands: Commands,
    assets: Res<AssetServer>,
    world: Res<World1950>,
    mut choice: ResMut<NationChoice>,
) {
    if choice.0.is_none() {
        choice.0 = Some(CountryTag("USA".into()));
    }
    let data = &world.0;

    // All nations sorted by display name.
    let mut nations: Vec<(CountryTag, String)> = data
        .countries
        .keys()
        .map(|tag| {
            let name = data
                .nations_meta
                .get(tag)
                .map(|m| m.display_name.clone())
                .unwrap_or_else(|| data.countries[tag].name.clone());
            (tag.clone(), name)
        })
        .collect();
    nations.sort_by(|a, b| a.1.cmp(&b.1));

    let interesting: Vec<CountryTag> = nations
        .iter()
        .filter(|(tag, _)| data.nations_meta.get(tag).is_some_and(|m| m.interesting))
        .map(|(tag, _)| tag.clone())
        .collect();

    commands
        .spawn((
            SelectRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(16.0)),
                row_gap: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.07, 0.09)),
        ))
        .with_children(|root| {
            // Header
            root.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|h| {
                h.spawn((
                    Text::new("CHOOSE A NATION - JANUARY 1, 1950"),
                    font(26.0),
                    TextColor(TEXT_MAIN),
                ));
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
                    b.spawn((Text::new("BACK"), font(16.0), TextColor(TEXT_DIM)));
                });
            });

            // Middle: list + details
            root.spawn(Node {
                flex_grow: 1.0,
                column_gap: Val::Px(12.0),
                min_height: Val::Px(0.0),
                ..default()
            })
            .with_children(|mid| {
                // Scrollable nation list
                mid.spawn((
                    NationList,
                    Node {
                        width: Val::Px(300.0),
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                ))
                .with_children(|list| {
                    for (tag, name) in &nations {
                        list.spawn((
                            Button,
                            NationRow(tag.clone()),
                            Node {
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(8.0),
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|row| {
                            if let Some(p) = flag_path(tag) {
                                row.spawn((
                                    ImageNode::new(assets.load(p)),
                                    Node {
                                        width: Val::Px(34.0),
                                        height: Val::Px(22.0),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                ));
                            }
                            row.spawn((Text::new(name.clone()), font(15.0), TextColor(TEXT_MAIN)));
                        });
                    }
                });

                // Details panel, rebuilt whenever the choice changes.
                mid.spawn((
                    DetailsPanel,
                    Node {
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(20.0)),
                        row_gap: Val::Px(10.0),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                ));
            });

            // Interesting picks
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|bottom| {
                bottom.spawn((
                    Text::new("INTERESTING NATIONS"),
                    font(13.0),
                    TextColor(ACCENT),
                ));
                bottom
                    .spawn(Node {
                        column_gap: Val::Px(10.0),
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
                                    flex_direction: FlexDirection::Column,
                                    align_items: AlignItems::Center,
                                    row_gap: Val::Px(3.0),
                                    padding: UiRect::all(Val::Px(6.0)),
                                    ..default()
                                },
                                BackgroundColor(PANEL_BG_LIGHT),
                            ))
                            .with_children(|b| {
                                if let Some(p) = flag_path(tag) {
                                    b.spawn((
                                        ImageNode::new(assets.load(p)),
                                        Node {
                                            width: Val::Px(56.0),
                                            height: Val::Px(36.0),
                                            ..default()
                                        },
                                    ));
                                }
                                b.spawn((Text::new(name), font(12.0), TextColor(TEXT_DIM)));
                            });
                        }
                    });
            });
        });
}

/// Rebuild the details panel whenever the highlighted nation changes.
fn refresh_details(
    mut commands: Commands,
    choice: Res<NationChoice>,
    world: Res<World1950>,
    assets: Res<AssetServer>,
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
        // Header row: flag, names, portrait
        d.spawn(Node {
            column_gap: Val::Px(18.0),
            align_items: AlignItems::FlexStart,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        })
        .with_children(|h| {
            h.spawn(Node {
                column_gap: Val::Px(18.0),
                align_items: AlignItems::FlexStart,
                ..default()
            })
            .with_children(|left| {
                if let Some(p) = flag_path(&tag) {
                    left.spawn((
                        ImageNode::new(assets.load(p)),
                        Node {
                            width: Val::Px(150.0),
                            height: Val::Px(96.0),
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
                    t.spawn((Text::new(display_name), font(30.0), TextColor(TEXT_MAIN)));
                    if let Some(m) = meta {
                        t.spawn((Text::new(m.government.clone()), font(16.0), TextColor(ACCENT)));
                        t.spawn((
                            Text::new(format!("{}  {}", m.leader_title, m.leader_name)),
                            font(16.0),
                            TextColor(TEXT_DIM),
                        ));
                    }
                });
            });
            if let Some(p) = leader_path(&tag) {
                h.spawn((
                    ImageNode::new(assets.load(p)),
                    Node {
                        width: Val::Px(110.0),
                        height: Val::Px(140.0),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ));
            }
        });

        // Stats strip
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
            column_gap: Val::Px(26.0),
            flex_wrap: FlexWrap::Wrap,
            padding: UiRect::axes(Val::Px(0.0), Val::Px(6.0)),
            ..default()
        })
        .with_children(|s| {
            for (label, value) in stats {
                s.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|col| {
                    col.spawn((Text::new(label), font(11.0), TextColor(TEXT_DIM)));
                    col.spawn((Text::new(value), font(18.0), TextColor(TEXT_MAIN)));
                });
            }
        });

        // Situation text
        if let Some(m) = meta {
            d.spawn((
                Text::new(m.situation.clone()),
                font(15.0),
                TextColor(TEXT_MAIN),
                Node {
                    max_width: Val::Px(760.0),
                    ..default()
                },
            ));
            d.spawn((
                Text::new(format!("\"{}\"", m.hook)),
                font(15.0),
                TextColor(ACCENT),
            ));
        } else {
            d.spawn((
                Text::new("No dossier compiled for this nation yet."),
                font(15.0),
                TextColor(TEXT_DIM),
            ));
        }

        // Play button
        d.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        d.spawn((
            Button,
            MenuButton::Play,
            Node {
                align_self: AlignSelf::FlexEnd,
                padding: UiRect::axes(Val::Px(38.0), Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.55, 0.44, 0.18)),
        ))
        .with_children(|b| {
            b.spawn((Text::new("PLAY"), font(22.0), TextColor(Color::srgb(0.98, 0.95, 0.88))));
        });
    });
}

// --- Interactions --------------------------------------------------------

fn nation_row_clicks(
    rows: Query<(&Interaction, &NationRow), Changed<Interaction>>,
    mut choice: ResMut<NationChoice>,
) {
    for (interaction, row) in &rows {
        if *interaction == Interaction::Pressed {
            choice.0 = Some(row.0.clone());
        }
    }
}

fn menu_buttons(
    buttons: Query<(&Interaction, &MenuButton), Changed<Interaction>>,
    choice: Res<NationChoice>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
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
            (Interaction::None, false) => {
                if menu_button.is_some() {
                    PANEL_BG_LIGHT
                } else {
                    Color::NONE
                }
            }
        };
    }
}

/// Mouse-wheel scrolling for the nation list (and details panel overflow).
fn scroll_list(
    mut wheel: MessageReader<MouseWheel>,
    mut lists: Query<&mut ScrollPosition, With<NationList>>,
) {
    let delta: f32 = wheel.read().map(|e| e.y).sum();
    if delta == 0.0 {
        return;
    }
    for mut pos in &mut lists {
        pos.y = (pos.y - delta * 36.0).max(0.0);
    }
}
