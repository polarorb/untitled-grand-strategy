//! War presentation: teletype event popups, formation markers on the
//! map, and occupation-driven map repainting hooks.

use bevy::prelude::*;
use ugs_sim::command::{PendingCommands, SimCommand};
use ugs_sim::events::FiredEvents;
use ugs_sim::military::Military;

use crate::audio::AudioHandles;
use crate::map::{project, Selected, WORLD_WRAP};
use crate::{font, AppState, Fonts, GameSpeed, PlayerNation, World1950};
use bevy::audio::{PlaybackSettings, Volume};
use ugs_sim::demography::Demographics;
use ugs_sim::military::{tuning, Posture};
use ugs_sim::SimClock;

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

#[derive(Component)]
struct WarPanel;

#[derive(Component, Clone)]
enum WarButton {
    TogglePosture(ugs_data::CountryTag),
    ToggleArmistice(ugs_data::CountryTag),
}

#[derive(Component)]
struct BattleMarker;

#[derive(Component)]
struct BattlePanel;

/// Deterministic display-side fuzz for enemy figures (never touches the
/// sim RNG). Same inputs, same estimate — re-sampled monthly so numbers
/// don't jitter every frame.
fn mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x
}

/// Round to two significant figures — intel reports never carry false
/// precision.
fn round_sig2(n: u64) -> u64 {
    if n < 100 {
        return n;
    }
    let digits = (n as f64).log10() as u32 + 1;
    let step = 10u64.pow(digits - 2);
    n / step * step
}

fn fmt_men(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        format!("{n}")
    }
}

/// Estimated enemy men as a low-high band around a fuzzed center.
fn est_men_range(true_men: u64, seed: u64) -> (u64, u64) {
    let factor = 800 + mix(seed) % 500; // 0.80x .. 1.30x
    let center = true_men * factor / 1000;
    (round_sig2(center * 85 / 100), round_sig2(center * 115 / 100))
}

/// Estimated enemy division count band.
fn est_div_range(count: u32, seed: u64) -> (u32, u32) {
    let center = (count as i64 + (mix(seed) % 3) as i64 - 1).max(1) as u32;
    (center.saturating_sub(1).max(1), center + 1)
}

pub struct WarUiPlugin;

impl Plugin for WarUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                announce_player_country,
                show_event_popups,
                show_notices,
                dismiss_popup,
                choice_buttons,
                toggle_war_panel,
                refresh_war_panel,
                war_buttons,
                sync_formation_markers,
                pulse_battle_markers,
                battle_inspector,
                draw_movement_arrows,
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
    audio: Res<AudioHandles>,
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
    commands.spawn((
        AudioPlayer::new(audio.teletype.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.5)),
    ));
    // War declarations get the attention signal on top of the teletype.
    let declares_war = event
        .effects
        .iter()
        .chain(event.options.iter().flat_map(|o| o.effects.iter()))
        .any(|e| matches!(e, ugs_data::EventEffect::DeclareWar { .. }));
    if declares_war {
        commands.spawn((
            AudioPlayer::new(audio.alert.clone()),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.35)),
        ));
    }
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

/// Rebuild division counters when the military picture changes (cheap:
/// tens of markers). Own stacks: exact count, men, strength + cohesion
/// bars. Enemy stacks: fuzzed count band, no bars — fog of war.
#[allow(clippy::too_many_arguments, clippy::type_complexity)] // Bevy systems take what they query
fn sync_formation_markers(
    mut commands: Commands,
    military: Res<Military>,
    world: Res<World1950>,
    clock: Res<SimClock>,
    fonts: Res<Fonts>,
    player: Option<Res<PlayerNation>>,
    mut last_hash: Local<u64>,
    markers: Query<Entity, Or<(With<FormationMarker>, With<BattleMarker>)>>,
) {
    if !military.is_changed() {
        return;
    }
    // Rebuild only when positions, ownership, readiness buckets, or the
    // battle set change.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for f in military.formations.values() {
        for v in [
            f.location.0 as u64,
            f.owner.0.bytes().map(u64::from).sum(),
            f.strength / 40,
            f.cohesion / 40,
        ] {
            hash = (hash ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    for b in &military.active_battles {
        hash = (hash ^ b.province.0 as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    if hash == *last_hash {
        return;
    }
    *last_hash = hash;

    for e in &markers {
        commands.entity(e).despawn();
    }

    // Aggregate per (province, owner).
    struct Stack {
        count: u32,
        men: u64,
        strength: u64,
        cohesion: u64,
    }
    let mut stacks: std::collections::BTreeMap<(u32, String), Stack> = Default::default();
    for f in military.formations.values() {
        let e = stacks
            .entry((f.location.0, f.owner.0.clone()))
            .or_insert(Stack { count: 0, men: 0, strength: 0, cohesion: 0 });
        e.count += 1;
        e.men += f.strength * tuning::MEN_PER_STRENGTH_POINT;
        e.strength += f.strength;
        e.cohesion += f.cohesion;
    }
    let month = clock.tick / (24 * 30);
    let ramp = |v: u64| -> Color {
        if v > 660 {
            Color::srgb(0.35, 0.75, 0.35)
        } else if v > 330 {
            Color::srgb(0.85, 0.65, 0.2)
        } else {
            Color::srgb(0.85, 0.25, 0.2)
        }
    };
    for ((province, owner), stack) in stacks {
        let Some(p) = world.0.provinces.get(&ugs_data::ProvinceId(province)) else {
            continue;
        };
        let n = stack.count as u64;
        let (avg_str, avg_coh) = (stack.strength / n, stack.cohesion / n);
        let pos = project(p.center.0, p.center.1);
        let tag = ugs_data::CountryTag(owner.clone());
        let is_enemy = player
            .as_ref()
            .map(|pl| military.at_war(&pl.0, &tag))
            .unwrap_or(false);
        let rgb = world
            .0
            .countries
            .get(&tag)
            .map(|c| c.color)
            .unwrap_or((128, 128, 128));
        let dim = if is_enemy { 0.42 } else { 0.6 };
        let color = Color::srgb(
            rgb.0 as f32 / 255.0 * dim,
            rgb.1 as f32 / 255.0 * dim,
            rgb.2 as f32 / 255.0 * dim,
        );
        const W: f32 = 26.0;
        const H: f32 = 15.0;
        let label = if is_enemy {
            let seed = province as u64 ^ owner.bytes().map(u64::from).sum::<u64>() ^ month;
            let (lo, hi) = est_div_range(stack.count, seed);
            format!("{lo}-{hi}?")
        } else {
            format!("{}", stack.count)
        };
        commands
            .spawn((
                FormationMarker,
                Sprite::from_color(color, Vec2::new(W, H)),
                Transform::from_translation(pos.extend(3.0)),
            ))
            .with_children(|m| {
                m.spawn((
                    Text2d::new(label),
                    font(&fonts.body_medium, if is_enemy { 9.0 } else { 11.0 }),
                    TextColor(Color::WHITE),
                    Transform::from_translation(Vec3::new(0.0, 1.0, 0.1)),
                ));
                if !is_enemy {
                    // Men figure under the box.
                    m.spawn((
                        Text2d::new(fmt_men(stack.men)),
                        font(&fonts.body_medium, 8.0),
                        TextColor(Color::srgb(0.92, 0.92, 0.92)),
                        Transform::from_translation(Vec3::new(0.0, -12.5, 0.1)),
                    ));
                    // Strength bar along the bottom edge.
                    let sw = W * avg_str as f32 / 1000.0;
                    m.spawn((
                        Sprite::from_color(ramp(avg_str), Vec2::new(sw.max(1.0), 2.2)),
                        Transform::from_translation(Vec3::new(
                            -(W - sw) / 2.0,
                            -H / 2.0 - 1.6,
                            0.1,
                        )),
                    ));
                    // Cohesion bar up the left edge.
                    let ch = H * avg_coh as f32 / 1000.0;
                    m.spawn((
                        Sprite::from_color(
                            if avg_coh > 660 {
                                Color::srgb(0.35, 0.7, 0.85)
                            } else {
                                ramp(avg_coh)
                            },
                            Vec2::new(2.2, ch.max(1.0)),
                        ),
                        Transform::from_translation(Vec3::new(
                            -W / 2.0 - 1.6,
                            -(H - ch) / 2.0,
                            0.1,
                        )),
                    ));
                }
            });
    }

    // Battle markers: a pulsing red diamond over every contested province.
    for b in &military.active_battles {
        let Some(p) = world.0.provinces.get(&b.province) else {
            continue;
        };
        let pos = project(p.center.0, p.center.1);
        commands.spawn((
            BattleMarker,
            Sprite::from_color(Color::srgb(0.95, 0.2, 0.15), Vec2::splat(9.0)),
            Transform::from_translation(pos.extend(3.6))
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)),
        ));
    }
}

/// Battles breathe: scale-pulse so the eye finds the fighting.
fn pulse_battle_markers(
    time: Res<Time>,
    mut markers: Query<&mut Transform, With<BattleMarker>>,
) {
    let s = 1.0 + 0.22 * (time.elapsed_secs() * 4.0).sin();
    for mut t in &mut markers {
        t.scale = Vec3::splat(s);
    }
}

/// Divisions in transit trail an arrow from where they came — no more
/// teleporting armies. Own moves in nation color, enemy moves dim red.
fn draw_movement_arrows(
    military: Res<Military>,
    world: Res<World1950>,
    player: Option<Res<PlayerNation>>,
    mut gizmos: Gizmos,
) {
    for f in military.formations.values() {
        if f.move_cooldown == 0 {
            continue;
        }
        let Some(from_id) = f.last_location else { continue };
        let (Some(from), Some(to)) = (
            world.0.provinces.get(&from_id),
            world.0.provinces.get(&f.location),
        ) else {
            continue;
        };
        let a = project(from.center.0, from.center.1);
        let b = project(to.center.0, to.center.1);
        let is_enemy = player
            .as_ref()
            .map(|pl| military.at_war(&pl.0, &f.owner))
            .unwrap_or(false);
        let color = if is_enemy {
            Color::srgba(0.8, 0.25, 0.2, 0.55)
        } else {
            let rgb = world
                .0
                .countries
                .get(&f.owner)
                .map(|c| c.color)
                .unwrap_or((200, 200, 200));
            Color::srgba(
                rgb.0 as f32 / 255.0,
                rgb.1 as f32 / 255.0,
                rgb.2 as f32 / 255.0,
                0.8,
            )
        };
        for offset in [-WORLD_WRAP, 0.0, WORLD_WRAP] {
            let off = Vec2::new(offset, 0.0);
            gizmos.arrow_2d(a + off, b + off, color);
        }
    }
}

/// The battle inspector: selecting a contested province opens a full
/// after-action view — both sides' numbers, the modifier ledger, a
/// break-time projection, and a one-line "why" diagnosis.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn battle_inspector(
    mut commands: Commands,
    selected: Res<Selected>,
    military: Res<Military>,
    world: Res<World1950>,
    clock: Res<SimClock>,
    fonts: Res<Fonts>,
    player: Option<Res<PlayerNation>>,
    panel: Query<Entity, With<BattlePanel>>,
) {
    if !selected.is_changed() && !military.is_changed() {
        return;
    }
    let battle = selected
        .0
        .and_then(|id| military.active_battles.iter().find(|b| b.province == id));
    let Some(b) = battle else {
        for e in &panel {
            commands.entity(e).despawn();
        }
        return;
    };
    for e in &panel {
        commands.entity(e).despawn();
    }
    let name = world
        .0
        .provinces
        .get(&b.province)
        .map(|p| p.name.to_uppercase())
        .unwrap_or_default();
    let hours = clock.tick.saturating_sub(b.since_tick).max(1);
    let month = clock.tick / (24 * 30);
    let me = player.as_ref().map(|p| &p.0);
    let side_is_mine = |owners: &[ugs_data::CountryTag]| me.is_some_and(|m| owners.contains(m));
    let side_is_enemy = |owners: &[ugs_data::CountryTag]| {
        me.is_some_and(|m| owners.iter().any(|o| military.at_war(m, o)))
    };

    // Modifier ledgers, as signed percentages vs baseline.
    let terrain_pct = tuning::terrain_defense_permille(b.terrain) as i64 / 10 - 100;
    let mut def_mods: Vec<String> = Vec::new();
    if terrain_pct != 0 {
        def_mods.push(format!("{:?} {:+}%", b.terrain, terrain_pct).to_uppercase());
    }
    if b.defender_home {
        def_mods.push(format!(
            "HOME GROUND {:+}%",
            tuning::HOME_DEFENSE_PERMILLE as i64 / 10 - 100
        ));
    }
    let qual_gap = b.attacker_quality as i64 - b.defender_quality as i64;
    let att_mods = if qual_gap.abs() >= 30 {
        vec![format!("QUALITY EDGE {:+}%", qual_gap / 10)]
    } else {
        Vec::new()
    };

    // Projection: hours until each side's average division breaks.
    let to_break = |cohesion: u64, loss: u64| {
        cohesion.saturating_sub(tuning::RETREAT_COHESION) / loss.max(1)
    };
    let att_breaks = to_break(b.attacker_cohesion, b.attacker_hourly_loss);
    let def_breaks = to_break(b.defender_cohesion, b.defender_hourly_loss);
    let projection = if def_breaks < att_breaks {
        format!("DEFENDER BREAKS IN ~{}H AT CURRENT RATE", def_breaks.max(1))
    } else if att_breaks < def_breaks {
        format!("ATTACKER BREAKS IN ~{}H AT CURRENT RATE", att_breaks.max(1))
    } else {
        "EVENLY MATCHED -- NO BREAK IN SIGHT".to_string()
    };

    // One-line diagnosis for the player's side, when they're in it.
    let diagnosis = me.and_then(|_| {
        let (i_attack, losing) = if side_is_mine(&b.attacker_owners) {
            (true, att_breaks <= def_breaks)
        } else if side_is_mine(&b.defender_owners) {
            (false, def_breaks <= att_breaks)
        } else {
            return None;
        };
        if !losing {
            return Some("YOU HOLD THE ADVANTAGE HERE.".to_string());
        }
        let mut reasons: Vec<(i64, String)> = Vec::new();
        if i_attack {
            if terrain_pct > 0 {
                reasons.push((
                    terrain_pct,
                    format!("ENEMY {:?} DEFENSE ({:+}%)", b.terrain, terrain_pct).to_uppercase(),
                ));
            }
            if b.defender_home {
                reasons.push((20, "ENEMY FIGHTING ON HOME GROUND (+20%)".into()));
            }
            if qual_gap < -30 {
                reasons.push((-qual_gap / 10, format!("ENEMY QUALITY EDGE ({:+}%)", -qual_gap / 10)));
            }
            if b.defender_men > b.attacker_men {
                reasons.push((
                    (b.defender_men as i64 - b.attacker_men as i64) / 1000,
                    "YOU ARE OUTNUMBERED".into(),
                ));
            }
        } else {
            if qual_gap > 30 {
                reasons.push((qual_gap / 10, format!("ENEMY QUALITY EDGE ({:+}%)", qual_gap / 10)));
            }
            if b.attacker_men > b.defender_men {
                reasons.push((
                    (b.attacker_men as i64 - b.defender_men as i64) / 1000,
                    "YOU ARE OUTNUMBERED".into(),
                ));
            }
        }
        reasons.sort_by_key(|(w, _)| -w);
        Some(match reasons.first() {
            Some((_, r)) => format!("YOU ARE LOSING PRIMARILY BECAUSE: {r}"),
            None => "YOU ARE LOSING ON COHESION ATTRITION.".to_string(),
        })
    });

    let total_coh = (b.attacker_cohesion + b.defender_cohesion).max(1) as f32;
    let att_share = b.attacker_cohesion as f32 / total_coh;

    commands
        .spawn((
            BattlePanel,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(56.0),
                width: Val::Px(360.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(7.0),
                padding: UiRect::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(format!("BATTLE OF {name}")),
                font(&fonts.display, 17.0),
                TextColor(ACCENT),
            ));
            p.spawn((
                Text::new(format!("FIGHTING FOR {hours}H -- {:?} TERRAIN", b.terrain)),
                font(&fonts.mono, 11.0),
                TextColor(Color::srgb(0.62, 0.66, 0.70)),
            ));
            // Balance-of-power bar: attacker red vs defender blue-grey,
            // driven by remaining cohesion.
            p.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(9.0),
                ..default()
            })
            .with_children(|bar| {
                bar.spawn((
                    Node {
                        width: Val::Percent(att_share * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.72, 0.28, 0.22)),
                ));
                bar.spawn((
                    Node {
                        width: Val::Percent((1.0 - att_share) * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.30, 0.45, 0.62)),
                ));
            });
            let side_line = |owners: &[ugs_data::CountryTag],
                             divisions: u32,
                             men: u64,
                             cohesion: u64,
                             loss: u64,
                             power: u64,
                             enemy_seed: u64|
             -> Vec<String> {
                let names: Vec<&str> = owners.iter().map(|t| t.0.as_str()).collect();
                let fogged = side_is_enemy(owners);
                let (divs, men_s) = if fogged {
                    let (lo, hi) = est_div_range(divisions, enemy_seed);
                    let (mlo, mhi) = est_men_range(men, enemy_seed ^ 0x9e37);
                    (
                        format!("EST {lo}-{hi} DIV"),
                        format!("~{}-{}", fmt_men(mlo), fmt_men(mhi)),
                    )
                } else {
                    (format!("{divisions} DIV"), fmt_men(men))
                };
                vec![
                    format!("{}  {divs}  {men_s} MEN", names.join("/")),
                    format!(
                        "COHESION {}%  FALLING {}/H  POWER {power}",
                        cohesion / 10,
                        loss
                    ),
                ]
            };
            let seed = b.province.0 as u64 ^ month;
            p.spawn((
                Text::new("ATTACKER"),
                font(&fonts.mono_bold, 11.0),
                TextColor(Color::srgb(0.85, 0.45, 0.38)),
            ));
            for line in side_line(
                &b.attacker_owners,
                b.attacker_divisions,
                b.attacker_men,
                b.attacker_cohesion,
                b.attacker_hourly_loss,
                b.attacker_power,
                seed,
            ) {
                p.spawn((Text::new(line), font(&fonts.mono, 11.5), TextColor(MAIN)));
            }
            if !att_mods.is_empty() {
                p.spawn((
                    Text::new(att_mods.join(" - ")),
                    font(&fonts.mono, 10.5),
                    TextColor(Color::srgb(0.62, 0.66, 0.70)),
                ));
            }
            p.spawn((
                Text::new("DEFENDER"),
                font(&fonts.mono_bold, 11.0),
                TextColor(Color::srgb(0.5, 0.65, 0.85)),
            ));
            for line in side_line(
                &b.defender_owners,
                b.defender_divisions,
                b.defender_men,
                b.defender_cohesion,
                b.defender_hourly_loss,
                b.defender_power,
                seed ^ 0x51ed,
            ) {
                p.spawn((Text::new(line), font(&fonts.mono, 11.5), TextColor(MAIN)));
            }
            if !def_mods.is_empty() {
                p.spawn((
                    Text::new(def_mods.join(" - ")),
                    font(&fonts.mono, 10.5),
                    TextColor(Color::srgb(0.62, 0.66, 0.70)),
                ));
            }
            p.spawn((
                Text::new(projection),
                font(&fonts.mono_bold, 11.5),
                TextColor(ACCENT),
            ));
            if let Some(d) = diagnosis {
                p.spawn((
                    Text::new(d),
                    font(&fonts.mono, 11.0),
                    TextColor(Color::srgb(0.9, 0.75, 0.5)),
                ));
            }
            // The player's divisions here, by name — raised somewhere
            // real, dying somewhere real.
            if let Some(me) = me {
                let mut mine: Vec<&ugs_sim::military::Formation> = military
                    .formations
                    .values()
                    .filter(|f| f.location == b.province && &f.owner == me)
                    .collect();
                mine.sort_by_key(|f| f.name.clone());
                if !mine.is_empty() {
                    p.spawn((
                        Text::new("YOUR DIVISIONS"),
                        font(&fonts.mono_bold, 11.0),
                        TextColor(Color::srgb(0.62, 0.66, 0.70)),
                    ));
                    for f in mine.iter().take(8) {
                        p.spawn((
                            Text::new(format!(
                                "{}  STR {}%  COH {}%",
                                f.name,
                                f.strength / 10,
                                f.cohesion / 10
                            )),
                            font(&fonts.mono, 10.5),
                            TextColor(MAIN),
                        ));
                    }
                    if mine.len() > 8 {
                        p.spawn((
                            Text::new(format!("AND {} MORE", mine.len() - 8)),
                            font(&fonts.mono, 10.5),
                            TextColor(Color::srgb(0.62, 0.66, 0.70)),
                        ));
                    }
                }
            }
        });
}

/// Tell the sim who the player is, once, through the command queue (so
/// it's in the replay log — armistice AI must not auto-decide for us).
fn announce_player_country(
    player: Option<Res<PlayerNation>>,
    mut pending: ResMut<PendingCommands>,
    mut announced: Local<bool>,
) {
    if *announced {
        return;
    }
    *announced = true;
    pending.push(SimCommand::SetPlayerCountry {
        country: player.map(|p| p.0.clone()),
    });
}

/// Dynamic notices (armistices, capitulations) get the same teletype
/// treatment as scripted events.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn show_notices(
    mut commands: Commands,
    fired: Res<FiredEvents>,
    fonts: Res<Fonts>,
    audio: Res<AudioHandles>,
    mut speed: ResMut<GameSpeed>,
    mut seen: Local<usize>,
    existing: Query<(), With<EventModal>>,
) {
    if fired.notices.len() <= *seen || !existing.is_empty() {
        return;
    }
    let (title, body) = fired.notices[*seen].clone();
    *seen += 1;
    speed.paused = true;
    commands.spawn((
        AudioPlayer::new(audio.teletype.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.5)),
    ));
    commands
        .spawn((
            EventModal,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(22.0),
                margin: UiRect::left(Val::Px(-240.0)),
                width: Val::Px(480.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                padding: UiRect::all(Val::Px(22.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|m| {
            m.spawn((
                Text::new("*** WIRE SERVICE ***"),
                font(&fonts.mono_bold, 13.0),
                TextColor(ACCENT),
            ));
            m.spawn((Text::new(title), font(&fonts.display, 22.0), TextColor(MAIN)));
            m.spawn((Text::new(body), font(&fonts.mono, 13.5), TextColor(MAIN)));
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
        });
}

/// R toggles the war room panel (W pans the camera).
/// Dev shortcut: UGS_PANEL=war boots with the panel open.
fn toggle_war_panel(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    panel: Query<Entity, With<WarPanel>>,
    mut booted: Local<bool>,
) {
    let auto_open = !*booted && std::env::var("UGS_PANEL").as_deref() == Ok("war");
    *booted = true;
    if !keys.just_pressed(KeyCode::KeyR) && !auto_open {
        return;
    }
    if let Ok(e) = panel.single() {
        commands.entity(e).despawn();
    } else {
        commands.spawn((
            WarPanel,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(56.0),
                width: Val::Px(384.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(7.0),
                padding: UiRect::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ));
    }
}

/// Fill the war panel: each war the player is in, with posture and
/// armistice controls.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn refresh_war_panel(
    mut commands: Commands,
    military: Res<Military>,
    world: Res<World1950>,
    demo: Res<Demographics>,
    clock: Res<SimClock>,
    fonts: Res<Fonts>,
    player: Option<Res<PlayerNation>>,
    panel: Query<Entity, Added<WarPanel>>,
    panel_any: Query<Entity, With<WarPanel>>,
) {
    // Rebuild when panel opens or the military picture changes.
    let rebuild = !panel.is_empty() || military.is_changed();
    if !rebuild {
        return;
    }
    let Ok(panel) = panel_any.single() else { return };
    commands.entity(panel).despawn_related::<Children>();
    commands.entity(panel).with_children(|p| {
        p.spawn((Text::new("WAR ROOM"), font(&fonts.display, 18.0), TextColor(MAIN)));
        let Some(player) = &player else {
            p.spawn((
                Text::new("OBSERVER"),
                font(&fonts.mono, 12.0),
                TextColor(Color::srgb(0.62, 0.66, 0.70)),
            ));
            return;
        };
        let me = &player.0;
        let my_wars: Vec<ugs_data::CountryTag> = military
            .wars
            .iter()
            .filter_map(|(a, b)| {
                if a == me {
                    Some(b.clone())
                } else if b == me {
                    Some(a.clone())
                } else {
                    None
                }
            })
            .collect();
        if my_wars.is_empty() {
            p.spawn((
                Text::new("AT PEACE"),
                font(&fonts.mono, 13.0),
                TextColor(Color::srgb(0.62, 0.66, 0.70)),
            ));
            return;
        }
        // --- Manpower pipeline: conservation of mass, population to
        // casualties, so the army is never magic. --------------------------
        let population: u64 = demo
            .provinces
            .iter()
            .filter(|(id, _)| {
                world.0.provinces.get(id).is_some_and(|pr| &pr.owner == me)
            })
            .map(|(_, c)| c.total())
            .sum();
        let fielded: u64 = military
            .formations
            .values()
            .filter(|f| &f.owner == me)
            .map(|f| f.strength * tuning::MEN_PER_STRENGTH_POINT)
            .sum();
        let divisions = military
            .formations
            .values()
            .filter(|f| &f.owner == me)
            .count();
        let reserve = military.manpower.get(me).copied().unwrap_or(0);
        let dead = military.casualties.get(me).copied().unwrap_or(0)
            * tuning::MEN_PER_STRENGTH_POINT;
        p.spawn((
            Text::new("MANPOWER PIPELINE"),
            font(&fonts.mono_bold, 11.0),
            TextColor(Color::srgb(0.62, 0.66, 0.70)),
        ));
        p.spawn((
            Text::new(format!(
                "POP {} > RESERVE {} > FIELD {} ({} DIV) > DEAD {}",
                fmt_men(population),
                fmt_men(reserve),
                fmt_men(fielded),
                divisions,
                fmt_men(dead),
            )),
            font(&fonts.mono, 11.5),
            TextColor(MAIN),
        ));
        if reserve < fielded / 4 {
            p.spawn((
                Text::new("WARNING: MANPOWER RESERVE LOW -- REINFORCEMENT WILL STALL"),
                font(&fonts.mono_bold, 11.0),
                TextColor(Color::srgb(0.92, 0.65, 0.25)),
            ));
        }
        let won = military.battles_won.get(me).copied().unwrap_or(0);
        let lost = military.battles_lost.get(me).copied().unwrap_or(0);
        let static_days = clock.tick.saturating_sub(military.last_line_change_tick) / 24;
        let front = if static_days == 0 {
            "FRONT MOVING".to_string()
        } else {
            format!("FRONT STATIC {static_days}D")
        };
        p.spawn((
            Text::new(format!("BATTLES {won}W/{lost}L    {front}")),
            font(&fonts.mono, 11.5),
            TextColor(MAIN),
        ));
        let month = clock.tick / (24 * 30);
        for enemy in my_wars {
            let enemy_name = world
                .0
                .nations_meta
                .get(&enemy)
                .map(|m| m.display_name.clone())
                .unwrap_or_else(|| enemy.0.clone());
            let posture = military.posture(me, &enemy);
            let offered = military.has_offered_armistice(me, &enemy);
            p.spawn((
                Text::new(format!("VS {}", enemy_name.to_uppercase())),
                font(&fonts.body_medium, 14.0),
                TextColor(ACCENT),
            ));
            // Enemy strength and losses as intelligence estimates: fuzzed,
            // banded, re-sampled monthly, rounded to 2 significant figures.
            let seed = enemy.0.bytes().map(u64::from).sum::<u64>() ^ month;
            let enemy_divs = military
                .formations
                .values()
                .filter(|f| f.owner == enemy)
                .count() as u32;
            let enemy_men: u64 = military
                .formations
                .values()
                .filter(|f| f.owner == enemy)
                .map(|f| f.strength * tuning::MEN_PER_STRENGTH_POINT)
                .sum();
            let enemy_dead =
                military.casualties.get(&enemy).copied().unwrap_or(0)
                    * tuning::MEN_PER_STRENGTH_POINT;
            let (dlo, dhi) = est_div_range(enemy_divs, seed);
            let (mlo, mhi) = est_men_range(enemy_men, seed ^ 0x9e37);
            let (klo, khi) = est_men_range(enemy_dead, seed ^ 0x51ed);
            p.spawn((
                Text::new(format!(
                    "STRENGTH EST {dlo}-{dhi} DIV, {}-{} MEN",
                    fmt_men(mlo),
                    fmt_men(mhi)
                )),
                font(&fonts.mono, 11.0),
                TextColor(Color::srgb(0.75, 0.78, 0.82)),
            ));
            p.spawn((
                Text::new(format!(
                    "ENEMY LOSSES EST {}-{}    OURS {} (EXACT)",
                    fmt_men(klo),
                    fmt_men(khi),
                    fmt_men(dead)
                )),
                font(&fonts.mono, 11.0),
                TextColor(Color::srgb(0.75, 0.78, 0.82)),
            ));
            // War momentum: a decomposed -100..+100 tug-of-war score.
            // Every term is printed — the number must explain itself.
            let mut occ_net: i64 = 0;
            for (prov, holder) in &military.occupation {
                let Some(pr) = world.0.provinces.get(prov) else { continue };
                if holder == me && pr.owner == enemy {
                    occ_net += 1;
                } else if *holder == enemy && &pr.owner == me {
                    occ_net -= 1;
                }
            }
            let occ_term = (occ_net * 4).clamp(-40, 40);
            let my_cas = military.casualties.get(me).copied().unwrap_or(0) as i64;
            let en_cas = military.casualties.get(&enemy).copied().unwrap_or(0) as i64;
            let ex_term = ((en_cas - my_cas) * 30 / (en_cas + my_cas).max(1)).clamp(-30, 30);
            let bw = military.battles_won.get(me).copied().unwrap_or(0) as i64;
            let bl = military.battles_lost.get(me).copied().unwrap_or(0) as i64;
            let battle_term = ((bw - bl) * 15 / (bw + bl).max(1)).clamp(-15, 15);
            let momentum = (occ_term + ex_term + battle_term).clamp(-100, 100);
            let share = (momentum + 100) as f32 / 200.0;
            p.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(8.0),
                ..default()
            })
            .with_children(|bar| {
                bar.spawn((
                    Node {
                        width: Val::Percent(share * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.30, 0.52, 0.34)),
                ));
                bar.spawn((
                    Node {
                        width: Val::Percent((1.0 - share) * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.62, 0.24, 0.20)),
                ));
            });
            p.spawn((
                Text::new(format!(
                    "MOMENTUM {momentum:+}   GROUND {occ_term:+}  EXCHANGE {ex_term:+}  BATTLES {battle_term:+}"
                )),
                font(&fonts.mono, 10.5),
                TextColor(Color::srgb(0.75, 0.78, 0.82)),
            ));
            let assessment = {
                let terms = [
                    (occ_term, "GROUND", "THE GROUND WAR"),
                    (ex_term, "EXCHANGE", "THE CASUALTY EXCHANGE"),
                    (battle_term, "BATTLES", "THE RUN OF BATTLES"),
                ];
                let dominant = terms.iter().max_by_key(|(v, _, _)| v.abs()).unwrap();
                if momentum > 15 {
                    format!("ASSESSMENT: YOU ARE WINNING -- {} FAVORS YOU", dominant.2)
                } else if momentum < -15 {
                    format!("ASSESSMENT: YOU ARE LOSING -- {} RUNS AGAINST YOU", dominant.2)
                } else {
                    "ASSESSMENT: STALEMATE -- NEITHER SIDE HOLDS THE INITIATIVE".to_string()
                }
            };
            p.spawn((
                Text::new(assessment),
                font(&fonts.mono_bold, 11.0),
                TextColor(if momentum > 15 {
                    Color::srgb(0.55, 0.8, 0.55)
                } else if momentum < -15 {
                    Color::srgb(0.9, 0.5, 0.42)
                } else {
                    ACCENT
                }),
            ));
            p.spawn(Node {
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Button,
                    WarButton::TogglePosture(enemy.clone()),
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
                        ..default()
                    },
                    BackgroundColor(if posture == Posture::Advance {
                        Color::srgb(0.5, 0.25, 0.18)
                    } else {
                        Color::srgba(0.14, 0.17, 0.21, 0.95)
                    }),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(format!("{posture:?}").to_uppercase()),
                        font(&fonts.display, 13.0),
                        TextColor(MAIN),
                    ));
                });
                row.spawn((
                    Button,
                    WarButton::ToggleArmistice(enemy.clone()),
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
                        ..default()
                    },
                    BackgroundColor(if offered {
                        Color::srgb(0.25, 0.4, 0.3)
                    } else {
                        Color::srgba(0.14, 0.17, 0.21, 0.95)
                    }),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(if offered {
                            "ARMISTICE OFFERED"
                        } else {
                            "OFFER ARMISTICE"
                        }),
                        font(&fonts.display, 13.0),
                        TextColor(MAIN),
                    ));
                });
            });
        }
        // --- The wire: latest war ticker lines, newest last. -------------
        if !military.war_log.is_empty() {
            p.spawn((
                Text::new("-- THE WIRE --"),
                font(&fonts.mono_bold, 11.0),
                TextColor(Color::srgb(0.62, 0.66, 0.70)),
            ));
            let recent = military.war_log.iter().rev().take(8).rev();
            for (tick, line) in recent {
                let days_ago = clock.tick.saturating_sub(*tick) / 24;
                let stamp = if days_ago == 0 {
                    "TODAY".to_string()
                } else {
                    format!("-{days_ago}D")
                };
                p.spawn((
                    Text::new(format!("[{stamp}] {line}")),
                    font(&fonts.mono, 10.5),
                    TextColor(Color::srgb(0.7, 0.73, 0.76)),
                ));
            }
        }
    });
}

fn war_buttons(
    buttons: Query<(&Interaction, &WarButton), Changed<Interaction>>,
    military: Res<Military>,
    player: Option<Res<PlayerNation>>,
    mut pending: ResMut<PendingCommands>,
) {
    let Some(player) = player else { return };
    let me = &player.0;
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            WarButton::TogglePosture(enemy) => {
                let next = match military.posture(me, enemy) {
                    Posture::Advance => Posture::Hold,
                    Posture::Hold => Posture::Advance,
                };
                pending.push(SimCommand::SetPosture {
                    country: me.clone(),
                    enemy: enemy.clone(),
                    posture: next,
                });
            }
            WarButton::ToggleArmistice(enemy) => {
                let offer = !military.has_offered_armistice(me, enemy);
                pending.push(SimCommand::SetArmisticeOffer {
                    country: me.clone(),
                    enemy: enemy.clone(),
                    offer,
                });
            }
        }
    }
}
