//! The Intelligence panel (I): per-rival coverage cards in the dossier
//! style, plus the network / counterintel / operation controls. Never
//! prints raw permille — only period coverage grades and bands.

use bevy::prelude::*;
use ugs_sim::command::{PendingCommands, SimCommand};
use ugs_sim::intel::{Domain, Intel, OpKind};
use ugs_sim::planning::Economies;

use crate::{font, AppState, Fonts, PlayerNation, World1950};

const PANEL_BG: Color = Color::srgba(0.07, 0.09, 0.12, 0.97);
const ACCENT: Color = Color::srgb(0.83, 0.69, 0.36);
const MAIN: Color = Color::srgb(0.88, 0.89, 0.90);
const DIM: Color = Color::srgb(0.62, 0.66, 0.70);

#[derive(Component)]
struct IntelPanel;

#[derive(Component, Clone)]
enum IntelButton {
    Fund(ugs_data::CountryTag, u8),
    Counterintel(u8),
    Op(ugs_data::CountryTag, OpKind),
}

pub struct IntelUiPlugin;

impl Plugin for IntelUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (toggle_panel, refresh_panel, intel_buttons).run_if(in_state(AppState::InGame)),
        );
    }
}

/// I toggles the panel. Dev shortcut: UGS_PANEL=intel boots open.
fn toggle_panel(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    panel: Query<Entity, With<IntelPanel>>,
    mut booted: Local<bool>,
) {
    let auto_open = !*booted && std::env::var("UGS_PANEL").as_deref() == Ok("intel");
    *booted = true;
    if !keys.just_pressed(KeyCode::KeyI) && !auto_open {
        return;
    }
    if let Ok(e) = panel.single() {
        commands.entity(e).despawn();
    } else {
        commands.spawn((
            IntelPanel,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(56.0),
                width: Val::Px(400.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ));
    }
}

fn grade(permille: u32) -> &'static str {
    match permille {
        0..=249 => "NEGLIGIBLE",
        250..=499 => "LIMITED",
        500..=749 => "PARTIAL",
        _ => "EXTENSIVE",
    }
}

/// Rivals worth showing: the great powers plus anyone the player runs a
/// network against.
fn rivals(
    world: &World1950,
    intel: &Intel,
    me: &ugs_data::CountryTag,
) -> Vec<ugs_data::CountryTag> {
    let mut v: Vec<ugs_data::CountryTag> = ["SOV", "USA", "PRC", "GBR"]
        .iter()
        .map(|t| ugs_data::CountryTag(t.to_string()))
        .filter(|t| t != me && world.0.countries.contains_key(t))
        .collect();
    for (owner, target) in intel.networks.keys() {
        if owner == me && !v.contains(target) {
            v.push(target.clone());
        }
    }
    v
}

#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn refresh_panel(
    mut commands: Commands,
    intel: Res<Intel>,
    economies: Res<Economies>,
    world: Res<World1950>,
    fonts: Res<Fonts>,
    player: Option<Res<PlayerNation>>,
    panel: Query<Entity, Added<IntelPanel>>,
    panel_any: Query<Entity, With<IntelPanel>>,
) {
    let rebuild = !panel.is_empty() || intel.is_changed();
    if !rebuild {
        return;
    }
    let Ok(panel) = panel_any.single() else { return };
    commands.entity(panel).despawn_related::<Children>();
    let Some(me) = player.as_ref().map(|p| p.0.clone()) else {
        commands.entity(panel).with_children(|p| {
            p.spawn((
                Text::new("CENTRAL INTELLIGENCE -- OBSERVER"),
                font(&fonts.display, 16.0),
                TextColor(DIM),
            ));
        });
        return;
    };
    let ci = intel.ci_level(&me);
    let deniability = intel.deniability_of(&me);
    commands.entity(panel).with_children(|p| {
        p.spawn((
            Text::new("CENTRAL INTELLIGENCE -- EYES ONLY"),
            font(&fonts.display, 16.0),
            TextColor(ACCENT),
        ));
        p.spawn((
            Text::new(format!(
                "COUNTERINTEL LEVEL {ci}    DENIABILITY {deniability}/100"
            )),
            font(&fonts.mono, 11.0),
            TextColor(DIM),
        ));
        // Counterintel funding row.
        p.spawn(Node {
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            for level in 0u8..=3 {
                row.spawn((
                    Button,
                    IntelButton::Counterintel(level),
                    crate::widgets::Tooltip::of(
                        "Counterintelligence funding 0-3: degrades and helps roll up foreign networks inside your borders, and slows what rivals learn about you. The lit level is current.",
                    ),
                    Node {
                        padding: UiRect::axes(Val::Px(9.0), Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(if ci == level {
                        Color::srgb(0.55, 0.44, 0.18)
                    } else {
                        Color::srgba(0.14, 0.17, 0.21, 0.95)
                    }),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(format!("CI {level}")),
                        font(&fonts.display, 12.0),
                        TextColor(MAIN),
                    ));
                });
            }
        });

        for rival in rivals(&world, &intel, &me) {
            let name = world
                .0
                .nations_meta
                .get(&rival)
                .map(|m| m.display_name.to_uppercase())
                .unwrap_or_else(|| rival.0.clone());
            let pen = intel.penetration_of(&me, &rival);
            let net = intel.networks.get(&(me.clone(), rival.clone())).cloned();
            let funding = net.as_ref().map(|n| n.funding).unwrap_or(0);
            let strength = net.as_ref().map(|n| n.strength).unwrap_or(0);
            p.spawn((
                Text::new(format!("-- {name} --")),
                font(&fonts.mono_bold, 12.0),
                TextColor(ACCENT),
            ));
            p.spawn((
                Text::new(format!(
                    "NUCLEAR {}  MILITARY {}",
                    grade(pen.nuclear),
                    grade(pen.military)
                )),
                font(&fonts.mono, 11.0),
                TextColor(MAIN),
            ));
            p.spawn((
                Text::new(format!(
                    "ECONOMIC {}  POLITICAL {}",
                    grade(pen.economic),
                    grade(pen.political)
                )),
                font(&fonts.mono, 11.0),
                TextColor(MAIN),
            ));
            // Economic coupling made visible: their industry as WE see
            // it (reported at zero intel, sliding toward truth). The
            // real figure only appears with economic penetration.
            let observed = observed_industry(&economies, &intel, &me, &rival);
            if observed > 0 {
                let flag = if pen.economic >= 550 {
                    "TRUE"
                } else if pen.economic >= 400 {
                    "FIGURES SUSPECT"
                } else {
                    "AS REPORTED"
                };
                p.spawn((
                    Text::new(format!(
                        "INDUSTRY EST {:.0} ({flag})",
                        observed as f64 / 100.0
                    )),
                    font(&fonts.mono, 10.5),
                    TextColor(MAIN),
                ));
            }
            p.spawn((
                Text::new(format!("NETWORK: FUNDING {funding}/3, STRENGTH {strength}/100")),
                font(&fonts.mono, 10.5),
                TextColor(DIM),
            ));
            // Funding buttons.
            p.spawn(Node {
                column_gap: Val::Px(5.0),
                ..default()
            })
            .with_children(|row| {
                for level in 0u8..=3 {
                    row.spawn((
                        Button,
                        IntelButton::Fund(rival.clone(), level),
                        crate::widgets::Tooltip::of(
                            "Collection network funding 0-3 against this country: builds penetration over months, sharpening every estimate you see about them (arsenal, army, intentions). The lit level is current; higher levels risk more on exposure.",
                        ),
                        Node {
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(if funding == level {
                            Color::srgb(0.3, 0.42, 0.3)
                        } else {
                            Color::srgba(0.14, 0.17, 0.21, 0.95)
                        }),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(format!("F{level}")),
                            font(&fonts.display, 11.0),
                            TextColor(MAIN),
                        ));
                    });
                }
            });
            // Operations (enabled only with a usable network).
            if strength >= 40 {
                p.spawn(Node {
                    column_gap: Val::Px(5.0),
                    ..default()
                })
                .with_children(|row| {
                    for (label, kind, tip) in [
                        (
                            "STEAL DESIGNS",
                            OpKind::StealDesigns,
                            "Covert operation: exfiltrate nuclear design data, accelerating your own program. Needs a strong network; failure burns assets and can blow back diplomatically.",
                        ),
                        (
                            "SABOTAGE",
                            OpKind::Sabotage,
                            "Covert operation: sabotage their fissile production. Needs a strong network; failure burns assets and raises tension if traced.",
                        ),
                    ] {
                        row.spawn((
                            Button,
                            IntelButton::Op(rival.clone(), kind),
                            crate::widgets::Tooltip::of(tip),
                            Node {
                                padding: UiRect::axes(Val::Px(9.0), Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.5, 0.25, 0.2)),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                Text::new(label),
                                font(&fonts.display, 11.0),
                                TextColor(MAIN),
                            ));
                        });
                    }
                });
            }
        }
        // The v1 domains legend.
        p.spawn((
            Text::new("ECONOMIC INTEL PIERCES REPORTED OUTPUT; NUCLEAR SHRINKS THE ARMS ESTIMATE."),
            font(&fonts.mono, 9.5),
            TextColor(DIM),
        ));
    });
}

fn intel_buttons(
    buttons: Query<(&Interaction, &IntelButton), Changed<Interaction>>,
    player: Option<Res<PlayerNation>>,
    mut pending: ResMut<PendingCommands>,
) {
    if player.is_none() {
        return;
    }
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            IntelButton::Fund(target, level) => pending.push(SimCommand::SetNetworkFunding {
                target: target.clone(),
                level: *level,
            }),
            IntelButton::Counterintel(level) => {
                pending.push(SimCommand::SetCounterintel { level: *level })
            }
            IntelButton::Op(target, kind) => pending.push(SimCommand::LaunchOperation {
                target: target.clone(),
                kind: kind.clone(),
            }),
        }
    }
}

/// The economic coupling made visible: what the player believes a
/// rival's industry to be, given their economic penetration. Read by
/// the econ panel; here as a reusable helper.
pub fn observed_industry(
    economies: &ugs_sim::planning::Economies,
    intel: &Intel,
    viewer: &ugs_data::CountryTag,
    subject: &ugs_data::CountryTag,
) -> u64 {
    let pen = intel.knowledge(viewer, subject, Domain::Economic);
    economies.observed_industry_centi(subject, pen)
}
