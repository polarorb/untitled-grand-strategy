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

/// The sim state the paper reads (bundled: Bevy systems take at most
/// sixteen parameters, and the paper reads more sources than that).
#[derive(bevy::ecs::system::SystemParam)]
struct Sources<'w> {
    military: Res<'w, Military>,
    settlements: Res<'w, Settlements>,
    snaps: Res<'w, RegionSnapshots>,
    econ: Res<'w, Economies>,
    tension: Res<'w, GlobalTension>,
    intel: Res<'w, ugs_sim::intel::Intel>,
    influence: Res<'w, ugs_sim::influence::Influence>,
    ledger: Res<'w, ugs_sim::score::Ledger>,
}

#[allow(clippy::too_many_arguments)]
fn refresh_paper(
    mut commands: Commands,
    open: Res<PaperOpen>,
    clock: Res<SimClock>,
    world: Res<World1950>,
    fired: Res<FiredEvents>,
    sources: Sources,
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
    let Sources {
        military,
        settlements,
        snaps,
        econ,
        tension,
        intel,
        influence,
        ledger,
    } = sources;
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
    // A reckoning page or the final edition takes the lead column.
    let score_page_showing = matches!(
        ledger.end,
        Some(ugs_sim::score::CampaignEnd::Reckoning { .. })
    ) || ledger
        .eras
        .last()
        .is_some_and(|e| clock.tick.saturating_sub(e.tick) <= 365 * 24);
    stories.truncate(if score_page_showing { 2 } else { 4 });
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
                    // THE COLONIAL QUESTION: open contests, believed lean.
                    let now = (clock.date.year, clock.date.month, clock.date.day);
                    let mut contests: Vec<(String, String)> = Vec::new();
                    for tag in data.countries.keys() {
                        let name = data.countries[tag].name.to_uppercase();
                        if let Some(until) = influence.contested_until.get(tag) {
                            if *until > clock.tick {
                                let months = (*until - clock.tick) / (30 * 24);
                                let lean = match military.alignment_of(&data, tag) {
                                    Alignment::WesternBloc => "LEANS WEST",
                                    Alignment::EasternBloc => "LEANS EAST",
                                    Alignment::NonAligned => match influence.position_of(tag) {
                                        p if p > 50 => "TILTS WEST",
                                        p if p < -50 => "TILTS EAST",
                                        _ => "UNDECIDED",
                                    },
                                };
                                contests.push((name, format!("{months} MONTHS -- {lean}")));
                                continue;
                            }
                        }
                        if influence.dormant.contains(tag)
                            && data.influence.seeds.iter().any(|s| {
                                &s.tag == tag && s.announced.is_some_and(|d| d <= now)
                            })
                        {
                            contests.push((name, "INDEPENDENCE ANNOUNCED".into()));
                        }
                    }
                    if !contests.is_empty() {
                        col.spawn((
                            Text::new("THE COLONIAL QUESTION"),
                            font(&fonts.display, 14.0),
                            TextColor(INK),
                        ));
                        for (name, line) in contests.iter().take(7) {
                            col.spawn((
                                Text::new(format!("{name}: {line}")),
                                font(&fonts.mono, 9.5),
                                TextColor(INK),
                            ));
                        }
                        if contests.len() > 7 {
                            col.spawn((
                                Text::new(format!("AND {} MORE FLAGS TO BE DECIDED.", contests.len() - 7)),
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
                    score_pages(col, &ledger, &influence, &military, &econ, &intel, &data, &clock, &fonts, me.as_ref(), organ);
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
                    if let Some(m) = &me {
                        for line in standing_lines(&ledger, &intel, &clock, m) {
                            col.spawn((
                                Text::new(line),
                                font(&fonts.mono_bold, 10.0),
                                TextColor(INK),
                            ));
                        }
                    }
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
                    // INFLUENCE STANDINGS: the reserved page, now printed.
                    if !influence.standings.is_empty() {
                        col.spawn((
                            Text::new("INFLUENCE STANDINGS"),
                            font(&fonts.display, 14.0),
                            TextColor(INK),
                        ));
                        let pop = {
                            let mut m: std::collections::BTreeMap<ugs_data::CountryTag, u64> = Default::default();
                            for p in data.provinces.values() {
                                *m.entry(military.owner_of(p.id, &p.owner)).or_default() += p.population_k as u64;
                            }
                            m
                        };
                        let totals = ugs_sim::influence::bloc_totals(&influence, &military, &data, &pop);
                        let sig2 = |n: u64| -> u64 {
                            if n < 100 { n } else {
                                let d = (n as f64).log10() as u32 + 1;
                                let step = 10u64.pow(d - 2);
                                n / step * step
                            }
                        };
                        col.spawn((
                            Text::new(format!(
                                "WEST {} STATES, {}M SOULS. EAST {} STATES, {}M. NON-ALIGNED {} STATES, {}M.",
                                totals[0].0, sig2(totals[0].1 / 1000), totals[1].0, sig2(totals[1].1 / 1000), totals[2].0, sig2(totals[2].1 / 1000)
                            )),
                            font(&fonts.mono, 10.0),
                            TextColor(INK),
                        ));
                        for (region, s) in &influence.standings {
                            let line = match (s.west_verdict, s.east_verdict) {
                                (w, e) if w > e => format!("WEST {}", w.label()),
                                (w, e) if e > w => format!("EAST {}", e.label()),
                                _ => "CONTESTED".to_string(),
                            };
                            col.spawn((
                                Text::new(format!("{}: {line} ({}W/{}E/{}N)", region.replace('_', " "), s.west, s.east, s.denied)),
                                font(&fonts.mono, 9.5),
                                TextColor(INK),
                            ));
                        }
                        if let Some(cp) = influence.checkpoints.last() {
                            col.spawn((
                                Text::new(format!("ON THE RECORD SINCE THE {} RECKONING.", cp.year)),
                                font(&fonts.mono, 9.0),
                                TextColor(INK_DIM),
                            ));
                        }
                        for (leader, list) in &influence.chequebook {
                            let who = if leader.0 == "USA" { "WASHINGTON" } else { "MOSCOW" };
                            let names: Vec<String> = list.iter().filter_map(|(t, k)| data.countries.get(t).map(|c| format!("{} ({})", c.name.to_uppercase(), k.label()))).collect();
                            if !names.is_empty() {
                                col.spawn((
                                    Text::new(format!("{who}'S CHEQUEBOOK: {}", names.join(", "))),
                                    font(&fonts.mono, 9.5),
                                    TextColor(INK_DIM),
                                ));
                            }
                        }
                        for l in influence.last_month.iter().take(4) {
                            col.spawn((
                                Text::new(format!("- {l}")),
                                font(&fonts.mono, 9.5),
                                TextColor(INK_DIM),
                            ));
                        }
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

// --- The score surfaces (docs/design/systems/scoring.md) --------------------

use ugs_sim::score::{self, Cause, Class, Grade, Ledger, Word};

fn name_of(data: &ugs_data::ScenarioData, tag: &ugs_data::CountryTag) -> String {
    data.nations_meta
        .get(tag)
        .map(|m| m.display_name.to_uppercase())
        .or_else(|| data.countries.get(tag).map(|c| c.name.to_uppercase()))
        .unwrap_or_else(|| tag.0.clone())
}

fn rival_of(
    military: &Military,
    data: &ugs_data::ScenarioData,
    me: &ugs_data::CountryTag,
) -> ugs_data::CountryTag {
    match military.alignment_of(data, me) {
        Alignment::EasternBloc => ugs_data::CountryTag("USA".into()),
        _ => ugs_data::CountryTag("SOV".into()),
    }
}

fn cause_text(cause: &Cause) -> String {
    match cause {
        Cause::None => "NOTHING MOVES".into(),
        Cause::Map(r) => format!("{} RUNS THE LEDGER [P]", r.replace('_', " ")),
        Cause::Output => "THE FACTORIES [E]".into(),
        Cause::Standing => "OUR STANDING AT THE TABLE [R]".into(),
        Cause::Peace => "THE PEACE, AND ITS PRICE [B]".into(),
    }
}

fn months_until_reckoning(clock: &SimClock) -> Option<(i32, i64)> {
    let next = ugs_sim::influence::tuning::CHECKPOINT_YEARS
        .iter()
        .copied()
        .find(|y| *y > clock.date.year)?;
    let months = (next as i64 - clock.date.year as i64) * 12 - (clock.date.month as i64 - 1);
    Some((next, months.max(0)))
}

/// THE STANDING: one line, a word and a cause; a second for the poles.
fn standing_lines(
    ledger: &Ledger,
    intel: &ugs_sim::intel::Intel,
    clock: &SimClock,
    me: &ugs_data::CountryTag,
) -> Vec<String> {
    let mut out = Vec::new();
    if matches!(ledger.end, Some(score::CampaignEnd::Reckoning { .. })) {
        out.push("THE RECORD IS CLOSED. SEE THE FINAL EDITION.".into());
        return out;
    }
    let (word, steady) = ledger.word_of(me);
    let cause = ledger
        .provisional
        .get(me)
        .map(|c| cause_text(&c.cause))
        .unwrap_or_else(|| "THE RECORD BEGINS".into());
    let next = months_until_reckoning(clock)
        .map(|(y, m)| format!(" -- NEXT RECKONING JAN {y} ({m} MONTHS)"))
        .unwrap_or_default();
    let arrow = match (word, steady) {
        (Word::Gaining, true) => " (AND RISING)",
        (Word::Slipping, true) => " (AND FALLING)",
        _ => "",
    };
    out.push(format!(
        "OUR STANDING: {}{arrow} -- {cause}{next}",
        word.label()
    ));
    if me.0 == "USA" || me.0 == "SOV" {
        let h = ledger.head_to_head();
        let mine_ahead = (me.0 == "USA" && h > 0) || (me.0 == "SOV" && h < 0);
        let rival = if me.0 == "USA" {
            "MOSCOW"
        } else {
            "WASHINGTON"
        };
        let rival_tag = ugs_data::CountryTag(if me.0 == "USA" {
            "SOV".into()
        } else {
            "USA".into()
        });
        let pen = intel.knowledge(me, &rival_tag, ugs_sim::intel::Domain::Economic);
        let word = if h == 0 || pen < 250 {
            "EVEN".to_string()
        } else if mine_ahead {
            format!(
                "{} LEADING {rival}",
                if pen >= 750 {
                    "ALMOST CERTAINLY"
                } else {
                    "PROBABLY"
                }
            )
        } else {
            format!(
                "{} TRAILING {rival}",
                if pen >= 750 {
                    "ALMOST CERTAINLY"
                } else {
                    "PROBABLY"
                }
            )
        };
        out.push(format!("HEAD TO HEAD: {word} (EST.)"));
    }
    if let Some(e) = ledger.eras.last() {
        out.push(format!("ON THE RECORD SINCE THE {} RECKONING.", e.year));
    }
    out
}

fn sig2(n: u64) -> u64 {
    if n < 100 {
        n
    } else {
        let d = (n as f64).log10() as u32 + 1;
        let step = 10u64.pow(d - 2);
        n / step * step
    }
}

#[allow(clippy::too_many_arguments)]
fn score_pages(
    col: &mut ChildSpawnerCommands,
    ledger: &Ledger,
    influence: &ugs_sim::influence::Influence,
    military: &Military,
    econ: &Economies,
    intel: &ugs_sim::intel::Intel,
    data: &ugs_data::ScenarioData,
    clock: &SimClock,
    fonts: &Fonts,
    me: Option<&ugs_data::CountryTag>,
    organ: bool,
) {
    let Some(me) = me else { return };
    let head = |col: &mut ChildSpawnerCommands, t: &str| {
        col.spawn((
            Text::new(t.to_string()),
            font(&fonts.display, 20.0),
            TextColor(INK),
        ));
    };
    let line = |col: &mut ChildSpawnerCommands, t: String, bold: bool| {
        col.spawn((
            Text::new(t),
            font(if bold { &fonts.mono_bold } else { &fonts.mono }, 10.0),
            TextColor(INK),
        ));
    };
    let rule = |col: &mut ChildSpawnerCommands| {
        col.spawn(Node {
            width: Val::Percent(60.0),
            height: Val::Px(1.0),
            ..default()
        })
        .insert(BackgroundColor(RULE));
    };
    let scale = score::scale_of(data, me);
    let rival = rival_of(military, data, me);

    // --- HOW THE CENTURY IS SCORED: the founding edition only.
    if clock.date.year == 1950 && clock.date.month == 1 {
        head(col, "HOW THE CENTURY IS SCORED");
        line(col, "THE RECORD IS KEPT IN FOUR TERMS AT FOUR DATES: 1 JANUARY 1955, 1960, 1965 AND 1970. THE MAP, COUNTED AS WHAT HAS CHANGED SINCE THIS MORNING IN THE REGIONS WE REACH; THE FACTORIES, AS GROWTH AGAINST THE RIVAL; OUR STANDING IN THE CHANCELLERIES; AND THE PEACE, WHICH CREDITS TREATIES AND DEBITS OUR DEAD. A STATE THAT JOINS NEITHER CAMP COUNTS AGAINST BOTH. A NUCLEAR WEAPON USED IS NEVER FORGIVEN. A GENERAL EXCHANGE ENDS THE RECORD FOR EVERYONE.".into(), false);
        let reach = score::reach_of(data, me);
        if reach.is_empty() {
            line(col, "THIS NATION IS NOT ON THE CONTESTED MAP. ITS LEDGER MOVES ON STANDING AND THE PEACE [R] [B].".into(), true);
        } else {
            line(
                col,
                format!(
                    "WE REACH: {}. SCALE {}.",
                    reach
                        .iter()
                        .map(|r| r.replace('_', " "))
                        .collect::<Vec<_>>()
                        .join(", "),
                    scale
                ),
                true,
            );
        }
        rule(col);
    }

    // --- THE FINAL EDITION: the record is closed.
    if let Some(score::CampaignEnd::Reckoning { year, .. }) = &ledger.end {
        let class = ledger.class_of(me, scale);
        let word = class.map(|c| c.label()).unwrap_or("NO CLASS");
        head(col, &format!("THE FINAL EDITION, {year}: {}", word));
        let total = ledger.campaign_total(me);
        if class == Some(Class::Costly) {
            line(col, format!("THE LEDGER READ {} ({total:+}), AT A PRICE THAT CAPS THE VERDICT: A WEAPON USED, OR OUR OWN DEAD BEYOND COUNTING.", ledger.ledger_class(me, scale).label()), true);
        } else {
            line(col, format!("THE ERAS SUM TO {total:+} SINCE 1950.",), true);
        }
        // Three bylines.
        let me_name = name_of(data, me);
        let rival_name = name_of(data, &rival);
        let own = match class {
            Some(Class::Won) => format!("{me_name} HAS WON THE CENTURY WITHOUT ENDING THE WORLD."),
            Some(Class::Held) => format!(
                "{me_name} HELD THE LINE. THE WORLD IS MUCH AS IT WAS, WHICH WAS THE POINT."
            ),
            Some(Class::Lost) => {
                format!("{me_name} LOST GROUND IN EVERY REGION THAT MATTERED AND KEPT THE PEACE.")
            }
            Some(Class::Costly) => {
                format!("{me_name} PAID FOR WHAT IT HOLDS IN A COIN THE WORLD DOES NOT FORGIVE.")
            }
            None => "THERE IS NO ONE LEFT TO WRITE THIS.".into(),
        };
        line(
            col,
            format!(
                "{}: {own}",
                if organ {
                    "THE PARTY ORGAN"
                } else {
                    "OUR OWN PAGE"
                }
            ),
            false,
        );
        let rival_view = match class {
            Some(Class::Won) => format!("{rival_name} SAYS THE MAP LIES AND THE FACTORIES DO NOT."),
            Some(Class::Lost) => format!("{rival_name} PRINTS THE MAP IN FULL COLOUR."),
            _ => format!("{rival_name} CLAIMS THE SAME DECADE AS ITS OWN."),
        };
        line(
            col,
            format!(
                "{}: {rival_view}",
                if organ {
                    "THE HERALD"
                } else {
                    "THE PEOPLE'S OBSERVER"
                }
            ),
            false,
        );
        line(
            col,
            "THE NATIONAL GAZETTE: THE NON-ALIGNED COUNTED THE YEARS THEY WERE NOT ASKED.".into(),
            false,
        );
        // The three things that decided it.
        line(col, "THE THREE THINGS THAT DECIDED IT:".into(), true);
        for (year, term, v) in ledger.decisive_terms(me) {
            line(col, format!("  {year}: {term} {v:+}"), false);
        }
        // The era grid.
        let mut grid = String::new();
        for e in &ledger.eras {
            if let Some(c) = e.cards.get(me) {
                let (g, s) = score::grade(c, scale);
                grid.push_str(&format!(
                    "{}: {}{}  ",
                    e.year,
                    g.label(),
                    if g == Grade::Stalemate {
                        ""
                    } else if s > 0 {
                        " GAIN"
                    } else {
                        " LOSS"
                    }
                ));
            }
        }
        line(col, grid, false);
        // Belief beside the record.
        line(col, "BELIEF BESIDE THE RECORD:".into(), true);
        if let Some(last) = ledger.eras.last() {
            if let Some(rc) = last.cards.get(&rival) {
                line(
                    col,
                    format!(
                        "  {rival_name} INDUSTRY PER HEAD: CLAIMED {}, RECORDED {}.",
                        sig2(rc.ipc_reported),
                        sig2(rc.ipc)
                    ),
                    false,
                );
            }
            if let Some(oc) = last.cards.get(me) {
                if oc.ipc_reported != oc.ipc {
                    line(
                        col,
                        format!(
                            "  OUR OWN BUREAU CLAIMED {}; THE RECORD SAYS {}.",
                            sig2(oc.ipc_reported),
                            sig2(oc.ipc)
                        ),
                        false,
                    );
                }
            }
        }
        rule(col);
        return;
    }

    // --- THE RECKONING: the page stays for a year after the freeze.
    let Some(era) = ledger.eras.last() else {
        return;
    };
    if clock.tick.saturating_sub(era.tick) > 365 * 24 {
        return;
    }
    let Some(card) = era.cards.get(me) else {
        return;
    };
    let (g, s) = score::grade(card, scale);
    let grade_word = format!(
        "{}{}",
        g.label(),
        if g == Grade::Stalemate {
            ""
        } else if s > 0 {
            " GAIN"
        } else {
            " LOSS"
        }
    );
    head(col, &format!("THE {} RECKONING: {grade_word}", era.year));
    if card.catastrophe == score::Catastrophe::Scarred {
        line(
            col,
            "SCARRED. THE CLASS IS CAPPED AT COSTLY FROM HERE.".into(),
            true,
        );
    }
    line(col, "WHERE WE STAND".into(), true);
    let rc = era.cards.get(&rival);
    let pen_econ = intel.knowledge(me, &rival, ugs_sim::intel::Domain::Economic);
    let theirs_map = rc
        .map(|c| format!("{:+}", c.map))
        .unwrap_or_else(|| "?".into());
    line(
        col,
        format!("  THE MAP       OURS {:+}   THEIRS {theirs_map}", card.map),
        false,
    );
    let ours_out = econ.dashboard_industry_centi(me);
    let theirs_out = if pen_econ >= 500 {
        let obs = econ.observed_industry_centi(&rival, pen_econ);
        format!("{} (EST.)", sig2(obs / 100))
    } else {
        "?".into()
    };
    let out_word = if score::tuning::OUTPUT_GATED {
        " (CONTEXT ONLY)"
    } else {
        ""
    };
    line(
        col,
        format!(
            "  THE FACTORIES INDUSTRY OURS {}{}   THEIRS {theirs_out}{out_word}",
            sig2(ours_out / 100),
            if econ.system.get(me) == Some(&EconomicSystem::Planned) {
                " (AS REPORTED)"
            } else {
                ""
            }
        ),
        false,
    );
    line(
        col,
        format!(
            "  OUR STANDING  {:+} THIS ERA (LEGITIMACY {})",
            card.standing, card.legitimacy
        ),
        false,
    );
    line(
        col,
        format!(
            "  THE PEACE     {:+}: {} FALLEN (EXACT), {} TREATIES, {}",
            card.peace,
            fmt_men(card.dead),
            card.treaties,
            if card.uses > 0 {
                "THE TABOO BROKEN BY OUR HAND"
            } else {
                "THE TABOO INTACT"
            }
        ),
        false,
    );
    line(col, "THE MAP BY REGION".into(), true);
    if let Some(cp) = influence.checkpoints.iter().find(|c| c.year == era.year) {
        for (region, st) in &cp.standings {
            let mine = match military.alignment_of(data, me) {
                Alignment::EasternBloc => st.east_verdict,
                _ => st.west_verdict,
            };
            let prize = if era.prize.as_deref() == Some(region.as_str()) {
                "  -- THE PRIZE"
            } else {
                ""
            };
            line(
                col,
                format!(
                    "  {}: {} ({}W/{}E/{}N){prize}",
                    region.replace('_', " "),
                    mine.label(),
                    st.west,
                    st.east,
                    st.denied
                ),
                false,
            );
        }
    }
    if let Some(rc) = rc {
        line(
            col,
            format!(
                "AS THEY SEE IT: {} COUNTS ITS MAP AT {:+} AND ITS INDUSTRY AT {} PER HEAD.",
                name_of(data, &rival),
                rc.map,
                sig2(rc.ipc_reported)
            ),
            false,
        );
    }
    let sum: i32 = ledger.campaign_total(me);
    line(
        col,
        format!("THE ERAS SO FAR SUM TO THE BOARD SINCE 1950: {sum:+}."),
        true,
    );
    rule(col);
}
