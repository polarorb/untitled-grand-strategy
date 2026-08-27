//! The Monthly Paper (toggle: N) — a full-screen period newspaper
//! summarizing the closed game month, printed AS THE PLAYER BELIEVES
//! IT (docs/design/systems/newspaper.md). The masthead is bloc-
//! flavored: the market West reads a blackletter broadsheet, the
//! planned East reads the party organ (whose figures are the REPORTED
//! ones), the non-aligned read a national gazette. Built as a section
//! registry so it grows into the game's ledger surface.

use bevy::prelude::*;
use ugs_data::Alignment;
use ugs_sim::{
    construction::RegionSnapshots,
    events::FiredEvents,
    military::{tuning as mil_tuning, Military},
    planning::{EconomicSystem, Economies},
    settlement::Settlements,
    tension::GlobalTension,
    SimClock,
};

use crate::war_ui::{est_men_range, fmt_men, intel_width};
use crate::{font, AppState, Fonts, PlayerNation, World1950};

/// Aged newsprint tones.
const PAPER: Color = Color::srgb(0.91, 0.88, 0.80);
const INK: Color = Color::srgb(0.13, 0.12, 0.11);
const INK_DIM: Color = Color::srgb(0.35, 0.33, 0.30);
const RULE: Color = Color::srgb(0.45, 0.42, 0.38);
const ORGAN_RED: Color = Color::srgb(0.55, 0.10, 0.08);

#[derive(Component)]
struct PaperPage;

#[derive(Resource, Default)]
struct PaperOpen(bool);

/// Month-boundary deltas the paper diffs against (display-only state;
/// deliberately UI-side per the design doc).
#[derive(Resource, Default)]
struct LastEdition {
    tension: i32,
    battles_won: u32,
    battles_lost: u32,
    casualties: u64,
    month_index: i64,
}

pub struct PaperUiPlugin;

impl Plugin for PaperUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaperOpen>();
        app.init_resource::<LastEdition>();
        app.add_systems(
            Update,
            (toggle_paper, refresh_paper, snapshot_edition)
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn month_index(clock: &SimClock) -> i64 {
    clock.date.year as i64 * 12 + clock.date.month as i64
}

/// Keep last month's closing numbers for the deltas the paper prints.
fn snapshot_edition(
    clock: Res<SimClock>,
    tension: Res<GlobalTension>,
    military: Res<Military>,
    player: Option<Res<PlayerNation>>,
    mut last: ResMut<LastEdition>,
) {
    let idx = month_index(&clock);
    if last.month_index == idx {
        return;
    }
    let me = player.as_ref().map(|p| &p.0);
    last.month_index = idx;
    last.tension = tension.value();
    last.battles_won = me
        .and_then(|m| military.battles_won.get(m))
        .copied()
        .unwrap_or(0);
    last.battles_lost = me
        .and_then(|m| military.battles_lost.get(m))
        .copied()
        .unwrap_or(0);
    last.casualties = me
        .and_then(|m| military.casualties.get(m))
        .copied()
        .unwrap_or(0);
}

fn toggle_paper(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut open: ResMut<PaperOpen>,
    page: Query<Entity, With<PaperPage>>,
    mut booted: Local<bool>,
) {
    let auto = !*booted && std::env::var("UGS_PANEL").as_deref() == Ok("paper");
    *booted = true;
    if !keys.just_pressed(KeyCode::KeyN) && !auto {
        return;
    }
    open.0 = !open.0 || auto;
    if !open.0 {
        for e in &page {
            commands.entity(e).despawn();
        }
    }
}

const MONTHS: [&str; 12] = [
    "JANUARY",
    "FEBRUARY",
    "MARCH",
    "APRIL",
    "MAY",
    "JUNE",
    "JULY",
    "AUGUST",
    "SEPTEMBER",
    "OCTOBER",
    "NOVEMBER",
    "DECEMBER",
];

fn roman(n: i32) -> String {
    let mut n = n.max(1) as u32;
    let mut out = String::new();
    for (v, s) in [(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")] {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

/// Rebuild the page when open (on open and at each month rollover).
#[allow(clippy::too_many_arguments)]
fn refresh_paper(
    mut commands: Commands,
    open: Res<PaperOpen>,
    clock: Res<SimClock>,
    world: Res<World1950>,
    fired: Res<FiredEvents>,
    military: Res<Military>,
    settlements: Res<Settlements>,
    snaps: Res<RegionSnapshots>,
    econ: Res<Economies>,
    tension: Res<GlobalTension>,
    intel: Res<ugs_sim::intel::Intel>,
    fonts: Res<Fonts>,
    player: Option<Res<PlayerNation>>,
    last: Res<LastEdition>,
    page: Query<Entity, With<PaperPage>>,
    mut shown_month: Local<i64>,
) {
    if !open.0 {
        *shown_month = 0;
        return;
    }
    let idx = month_index(&clock);
    if !page.is_empty() && *shown_month == idx {
        return;
    }
    *shown_month = idx;
    for e in &page {
        commands.entity(e).despawn();
    }
    let data = world.0.clone();

    let me = player.as_ref().map(|p| p.0.clone());
    let alignment = me
        .as_ref()
        .map(|m| military.alignment_of(&data, m))
        .unwrap_or(Alignment::NonAligned);
    let planned = me
        .as_ref()
        .and_then(|m| econ.system.get(m))
        .map(|s| *s == EconomicSystem::Planned)
        .unwrap_or(false);
    let (masthead_text, organ) = match alignment {
        Alignment::WesternBloc => ("The International Herald", false),
        Alignment::EasternBloc => ("THE PEOPLE'S OBSERVER", true),
        Alignment::NonAligned => ("The National Gazette", false),
    };
    // The closed month (the edition reports the month that ENDED).
    let (ed_year, ed_month) = if clock.date.month == 1 {
        (clock.date.year - 1, 12u8)
    } else {
        (clock.date.year, clock.date.month - 1)
    };
    let window_start = fired
        .fired_ticks
        .values()
        .copied()
        .max()
        .map(|_| {
            clock
                .tick
                .saturating_sub(24 * 31 + clock.date.day as u64 * 24)
        })
        .unwrap_or(0);

    // --- Section data ------------------------------------------------------
    // Lead stories: events fired within the window, newest first.
    let mut stories: Vec<(&str, &str)> = fired
        .fired_ticks
        .iter()
        .filter(|(_, t)| **t >= window_start && **t < clock.tick)
        .filter_map(|(id, t)| {
            data.events
                .iter()
                .find(|e| &e.id == id)
                .map(|e| (*t, e.title.as_str(), e.body.as_str()))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(_, t, b)| (t, b))
        .collect();
    stories.truncate(4);
    let more_stories = fired
        .fired_ticks
        .values()
        .filter(|t| **t >= window_start && **t < clock.tick)
        .count()
        .saturating_sub(stories.len());

    // Births this month: fired events carrying an Independence effect.
    let births: Vec<String> = fired
        .fired_ticks
        .iter()
        .filter(|(_, t)| **t >= window_start)
        .filter_map(|(id, _)| data.events.iter().find(|e| &e.id == id))
        .flat_map(|e| {
            e.effects
                .iter()
                .chain(e.options.iter().flat_map(|o| o.effects.iter()))
        })
        .filter_map(|eff| match eff {
            ugs_data::EventEffect::Independence { country, .. } => {
                data.countries.get(country).map(|c| c.name.to_uppercase())
            }
            _ => None,
        })
        .collect();

    let treaties_month = settlements
        .treaties
        .iter()
        .filter(|t| t.tick >= window_start)
        .count();
    let wars_now = military.wars.len();
    let t_now = tension.value();
    let t_delta = t_now - last.tension;

    // War report numbers (deltas vs the last edition snapshot).
    let at_war = me
        .as_ref()
        .map(|m| military.wars.iter().any(|(a, b)| a == m || b == m))
        .unwrap_or(false);
    let month = clock.tick / (24 * 30);

    commands
        .spawn((
            PaperPage,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(6.0),
                right: Val::Percent(6.0),
                top: Val::Px(40.0),
                bottom: Val::Px(28.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(26.0)),
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(PAPER),
            GlobalZIndex(40),
        ))
        .with_children(|p| {
            // --- Masthead ---------------------------------------------------
            p.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(2.0),
                ..default()
            })
            .insert(BackgroundColor(RULE));
            p.spawn(Node {
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(masthead_text),
                    if organ {
                        font(&fonts.display, 46.0)
                    } else {
                        font(&fonts.masthead, 52.0)
                    },
                    TextColor(if organ { ORGAN_RED } else { INK }),
                ));
            });
            p.spawn(Node {
                width: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(format!("VOL. {}, No. {}", roman(ed_year - 1949), ed_month)),
                    font(&fonts.mono, 11.0),
                    TextColor(INK_DIM),
                ));
                row.spawn((
                    Text::new(format!("{} {} EDITION", MONTHS[(ed_month as usize - 1).min(11)], ed_year)),
                    font(&fonts.mono_bold, 12.0),
                    TextColor(INK),
                ));
                row.spawn((
                    Text::new(if organ { "PRICE 5 KOP." } else { "PRICE 10 CENTS" }),
                    font(&fonts.mono, 11.0),
                    TextColor(INK_DIM),
                ));
            });
            p.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                ..default()
            })
            .insert(BackgroundColor(RULE));

            // --- Body: three columns ---------------------------------------
            p.spawn(Node {
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                column_gap: Val::Px(18.0),
                ..default()
            })
            .with_children(|body| {
                // LEFT RAIL: COMMERCE & INDUSTRY.
                body.spawn(Node {
                    width: Val::Percent(22.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(5.0),
                    ..default()
                })
                .with_children(|col| {
                    col.spawn((
                        Text::new("COMMERCE & INDUSTRY"),
                        font(&fonts.display, 14.0),
                        TextColor(INK),
                    ));
                    if let Some(m) = &me {
                        let shown = econ.dashboard_industry_centi(m);
                        col.spawn((
                            Text::new(if planned {
                                format!("PLAN FULFILLED. INDUSTRIAL OUTPUT {:.1} POINTS, THE MINISTRIES REPORT.", shown as f64 / 100.0)
                            } else {
                                format!("INDUSTRIAL INDEX AT {:.1} (BUREAU SURVEY, PRIOR QUARTER).", shown as f64 / 100.0)
                            }),
                            font(&fonts.mono, 10.0),
                            TextColor(INK),
                        ));
                        let empty = Vec::new();
                        let lines = snaps.wire.get(m).unwrap_or(&empty);
                        for line in lines.iter().take(5) {
                            col.spawn((
                                Text::new(format!("- {line}")),
                                font(&fonts.mono, 9.5),
                                TextColor(INK_DIM),
                            ));
                        }
                        if lines.is_empty() {
                            col.spawn((
                                Text::new("TRADE STEADY. NO DISRUPTIONS REPORTED."),
                                font(&fonts.mono, 9.5),
                                TextColor(INK_DIM),
                            ));
                        }
                    }
                });
                // LEAD COLUMN.
                body.spawn(Node {
                    width: Val::Percent(52.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|col| {
                    if stories.is_empty() {
                        col.spawn((
                            Text::new("A QUIET MONTH"),
                            font(&fonts.display, 26.0),
                            TextColor(INK),
                        ));
                        col.spawn((
                            Text::new(
                                "NO EVENT OF THE FIRST RANK REACHED THE WIRES THIS MONTH. THE CHANCELLERIES REST; THE ARSENALS DO NOT.",
                            ),
                            font(&fonts.mono, 11.0),
                            TextColor(INK),
                        ));
                    }
                    for (i, (title, bodytext)) in stories.iter().enumerate() {
                        col.spawn((
                            Text::new(*title),
                            font(&fonts.display, if i == 0 { 26.0 } else { 16.0 }),
                            TextColor(INK),
                        ));
                        let mut b = bodytext.to_string();
                        if i > 0 && b.chars().count() > 220 {
                            b = b.chars().take(220).collect::<String>() + "...";
                        }
                        col.spawn((
                            Text::new(b),
                            font(&fonts.mono, if i == 0 { 11.5 } else { 10.0 }),
                            TextColor(if i == 0 { INK } else { INK_DIM }),
                        ));
                        col.spawn(Node {
                            width: Val::Percent(60.0),
                            height: Val::Px(1.0),
                            ..default()
                        })
                        .insert(BackgroundColor(RULE));
                    }
                    if more_stories > 0 {
                        col.spawn((
                            Text::new(format!("AND {more_stories} MORE ON THE WIRES.")),
                            font(&fonts.mono, 9.5),
                            TextColor(INK_DIM),
                        ));
                    }
                });
                // RIGHT RAIL: NUMBERS + WAR REPORT.
                body.spawn(Node {
                    width: Val::Percent(24.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(5.0),
                    ..default()
                })
                .with_children(|col| {
                    col.spawn((
                        Text::new("THE WORLD IN NUMBERS"),
                        font(&fonts.display, 14.0),
                        TextColor(INK),
                    ));
                    let band = format!("{}", tension.band()).to_uppercase();
                    col.spawn((
                        Text::new(format!(
                            "GLOBAL TENSION {:.1} ({band}) {}",
                            t_now as f64 / 10.0,
                            match t_delta {
                                d if d > 0 => format!("UP {:.1} ON THE MONTH", d as f64 / 10.0),
                                d if d < 0 => format!("DOWN {:.1}", -d as f64 / 10.0),
                                _ => "UNCHANGED".into(),
                            }
                        )),
                        font(&fonts.mono, 10.0),
                        TextColor(INK),
                    ));
                    col.spawn((
                        Text::new(format!("WARS IN PROGRESS: {wars_now}")),
                        font(&fonts.mono, 10.0),
                        TextColor(INK),
                    ));
                    col.spawn((
                        Text::new(format!("TREATIES SIGNED THIS MONTH: {treaties_month}")),
                        font(&fonts.mono, 10.0),
                        TextColor(INK),
                    ));
                    for b in births.iter().take(3) {
                        col.spawn((
                            Text::new(format!("A NATION IS BORN: {b}")),
                            font(&fonts.mono_bold, 10.0),
                            TextColor(INK),
                        ));
                    }
                    if let Some(m) = &me {
                        col.spawn((
                            Text::new(format!(
                                "OUR STANDING (LEGITIMACY): {}",
                                settlements.legitimacy_of(m)
                            )),
                            font(&fonts.mono, 10.0),
                            TextColor(INK),
                        ));
                        // Rival arsenal estimate: what we BELIEVE.
                        let rival = match alignment {
                            Alignment::EasternBloc => "USA",
                            _ => "SOV",
                        };
                        let rival_tag = ugs_data::CountryTag(rival.into());
                        let rival_men: u64 = military
                            .formations
                            .values()
                            .filter(|f| f.owner == rival_tag)
                            .map(|f| f.strength * mil_tuning::MEN_PER_STRENGTH_POINT)
                            .sum();
                        let w = intel_width(&intel, Some(m), &rival_tag);
                        let seed = month ^ rival_tag.0.bytes().map(u64::from).sum::<u64>();
                        let (lo, hi) = est_men_range(rival_men.max(1), seed, w);
                        col.spawn((
                            Text::new(format!(
                                "{rival} FORCES EST {}-{} MEN",
                                fmt_men(lo),
                                fmt_men(hi)
                            )),
                            font(&fonts.mono, 10.0),
                            TextColor(INK_DIM),
                        ));
                    }
                    if at_war {
                        if let Some(m) = &me {
                            col.spawn((
                                Text::new("THE WAR REPORT"),
                                font(&fonts.display, 14.0),
                                TextColor(INK),
                            ));
                            let won = military.battles_won.get(m).copied().unwrap_or(0)
                                - last.battles_won.min(
                                    military.battles_won.get(m).copied().unwrap_or(0),
                                );
                            let lost = military.battles_lost.get(m).copied().unwrap_or(0)
                                - last.battles_lost.min(
                                    military.battles_lost.get(m).copied().unwrap_or(0),
                                );
                            let cas = (military.casualties.get(m).copied().unwrap_or(0))
                                .saturating_sub(last.casualties)
                                * mil_tuning::MEN_PER_STRENGTH_POINT;
                            col.spawn((
                                Text::new(format!(
                                    "BATTLES THIS MONTH: {won} WON, {lost} LOST. OUR FALLEN: {} (EXACT).",
                                    fmt_men(cas)
                                )),
                                font(&fonts.mono, 10.0),
                                TextColor(INK),
                            ));
                            let static_days =
                                clock.tick.saturating_sub(military.last_line_change_tick) / 24;
                            col.spawn((
                                Text::new(if static_days > 30 {
                                    format!("THE FRONT HAS NOT MOVED IN {static_days} DAYS.")
                                } else {
                                    "THE FRONT IS IN MOTION.".to_string()
                                }),
                                font(&fonts.mono, 10.0),
                                TextColor(INK_DIM),
                            ));
                        }
                    }
                });
            });

            // --- Footer -----------------------------------------------------
            p.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                ..default()
            })
            .insert(BackgroundColor(RULE));
            p.spawn((
                Text::new(format!(
                    "ALL FIGURES AS OF 1 {} {}. FOREIGN FIGURES ARE ESTIMATES. PRESS N TO FOLD THE PAPER.",
                    MONTHS[(clock.date.month as usize - 1).min(11)],
                    clock.date.year
                )),
                font(&fonts.mono, 9.0),
                TextColor(INK_DIM),
            ));
        });
}
