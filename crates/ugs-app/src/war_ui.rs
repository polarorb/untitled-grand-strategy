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
use ugs_sim::intel::{Domain, Intel};
use ugs_sim::military::{
    tuning, Archetype, FormationId, Posture, Readiness, TheaterId, TheaterPosture,
};
use ugs_sim::planning::Economies;
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
    /// Set the country posture explicitly (radio segments, not a cycle).
    SetPostureTo(ugs_data::CountryTag, Posture),
    ToggleArmistice(ugs_data::CountryTag),
    Tab(WarTab),
    /// Raise one division of this archetype, home = selected province.
    Raise(Archetype),
    ToggleReadiness(FormationId),
    /// Cycle a formation through my theaters (then unassigned).
    CycleFormationTheater(FormationId),
    /// Batch readiness for a whole theater group (None = unassigned).
    GroupReadiness(Option<TheaterId>, bool),
    NewTheater,
    /// Set a theater posture explicitly (radio segments).
    SetTheaterPostureTo(TheaterId, TheaterPosture),
    /// Set the echelon share explicitly (radio segments).
    SetEchelonTo(TheaterId, u16),
    ToggleTheaterRoe(TheaterId, ugs_data::CountryTag),
    DeleteTheater(TheaterId),
    /// Toggle map paint mode for this theater's provinces.
    PaintMode(TheaterId),
    /// Toggle map objective-picking mode for this theater.
    ObjectiveMode(TheaterId),
}

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
enum WarTab {
    #[default]
    Overview,
    Forces,
    Theaters,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum EditMode {
    #[default]
    None,
    Paint,
    Objectives,
}

/// Which theater map clicks are editing, and how.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct TheaterEdit {
    theater: Option<TheaterId>,
    mode: EditMode,
}

/// Fixed palette for theater tints on the map and in lists.
const THEATER_COLORS: [Color; 6] = [
    Color::srgb(0.83, 0.69, 0.36),
    Color::srgb(0.45, 0.70, 0.85),
    Color::srgb(0.55, 0.80, 0.50),
    Color::srgb(0.85, 0.50, 0.65),
    Color::srgb(0.70, 0.55, 0.90),
    Color::srgb(0.90, 0.60, 0.35),
];

pub(crate) fn theater_color(id: TheaterId) -> Color {
    THEATER_COLORS[id.0 as usize % THEATER_COLORS.len()]
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

/// Estimate band width in permille from the player's military
/// penetration of `subject`: ±35% blind, ±5% floor at deep intel.
fn intel_width(
    intel: &Intel,
    viewer: Option<&ugs_data::CountryTag>,
    subject: &ugs_data::CountryTag,
) -> u64 {
    let pen = viewer
        .map(|v| intel.knowledge(v, subject, Domain::Military))
        .unwrap_or(0) as u64;
    50 + (1000 - pen.min(1000)) * 300 / 1000
}

/// Estimated enemy men as a low-high band; `width` permille half-band.
fn est_men_range(true_men: u64, seed: u64, width: u64) -> (u64, u64) {
    // Center jitter scales with width too: poor intel is biased, not
    // just wide.
    let jitter = 1000 - width / 2 + mix(seed) % (width + 1);
    let center = true_men * jitter / 1000;
    (
        round_sig2(center * (1000 - width) / 1000),
        round_sig2(center * (1000 + width) / 1000),
    )
}

/// Estimated enemy division count band; width in permille.
fn est_div_range(count: u32, seed: u64, width: u64) -> (u32, u32) {
    let span = (count as u64 * width / 1000).max(1) as u32;
    let center =
        (count as i64 + (mix(seed) % (span as u64 * 2 + 1)) as i64 - span as i64).max(1) as u32;
    (center.saturating_sub(span).max(1), center + span)
}

pub struct WarUiPlugin;

impl Plugin for WarUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WarTab>();
        app.init_resource::<TheaterEdit>();
        app.add_systems(
            Update,
            (
                announce_player_country,
                theater_map_edit,
                // Chained so each spawner's deferred `spawn` is flushed before
                // the next one checks the `With<EventModal>` guard — unordered,
                // all three can spawn a modal in the same frame and the popups
                // stack on top of each other.
                (show_event_popups, show_dynamic_popups, show_notices).chain(),
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
        && player
            .as_ref()
            .map(|p| Some(&p.0) == event.country.as_ref())
            .unwrap_or(false);
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
    intel: Res<Intel>,
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
            .or_insert(Stack {
                count: 0,
                men: 0,
                strength: 0,
                cohesion: 0,
            });
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
            let width = intel_width(&intel, player.as_ref().map(|p| &p.0), &tag);
            let (lo, hi) = est_div_range(stack.count, seed, width);
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
fn pulse_battle_markers(time: Res<Time>, mut markers: Query<&mut Transform, With<BattleMarker>>) {
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
        let Some(from_id) = f.last_location else {
            continue;
        };
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
    intel: Res<Intel>,
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
    let to_break =
        |cohesion: u64, loss: u64| cohesion.saturating_sub(tuning::RETREAT_COHESION) / loss.max(1);
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
                reasons.push((
                    -qual_gap / 10,
                    format!("ENEMY QUALITY EDGE ({:+}%)", -qual_gap / 10),
                ));
            }
            if b.defender_men > b.attacker_men {
                reasons.push((
                    (b.defender_men as i64 - b.attacker_men as i64) / 1000,
                    "YOU ARE OUTNUMBERED".into(),
                ));
            }
        } else {
            if qual_gap > 30 {
                reasons.push((
                    qual_gap / 10,
                    format!("ENEMY QUALITY EDGE ({:+}%)", qual_gap / 10),
                ));
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
                    let w = owners
                        .first()
                        .map(|subj| intel_width(&intel, me, subj))
                        .unwrap_or(350);
                    let (lo, hi) = est_div_range(divisions, enemy_seed, w);
                    let (mlo, mhi) = est_men_range(men, enemy_seed ^ 0x9e37, w);
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

/// Sim-generated decisions (crises, commander requests): the teletype
/// modal with live option buttons, resolved through the command log.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn show_dynamic_popups(
    mut commands: Commands,
    fired: Res<FiredEvents>,
    fonts: Res<Fonts>,
    audio: Res<AudioHandles>,
    player: Option<Res<PlayerNation>>,
    clock: Res<ugs_sim::SimClock>,
    mut speed: ResMut<GameSpeed>,
    mut shown: Local<Vec<String>>,
    existing: Query<(), With<EventModal>>,
) {
    if !existing.is_empty() {
        return;
    }
    let Some(choice) = fired
        .dynamic
        .iter()
        .find(|d| !shown.iter().any(|s| s == &d.id))
    else {
        return;
    };
    shown.push(choice.id.clone());
    shown.retain(|s| s == &choice.id || fired.dynamic.iter().any(|d| &d.id == s));
    let is_mine = player
        .as_ref()
        .map(|p| p.0 == choice.country)
        .unwrap_or(false);
    speed.paused = true;
    commands.spawn((
        AudioPlayer::new(audio.alert.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.4)),
    ));
    let hours_left = choice.deadline_tick.saturating_sub(clock.tick);
    commands
        .spawn((
            EventModal,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(20.0),
                margin: UiRect::left(Val::Px(-270.0)),
                width: Val::Px(540.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                padding: UiRect::all(Val::Px(22.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|m| {
            m.spawn((
                Text::new("*** CRITIC / FLASH ***"),
                font(&fonts.mono_bold, 13.0),
                TextColor(Color::srgb(0.9, 0.4, 0.35)),
            ));
            m.spawn((
                Text::new(choice.title.clone()),
                font(&fonts.display, 22.0),
                TextColor(MAIN),
            ));
            m.spawn((
                Text::new(choice.body.clone()),
                font(&fonts.mono, 13.5),
                TextColor(MAIN),
            ));
            m.spawn((
                Text::new(format!("RESPONSE REQUIRED WITHIN {hours_left} HOURS")),
                font(&fonts.mono, 11.5),
                TextColor(Color::srgb(0.62, 0.66, 0.70)),
            ));
            if is_mine {
                for (i, label) in choice.options.iter().enumerate() {
                    m.spawn((
                        Button,
                        ChoiceButton {
                            event_id: choice.id.clone(),
                            option: i as u8,
                        },
                        Node {
                            padding: UiRect::axes(Val::Px(20.0), Val::Px(9.0)),
                            ..default()
                        },
                        BackgroundColor(if label.contains("ESCALATE") {
                            Color::srgb(0.5, 0.22, 0.18)
                        } else {
                            Color::srgba(0.14, 0.17, 0.21, 0.95)
                        }),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(label.clone()),
                            font(&fonts.display, 15.0),
                            TextColor(Color::srgb(0.98, 0.95, 0.88)),
                        ));
                    });
                }
            } else {
                m.spawn((
                    Text::new(format!("DECISION RESTS WITH {}", choice.country.0)),
                    font(&fonts.mono, 12.0),
                    TextColor(Color::srgb(0.62, 0.66, 0.70)),
                ));
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
            m.spawn((
                Text::new(title),
                font(&fonts.display, 22.0),
                TextColor(MAIN),
            ));
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
/// Dev shortcut: UGS_PANEL=war boots with the panel open;
/// war-forces / war-theaters open straight into that tab.
fn toggle_war_panel(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    panel: Query<Entity, With<WarPanel>>,
    mut tab: ResMut<WarTab>,
    mut booted: Local<bool>,
) {
    let env = std::env::var("UGS_PANEL").unwrap_or_default();
    let auto_open = !*booted && env.starts_with("war");
    if auto_open {
        match env.as_str() {
            "war-forces" => *tab = WarTab::Forces,
            "war-theaters" => *tab = WarTab::Theaters,
            _ => {}
        }
    }
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
    crises: Res<ugs_sim::crisis::Crises>,
    intel: Res<Intel>,
    econ: Res<Economies>,
    clock: Res<SimClock>,
    fonts: Res<Fonts>,
    tab: Res<WarTab>,
    edit: Res<TheaterEdit>,
    selected: Res<Selected>,
    player: Option<Res<PlayerNation>>,
    panel: Query<Entity, Added<WarPanel>>,
    panel_any: Query<Entity, With<WarPanel>>,
) {
    // Rebuild when panel opens, a tab/edit change happens, the selected
    // province changes (raise-home hint), or the military picture moves.
    let rebuild = !panel.is_empty()
        || military.is_changed()
        || tab.is_changed()
        || edit.is_changed()
        || selected.is_changed();
    if !rebuild {
        return;
    }
    let Ok(panel) = panel_any.single() else {
        return;
    };
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
        // Tab row: the command layer lives behind FORCES and THEATERS.
        p.spawn(Node {
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            for (t, label, tip) in [
                (
                    WarTab::Overview,
                    "OVERVIEW",
                    "The war at a glance: manpower pipeline, momentum, crises, posture and armistice controls per enemy.",
                ),
                (
                    WarTab::Forces,
                    "FORCES",
                    "Your army roster: raise divisions from the military stockpile, set Active/Reserve readiness, assign divisions to theaters.",
                ),
                (
                    WarTab::Theaters,
                    "THEATERS",
                    "Command areas: paint provinces, set posture, objectives, echelon share, and rules of engagement. Divisions execute; you direct.",
                ),
            ] {
                crate::widgets::segment(row, WarButton::Tab(t), label, *tab == t, &fonts, 12.0, tip);
            }
        });
        match *tab {
            WarTab::Forces => {
                forces_tab(p, me, &military, &world, &econ, &selected, &fonts);
                return;
            }
            WarTab::Theaters => {
                theaters_tab(p, me, &military, &world, &edit, &fonts);
                return;
            }
            WarTab::Overview => {}
        }
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
        crate::widgets::tipped_text(
            p,
            "MANPOWER PIPELINE".into(),
            &fonts,
            11.0,
            Color::srgb(0.62, 0.66, 0.70),
            "Conservation of men: population feeds the reserve pool (1.5% at peace, +0.2%/month at war), the pool fills fielded divisions, casualties leave permanently - debited from each division's real home province.",
        );
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
        for c in &crises.active {
            let ours = crises.resolve_of(me);
            let theirs = crises.resolve_of(&c.other(me));
            let seed = c.id as u64 ^ month;
            let (lo, hi) = {
                let center = (theirs
                    + ((mix(seed) % 21) as i64 - 10))
                    .clamp(5, 95);
                ((center - 10).max(0), (center + 10).min(100))
            };
            p.spawn((
                Text::new(format!(
                    "CRISIS: {} -- RUNG {}/8 -- MOVE: {}",
                    c.title, c.rung, c.ball.0
                )),
                font(&fonts.mono_bold, 11.0),
                TextColor(Color::srgb(0.9, 0.45, 0.4)),
            ));
            p.spawn((
                Text::new(format!(
                    "  OUR RESOLVE {ours} (EXACT)   THEIRS EST {lo}-{hi}"
                )),
                font(&fonts.mono, 10.5),
                TextColor(Color::srgb(0.75, 0.78, 0.82)),
            ));
        }
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
            let ew = intel_width(&intel, Some(me), &enemy);
            let (dlo, dhi) = est_div_range(enemy_divs, seed, ew);
            let (mlo, mhi) = est_men_range(enemy_men, seed ^ 0x9e37, ew);
            let (klo, khi) = est_men_range(enemy_dead, seed ^ 0x51ed, ew);
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
            crate::widgets::tipped_text(
                p,
                format!(
                    "MOMENTUM {momentum:+}   GROUND {occ_term:+}  EXCHANGE {ex_term:+}  BATTLES {battle_term:+}"
                ),
                &fonts,
                10.5,
                Color::srgb(0.75, 0.78, 0.82),
                "Who is winning, decomposed: GROUND = provinces taken vs lost, EXCHANGE = casualty ratio, BATTLES = recent win/loss run. -100 to +100; every term is shown so the number explains itself.",
            );
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
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("POSTURE:"),
                    font(&fonts.mono, 10.5),
                    TextColor(Color::srgb(0.62, 0.66, 0.70)),
                ));
                crate::widgets::segment(
                    row,
                    WarButton::SetPostureTo(enemy.clone(), Posture::Hold),
                    "HOLD",
                    posture == Posture::Hold,
                    &fonts,
                    11.0,
                    "Hold: your divisions defend the line and never enter enemy territory. Sets the default directive for your auto-theater; player-made theaters override it.",
                );
                crate::widgets::segment(
                    row,
                    WarButton::SetPostureTo(enemy.clone(), Posture::Advance),
                    "ADVANCE",
                    posture == Posture::Advance,
                    &fonts,
                    11.0,
                    "Advance: your divisions push into enemy territory toward the enemy capital. Sets the default directive for your auto-theater; player-made theaters override it.",
                );
                crate::widgets::toggle(
                    row,
                    WarButton::ToggleArmistice(enemy.clone()),
                    if offered { "ARMISTICE OFFERED" } else { "OFFER ARMISTICE" },
                    offered,
                    false,
                    &fonts,
                    11.0,
                    "Offer to end the war at the current line of control. The war ends when BOTH sides are willing; the AI grows willing after long wars with a static front, or when its army is broken. Click again to retract.",
                );
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

/// FORCES tab: the stockpile ledger, raising, and the division list
/// grouped by theater with readiness and assignment controls. Decisions
/// only — no per-unit movement exists anywhere in this UI.
#[allow(clippy::too_many_arguments)]
fn forces_tab(
    p: &mut ChildSpawnerCommands,
    me: &ugs_data::CountryTag,
    military: &Military,
    world: &World1950,
    econ: &Economies,
    selected: &Selected,
    fonts: &Fonts,
) {
    let dim = Color::srgb(0.62, 0.66, 0.70);
    let stock = econ.industry.get(me).map(|s| s.military_stock).unwrap_or(0);
    let burn_centi: u64 = military
        .formations
        .values()
        .filter(|f| &f.owner == me)
        .map(|f| {
            let mut c = f.archetype.upkeep_centi();
            if f.readiness.stood_down() {
                c = c * tuning::RESERVE_UPKEEP_PERMILLE / 1000;
            }
            let overseas = world
                .0
                .provinces
                .get(&f.location)
                .is_some_and(|pr| &pr.owner != me);
            if overseas {
                c *= tuning::OVERSEAS_UPKEEP_MULT;
            }
            c
        })
        .sum();
    crate::widgets::tipped_text(
        p,
        format!(
            "STOCKPILE {stock}   UPKEEP ~{}.{}/MO",
            burn_centi / 100,
            burn_centi % 100 / 10
        ),
        fonts,
        11.5,
        MAIN,
        "Military stockpile: produced monthly by your economy's military allocation. Raising divisions spends it; every division costs monthly upkeep (3x when based overseas; reserves 20%). An unpaid army decays and melts.",
    );
    if military.upkeep_arrears.contains_key(me) {
        p.spawn((
            Text::new("ARMY UNPAID -- QUALITY DECAYING, STRENGTH MELTING"),
            font(&fonts.mono_bold, 11.0),
            TextColor(Color::srgb(0.92, 0.4, 0.3)),
        ));
    }

    // Raising: home = the selected province (own or co-belligerent soil).
    let home = selected.0.filter(|id| {
        world
            .0
            .provinces
            .get(id)
            .is_some_and(|_| military.may_operate(&world.0, me, *id))
    });
    let home_name = home
        .and_then(|id| world.0.provinces.get(&id))
        .map(|pr| pr.name.to_uppercase());
    crate::widgets::tipped_text(
        p,
        match &home_name {
            Some(n) => format!("RAISE AT {n} (OVERSEAS BASING = TRAINING IN PLACE)"),
            None => "RAISE: SELECT A HOME PROVINCE ON THE MAP".to_string(),
        },
        fonts,
        10.5,
        if home_name.is_some() { MAIN } else { dim },
        "New divisions are raised from a home province you select on the map — your own soil or a co-belligerent's. They spawn green (10% strength, untrained), fill from the manpower pool, and train over 3-5 months. Casualties come off the home province's real population.",
    );
    p.spawn(Node {
        column_gap: Val::Px(6.0),
        ..default()
    })
    .with_children(|row| {
        for (arch, label) in [
            (Archetype::Infantry, "INFANTRY"),
            (Archetype::Motorized, "MOTORIZED"),
            (Archetype::Armor, "ARMOR"),
        ] {
            let affordable = stock >= arch.raise_cost() && home_name.is_some();
            let tip = match arch {
                Archetype::Infantry => "Raise one infantry division (10,000 men): cheap, tough on defense, slow. Trains in 90 days; fights green at half weight before that.",
                Archetype::Motorized => "Raise one motorized division: balanced and fast-moving. Trains in 120 days.",
                Archetype::Armor => "Raise one armor division: the offensive punch, fragile on defense, highest upkeep. Trains in 150 days.",
            };
            row.spawn((
                Button,
                WarButton::Raise(arch),
                crate::widgets::Tooltip::of(tip),
                Node {
                    padding: UiRect::axes(Val::Px(9.0), Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(if affordable {
                    Color::srgb(0.30, 0.42, 0.28)
                } else {
                    Color::srgba(0.14, 0.17, 0.21, 0.95)
                }),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new(format!("{label} ({})", arch.raise_cost())),
                    font(&fonts.display, 12.0),
                    TextColor(MAIN),
                ));
            });
        }
    });

    // Division list, grouped by theater. Batch rows keep 30+ divisions
    // from becoming bookkeeping.
    let mut groups: Vec<(Option<TheaterId>, String, Color)> = military
        .theaters
        .iter()
        .filter(|(_, t)| &t.owner == me)
        .map(|(id, t)| (Some(*id), t.name.clone(), theater_color(*id)))
        .collect();
    groups.push((None, "UNASSIGNED".into(), dim));
    let mut rows_left: i32 = 16;
    for (gid, gname, gcolor) in groups {
        let mut members: Vec<(FormationId, &ugs_sim::military::Formation)> = military
            .formations
            .iter()
            .filter(|(_, f)| &f.owner == me && f.theater == gid)
            .map(|(id, f)| (*id, f))
            .collect();
        members.sort_by_key(|(id, _)| *id);
        if members.is_empty() {
            continue;
        }
        p.spawn(Node {
            column_gap: Val::Px(6.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Node {
                    width: Val::Px(8.0),
                    height: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(gcolor),
            ));
            row.spawn((
                Text::new(format!("{gname} ({})", members.len())),
                font(&fonts.mono_bold, 11.0),
                TextColor(gcolor),
            ));
            for (label, active, tip) in [
                (
                    "ALL ACT",
                    true,
                    "Mobilize every reserve division in this group. Reserve to Active takes 21 days and is a public signal (tension at peace).",
                ),
                (
                    "ALL RES",
                    false,
                    "Stand the whole group down to Reserve: 20% upkeep, but immobile, defending at reduced weight, and 21 days from being fieldable again.",
                ),
            ] {
                row.spawn((
                    Button,
                    WarButton::GroupReadiness(gid, active),
                    crate::widgets::Tooltip::of(tip),
                    Node {
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.14, 0.17, 0.21, 0.95)),
                ))
                .with_children(|b| {
                    b.spawn((Text::new(label), font(&fonts.mono, 9.5), TextColor(MAIN)));
                });
            }
        });
        for (id, f) in &members {
            if rows_left <= 0 {
                break;
            }
            rows_left -= 1;
            let readiness = match f.readiness {
                Readiness::Mobilizing { days_left } => format!("  {days_left}D LEFT"),
                _ => String::new(),
            };
            let training = if f.training < 1000 {
                format!("  TRN {}%", f.training / 10)
            } else {
                String::new()
            };
            p.spawn(Node {
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                crate::widgets::tipped_text(
                    row,
                    format!(
                        "{}  S{}% C{}%{training}{readiness}",
                        f.name,
                        f.strength / 10,
                        f.cohesion / 10,
                    ),
                    fonts,
                    10.0,
                    MAIN,
                    "S = strength (men and equipment; dies slowly, refilled from the manpower pool). C = cohesion (fighting spirit; breaks battles, recovers fast). TRN = training - green divisions fight at half weight until trained.",
                );
                let state_label = match f.readiness {
                    Readiness::Active => "ACTIVE",
                    Readiness::Reserve => "RESERVE",
                    Readiness::Mobilizing { .. } => "MOBILIZING",
                };
                crate::widgets::toggle(
                    row,
                    WarButton::ToggleReadiness(*id),
                    state_label,
                    !f.readiness.stood_down(),
                    false,
                    fonts,
                    9.0,
                    "Readiness. Lit = Active (full upkeep, holds a front slot). Click to stand down to Reserve (20% upkeep, immobile, weak on defense) or to mobilize back — mobilization takes 21 days and is a public signal.",
                );
                row.spawn((
                    Button,
                    WarButton::CycleFormationTheater(*id),
                    crate::widgets::Tooltip::of(
                        "Reassign this division to your next theater (cycles through your theaters, then Unassigned). Unassigned divisions walk home and sit.",
                    ),
                    Node {
                        padding: UiRect::axes(Val::Px(5.0), Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.14, 0.17, 0.21, 0.95)),
                ))
                .with_children(|b| {
                    b.spawn((Text::new("THTR>"), font(&fonts.mono, 9.0), TextColor(MAIN)));
                });
            });
        }
        if rows_left <= 0 {
            let total: usize = military
                .formations
                .values()
                .filter(|f| &f.owner == me)
                .count();
            p.spawn((
                Text::new(format!("... {total} DIVISIONS TOTAL (LIST TRUNCATED)")),
                font(&fonts.mono, 10.0),
                TextColor(dim),
            ));
            break;
        }
    }
    let pool = military.manpower.get(me).copied().unwrap_or(0);
    crate::widgets::tipped_text(
        p,
        format!("MANPOWER POOL {}", fmt_men(pool)),
        fonts,
        10.5,
        dim,
        "Trained men available to fill divisions, drawn from your real population (1.5% at peace, +0.2%/month at war). Reinforcement stalls when it runs dry.",
    );
}

/// THEATERS tab: create, paint, direct. Each theater is 3-6 decisions:
/// boundary, posture, objectives, echelon, and ROE lines.
fn theaters_tab(
    p: &mut ChildSpawnerCommands,
    me: &ugs_data::CountryTag,
    military: &Military,
    world: &World1950,
    edit: &TheaterEdit,
    fonts: &Fonts,
) {
    let dim = Color::srgb(0.62, 0.66, 0.70);
    p.spawn(Node {
        column_gap: Val::Px(6.0),
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            Button,
            WarButton::NewTheater,
            crate::widgets::Tooltip::of(
                "Create an empty theater, then PAINT provinces into it on the map and assign divisions from the FORCES tab. You direct theaters; divisions position themselves.",
            ),
            Node {
                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.30, 0.42, 0.28)),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new("NEW THEATER"),
                font(&fonts.display, 12.0),
                TextColor(MAIN),
            ));
        });
    });
    match edit.mode {
        EditMode::Paint => {
            p.spawn((
                Text::new("PAINTING: CLICK PROVINCES ON THE MAP TO ADD/REMOVE"),
                font(&fonts.mono_bold, 10.5),
                TextColor(ACCENT),
            ));
        }
        EditMode::Objectives => {
            p.spawn((
                Text::new("OBJECTIVES: CLICK UP TO 3 PROVINCES ON THE MAP"),
                font(&fonts.mono_bold, 10.5),
                TextColor(ACCENT),
            ));
        }
        EditMode::None => {}
    }
    let mine: Vec<(TheaterId, &ugs_sim::military::Theater)> = military
        .theaters
        .iter()
        .filter(|(_, t)| &t.owner == me)
        .map(|(id, t)| (*id, t))
        .collect();
    if mine.is_empty() {
        p.spawn((
            Text::new("NO THEATERS -- YOUR DIVISIONS SIT AT HOME UNTIL COMMANDED"),
            font(&fonts.mono, 10.5),
            TextColor(dim),
        ));
    }
    for (id, t) in mine {
        let assigned = military
            .formations
            .values()
            .filter(|f| f.theater == Some(id))
            .count();
        p.spawn(Node {
            column_gap: Val::Px(6.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Node {
                    width: Val::Px(10.0),
                    height: Val::Px(10.0),
                    ..default()
                },
                BackgroundColor(theater_color(id)),
            ));
            row.spawn((
                Text::new(format!("{}{}", t.name, if t.auto { " [AUTO]" } else { "" })),
                font(&fonts.body_medium, 13.0),
                TextColor(theater_color(id)),
            ));
        });
        p.spawn((
            Text::new(format!(
                "{} PROVINCES  {assigned} DIV  {} OBJ  ECHELON {}%",
                t.provinces.len(),
                t.objectives.len(),
                t.echelon_permille / 10,
            )),
            font(&fonts.mono, 10.0),
            TextColor(dim),
        ));
        p.spawn(Node {
            column_gap: Val::Px(5.0),
            ..default()
        })
        .with_children(|row| {
            // Posture: a three-way radio, the lit segment is the order
            // in force.
            for (posture, label, tip) in [
                (
                    TheaterPosture::Defend,
                    "DEFEND",
                    "Defend: hold the theater's front provinces; never enter enemy territory.",
                ),
                (
                    TheaterPosture::Probe,
                    "PROBE",
                    "Probe: push one province deep into adjacent enemy territory where the front allows — limited bites, no deep advance.",
                ),
                (
                    TheaterPosture::Offensive,
                    "OFFENSIVE",
                    "Offensive: roll the front forward into enemy territory, aiming at this theater's objectives.",
                ),
            ] {
                crate::widgets::segment(
                    row,
                    WarButton::SetTheaterPostureTo(id, posture),
                    label,
                    t.posture == posture,
                    fonts,
                    9.5,
                    tip,
                );
            }
        });
        p.spawn(Node {
            column_gap: Val::Px(5.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((Text::new("ECHELON:"), font(&fonts.mono, 9.5), TextColor(dim)));
            for permille in [0u16, 250, 500] {
                crate::widgets::segment(
                    row,
                    WarButton::SetEchelonTo(id, permille),
                    &format!("{}%", permille / 10),
                    t.echelon_permille == permille,
                    fonts,
                    9.5,
                    "Echelon: the share of this theater's divisions held back from the front as a rear reserve (the newest, greenest divisions are held back first).",
                );
            }
            let painting = edit.mode == EditMode::Paint && edit.theater == Some(id);
            crate::widgets::toggle(
                row,
                WarButton::PaintMode(id),
                "PAINT",
                painting,
                false,
                fonts,
                9.5,
                "Paint this theater's provinces: while lit, clicking provinces on the map adds or removes them (works while paused; the map switches to the WAR view). Click again to finish.",
            );
            let obj = edit.mode == EditMode::Objectives && edit.theater == Some(id);
            crate::widgets::toggle(
                row,
                WarButton::ObjectiveMode(id),
                "OBJECTIVES",
                obj,
                false,
                fonts,
                9.5,
                "Pick up to 3 objective provinces on the map: offensives aim their advance along the axes toward them. Click a set objective to clear it; click again to finish.",
            );
            row.spawn((
                Button,
                WarButton::DeleteTheater(id),
                crate::widgets::Tooltip::of(
                    "Disband this theater. Its divisions go unassigned and walk to their home provinces until re-assigned.",
                ),
                Node {
                    padding: UiRect::axes(Val::Px(7.0), Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.30, 0.14, 0.12, 0.95)),
            ))
            .with_children(|b| {
                b.spawn((Text::new("DEL"), font(&fonts.mono, 9.5), TextColor(MAIN)));
            });
        });
        // ROE: one line per enemy — "may not cross the Yalu" lives here.
        let enemies: Vec<ugs_data::CountryTag> = military
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
        if !enemies.is_empty() {
            p.spawn(Node {
                column_gap: Val::Px(5.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((Text::new("ROE:"), font(&fonts.mono, 9.5), TextColor(dim)));
                for enemy in enemies {
                    let banned = t.forbidden.contains(&enemy);
                    crate::widgets::toggle(
                        row,
                        WarButton::ToggleTheaterRoe(id, enemy.clone()),
                        &format!("NO ENTRY {}", enemy.0),
                        banned,
                        true,
                        fonts,
                        9.0,
                        "Rule of engagement: while lit, this theater's divisions may NEVER enter that country's soil — the sanctuary line ('may not cross the Yalu'). A hard constraint on movement, not a suggestion.",
                    );
                }
            });
        }
    }
    let world_name = world
        .0
        .nations_meta
        .get(me)
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| me.0.clone());
    p.spawn((
        Text::new(format!(
            "{} DIVISIONS FOLLOW THEATER DIRECTIVES -- NO UNIT MICRO EXISTS",
            world_name.to_uppercase()
        )),
        font(&fonts.mono, 9.5),
        TextColor(dim),
    ));
}

/// While a theater edit mode is armed, map clicks paint provinces or
/// place objectives instead of opening the battle inspector.
fn theater_map_edit(
    mut selected: ResMut<Selected>,
    edit: Res<TheaterEdit>,
    military: Res<Military>,
    player: Option<Res<PlayerNation>>,
    mut pending: ResMut<PendingCommands>,
) {
    if edit.mode == EditMode::None || !selected.is_changed() {
        return;
    }
    let Some(pid) = selected.0 else { return };
    let (Some(tid), Some(player)) = (edit.theater, player) else {
        return;
    };
    let Some(t) = military.theaters.get(&tid) else {
        return;
    };
    match edit.mode {
        EditMode::Paint => {
            let add = !t.provinces.contains(&pid);
            pending.push(SimCommand::PaintTheater {
                country: player.0.clone(),
                id: tid,
                province: pid,
                add,
            });
        }
        EditMode::Objectives => {
            let mut objectives = t.objectives.clone();
            if let Some(pos) = objectives.iter().position(|o| *o == pid) {
                objectives.remove(pos);
            } else {
                objectives.push(pid);
            }
            pending.push(SimCommand::SetTheaterObjectives {
                country: player.0.clone(),
                id: tid,
                objectives,
            });
        }
        EditMode::None => {}
    }
    selected.0 = None; // the click was an edit, not a selection
}

#[allow(clippy::too_many_arguments)]
fn war_buttons(
    buttons: Query<(&Interaction, &WarButton), Changed<Interaction>>,
    military: Res<Military>,
    selected: Res<Selected>,
    player: Option<Res<PlayerNation>>,
    mut tab: ResMut<WarTab>,
    mut edit: ResMut<TheaterEdit>,
    mut map_mode: ResMut<crate::map::MapMode>,
    mut pending: ResMut<PendingCommands>,
) {
    let Some(player) = player else { return };
    let me = &player.0;
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            WarButton::SetPostureTo(enemy, posture) => {
                pending.push(SimCommand::SetPosture {
                    country: me.clone(),
                    enemy: enemy.clone(),
                    posture: *posture,
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
            WarButton::Tab(t) => {
                *tab = *t;
                if *t != WarTab::Theaters {
                    edit.mode = EditMode::None;
                }
            }
            WarButton::Raise(archetype) => {
                if let Some(home) = selected.0 {
                    pending.push(SimCommand::RaiseFormation {
                        country: me.clone(),
                        archetype: *archetype,
                        home,
                        count: 1,
                    });
                }
            }
            WarButton::ToggleReadiness(id) => {
                if let Some(f) = military.formations.get(id) {
                    pending.push(SimCommand::SetReadiness {
                        country: me.clone(),
                        id: *id,
                        active: f.readiness.stood_down(),
                    });
                }
            }
            WarButton::CycleFormationTheater(id) => {
                let Some(f) = military.formations.get(id) else {
                    continue;
                };
                let mine: Vec<TheaterId> = military
                    .theaters
                    .iter()
                    .filter(|(_, t)| &t.owner == me)
                    .map(|(tid, _)| *tid)
                    .collect();
                let next = match f.theater {
                    None => mine.first().copied(),
                    Some(current) => {
                        let pos = mine.iter().position(|t| *t == current);
                        match pos {
                            Some(i) if i + 1 < mine.len() => Some(mine[i + 1]),
                            _ => None,
                        }
                    }
                };
                pending.push(SimCommand::AssignTheater {
                    country: me.clone(),
                    formation: *id,
                    theater: next,
                });
            }
            WarButton::GroupReadiness(gid, active) => {
                for (fid, f) in military
                    .formations
                    .iter()
                    .filter(|(_, f)| &f.owner == me && f.theater == *gid)
                {
                    // Only formations not already in the requested state.
                    if f.readiness.stood_down() != *active {
                        continue;
                    }
                    pending.push(SimCommand::SetReadiness {
                        country: me.clone(),
                        id: *fid,
                        active: *active,
                    });
                }
            }
            WarButton::NewTheater => {
                let n = military
                    .theaters
                    .values()
                    .filter(|t| &t.owner == me)
                    .count();
                pending.push(SimCommand::CreateTheater {
                    country: me.clone(),
                    name: format!("THEATER {}", n + 1),
                });
            }
            WarButton::SetTheaterPostureTo(id, posture) => {
                pending.push(SimCommand::SetTheaterPosture {
                    country: me.clone(),
                    id: *id,
                    posture: *posture,
                });
            }
            WarButton::SetEchelonTo(id, permille) => {
                pending.push(SimCommand::SetTheaterEchelon {
                    country: me.clone(),
                    id: *id,
                    permille: *permille,
                });
            }
            WarButton::ToggleTheaterRoe(id, target) => {
                if let Some(t) = military.theaters.get(id) {
                    pending.push(SimCommand::SetTheaterRoe {
                        country: me.clone(),
                        id: *id,
                        tag: target.clone(),
                        forbidden: !t.forbidden.contains(target),
                    });
                }
            }
            WarButton::DeleteTheater(id) => {
                pending.push(SimCommand::DeleteTheater {
                    country: me.clone(),
                    id: *id,
                });
                if edit.theater == Some(*id) {
                    edit.mode = EditMode::None;
                    edit.theater = None;
                }
            }
            WarButton::PaintMode(id) => {
                if edit.mode == EditMode::Paint && edit.theater == Some(*id) {
                    edit.mode = EditMode::None;
                } else {
                    edit.mode = EditMode::Paint;
                    edit.theater = Some(*id);
                    *map_mode = crate::map::MapMode::War;
                }
            }
            WarButton::ObjectiveMode(id) => {
                if edit.mode == EditMode::Objectives && edit.theater == Some(*id) {
                    edit.mode = EditMode::None;
                } else {
                    edit.mode = EditMode::Objectives;
                    edit.theater = Some(*id);
                    *map_mode = crate::map::MapMode::War;
                }
            }
        }
    }
}
