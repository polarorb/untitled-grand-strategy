//! The atomic dossier (B): own program as an exact milestone dossier,
//! rival programs as National Intelligence Estimate cards that are
//! allowed to be wrong. Clinical voice, numbers over adjectives.

use bevy::prelude::*;
use ugs_sim::command::{PendingCommands, SimCommand};
use ugs_sim::deterrence::{Deterrence, DyadClass};
use ugs_sim::nuclear::{tuning, NuclearPrograms, ProgramPosture, Stage};
use ugs_sim::SimClock;

use crate::{font, AppState, Fonts, PlayerNation, World1950};

const PANEL_BG: Color = Color::srgba(0.07, 0.09, 0.12, 0.97);
const ACCENT: Color = Color::srgb(0.83, 0.69, 0.36);
const MAIN: Color = Color::srgb(0.88, 0.89, 0.90);
const DIM: Color = Color::srgb(0.62, 0.66, 0.70);

#[derive(Component)]
struct AtomicPanel;

#[derive(Component, Clone)]
enum AtomicButton {
    Found,
    Posture(ProgramPosture),
    Expand(&'static str),
    Deception(bool),
    Alert(u8),
}

pub struct AtomicUiPlugin;

impl Plugin for AtomicUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (toggle_panel, refresh_panel, atomic_buttons).run_if(in_state(AppState::InGame)),
        );
    }
}

/// B toggles the dossier. Dev shortcut: UGS_PANEL=atomic boots open.
fn toggle_panel(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    panel: Query<Entity, With<AtomicPanel>>,
    mut booted: Local<bool>,
) {
    let auto_open = !*booted && std::env::var("UGS_PANEL").as_deref() == Ok("atomic");
    *booted = true;
    if !keys.just_pressed(KeyCode::KeyB) && !auto_open {
        return;
    }
    if let Ok(e) = panel.single() {
        commands.entity(e).despawn();
    } else {
        commands.spawn((
            AtomicPanel,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(56.0),
                width: Val::Px(400.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(7.0),
                padding: UiRect::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ));
    }
}

/// Deterministic display-side fuzz (never the sim RNG).
fn mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x
}

fn round_sig2(n: u64) -> u64 {
    if n < 100 {
        return n;
    }
    let digits = (n as f64).log10() as u32 + 1;
    let step = 10u64.pow(digits - 2);
    n / step * step
}

#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn refresh_panel(
    mut commands: Commands,
    nukes: Res<NuclearPrograms>,
    deterrence: Res<Deterrence>,
    world: Res<World1950>,
    clock: Res<SimClock>,
    fonts: Res<Fonts>,
    player: Option<Res<PlayerNation>>,
    panel: Query<Entity, Added<AtomicPanel>>,
    panel_any: Query<Entity, With<AtomicPanel>>,
) {
    let rebuild = !panel.is_empty() || nukes.is_changed();
    if !rebuild {
        return;
    }
    let Ok(panel) = panel_any.single() else {
        return;
    };
    commands.entity(panel).despawn_related::<Children>();
    let me = player.as_ref().map(|p| p.0.clone());
    let quarter = clock.tick / (24 * 90); // estimates re-sample quarterly
    commands.entity(panel).with_children(|p| {
        p.spawn((
            Text::new("ATOMIC ENERGY -- EYES ONLY"),
            font(&fonts.display, 17.0),
            TextColor(ACCENT),
        ));

        // --- Own program dossier -------------------------------------
        let own = me.as_ref().and_then(|m| nukes.programs.get(m));
        match (own, &me) {
            (Some(prog), Some(_)) => {
                let stage = match prog.stage {
                    Stage::Founded => "ESTABLISHMENT",
                    Stage::Producing => "FISSILE PRODUCTION",
                    Stage::Tested => "NUCLEAR POWER",
                    Stage::Thermonuclear => "THERMONUCLEAR POWER",
                };
                p.spawn((
                    Text::new(format!("NATIONAL PROGRAM: {stage}")),
                    font(&fonts.mono_bold, 12.0),
                    TextColor(MAIN),
                ));
                p.spawn((
                    Text::new(format!(
                        "ROUTE {:?}  POSTURE {:?}  SCIENTISTS {}",
                        prog.route, prog.posture, prog.scientists
                    )),
                    font(&fonts.mono, 11.0),
                    TextColor(DIM),
                ));
                p.spawn((
                    Text::new(format!(
                        "PLANT: ENRICHMENT L{}  REACTOR L{}{}",
                        prog.enrichment_level,
                        prog.reactor_level,
                        if prog.building.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "  ({} UNDER CONSTRUCTION, {}MO)",
                                prog.building.len(),
                                prog.building.iter().map(|(_, m)| *m).max().unwrap_or(0)
                            )
                        }
                    )),
                    font(&fonts.mono, 11.0),
                    TextColor(MAIN),
                ));
                p.spawn((
                    Text::new(format!(
                        "FISSILE BANK {:.1} KG  (+{:.1} KG/MO BASE)",
                        prog.fissile_g as f64 / 1000.0,
                        prog.base_production_g() as f64 / 1000.0
                    )),
                    font(&fonts.mono, 11.0),
                    TextColor(MAIN),
                ));
                p.spawn((
                    Text::new(format!(
                        "STOCKPILE: {:04}   ASSEMBLED: {:04}",
                        prog.stockpile, prog.assembled
                    )),
                    font(&fonts.mono_bold, 13.0),
                    TextColor(ACCENT),
                ));
                if prog.thermonuclear_authorized && prog.stage == Stage::Tested {
                    p.spawn((
                        Text::new("THE SUPER: AUTHORIZED -- DEVELOPMENT PROCEEDING"),
                        font(&fonts.mono, 10.5),
                        TextColor(DIM),
                    ));
                }
                // Posture + facility controls.
                p.spawn(Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|row| {
                    for (label, posture, tip) in [
                        (
                            "COVERT",
                            ProgramPosture::Covert,
                            "Covert: slowest progress, hardest for rival intelligence to see. What they don't believe can't deter them - or provoke them.",
                        ),
                        (
                            "STANDARD",
                            ProgramPosture::Standard,
                            "Standard: normal program pace and visibility.",
                        ),
                        (
                            "CRASH",
                            ProgramPosture::Crash,
                            "Crash: maximum speed, loud - rivals see it, and it raises tension. The Manhattan pace.",
                        ),
                    ] {
                        let active = prog.posture == posture;
                        row.spawn((
                            Button,
                            AtomicButton::Posture(posture),
                            crate::widgets::Tooltip::of(tip),
                            Node {
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(if active {
                                Color::srgb(0.55, 0.44, 0.18)
                            } else {
                                Color::srgba(0.14, 0.17, 0.21, 0.95)
                            }),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                Text::new(label),
                                font(&fonts.display, 12.0),
                                TextColor(MAIN),
                            ));
                        });
                    }
                });
                // Deterrence standing vs each rival program.
                for ((a, b), dyad) in &deterrence.dyads {
                    let me_tag = me.as_ref().unwrap();
                    let other = if a == me_tag {
                        b
                    } else if b == me_tag {
                        a
                    } else {
                        continue;
                    };
                    let (their_est_of_us, our_est_of_them) = if a == me_tag {
                        (dyad.b_believes_a_delivers, dyad.a_believes_b_delivers)
                    } else {
                        (dyad.a_believes_b_delivers, dyad.b_believes_a_delivers)
                    };
                    let class = match dyad.class {
                        DyadClass::Mutual => "MUTUAL DETERRENCE",
                        DyadClass::OneSided => "ONE-SIDED",
                        DyadClass::None => "NO DETERRENT",
                    };
                    p.spawn((
                        Text::new(format!(
                            "VS {}: {class} -- THEY CREDIT US {their_est_of_us}, WE CREDIT THEM {our_est_of_them}",
                            other.0
                        )),
                        font(&fonts.mono, 10.5),
                        TextColor(if dyad.class == DyadClass::Mutual {
                            ACCENT
                        } else {
                            DIM
                        }),
                    ));
                }
                // Alert lever: 4 detents, each with real costs.
                p.spawn((
                    Text::new("STRATEGIC FORCES ALERT"),
                    font(&fonts.mono_bold, 11.0),
                    TextColor(DIM),
                ));
                p.spawn(Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|row| {
                    for (label, level, tip) in [
                        (
                            "PEACETIME",
                            0u8,
                            "Bombers on airfields, crews at rest. No cost, no signal.",
                        ),
                        (
                            "INCREASED",
                            1,
                            "Increased readiness: dispersed bombers, shorter reaction time. Rivals notice; tension rises when you go up.",
                        ),
                        (
                            "AIRBORNE",
                            2,
                            "Airborne alert: a share of the force always aloft. Fast, expensive, and unmistakably a signal.",
                        ),
                        (
                            "MAXIMUM",
                            3,
                            "Maximum alert: everything ready to fly. The loudest signal short of launching - hold it long and accidents beckon.",
                        ),
                    ] {
                        let active = prog.alert == level;
                        row.spawn((
                            Button,
                            AtomicButton::Alert(level),
                            crate::widgets::Tooltip::of(tip),
                            Node {
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(if active {
                                if level >= 2 {
                                    Color::srgb(0.55, 0.2, 0.16)
                                } else {
                                    Color::srgb(0.55, 0.44, 0.18)
                                }
                            } else {
                                Color::srgba(0.14, 0.17, 0.21, 0.95)
                            }),
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
                // Parade deception toggle.
                p.spawn(Node {
                    ..default()
                })
                .with_children(|row| {
                    crate::widgets::toggle(
                        row,
                        AtomicButton::Deception(!prog.deception),
                        "PARADE DECEPTION",
                        prog.deception,
                        false,
                        &fonts,
                        12.0,
                        "Fly the same bombers past the reviewing stand twice: rivals overestimate your arsenal. Deterrence runs on what they BELIEVE - but being caught inflating it provokes, and turning it on raises tension.",
                    );
                });
                p.spawn(Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|row| {
                    for (label, kind) in
                        [("BUILD REACTOR", "Reactor"), ("BUILD ENRICHMENT", "Enrichment")]
                    {
                        row.spawn((
                            Button,
                            AtomicButton::Expand(kind),
                            Node {
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.14, 0.17, 0.21, 0.95)),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                Text::new(label),
                                font(&fonts.display, 12.0),
                                TextColor(MAIN),
                            ));
                        });
                    }
                });
            }
            (None, Some(_)) => {
                p.spawn((
                    Text::new("NO NATIONAL PROGRAM"),
                    font(&fonts.mono_bold, 12.0),
                    TextColor(DIM),
                ));
                p.spawn((
                    Button,
                    AtomicButton::Found,
                    Node {
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.55, 0.44, 0.18)),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("FOUND ATOMIC PROGRAM"),
                        font(&fonts.display, 13.0),
                        TextColor(Color::srgb(0.98, 0.95, 0.88)),
                    ));
                });
            }
            _ => {
                p.spawn((
                    Text::new("OBSERVER"),
                    font(&fonts.mono, 11.0),
                    TextColor(DIM),
                ));
            }
        }

        // --- NIE cards for rival programs ------------------------------
        let rivals: Vec<_> = nukes
            .programs
            .iter()
            .filter(|(tag, _)| Some(*tag) != me.as_ref())
            .collect();
        if !rivals.is_empty() {
            p.spawn((
                Text::new("NATIONAL INTELLIGENCE ESTIMATES"),
                font(&fonts.mono_bold, 11.0),
                TextColor(DIM),
            ));
        }
        for (tag, prog) in rivals {
            let name = world
                .0
                .nations_meta
                .get(tag)
                .map(|m| m.display_name.to_uppercase())
                .unwrap_or_else(|| tag.0.clone());
            let seed = tag.0.bytes().map(u64::from).sum::<u64>() ^ quarter;
            let exposure = prog.exposure_permille as u64;
            // Historical NIEs overestimated: bias 1.0-2.6x, shrinking
            // with exposure; width also shrinks with exposure.
            let bias = 1000 + (mix(seed) % 1600) * (1000 - exposure.min(950)) / 1000;
            let confidence = match exposure {
                0..=300 => "LOW",
                301..=650 => "MODERATE",
                _ => "HIGH",
            };
            let line = if prog.stage >= Stage::Tested {
                let center = prog.stockpile as u64 * bias / 1000;
                let width = 150 + (1000 - exposure.min(950)) / 2; // permille
                let lo = round_sig2(center * (1000 - width) / 1000);
                let hi = round_sig2(center * (1000 + width) / 1000).max(lo + 1);
                let kind = if prog.stage == Stage::Thermonuclear {
                    "THERMONUCLEAR ARSENAL"
                } else {
                    "ATOMIC ARSENAL"
                };
                format!("{kind} EST {lo}-{hi} WEAPONS")
            } else {
                // Estimate of first test date, biased late (Joe-1 style).
                let months_left = (tuning::TEST_DEVICE_BANK_G
                    .saturating_sub(prog.fissile_g)
                    / prog.base_production_g().max(1000))
                    + 1;
                let opt_year = 1950 + (clock.tick / (24 * 365) + months_left / 12) as u32;
                let slack = 1 + (mix(seed ^ 0x51ed) % 3) as u32;
                format!("EST FIRST TEST: {}-{}", opt_year + 1, opt_year + 1 + slack)
            };
            p.spawn((
                Text::new(format!("{name}: {line}")),
                font(&fonts.mono, 11.0),
                TextColor(MAIN),
            ));
            p.spawn((
                Text::new(format!("  CONFIDENCE: {confidence}")),
                font(&fonts.mono, 10.0),
                TextColor(DIM),
            ));
        }
    });
}

fn atomic_buttons(
    buttons: Query<(&Interaction, &AtomicButton), Changed<Interaction>>,
    player: Option<Res<PlayerNation>>,
    mut pending: ResMut<PendingCommands>,
) {
    let Some(player) = player else { return };
    let me = player.0.clone();
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            AtomicButton::Found => pending.push(SimCommand::FoundNuclearProgram {
                country: me.clone(),
                route: "Plutonium".into(),
            }),
            AtomicButton::Posture(posture) => pending.push(SimCommand::SetProgramPosture {
                country: me.clone(),
                posture: format!("{posture:?}"),
            }),
            AtomicButton::Expand(kind) => pending.push(SimCommand::ExpandNuclearFacility {
                country: me.clone(),
                kind: (*kind).into(),
            }),
            AtomicButton::Deception(on) => pending.push(SimCommand::SetParadeDeception {
                country: me.clone(),
                on: *on,
            }),
            AtomicButton::Alert(level) => pending.push(SimCommand::SetAlertLevel {
                country: me.clone(),
                level: *level,
            }),
        }
    }
}
