//! The economy panel (toggle: E). The first-hour difference between the
//! two systems lives here: planned economies get quota rows, market
//! economies get policy levers — same skeleton, different verbs.

use bevy::prelude::*;
use ugs_sim::{
    agriculture::{Agriculture, Quota},
    command::{PendingCommands, SimCommand},
    demography::LivingStandards,
    planning::{EconomicSystem, Economies, Policy, Procurement},
};

use crate::{font, AppState, Fonts, PlayerNation, World1950};

const BG: Color = Color::srgba(0.07, 0.09, 0.12, 0.96);
const BG_LIGHT: Color = Color::srgba(0.14, 0.17, 0.21, 0.95);
const ACCENT: Color = Color::srgb(0.83, 0.69, 0.36);
const DIM: Color = Color::srgb(0.62, 0.66, 0.70);
const MAIN: Color = Color::srgb(0.88, 0.89, 0.90);

#[derive(Resource, Default)]
struct PanelOpen(bool);

#[derive(Component)]
struct EconPanel;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum EconButton {
    InvestUp,
    InvestDown,
    MilUp,
    MilDown,
    RateUp,
    RateDown,
    TaxUp,
    TaxDown,
    ProcCycle,
    QuotaCycle,
    Collectivize,
}

pub struct EconUiPlugin;

impl Plugin for EconUiPlugin {
    fn build(&self, app: &mut App) {
        // Dev shortcut: UGS_PANEL=econ boots with the panel open.
        app.insert_resource(PanelOpen(
            std::env::var("UGS_PANEL").as_deref() == Ok("econ"),
        ));
        app.add_systems(OnEnter(AppState::InGame), spawn_panel);
        app.add_systems(
            Update,
            (toggle_panel, econ_buttons, refresh_panel)
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn spawn_panel(mut commands: Commands, open: Res<PanelOpen>, existing: Query<(), With<EconPanel>>) {
    if !existing.is_empty() {
        return;
    }
    let display = if open.0 { Display::Flex } else { Display::None };
    commands.spawn((
        EconPanel,
        Interaction::default(),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(56.0),
            right: Val::Px(12.0),
            width: Val::Px(320.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(14.0)),
            display,
            ..default()
        },
        BackgroundColor(BG),
    ));
}

fn toggle_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut open: ResMut<PanelOpen>,
    mut panel: Query<&mut Node, With<EconPanel>>,
) {
    if keys.just_pressed(KeyCode::KeyE) {
        open.0 = !open.0;
        for mut node in &mut panel {
            node.display = if open.0 { Display::Flex } else { Display::None };
        }
    }
}

/// Apply lever clicks by pushing commands for the player nation.
fn econ_buttons(
    buttons: Query<(&Interaction, &EconButton), Changed<Interaction>>,
    player: Option<Res<PlayerNation>>,
    econ: Res<Economies>,
    agri: Res<Agriculture>,
    mut pending: ResMut<PendingCommands>,
) {
    let Some(player) = player else { return };
    let tag = &player.0;
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        // Agriculture controls (planned only).
        if matches!(button, EconButton::QuotaCycle | EconButton::Collectivize) {
            let policy = agri.policy.get(tag).copied().unwrap_or_default();
            let (collectivized, quota) = match button {
                EconButton::QuotaCycle => (
                    policy.collectivized,
                    match policy.quota {
                        Quota::Low => Quota::Normal,
                        Quota::Normal => Quota::High,
                        Quota::High => Quota::Low,
                    },
                ),
                _ => (true, policy.quota),
            };
            pending.push(SimCommand::SetAgriPolicy {
                country: tag.clone(),
                collectivized,
                quota,
            });
            continue;
        }
        match econ.policy.get(tag) {
            Some(Policy::Planned {
                consumer: _,
                investment,
                military,
            }) => {
                let (mut invest, mut mil) = (*investment as i32, *military as i32);
                match button {
                    EconButton::InvestUp => invest += 50,
                    EconButton::InvestDown => invest -= 50,
                    EconButton::MilUp => mil += 50,
                    EconButton::MilDown => mil -= 50,
                    _ => continue,
                }
                let invest = invest.clamp(100, 700) as u16;
                let mil = mil.clamp(50, 500) as u16;
                let consumer = 1000u16.saturating_sub(invest + mil);
                if consumer < 100 {
                    continue; // never squeeze households below 10%
                }
                pending.push(SimCommand::SetPlannedAllocation {
                    country: tag.clone(),
                    consumer,
                    investment: invest,
                    military: mil,
                });
            }
            Some(Policy::Market {
                interest_bp,
                tax_permille,
                procurement,
            }) => {
                let (mut rate, mut tax, mut proc) =
                    (*interest_bp as i32, *tax_permille as i32, *procurement);
                match button {
                    EconButton::RateUp => rate += 50,
                    EconButton::RateDown => rate -= 50,
                    EconButton::TaxUp => tax += 25,
                    EconButton::TaxDown => tax -= 25,
                    EconButton::ProcCycle => {
                        proc = match proc {
                            Procurement::Low => Procurement::Med,
                            Procurement::Med => Procurement::High,
                            Procurement::High => Procurement::Low,
                        }
                    }
                    _ => continue,
                }
                pending.push(SimCommand::SetMarketPolicy {
                    country: tag.clone(),
                    interest_bp: rate.clamp(50, 1200) as u16,
                    tax_permille: tax.clamp(50, 600) as u16,
                    procurement: proc,
                });
            }
            None => {}
        }
    }
}

/// Rebuild panel contents when opened, monthly, or after policy changes.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn refresh_panel(
    mut commands: Commands,
    open: Res<PanelOpen>,
    econ: Res<Economies>,
    agri: Res<Agriculture>,
    sol: Res<LivingStandards>,
    world: Res<World1950>,
    fonts: Res<Fonts>,
    player: Option<Res<PlayerNation>>,
    panel: Query<Entity, With<EconPanel>>,
) {
    if !open.0 || (!open.is_changed() && !econ.is_changed() && !agri.is_changed()) {
        return;
    }
    let Ok(panel) = panel.single() else { return };
    commands.entity(panel).despawn_related::<Children>();

    let Some(player) = player else {
        commands.entity(panel).with_children(|p| {
            p.spawn((
                Text::new("OBSERVER - no nation"),
                font(&fonts.display, 16.0),
                TextColor(DIM),
            ));
        });
        return;
    };
    let tag = player.0.clone();
    let system = econ.system.get(&tag).copied();
    let Some(st) = econ.industry.get(&tag).copied() else {
        return;
    };
    let policy = econ.policy.get(&tag).copied();
    let dashboard = econ.dashboard_industry_centi(&tag);
    let sol_value = sol.by_country.get(&tag).copied().unwrap_or(0);
    let name = world
        .0
        .nations_meta
        .get(&tag)
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| tag.0.clone());

    commands.entity(panel).with_children(|p| {
        let title = match system {
            Some(EconomicSystem::Planned) => "STATE PLANNING",
            Some(EconomicSystem::Market) => "ECONOMIC POLICY",
            None => "ECONOMY",
        };
        p.spawn((
            Text::new(title),
            font(&fonts.display, 18.0),
            TextColor(MAIN),
        ));
        p.spawn((Text::new(name), font(&fonts.body, 12.0), TextColor(DIM)));

        // Stat rows.
        let growth_pct =
            st.last_growth_centi as f64 * 12.0 / (st.actual_centi.max(1)) as f64 * 100.0;
        let mut stats: Vec<(&str, String)> = vec![
            ("INDUSTRY", format!("{:.1}", dashboard as f64 / 100.0)),
            ("GROWTH /yr", format!("{growth_pct:+.1}%")),
            ("LIVING STD", format!("{sol_value}")),
            ("MIL STOCK", format!("{}", st.military_stock)),
        ];
        if system == Some(EconomicSystem::Market) {
            stats.push(("INFLATION", format!("{:.1}%", st.inflation as f64 / 10.0)));
        }
        if system == Some(EconomicSystem::Planned) {
            stats.push(("", "(figures as reported by Gosplan)".into()));
        }
        for (label, value) in stats {
            p.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(label),
                    font(&fonts.body_medium, 12.0),
                    TextColor(DIM),
                ));
                row.spawn((Text::new(value), font(&fonts.mono, 13.0), TextColor(MAIN)));
            });
        }

        // Control rows per system.
        let lever = |p: &mut bevy::prelude::ChildSpawnerCommands,
                     fonts: &Fonts,
                     label: &str,
                     value: String,
                     down: EconButton,
                     up: EconButton,
                     tip: &str| {
            p.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(label.to_string()),
                    font(&fonts.body_medium, 13.0),
                    TextColor(ACCENT),
                    Interaction::default(),
                    crate::widgets::Tooltip::of(tip),
                ));
                row.spawn(Node {
                    column_gap: Val::Px(6.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|c| {
                    for (b, txt) in [(down, "-"), (up, "+")] {
                        if txt == "-" {
                            c.spawn((
                                Text::new(value.clone()),
                                font(&fonts.mono, 14.0),
                                TextColor(MAIN),
                            ));
                        }
                        c.spawn((
                            Button,
                            b,
                            Node {
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(BG_LIGHT),
                        ))
                        .with_children(|bb| {
                            bb.spawn((
                                Text::new(txt.to_string()),
                                font(&fonts.body_medium, 15.0),
                                TextColor(MAIN),
                            ));
                        });
                    }
                });
            });
        };

        match policy {
            Some(Policy::Planned {
                consumer,
                investment,
                military,
            }) => {
                lever(
                    p,
                    &fonts,
                    "INVESTMENT",
                    format!("{}%", investment / 10),
                    EconButton::InvestDown,
                    EconButton::InvestUp,
                    "Share of output plowed back into industry: compounds into growth, at the cost of consumer goods (stability) and the military share.",
                );
                lever(
                    p,
                    &fonts,
                    "MILITARY",
                    format!("{}%", military / 10),
                    EconButton::MilDown,
                    EconButton::MilUp,
                    "Share of output into the military stockpile - the currency that raises divisions and pays their upkeep (see the war room's FORCES tab). Guns, at the price of butter.",
                );
                p.spawn((
                    Text::new(format!("CONSUMER GOODS  {}%  (remainder)", consumer / 10)),
                    font(&fonts.body, 12.0),
                    TextColor(DIM),
                ));
            }
            Some(Policy::Market {
                interest_bp,
                tax_permille,
                procurement,
            }) => {
                lever(
                    p,
                    &fonts,
                    "INTEREST",
                    format!("{:.2}%", interest_bp as f64 / 100.0),
                    EconButton::RateDown,
                    EconButton::RateUp,
                    "The central bank rate: low runs the economy hot (growth, inflation risk), high cools it. Markets steer with prices, not quotas.",
                );
                lever(
                    p,
                    &fonts,
                    "TAXES",
                    format!("{:.1}%", tax_permille as f64 / 10.0),
                    EconButton::TaxDown,
                    EconButton::TaxUp,
                    "Tax take: funds procurement without inflation, but squeezes consumption and growth.",
                );
                p.spawn(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("PROCUREMENT"),
                        font(&fonts.body_medium, 13.0),
                        TextColor(ACCENT),
                    ));
                    row.spawn((
                        Button,
                        EconButton::ProcCycle,
                        Node {
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(BG_LIGHT),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(format!("{procurement:?}")),
                            font(&fonts.mono, 13.0),
                            TextColor(MAIN),
                        ));
                    });
                });
                p.spawn((
                    Text::new("firms set output; you set the terms"),
                    font(&fonts.body, 11.0),
                    TextColor(DIM),
                ));
            }
            None => {}
        }

        // Food section.
        if let Some(status) = agri.status.get(&tag).copied() {
            let policy = agri.policy.get(&tag).copied().unwrap_or_default();
            p.spawn((
                Text::new(format!(
                    "FOOD {}%   HARVEST {}%",
                    status.food_ratio_permille / 10,
                    status.harvest_permille / 10
                )),
                font(&fonts.body_medium, 12.0),
                TextColor(if status.famine {
                    Color::srgb(0.9, 0.35, 0.3)
                } else {
                    DIM
                }),
            ));
            if status.famine {
                p.spawn((
                    Text::new(format!("FAMINE - {} dead", status.famine_deaths)),
                    font(&fonts.mono_bold, 12.0),
                    TextColor(Color::srgb(0.9, 0.35, 0.3)),
                ));
            }
            if system == Some(EconomicSystem::Planned) {
                p.spawn(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("GRAIN QUOTA"),
                        font(&fonts.body_medium, 13.0),
                        TextColor(ACCENT),
                    ));
                    row.spawn((
                        Button,
                        EconButton::QuotaCycle,
                        crate::widgets::Tooltip::of(
                            "Grain procurement quota (click to cycle Low/Medium/High): high quotas feed the cities and exports now, and starve the countryside's incentive to plant next year.",
                        ),
                        Node {
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(BG_LIGHT),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(format!("{:?}", policy.quota)),
                            font(&fonts.mono, 13.0),
                            TextColor(MAIN),
                        ));
                    });
                });
                p.spawn(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("AGRICULTURE"),
                        font(&fonts.body_medium, 13.0),
                        TextColor(ACCENT),
                    ));
                    if policy.collectivized {
                        let label = if policy.shock_months > 0 {
                            format!("COLLECTIVIZED ({}mo shock)", policy.shock_months)
                        } else {
                            "COLLECTIVIZED".to_string()
                        };
                        row.spawn((Text::new(label), font(&fonts.mono, 12.0), TextColor(MAIN)));
                    } else {
                        row.spawn((
                            Button,
                            EconButton::Collectivize,
                            crate::widgets::Tooltip::of(
                                "Collectivize agriculture: one-way, with a multi-month shock to output while the countryside is reorganized (and worse). Trades rural welfare for state control of the harvest.",
                            ),
                            Node {
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.45, 0.2, 0.15)),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                Text::new("COLLECTIVIZE"),
                                font(&fonts.body_medium, 12.0),
                                TextColor(Color::srgb(0.95, 0.85, 0.8)),
                            ));
                        });
                    }
                });
            }
        }
    });
}
