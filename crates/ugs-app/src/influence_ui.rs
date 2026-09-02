//! The politics panel (P): the battleground triage list, the
//! commitments ledger, and the one-page political dossier with its
//! verb buttons (docs/design/systems/influence.md). Foreign positions
//! render through political-domain penetration as an estimate; bands
//! and Kent words, never permille; every disabled verb prints why.

use bevy::prelude::*;
use ugs_data::{Alignment, CountryTag};
use ugs_sim::command::{PendingCommands, SimCommand};
use ugs_sim::influence::{
    self, battleground_weight, kent_word, next_election, Influence, InfluenceOpKind, ProgramKind,
};
use ugs_sim::intel::{Domain, Intel};
use ugs_sim::military::Military;
use ugs_sim::settlement::Settlements;
use ugs_sim::tension::GlobalTension;
use ugs_sim::SimClock;

use crate::{font, AppState, Fonts, PlayerNation, World1950};

const PANEL_BG: Color = Color::srgba(0.07, 0.09, 0.12, 0.97);
const ACCENT: Color = Color::srgb(0.83, 0.69, 0.36);
const MAIN: Color = Color::srgb(0.88, 0.89, 0.90);
const DIM: Color = Color::srgb(0.62, 0.66, 0.70);
const WEST: Color = Color::srgb(0.55, 0.68, 0.95);
const EAST: Color = Color::srgb(0.95, 0.55, 0.50);
const OLIVE: Color = Color::srgb(0.72, 0.74, 0.50);
const ROW_BG: Color = Color::srgba(0.12, 0.15, 0.19, 0.9);

#[derive(Component)]
struct InfluencePanel;

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum Tab {
    #[default]
    Ledger,
    Commitments,
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum Chip {
    #[default]
    Battlegrounds,
    MyBloc,
    Contests,
    All,
}

/// Which country's dossier is open (None = the ledger).
#[derive(Resource, Default)]
struct Dossier(Option<CountryTag>);

#[derive(Component, Clone)]
enum InfluenceButton {
    Tab(Tab),
    Chip(Chip),
    Open(CountryTag),
    Close,
    Start(CountryTag, ProgramKind, u8),
    Stop(CountryTag),
    Op(CountryTag, InfluenceOpKind),
    Cancel(CountryTag),
}

pub struct InfluenceUiPlugin;

impl Plugin for InfluenceUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tab>();
        app.init_resource::<Chip>();
        app.init_resource::<Dossier>();
        app.add_systems(
            Update,
            (toggle_panel, influence_buttons, refresh_panel)
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// P toggles the panel. Dev shortcut: UGS_PANEL=influence boots open.
fn toggle_panel(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    panel: Query<Entity, With<InfluencePanel>>,
    mut booted: Local<bool>,
) {
    let auto_open = !*booted && std::env::var("UGS_PANEL").as_deref() == Ok("influence");
    *booted = true;
    if !keys.just_pressed(KeyCode::KeyP) && !auto_open {
        return;
    }
    if let Ok(e) = panel.single() {
        commands.entity(e).despawn();
    } else {
        commands.spawn((
            InfluencePanel,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(56.0),
                width: Val::Px(620.0),
                max_height: Val::Percent(88.0),
                overflow: Overflow::scroll_y(),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                padding: UiRect::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ));
    }
}

fn band_color(a: Alignment) -> Color {
    match a {
        Alignment::WesternBloc => WEST,
        Alignment::EasternBloc => EAST,
        Alignment::NonAligned => OLIVE,
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

/// How the player sees a country's position: exact at home and at
/// EXTENSIVE coverage, a bracket at PARTIAL/LIMITED, the band alone
/// below that. Bands are public (the flags fly); depth is not.
fn position_text(
    influence: &Influence,
    intel: &Intel,
    me: &CountryTag,
    tag: &CountryTag,
    tick: u64,
) -> String {
    let pos = influence.position_of(tag);
    let depth = influence.depth_label(tag, tick);
    if me == tag {
        return format!("{pos:+} {depth}");
    }
    let pen = intel.knowledge(me, tag, Domain::Political);
    if pen >= 750 {
        format!("{pos:+} {depth}")
    } else if pen >= 250 {
        let width = ((1000 - pen) / 10) as i16;
        let bucket = 50;
        let center = (pos / bucket) * bucket;
        format!("{center:+}±{width} {depth} (EST.)")
    } else {
        format!("{depth} (NO COVERAGE)")
    }
}

/// The rival's commitment on a target, through the player's coverage.
fn theirs_text(influence: &Influence, intel: &Intel, me: &CountryTag, tag: &CountryTag) -> String {
    let rivals: Vec<&CountryTag> = influence
        .programs
        .keys()
        .filter(|(s, t)| t == tag && s != me)
        .map(|(s, _)| s)
        .collect();
    if rivals.is_empty() {
        return "-".into();
    }
    let pen = intel.knowledge(me, tag, Domain::Political);
    if pen >= 750 {
        rivals
            .iter()
            .map(|s| {
                let p = &influence.programs[&((*s).clone(), tag.clone())];
                format!("{} {} T{}", s.0, p.kind.label(), p.tier)
            })
            .collect::<Vec<_>>()
            .join(", ")
    } else if pen >= 500 {
        format!(
            "{} ACTIVE",
            rivals
                .iter()
                .map(|s| s.0.as_str())
                .collect::<Vec<_>>()
                .join("/")
        )
    } else {
        "?".into()
    }
}

fn next_text(
    influence: &Influence,
    world: &World1950,
    clock: &SimClock,
    tag: &CountryTag,
) -> String {
    if let Some((_, days, e)) = next_election(&world.0, influence, clock, tag) {
        return format!("ELECTION {}D: {}", days, e.stake);
    }
    if let Some(t) = influence.contested_until.get(tag) {
        if *t > clock.tick {
            return format!("CONTEST CLOSES {}MO", (t - clock.tick) / (30 * 24));
        }
    }
    if let Some(l) = influence.lock.get(tag) {
        if l.until_tick > clock.tick {
            return if l.until_tick == u64::MAX {
                format!("LOCKED: {}", l.label)
            } else {
                format!(
                    "{} LAPSES {}MO",
                    l.label,
                    (l.until_tick - clock.tick) / (30 * 24)
                )
            };
        }
    }
    "-".into()
}

fn trend_text(influence: &Influence, tag: &CountryTag) -> String {
    let Some(v) = influence.pressures.get(tag) else {
        return "STEADY".into();
    };
    let sum: i32 = v.iter().map(|p| p.delta as i32).sum();
    let cause = v
        .iter()
        .max_by_key(|p| p.delta.abs())
        .map(|p| p.label.clone())
        .unwrap_or_default();
    match sum {
        s if s > 0 => format!("WEST {s:+} ({cause})"),
        s if s < 0 => format!("EAST {s:+} ({cause})"),
        _ => "STEADY".into(),
    }
}

#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
fn refresh_panel(
    mut commands: Commands,
    influence: Res<Influence>,
    intel: Res<Intel>,
    military: Res<Military>,
    tension: Res<GlobalTension>,
    settlements: Res<Settlements>,
    world: Res<World1950>,
    clock: Res<SimClock>,
    fonts: Res<Fonts>,
    tab: Res<Tab>,
    chip: Res<Chip>,
    dossier: Res<Dossier>,
    player: Option<Res<PlayerNation>>,
    panel: Query<Entity, Added<InfluencePanel>>,
    panel_any: Query<Entity, With<InfluencePanel>>,
    mut shown_sig: Local<u64>,
) {
    // A cheap signature instead of change detection: the resource is
    // touched hourly, but the panel only needs to follow real moves.
    let month = clock.date.year as u64 * 12 + clock.date.month as u64;
    let sig = month
        ^ (influence.wire.len() as u64) << 8
        ^ (influence.programs.len() as u64) << 16
        ^ (influence.ops.len() as u64) << 24
        ^ (influence.pressures.len() as u64) << 32
        ^ influence
            .wire
            .last()
            .map(|(t, _)| *t)
            .unwrap_or(0)
            .rotate_left(40);
    let rebuild = !panel.is_empty()
        || tab.is_changed()
        || chip.is_changed()
        || dossier.is_changed()
        || *shown_sig != sig;
    if !rebuild {
        return;
    }
    *shown_sig = sig;
    let Ok(panel) = panel_any.single() else {
        return;
    };
    commands.entity(panel).despawn_related::<Children>();
    let data = &world.0;
    let Some(me) = player.as_ref().map(|p| p.0.clone()) else {
        commands.entity(panel).with_children(|p| {
            p.spawn((
                Text::new("POLITICAL AFFAIRS -- OBSERVER"),
                font(&fonts.display, 16.0),
                TextColor(DIM),
            ));
        });
        return;
    };
    let my_band = military.alignment_of(data, &me);
    let tick = clock.tick;

    commands.entity(panel).with_children(|p| {
        p.spawn((
            Text::new("POLITICAL AFFAIRS -- EYES ONLY"),
            font(&fonts.display, 16.0),
            TextColor(ACCENT),
        ));
        p.spawn((
            Text::new(format!(
                "COMMITMENTS {}/{}   ACTIVE OPERATION {}/{}   LEGITIMACY {}   TENSION {}",
                influence.programs_of(&me),
                influence.slots_of(&me),
                influence.ops_of(&me),
                influence.op_slots_of(&me),
                settlements.legitimacy_of(&me),
                tension.band()
            )),
            font(&fonts.mono, 10.5),
            TextColor(DIM),
        ));

        if let Some(tag) = dossier.0.clone() {
            dossier_page(
                p, &influence, &intel, &military, &tension, &world, &clock, &fonts, &me, &tag,
            );
            return;
        }

        // Tabs.
        p.spawn(Node {
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            for (t, label, tip) in [
                (
                    Tab::Ledger,
                    "LEDGER",
                    "The triage list: battlegrounds, your bloc, open contests. Click a row for the dossier and the verbs.",
                ),
                (
                    Tab::Commitments,
                    "COMMITMENTS",
                    "Your standing programs and the operation in preparation, against your slot caps.",
                ),
            ] {
                crate::widgets::segment(row, InfluenceButton::Tab(t), label, *tab == t, &fonts, 11.0, tip);
            }
        });

        match *tab {
            Tab::Commitments => {
                commitments_tab(p, &influence, &world, &clock, &fonts, &me);
            }
            Tab::Ledger => {
                // Chips.
                p.spawn(Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|row| {
                    for (c, label, tip) in [
                        (Chip::Battlegrounds, "BATTLEGROUNDS", "The sourced contested set the standings score over, plus any open contest."),
                        (Chip::MyBloc, "MY BLOC", "The mirror: your own bloc's members, satellites and allies, and how firmly they hold."),
                        (Chip::Contests, "CONTESTS", "Open independence windows and elections within six months."),
                        (Chip::All, "ALL", "Every state. Read the map first; this list is long."),
                    ] {
                        crate::widgets::segment(row, InfluenceButton::Chip(c), label, *chip == c, &fonts, 10.5, tip);
                    }
                });
                // Standings strip.
                let strip: Vec<String> = influence
                    .standings
                    .iter()
                    .map(|(r, s)| {
                        let mine = match my_band {
                            Alignment::EasternBloc => s.east_verdict,
                            _ => s.west_verdict,
                        };
                        format!("{}: {} ({}W/{}E/{}N)", r.replace('_', " "), mine.label(), s.west, s.east, s.denied)
                    })
                    .collect();
                if !strip.is_empty() {
                    crate::widgets::tipped_text(
                        p,
                        strip.join("  ·  "),
                        &fonts,
                        9.5,
                        DIM,
                        "Regional standings over the battleground set: PRESENCE, DOMINATES, CONTROLS. Non-aligned battlegrounds are denied to both. Frozen at 1955, 1960, 1965.",
                    );
                }
                // Rows.
                let mut rows: Vec<(u64, CountryTag)> = data
                    .countries
                    .keys()
                    .filter(|t| **t != me && !influence.dormant.contains(*t) || {
                        // Announced newborns show in contests.
                        influence.dormant.contains(*t)
                            && data.influence.seeds.iter().any(|s| {
                                &s.tag == *t
                                    && s.announced.is_some_and(|d| d <= (clock.date.year, clock.date.month, clock.date.day))
                            })
                    })
                    .filter(|t| match *chip {
                        Chip::Battlegrounds => battleground_weight(data, t) > 0 || influence.is_contested(t, tick),
                        Chip::MyBloc => military.alignment_of(data, t) == my_band && my_band != Alignment::NonAligned,
                        Chip::Contests => {
                            influence.is_contested(t, tick)
                                || influence.dormant.contains(*t)
                                || next_election(data, &influence, &clock, t).is_some()
                        }
                        Chip::All => true,
                    })
                    .map(|t| {
                        let w = battleground_weight(data, t) as u64 * 1000
                            + if influence.is_contested(t, tick) { 500 } else { 0 }
                            + influence.pressures.get(t).map(|v| v.iter().map(|p| p.delta.unsigned_abs() as u64).sum()).unwrap_or(0);
                        (w, t.clone())
                    })
                    .collect();
                rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                let shown = rows.len().min(if *chip == Chip::All { 60 } else { 24 });
                p.spawn((
                    Text::new(format!(
                        "{:<18}{:<26}{:<24}{:<10}{}",
                        "COUNTRY", "POSITION", "TREND", "OURS", "THEIRS"
                    )),
                    font(&fonts.mono_bold, 10.0),
                    TextColor(DIM),
                ));
                for (_, tag) in rows.iter().take(shown) {
                    let name = data
                        .countries
                        .get(tag)
                        .map(|c| c.name.to_uppercase())
                        .unwrap_or_else(|| tag.0.clone());
                    let name: String = name.chars().take(17).collect();
                    let band = military.alignment_of(data, tag);
                    let ours = influence
                        .programs
                        .get(&(me.clone(), tag.clone()))
                        .map(|p| format!("{} T{}", p.kind.label(), p.tier))
                        .or_else(|| influence.ops.get(&(me.clone(), tag.clone())).map(|o| o.kind.label().to_string()))
                        .unwrap_or_else(|| "-".into());
                    let pos = position_text(&influence, &intel, &me, tag, tick);
                    let trend = trend_text(&influence, tag);
                    let theirs = theirs_text(&influence, &intel, &me, tag);
                    let next = next_text(&influence, &world, &clock, tag);
                    p.spawn((
                        Button,
                        InfluenceButton::Open(tag.clone()),
                        crate::widgets::Tooltip::of(format!(
                            "{name}: {}. NEXT: {next}. Click for the dossier and the verbs.",
                            influence::verdict_sentence(&influence, &military, data, &clock, tag)
                        )),
                        Node {
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                            column_gap: Val::Px(6.0),
                            ..default()
                        },
                        BackgroundColor(ROW_BG),
                    ))
                    .with_children(|r| {
                        r.spawn((
                            Text::new(format!("{:<18}", name)),
                            font(&fonts.mono, 10.0),
                            TextColor(band_color(band)),
                        ));
                        r.spawn((
                            Text::new(format!(
                                "{:<26}{:<24}{:<10}{}",
                                pos.chars().take(25).collect::<String>(),
                                trend.chars().take(23).collect::<String>(),
                                ours.chars().take(9).collect::<String>(),
                                theirs.chars().take(18).collect::<String>()
                            )),
                            font(&fonts.mono, 10.0),
                            TextColor(MAIN),
                        ));
                    });
                }
                if rows.len() > shown {
                    p.spawn((
                        Text::new(format!("AND {} MORE. READ THE MAP.", rows.len() - shown)),
                        font(&fonts.mono, 9.5),
                        TextColor(DIM),
                    ));
                }
                // This month's wire.
                if !influence.last_month.is_empty() {
                    p.spawn((
                        Text::new("THE POLITICAL WIRE"),
                        font(&fonts.display, 12.0),
                        TextColor(ACCENT),
                    ));
                    for l in &influence.last_month {
                        p.spawn((
                            Text::new(format!("- {l}")),
                            font(&fonts.mono, 9.5),
                            TextColor(DIM),
                        ));
                    }
                }
            }
        }
        p.spawn((
            Text::new("ESTIMATIVE LEGEND: ALMOST CERTAIN > PROBABLE > CHANCES ABOUT EVEN > PROBABLY NOT > ALMOST CERTAINLY NOT. FOREIGN COMMITMENTS ARE ESTIMATES."),
            font(&fonts.mono, 8.5),
            TextColor(DIM),
        ));
    });
}

fn commitments_tab(
    p: &mut ChildSpawnerCommands,
    influence: &Influence,
    world: &World1950,
    clock: &SimClock,
    fonts: &Fonts,
    me: &CountryTag,
) {
    let data = &world.0;
    let name_of = |t: &CountryTag| {
        data.countries
            .get(t)
            .map(|c| c.name.to_uppercase())
            .unwrap_or_else(|| t.0.clone())
    };
    p.spawn((
        Text::new(format!(
            "STANDING PROGRAMS {}/{}",
            influence.programs_of(me),
            influence.slots_of(me)
        )),
        font(&fonts.mono_bold, 11.0),
        TextColor(MAIN),
    ));
    if influence.slots_of(me) == 0 {
        p.spawn((
            Text::new(
                "NO STANDING PROGRAMS: MINOR POWER. THE MAP IS READ, NOT PAINTED, FROM HERE.",
            ),
            font(&fonts.mono, 10.0),
            TextColor(DIM),
        ));
    }
    for ((s, t), prog) in &influence.programs {
        if s != me {
            continue;
        }
        p.spawn(Node {
            column_gap: Val::Px(8.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(format!(
                    "{:<20} {} TIER {}  SINCE {}MO  DELIVERED {}",
                    name_of(t),
                    prog.kind.label(),
                    prog.tier,
                    (clock.tick.saturating_sub(prog.started_tick)) / (30 * 24),
                    prog.months_delivered
                )),
                font(&fonts.mono, 10.0),
                TextColor(MAIN),
            ));
            let tip = if prog.kind == ProgramKind::Aid && prog.months_delivered > 0 {
                "WITHDRAW: the offer dies in public. The target is shoved the other way and tension rises (Aswan, July 1956)."
            } else {
                "Close the program and free the slot."
            };
            crate::widgets::segment(row, InfluenceButton::Stop(t.clone()), if prog.kind == ProgramKind::Aid && prog.months_delivered > 0 { "WITHDRAW" } else { "CLOSE" }, false, fonts, 9.5, tip);
        });
    }
    p.spawn((
        Text::new(format!(
            "ACTIVE OPERATIONS {}/{}",
            influence.ops_of(me),
            influence.op_slots_of(me)
        )),
        font(&fonts.mono_bold, 11.0),
        TextColor(MAIN),
    ));
    for ((s, t), op) in &influence.ops {
        if s != me {
            continue;
        }
        p.spawn(Node {
            column_gap: Val::Px(8.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(format!(
                    "{:<20} {}  RESOLVES IN {}D",
                    name_of(t),
                    op.kind.label(),
                    op.resolve_tick.saturating_sub(clock.tick) / 24
                )),
                font(&fonts.mono, 10.0),
                TextColor(MAIN),
            ));
            crate::widgets::segment(row, InfluenceButton::Cancel(t.clone()), "ABORT", false, fonts, 9.5, "Clean abort: assets stand down, nothing is spent beyond what the launch already cost.");
        });
    }
    if let Some(list) = influence.chequebook.get(&CountryTag(if me.0 == "USA" {
        "SOV".into()
    } else {
        "USA".into()
    })) {
        p.spawn((
            Text::new(format!(
                "THE RIVAL'S CHEQUEBOOK (EST.): {}",
                list.iter()
                    .map(|(t, k)| format!("{} ({})", name_of(t), k.label()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            font(&fonts.mono, 9.5),
            TextColor(DIM),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn dossier_page(
    p: &mut ChildSpawnerCommands,
    influence: &Influence,
    intel: &Intel,
    military: &Military,
    tension: &GlobalTension,
    world: &World1950,
    clock: &SimClock,
    fonts: &Fonts,
    me: &CountryTag,
    tag: &CountryTag,
) {
    let data = &world.0;
    let tick = clock.tick;
    let name = data
        .countries
        .get(tag)
        .map(|c| c.name.to_uppercase())
        .unwrap_or_else(|| tag.0.clone());
    let band = military.alignment_of(data, tag);
    let meta = data.nations_meta.get(tag);
    p.spawn(Node {
        column_gap: Val::Px(8.0),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|row| {
        crate::widgets::segment(
            row,
            InfluenceButton::Close,
            "< LEDGER",
            false,
            fonts,
            10.0,
            "Back to the triage list.",
        );
        row.spawn((
            Text::new(format!("POLITICAL DOSSIER: {name}")),
            font(&fonts.display, 14.0),
            TextColor(band_color(band)),
        ));
    });
    let gov = if influence.dormant.contains(tag) {
        "PROVISIONAL GOVERNMENT (INDEPENDENCE ANNOUNCED)".to_string()
    } else {
        meta.map(|m| {
            format!(
                "{} -- {}, {}",
                m.government.to_uppercase(),
                m.leader_name.to_uppercase(),
                m.leader_title.to_uppercase()
            )
        })
        .unwrap_or_else(|| {
            if influence.is_closed(tag) {
                "CLOSED REGIME".into()
            } else {
                "OPEN REGIME".into()
            }
        })
    };
    p.spawn((Text::new(gov), font(&fonts.mono, 10.0), TextColor(DIM)));
    let pen = intel.knowledge(me, tag, Domain::Political);
    p.spawn((
        Text::new(format!(
            "POSITION: {}   COVERAGE {}   AS OF 1 {}",
            position_text(influence, intel, me, tag, tick),
            if me == tag { "HOME" } else { grade(pen) },
            month_name(clock.date.month)
        )),
        font(&fonts.mono_bold, 11.0),
        TextColor(MAIN),
    ));
    // Axis strip: a crude teletype gauge.
    let pos = influence.position_of(tag);
    let cells = 21i32;
    let idx = ((pos as i32 + 1000) * (cells - 1) / 2000).clamp(0, cells - 1);
    let strip: String = (0..cells)
        .map(|i| {
            if i == idx {
                '|'
            } else if i == cells / 2 {
                ':'
            } else {
                '.'
            }
        })
        .collect();
    p.spawn((
        Text::new(format!("EAST {strip} WEST")),
        font(&fonts.mono, 10.0),
        TextColor(band_color(band)),
    ));
    // Pressures ledger.
    p.spawn((
        Text::new("PRESSURES THIS MONTH"),
        font(&fonts.display, 11.0),
        TextColor(ACCENT),
    ));
    match influence.pressures.get(tag) {
        Some(v) if !v.is_empty() => {
            for pr in v.iter().take(6) {
                let visible = me == tag || pen >= 500 || pr.label.starts_with(&me.0);
                p.spawn((
                    Text::new(if visible {
                        format!("  {:<32} {:+}", pr.label, pr.delta)
                    } else {
                        format!("  {:<32} (EST.)", "FOREIGN PRESSURE")
                    }),
                    font(&fonts.mono, 10.0),
                    TextColor(MAIN),
                ));
            }
        }
        _ => {
            p.spawn((
                Text::new("  NOTHING MOVES. NOBODY SPENDS."),
                font(&fonts.mono, 10.0),
                TextColor(DIM),
            ));
        }
    }
    // NEXT block.
    p.spawn((
        Text::new("NEXT"),
        font(&fonts.display, 11.0),
        TextColor(ACCENT),
    ));
    p.spawn((
        Text::new(format!("  {}", next_text(influence, world, clock, tag))),
        font(&fonts.mono, 10.0),
        TextColor(MAIN),
    ));
    if let Some(list) = influence
        .chequebook
        .iter()
        .find(|(s, l)| *s != me && l.iter().any(|(t, _)| t == tag))
    {
        p.spawn((
            Text::new(format!(
                "  {} OPENED A PROGRAM HERE THIS MONTH{}",
                list.0 .0,
                if pen >= 500 { "" } else { " (EST.)" }
            )),
            font(&fonts.mono, 10.0),
            TextColor(DIM),
        ));
    }
    // Verdict.
    p.spawn((
        Text::new(format!(
            "VERDICT: {}",
            influence::verdict_sentence(influence, military, data, clock, tag)
        )),
        font(&fonts.mono_bold, 10.5),
        TextColor(ACCENT),
    ));
    if me == tag {
        return;
    }
    // Verbs.
    p.spawn((
        Text::new("VERBS"),
        font(&fonts.display, 11.0),
        TextColor(ACCENT),
    ));
    let running = influence.programs.get(&(me.clone(), tag.clone()));
    p.spawn(Node {
        column_gap: Val::Px(6.0),
        flex_wrap: FlexWrap::Wrap,
        row_gap: Val::Px(4.0),
        ..default()
    })
    .with_children(|row| {
        if let Some(prog) = running {
            let label = if prog.kind == ProgramKind::Aid && prog.months_delivered > 0 { "WITHDRAW AID" } else { "CLOSE PROGRAM" };
            crate::widgets::segment(row, InfluenceButton::Stop(tag.clone()), label, false, fonts, 10.0, "Free the slot. A delivering aid program withdrawn shoves the target the other way and raises tension.");
        } else {
            for (kind, tier, label) in [
                (ProgramKind::Aid, 1, "AID T1"),
                (ProgramKind::Aid, 2, "AID T2"),
                (ProgramKind::Aid, 3, "AID T3"),
                (ProgramKind::Presence, 1, "PRESENCE T1"),
                (ProgramKind::Presence, 2, "PRESENCE T2"),
            ] {
                let why = influence.can_start_program(military, data, clock, me, tag, kind, tier).err();
                let tip = match (kind, why) {
                    (_, Some(w)) => format!("DISABLED: {w}"),
                    (ProgramKind::Aid, None) => format!(
                        "Standing aid: {} centi a month from your construction pool (a factory not built at home). +{} at announcement, then +{}/month toward us; doubled for small states and inside a contest.",
                        influence::tuning::AID_CENTI_PER_TIER * tier as u64,
                        influence::tuning::AID_ANNOUNCE,
                        influence::tuning::AID_FLOW * tier as i16
                    ),
                    (ProgramKind::Presence, None) => format!(
                        "Radio and missions: near-free, +{}/month toward us, halved in closed regimes.",
                        influence::tuning::PRESENCE_FLOW * tier as i16
                    ),
                };
                row.spawn((
                    Button,
                    InfluenceButton::Start(tag.clone(), kind, tier),
                    crate::widgets::Tooltip::of(&tip),
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(if tip.starts_with("DISABLED") { Color::srgba(0.12, 0.13, 0.15, 0.9) } else { Color::srgba(0.30, 0.26, 0.14, 0.95) }),
                ))
                .with_children(|b| {
                    b.spawn((Text::new(label), font(&fonts.mono, 10.0), TextColor(if tip.starts_with("DISABLED") { DIM } else { MAIN })));
                });
            }
        }
    });
    // Operations.
    let op = influence.ops.get(&(me.clone(), tag.clone()));
    p.spawn(Node {
        column_gap: Val::Px(6.0),
        flex_wrap: FlexWrap::Wrap,
        row_gap: Val::Px(4.0),
        ..default()
    })
    .with_children(|row| {
        if let Some(op) = op {
            crate::widgets::segment(row, InfluenceButton::Cancel(tag.clone()), &format!("ABORT {}", op.kind.label()), false, fonts, 10.0, "Clean abort: assets stand down.");
        } else {
            for kind in [InfluenceOpKind::ElectionPush, InfluenceOpKind::SponsorCoup] {
                let why = influence.can_launch_op(military, intel, tension, data, clock, me, tag, kind).err();
                let (permille, frontier) = influence.coup_frontier(military, intel, tension, data, me, tag);
                let tip = match (kind, why) {
                    (_, Some(w)) => format!("DISABLED: {w}"),
                    (InfluenceOpKind::ElectionPush, None) => format!(
                        "Bounded push on the coming ballot (+/-{} on the roll; three points, as history says). Spends network strength. If exposed, the push turns against us and costs deniability and legitimacy.",
                        influence::tuning::ELECTION_PUSH
                    ),
                    (InfluenceOpKind::SponsorCoup, None) => format!(
                        "SPONSOR COUP -- {}: {}. Ninety days of preparation, clean abort until then. Even a clean flip pays tension and legitimacy; an exposed failure moves them against us.",
                        kent_word(permille),
                        frontier
                    ),
                };
                let disabled = tip.starts_with("DISABLED");
                row.spawn((
                    Button,
                    InfluenceButton::Op(tag.clone(), kind),
                    crate::widgets::Tooltip::of(&tip),
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(if disabled { Color::srgba(0.12, 0.13, 0.15, 0.9) } else { Color::srgba(0.36, 0.18, 0.14, 0.95) }),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(match kind {
                            InfluenceOpKind::ElectionPush => "ELECTION PUSH".to_string(),
                            InfluenceOpKind::SponsorCoup => format!("SPONSOR COUP ({})", kent_word(permille)),
                        }),
                        font(&fonts.mono, 10.0),
                        TextColor(if disabled { DIM } else { MAIN }),
                    ));
                });
            }
        }
    });
}

fn month_name(m: u8) -> &'static str {
    [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ][(m as usize).clamp(1, 12) - 1]
}

fn influence_buttons(
    buttons: Query<(&Interaction, &InfluenceButton), Changed<Interaction>>,
    player: Option<Res<PlayerNation>>,
    mut pending: ResMut<PendingCommands>,
    mut tab: ResMut<Tab>,
    mut chip: ResMut<Chip>,
    mut dossier: ResMut<Dossier>,
) {
    let Some(player) = player else { return };
    let me = player.0.clone();
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            InfluenceButton::Tab(t) => *tab = *t,
            InfluenceButton::Chip(c) => *chip = *c,
            InfluenceButton::Open(t) => dossier.0 = Some(t.clone()),
            InfluenceButton::Close => dossier.0 = None,
            InfluenceButton::Start(t, kind, tier) => pending.push(SimCommand::StartProgram {
                sponsor: me.clone(),
                target: t.clone(),
                kind: *kind,
                tier: *tier,
            }),
            InfluenceButton::Stop(t) => pending.push(SimCommand::StopProgram {
                sponsor: me.clone(),
                target: t.clone(),
            }),
            InfluenceButton::Op(t, kind) => pending.push(SimCommand::LaunchInfluenceOp {
                sponsor: me.clone(),
                target: t.clone(),
                kind: *kind,
            }),
            InfluenceButton::Cancel(t) => pending.push(SimCommand::CancelInfluenceOp {
                sponsor: me.clone(),
                target: t.clone(),
            }),
        }
    }
}
