//! The economy panel (toggle: E). The first-hour difference between the
//! two systems lives here: planned economies get quota rows, market
//! economies get policy levers — same skeleton, different verbs.

use bevy::prelude::*;
use ugs_data::RegionId;
use ugs_sim::{
    agriculture::{Agriculture, Quota},
    command::{PendingCommands, SimCommand},
    construction::{self, Construction, ProjectId, ProjectKind, RegionSnapshots},
    demography::LivingStandards,
    economy::{EconomyStatic, NationalBalances, RegionalPower},
    planning::{EconomicSystem, Economies, Policy, Procurement},
    SimClock,
};

use crate::{font, AppState, Fonts, PlayerNation, World1950};

const BG: Color = Color::srgba(0.07, 0.09, 0.12, 0.96);
const BG_LIGHT: Color = Color::srgba(0.14, 0.17, 0.21, 0.95);
const ACCENT: Color = Color::srgb(0.83, 0.69, 0.36);
const DIM: Color = Color::srgb(0.62, 0.66, 0.70);
const MAIN: Color = Color::srgb(0.88, 0.89, 0.90);

#[derive(Resource, Default)]
struct PanelOpen(bool);

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum EconTab {
    #[default]
    Overview,
    Regions,
    Projects,
}

/// Which region's dossier is open (None = closed).
#[derive(Resource, Default)]
struct SelectedRegion(Option<RegionId>);

#[derive(Component)]
struct RegionDossier;

#[derive(Component)]
struct EconPanel;

#[derive(Component, Clone, PartialEq, Eq)]
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
    Tab(EconTab),
    OpenRegion(RegionId),
    CloseDossier,
    /// 0 = industrial expansion, 1 = power station, 2 = agri mech.
    StartGeneric(RegionId, u8),
    StartGreat(String),
    CancelProjectBtn(ProjectId),
    ZoneToggle(RegionId),
}

pub struct EconUiPlugin;

impl Plugin for EconUiPlugin {
    fn build(&self, app: &mut App) {
        // Dev shortcut: UGS_PANEL=econ boots with the panel open.
        app.insert_resource(PanelOpen(matches!(
            std::env::var("UGS_PANEL").as_deref(),
            Ok("econ") | Ok("econ-regions") | Ok("econ-projects")
        )));
        app.insert_resource(match std::env::var("UGS_PANEL").as_deref() {
            Ok("econ-regions") => EconTab::Regions,
            Ok("econ-projects") => EconTab::Projects,
            _ => EconTab::Overview,
        });
        app.init_resource::<SelectedRegion>();
        app.add_systems(OnEnter(AppState::InGame), spawn_panel);
        app.add_systems(
            Update,
            (toggle_panel, econ_buttons, refresh_panel, refresh_dossier)
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
#[allow(clippy::too_many_arguments)]
fn econ_buttons(
    buttons: Query<(&Interaction, &EconButton), Changed<Interaction>>,
    player: Option<Res<PlayerNation>>,
    econ: Res<Economies>,
    agri: Res<Agriculture>,
    cons: Res<Construction>,
    mut tab: ResMut<EconTab>,
    mut selected: ResMut<SelectedRegion>,
    mut pending: ResMut<PendingCommands>,
) {
    let Some(player) = player else { return };
    let tag = &player.0;
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            EconButton::Tab(t) => {
                *tab = *t;
                continue;
            }
            EconButton::OpenRegion(r) => {
                selected.0 = Some(*r);
                continue;
            }
            EconButton::CloseDossier => {
                selected.0 = None;
                continue;
            }
            EconButton::StartGeneric(region, kind_ix) => {
                let kind = match kind_ix {
                    1 => ProjectKind::PowerStation,
                    2 => ProjectKind::AgriMechanization,
                    _ => ProjectKind::IndustrialExpansion,
                };
                pending.push(SimCommand::StartProject {
                    country: tag.clone(),
                    region: *region,
                    kind,
                });
                continue;
            }
            EconButton::StartGreat(id) => {
                pending.push(SimCommand::StartProject {
                    country: tag.clone(),
                    region: RegionId(0), // catalog resolves the site
                    kind: ProjectKind::Great(id.clone()),
                });
                continue;
            }
            EconButton::CancelProjectBtn(id) => {
                pending.push(SimCommand::CancelProject {
                    country: tag.clone(),
                    id: *id,
                });
                continue;
            }
            EconButton::ZoneToggle(region) => {
                let zoned = cons.zones.get(tag).is_some_and(|z| z.contains(region));
                pending.push(SimCommand::SetDevelopmentZone {
                    country: tag.clone(),
                    region: *region,
                    on: !zoned,
                });
                continue;
            }
            _ => {}
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
    tab: Res<EconTab>,
    construction: Res<Construction>,
    snaps: Res<RegionSnapshots>,
    stat: Res<EconomyStatic>,
    clock: Res<SimClock>,
    national: Res<NationalBalances>,
    power: Res<RegionalPower>,
    player: Option<Res<PlayerNation>>,
    panel: Query<Entity, With<EconPanel>>,
) {
    if !open.0
        || (!open.is_changed()
            && !econ.is_changed()
            && !agri.is_changed()
            && !tab.is_changed()
            && !construction.is_changed()
            && !snaps.is_changed())
    {
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

        p.spawn(Node {
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            for (t, label, tip) in [
                (
                    EconTab::Overview,
                    "OVERVIEW",
                    "National figures, policy levers, and last month's economic wire.",
                ),
                (
                    EconTab::Regions,
                    "REGIONS",
                    "The national economy ledger: one row per region with its binding constraint. Click a row to open the region dossier.",
                ),
                (
                    EconTab::Projects,
                    "PROJECTS",
                    "Your construction portfolio: the pool, active projects, and the Great Project offer board. Start generic projects from a region's dossier.",
                ),
            ] {
                crate::widgets::segment(row, EconButton::Tab(t), label, *tab == t, &fonts, 11.0, tip);
            }
        });
        match *tab {
            EconTab::Regions => {
                regions_tab(p, &tag, system, &snaps, &stat, &world, &fonts);
                return;
            }
            EconTab::Projects => {
                projects_tab(
                    p, &tag, system, &construction, &world, &clock, &national, &power,
                    &stat, &fonts,
                );
                return;
            }
            EconTab::Overview => {}
        }

        // The heartbeat: last month's ranked wire, date-stamped.
        if let Some(lines) = snaps.wire.get(&tag) {
            if !lines.is_empty() {
                crate::widgets::tipped_text(
                    p,
                    format!("-- LAST MONTH ({}) --", snaps.as_of),
                    &fonts,
                    10.5,
                    ACCENT,
                    "The monthly economic wire: 3-6 severity-ranked changes, each naming a region and a cause. Economic figures update monthly and hold still in between.",
                );
                for line in lines.iter().take(6) {
                    p.spawn((Text::new(line.clone()), font(&fonts.mono, 10.0), TextColor(MAIN)));
                }
            }
        }

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

fn severity_color(s: ugs_sim::construction::Severity) -> Color {
    match s {
        ugs_sim::construction::Severity::Healthy => Color::srgb(0.45, 0.62, 0.45),
        ugs_sim::construction::Severity::Strained => Color::srgb(0.85, 0.65, 0.2),
        ugs_sim::construction::Severity::Critical => Color::srgb(0.88, 0.35, 0.28),
    }
}

/// REGIONS: the national economy ledger — one row per region, sorted
/// worst-first. The sorting IS the triage; clicking opens the dossier.
fn regions_tab(
    p: &mut ChildSpawnerCommands,
    tag: &ugs_data::CountryTag,
    system: Option<EconomicSystem>,
    snaps: &RegionSnapshots,
    stat: &EconomyStatic,
    world: &World1950,
    fonts: &Fonts,
) {
    let planner = system == Some(EconomicSystem::Planned);
    crate::widgets::tipped_text(
        p,
        format!(
            "{}  ({})",
            snaps.as_of,
            if planner {
                "FIGURES AS REPORTED"
            } else {
                "SURVEY, PRIOR QUARTER"
            }
        ),
        fonts,
        9.5,
        DIM,
        if planner {
            "Planned-economy dashboards show what officials REPORT, beside the plan. The two can drift apart; audits arrive in a later build."
        } else {
            "Market statistics are honest but late: surveyed figures lag a quarter behind the ground truth."
        },
    );
    let header = if planner {
        "REGION            POP    PLAN   REPORTED PWR% STATUS"
    } else {
        "REGION            POP    IND    PWR% STATUS"
    };
    p.spawn((
        Text::new(header),
        font(&fonts.mono_bold, 9.5),
        TextColor(DIM),
    ));
    let mut rows: Vec<(RegionId, &ugs_sim::construction::RegionSnapshot)> = snaps
        .by_region
        .iter()
        .filter(|(r, _)| stat.region_owner.get(r) == Some(tag))
        .map(|(r, s)| (*r, s))
        .collect();
    rows.sort_by_key(|(r, s)| (std::cmp::Reverse(s.severity), *r));
    let total = rows.len();
    for (region, snap) in rows.into_iter().take(16) {
        let name = construction::region_name(&world.0, region);
        let name = if name.len() > 16 {
            name.chars().take(16).collect()
        } else {
            name
        };
        let trend = match snap.pop_trend_permille {
            t if t > 0 => "+",
            t if t < 0 => "-",
            _ => " ",
        };
        let line = if planner {
            format!(
                "{name:<17} {:>5}K {:>6.1} {:>7.1} {:>4} {}{}",
                snap.pop / 1000,
                snap.industry_centi as f64 / 100.0,
                snap.reported_centi as f64 / 100.0,
                snap.power_permille / 10,
                snap.constraint.label(),
                trend
            )
        } else {
            format!(
                "{name:<17} {:>5}K {:>6.1} {:>4} {}{}",
                snap.pop / 1000,
                snap.industry_centi as f64 / 100.0,
                snap.power_permille / 10,
                snap.constraint.label(),
                trend
            )
        };
        p.spawn((
            Button,
            EconButton::OpenRegion(region),
            crate::widgets::Tooltip::of(
                "Open this region's dossier: people, power, production, the binding constraint, and the verbs that answer it.",
            ),
            Node {
                padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(line),
                font(&fonts.mono, 9.5),
                TextColor(severity_color(snap.severity)),
            ));
        });
    }
    if total > 16 {
        p.spawn((
            Text::new(format!("... {total} REGIONS (WORST FIRST)")),
            font(&fonts.mono, 9.0),
            TextColor(DIM),
        ));
    }
}

/// PROJECTS: the pool, the portfolio, and the offer board.
#[allow(clippy::too_many_arguments)]
fn projects_tab(
    p: &mut ChildSpawnerCommands,
    tag: &ugs_data::CountryTag,
    system: Option<EconomicSystem>,
    cons: &Construction,
    world: &World1950,
    clock: &SimClock,
    national: &NationalBalances,
    power: &RegionalPower,
    stat: &EconomyStatic,
    fonts: &Fonts,
) {
    use ugs_sim::construction::tuning as ct;
    let pool = cons.pool.get(tag).copied().unwrap_or(0);
    let (generic, great) = cons.active_for(tag);
    crate::widgets::tipped_text(
        p,
        format!(
            "CONSTRUCTION POOL {:.1}   SLOTS {}/{} + GREAT {}/1",
            pool as f64 / 100.0,
            generic,
            ct::GENERIC_SLOTS,
            great,
        ),
        fonts,
        11.0,
        MAIN,
        "The pool accrues from your investment allocation (half is directed; the rest grows industry on its own). Projects draw from it monthly, throttled by the host region's grid and national materials -- construction cannot buy its own inputs.",
    );
    let mine: Vec<(&ProjectId, &ugs_sim::construction::Project)> = cons
        .projects
        .iter()
        .filter(|(_, pr)| &pr.country == tag)
        .collect();
    for (id, pr) in &mine {
        let pct = pr.progress_centi * 100 / pr.cost_centi.max(1);
        let status = match pr.slowed_by {
            Some(c) => format!("SLOWED: {}", c.label()),
            None => "ON SCHEDULE".into(),
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
                    "{} ({}) {}% -- {}",
                    pr.kind.label(),
                    construction::region_name(&world.0, pr.region),
                    pct,
                    status
                ),
                fonts,
                9.5,
                if pr.slowed_by.is_some() {
                    Color::srgb(0.85, 0.65, 0.2)
                } else {
                    MAIN
                },
                "A project's monthly intake is pool draw x host grid factor x national materials. A slowed project always names its bottleneck.",
            );
            row.spawn((
                Button,
                EconButton::CancelProjectBtn(**id),
                crate::widgets::Tooltip::of(
                    "Cancel: refunds 30% of the remaining cost to the pool. Progress is lost.",
                ),
                Node {
                    padding: UiRect::axes(Val::Px(5.0), Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.30, 0.14, 0.12, 0.95)),
            ))
            .with_children(|b| {
                b.spawn((Text::new("CANCEL"), font(&fonts.mono, 9.0), TextColor(MAIN)));
            });
        });
    }
    if mine.is_empty() {
        p.spawn((
            Text::new(if system == Some(EconomicSystem::Planned) {
                "NO ACTIVE PROJECTS -- OPEN A REGION DOSSIER TO START ONE"
            } else {
                "NO ACTIVE PUBLIC WORKS -- POWER AND GREAT PROJECTS ONLY; INDUSTRY BELONGS TO FIRMS"
            }),
            font(&fonts.mono, 9.5),
            TextColor(DIM),
        ));
    }
    // The offer board.
    let offers = construction::offered_projects(&world.0, clock, national, power, stat, cons, tag);
    if !offers.is_empty() {
        p.spawn((
            Text::new("-- OFFER BOARD --"),
            font(&fonts.mono_bold, 10.0),
            TextColor(ACCENT),
        ));
        for def in offers {
            p.spawn(Node {
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                crate::widgets::tipped_text(
                    row,
                    format!("{} (COST {:.0})", def.name, def.cost_centi as f64 / 100.0),
                    fonts,
                    9.5,
                    MAIN,
                    &def.blurb,
                );
                row.spawn((
                    Button,
                    EconButton::StartGreat(def.id.clone()),
                    crate::widgets::Tooltip::of(
                        "Commit the Great Project slot. Its site, cost, and timeline come from the historical record; completion is a ceremony with real step-change effects.",
                    ),
                    Node {
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.30, 0.42, 0.28)),
                ))
                .with_children(|b| {
                    b.spawn((Text::new("START"), font(&fonts.mono, 9.0), TextColor(MAIN)));
                });
            });
        }
    }
    // The econ wire ring (project starts/completions).
    if !cons.log.is_empty() {
        p.spawn((
            Text::new("-- THE LEDGER WIRE --"),
            font(&fonts.mono_bold, 10.0),
            TextColor(DIM),
        ));
        for (_, line) in cons.log.iter().rev().take(5).rev() {
            p.spawn((
                Text::new(line.clone()),
                font(&fonts.mono, 9.0),
                TextColor(DIM),
            ));
        }
    }
}

/// The Region Dossier: a fixed one-page teletype document — the
/// economic sibling of the battle inspector. Verdict at the bottom,
/// verbs beside it.
#[allow(clippy::too_many_arguments)]
fn refresh_dossier(
    mut commands: Commands,
    selected: Res<SelectedRegion>,
    snaps: Res<RegionSnapshots>,
    stat: Res<EconomyStatic>,
    world: Res<World1950>,
    cons: Res<Construction>,
    econ: Res<Economies>,
    fonts: Res<Fonts>,
    player: Option<Res<PlayerNation>>,
    panel: Query<Entity, With<RegionDossier>>,
) {
    if !selected.is_changed() && !snaps.is_changed() && !cons.is_changed() {
        return;
    }
    for e in &panel {
        commands.entity(e).despawn();
    }
    let Some(region) = selected.0 else { return };
    let Some(snap) = snaps.by_region.get(&region) else {
        return;
    };
    let owner = stat.region_owner.get(&region).cloned();
    let me = player.as_ref().map(|p| p.0.clone());
    let is_mine = owner.is_some() && owner == me;
    let system = me.as_ref().and_then(|t| econ.system.get(t)).copied();
    let planner = system == Some(EconomicSystem::Planned);
    let name = construction::region_name(&world.0, region);
    let deposits: Vec<String> = {
        let mut kinds: Vec<String> = world
            .0
            .provinces
            .values()
            .filter(|p| p.region == region)
            .flat_map(|p| {
                p.deposits
                    .iter()
                    .map(|(k, _)| format!("{k:?}").to_uppercase())
            })
            .collect();
        kinds.sort();
        kinds.dedup();
        kinds
    };
    let project = cons
        .projects
        .iter()
        .find(|(_, p)| p.region == region)
        .map(|(id, p)| (*id, p.clone()));

    commands
        .spawn((
            RegionDossier,
            Interaction::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(34.0),
                width: Val::Px(390.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(BG),
        ))
        .with_children(|d| {
            d.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(format!("REGION DOSSIER: {name}")),
                    font(&fonts.display, 15.0),
                    TextColor(ACCENT),
                ));
                row.spawn((
                    Button,
                    EconButton::CloseDossier,
                    Node {
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(BG_LIGHT),
                ))
                .with_children(|b| {
                    b.spawn((Text::new("X"), font(&fonts.mono, 10.0), TextColor(MAIN)));
                });
            });
            let owner_name = owner
                .as_ref()
                .map(|t| t.0.clone())
                .unwrap_or_else(|| "?".into());
            d.spawn((
                Text::new(format!(
                    "{}  --  {}  --  {}",
                    snaps.as_of,
                    owner_name,
                    if deposits.is_empty() {
                        "AGRARIAN".to_string()
                    } else {
                        deposits.join(" / ")
                    }
                )),
                font(&fonts.mono, 9.5),
                TextColor(DIM),
            ));
            crate::widgets::tipped_text(
                d,
                format!(
                    "PEOPLE     {:.1}M {}",
                    snap.pop as f64 / 1e6,
                    match snap.pop_trend_permille {
                        t if t > 0 => "RISING",
                        t if t < 0 => "FALLING",
                        _ => "STEADY",
                    }
                ),
                &fonts,
                10.5,
                MAIN,
                "Regional population and its month-over-month trend. People are the labor the industry needs and the demand the grid serves.",
            );
            // Power bar: generation vs demand.
            let share = if snap.power_demand == 0 {
                1.0
            } else {
                (snap.power_generation as f32 / snap.power_demand as f32).min(1.0)
            };
            crate::widgets::tipped_text(
                d,
                format!(
                    "POWER      GEN {} / DEMAND {}  ({}%)",
                    snap.power_generation,
                    snap.power_demand,
                    snap.power_permille / 10
                ),
                &fonts,
                10.5,
                MAIN,
                "The regional grid. Deficit softly throttles every consumer in the region -- industry, projects, enrichment plants. Power stations and Great Projects add generation here.",
            );
            d.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(6.0),
                ..default()
            })
            .with_children(|bar| {
                bar.spawn((
                    Node {
                        width: Val::Percent(share * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(if share >= 1.0 {
                        Color::srgb(0.35, 0.6, 0.4)
                    } else {
                        Color::srgb(0.85, 0.65, 0.2)
                    }),
                ));
                bar.spawn((
                    Node {
                        width: Val::Percent((1.0 - share) * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.45, 0.2, 0.16)),
                ));
            });
            let ind_line = if planner && is_mine {
                format!(
                    "PRODUCTION IND {:.1} (REPORTED {:.1})",
                    snap.industry_centi as f64 / 100.0,
                    snap.reported_centi as f64 / 100.0
                )
            } else {
                format!("PRODUCTION IND {:.1}", snap.industry_centi as f64 / 100.0)
            };
            crate::widgets::tipped_text(
                d,
                ind_line,
                &fonts,
                10.5,
                MAIN,
                if planner {
                    "Actual regional industry beside what the reports claim. When they diverge, someone is lying to the Council of Ministers."
                } else {
                    "Regional industrial capacity, in the same units as the national dashboard."
                },
            );
            if !planner && is_mine {
                crate::widgets::tipped_text(
                    d,
                    format!(
                        "PRIVATE INVESTMENT THIS MONTH: +{:.2}",
                        snap.private_last_centi as f64 / 100.0
                    ),
                    &fonts,
                    10.0,
                    DIM,
                    "Where the allocator put capital last month, from the published formula: grid health + labor + development zone - tax drag. Steer the river; don't push the water.",
                );
            }
            if let Some((id, pr)) = &project {
                let pct = pr.progress_centi * 100 / pr.cost_centi.max(1);
                crate::widgets::tipped_text(
                    d,
                    format!(
                        "PROJECT    {} {}%{}",
                        pr.kind.label(),
                        pct,
                        match pr.slowed_by {
                            Some(c) => format!(" -- SLOWED ({})", c.label()),
                            None => String::new(),
                        }
                    ),
                    &fonts,
                    10.5,
                    ACCENT,
                    "The active project on this region, its progress, and -- when delayed -- exactly what is starving it.",
                );
                let _ = id;
            }
            // The verdict footer + verbs.
            let verdict = match snap.constraint {
                ugs_sim::construction::ConstraintKind::Power => format!(
                    "OUTPUT LIMITED BY POWER: GENERATION COVERS {}% OF DEMAND",
                    snap.power_permille / 10
                ),
                ugs_sim::construction::ConstraintKind::Materials =>
                    "OUTPUT LIMITED BY MATERIALS: NATIONAL COAL BALANCE SHORT".to_string(),
                ugs_sim::construction::ConstraintKind::Labor =>
                    "OUTPUT LIMITED BY LABOR: NOT ENOUGH URBAN WORKERS".to_string(),
                ugs_sim::construction::ConstraintKind::Contested =>
                    "REGION CONTESTED: WAR SUSPENDS THE CIVIL ECONOMY".to_string(),
                ugs_sim::construction::ConstraintKind::Healthy =>
                    "NO BINDING CONSTRAINT: THE REGION RUNS AT CAPACITY".to_string(),
            };
            d.spawn((
                Text::new(verdict),
                font(&fonts.mono_bold, 10.5),
                TextColor(severity_color(snap.severity)),
            ));
            if is_mine {
                d.spawn(Node {
                    column_gap: Val::Px(5.0),
                    ..default()
                })
                .with_children(|row| {
                    if planner {
                        for (kind_ix, label, tip) in [
                            (0u8, "EXPAND INDUSTRY", "Start an industrial expansion here (cost ~6.0 pool, months of work): +4.0 regional industry on completion -- which also raises this grid's demand."),
                            (1u8, "POWER STATION", "Start a power station here (cost ~9.0 pool): adds generation sized to this region's demand. The answer to a POWER-LIMITED verdict."),
                            (2u8, "MECHANIZE AGRI", "Start agricultural mechanization (cost ~5.0 pool): permanent national yield bonus on completion."),
                        ] {
                            row.spawn((
                                Button,
                                EconButton::StartGeneric(region, kind_ix),
                                crate::widgets::Tooltip::of(tip),
                                Node {
                                    padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.30, 0.42, 0.28)),
                            ))
                            .with_children(|b| {
                                b.spawn((Text::new(label), font(&fonts.mono, 9.0), TextColor(MAIN)));
                            });
                        }
                    } else {
                        let zoned = me
                            .as_ref()
                            .and_then(|t| cons.zones.get(t))
                            .is_some_and(|z| z.contains(&region));
                        crate::widgets::toggle(
                            row,
                            EconButton::ZoneToggle(region),
                            "DEVELOPMENT ZONE",
                            zoned,
                            false,
                            &fonts,
                            9.5,
                            "Tilt the private-investment allocator toward this region (max 3 zones). Firms still choose -- next month's attribution line shows whether they came.",
                        );
                        row.spawn((
                            Button,
                            EconButton::StartGeneric(region, 1),
                            crate::widgets::Tooltip::of(
                                "Public works: start a power station here. Industry placement belongs to firms; the grid belongs to the government.",
                            ),
                            Node {
                                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.30, 0.42, 0.28)),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                Text::new("POWER STATION"),
                                font(&fonts.mono, 9.0),
                                TextColor(MAIN),
                            ));
                        });
                    }
                });
            }
        });
}
