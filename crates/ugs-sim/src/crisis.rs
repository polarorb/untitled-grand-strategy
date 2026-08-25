//! Crisis ladders — phase 3 of the escalation pillar. A crisis is a
//! discrete confrontation with 8 rungs and 3 firebreaks; the global
//! tension meter is background weather that caps where crises OPEN.
//! Schelling made literal: while a crisis sits high, a deterministic
//! incident hazard can climb the ladder for you.
//!
//! Rungs: 1 protest, 2 show of force, 3 mobilization/blockade,
//! 4 conventional clash | FIREBREAK A | 5 open war, 6 nuclear
//! ultimatum | FIREBREAK B | 7 tactical use | FIREBREAK C | 8 general
//! exchange. Phase 3 runs the ladder to rung 6; crossing firebreak B
//! belongs to the use/endgame layer.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::CountryTag;

use crate::demography::SimScenario;
use crate::deterrence::{Deterrence, DyadClass};
use crate::events::{DynamicChoice, FiredEvents};
use crate::military::PlayerCountry;
use crate::rng::SimRng;
use crate::tension::GlobalTension;
use crate::SimClock;

pub mod tuning {
    /// At most this many live crises worldwide.
    pub const MAX_ACTIVE: usize = 1;
    /// Monthly spawn chance, permille, scaled by tension value/1000.
    pub const SPAWN_BASE_PERMILLE: u32 = 400;
    /// Under mutual deterrence crises come more often (the
    /// stability-instability paradox).
    pub const MUTUAL_SPAWN_MULT_PERMILLE: u32 = 2000;
    /// Hours the ball-holder has to answer.
    pub const DECISION_HOURS: u64 = 96;
    /// Tension added per rung climbed (x rung).
    pub const TENSION_PER_RUNG: i32 = 3;
    /// Incident hazard, basis points per hour, by rung index 0..=8.
    pub const HAZARD_BP: [u32; 9] = [0, 0, 0, 1, 2, 4, 12, 20, 0];
    /// Resolve cost per rung climbed when backing down, by phase.
    pub const BACKDOWN_COST_LOW: i64 = 2; // rungs 1-3
    pub const BACKDOWN_COST_MID: i64 = 6; // rungs 4-5
    pub const BACKDOWN_COST_HIGH: i64 = 12; // rung 6+
    /// Resolve gained by the side that stood firm.
    pub const VICTORY_RESOLVE: i64 = 8;
    pub const START_RESOLVE: i64 = 50;
    /// Tension released when a crisis ends.
    pub const CRISIS_END_TENSION: i32 = -20;
    pub const INCIDENT_TENSION: i32 = 30;
}

/// An authored crisis flashpoint. Content lives here as a clearly
/// marked table (v1); moves to RON when the roster grows.
pub struct Flashpoint {
    pub slug: &'static str,
    pub title: &'static str,
    pub stake: &'static str,
    pub initiator: &'static str,
    pub target: &'static str,
    pub body: &'static str,
}

pub const FLASHPOINTS: &[Flashpoint] = &[
    Flashpoint {
        slug: "berlin-access",
        title: "BERLIN ACCESS CRISIS",
        stake: "WESTERN ACCESS TO BERLIN",
        initiator: "SOV",
        target: "USA",
        body: "BERLIN -- SOVIET AUTHORITIES HALT ALLIED ROAD AND RAIL TRAFFIC AT THE HELMSTEDT CHECKPOINT CITING TECHNICAL DIFFICULTIES. GARRISON SUPPLIES FINITE. THE LAST BLOCKADE ENDED EIGHTEEN MONTHS AGO. THE CITY IS A HOSTAGE AGAIN.",
    },
    Flashpoint {
        slug: "taiwan-strait",
        title: "TAIWAN STRAIT CRISIS",
        stake: "THE OFFSHORE ISLANDS",
        initiator: "PRC",
        target: "USA",
        body: "TAIPEI -- HEAVY COMMUNIST SHELLING OF THE OFFSHORE ISLANDS. INVASION FLOTILLAS REPORTED MASSING OPPOSITE QUEMOY. SEVENTH FLEET STEAMING NORTH. NATIONALIST COMMAND REQUESTS AMERICAN GUARANTEE.",
    },
    Flashpoint {
        slug: "turkish-straits",
        title: "TURKISH STRAITS CRISIS",
        stake: "CONTROL OF THE BOSPHORUS",
        initiator: "SOV",
        target: "TUR",
        body: "ANKARA -- MOSCOW RENEWS DEMAND FOR JOINT DEFENSE OF THE STRAITS AND BASING RIGHTS ON TURKISH SOIL. SOVIET ARMOR REPORTED EXERCISING IN THE CAUCASUS. TURKEY APPEALS TO THE WEST.",
    },
];

/// Inserted when the general exchange happens. The campaign is over;
/// the presentation layer owns the funeral.
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct GameOver {
    pub initiator: CountryTag,
    pub against: CountryTag,
    pub tick: u64,
    /// Estimated dead across both homelands, computed from the real
    /// demography at the moment of exchange.
    pub dead: u64,
    /// The worst-hit cities, for the final wire (name, population).
    pub cities: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crisis {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub stake: String,
    pub initiator: CountryTag,
    pub target: CountryTag,
    pub rung: u8,
    /// Whose move it is.
    pub ball: CountryTag,
    pub deadline_tick: u64,
    /// Rungs each side has personally climbed (back-down cost basis).
    pub initiator_climbed: u8,
    pub target_climbed: u8,
}

impl Crisis {
    pub fn other(&self, tag: &CountryTag) -> CountryTag {
        if *tag == self.initiator {
            self.target.clone()
        } else {
            self.initiator.clone()
        }
    }
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Crises {
    pub active: Vec<Crisis>,
    /// National willingness to stand at the brink, 0-100. Own value is
    /// shown exactly; rivals' only as intelligence estimates.
    pub resolve: BTreeMap<CountryTag, i64>,
    /// Flashpoints already used (slug).
    pub used: Vec<String>,
    /// Cursor into FiredEvents.resolved.
    resolved_cursor: usize,
    next_id: u32,
}

impl Crises {
    pub fn resolve_of(&self, tag: &CountryTag) -> i64 {
        self.resolve
            .get(tag)
            .copied()
            .unwrap_or(tuning::START_RESOLVE)
    }

    /// A patron's ultimatum crisis, opened directly at rung 6 (used by
    /// the nuclear-use chain). The ball goes to the accused.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_ultimatum(
        &mut self,
        clock: &SimClock,
        initiator: CountryTag,
        target: CountryTag,
        stake: &str,
        fired: &mut FiredEvents,
        taboo_broken: bool,
    ) {
        self.next_id += 1;
        let crisis = Crisis {
            id: self.next_id,
            slug: format!("ultimatum-{}", self.next_id),
            title: format!("{} ULTIMATUM", initiator.0),
            stake: stake.to_string(),
            initiator: initiator.clone(),
            target: target.clone(),
            rung: 6,
            ball: target,
            deadline_tick: clock.tick + tuning::DECISION_HOURS,
            initiator_climbed: 6,
            target_climbed: 0,
        };
        fired.notices.push((
            format!("{} ISSUES ULTIMATUM", initiator.0),
            format!(
                "{} DECLARES IT WILL NOT STAND ASIDE. STRATEGIC FORCES REPORTED AT READINESS. THE DEMAND: IMMEDIATE CESSATION. THE CRISIS OPENS AT THE NUCLEAR RUNG.",
                initiator.0
            ),
        ));
        post_decision(fired, &crisis, clock, tuning::DECISION_HOURS, taboo_broken);
        self.active.push(crisis);
    }

    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let fold = |mut h: u64, s: &str| {
            for b in s.bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
            h
        };
        for c in &self.active {
            for v in [c.id as u64, c.rung as u64, c.deadline_tick] {
                h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        for (tag, r) in &self.resolve {
            h = fold(h, &tag.0);
            h = (h ^ (*r as u64)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        for slug in &self.used {
            h = fold(h, slug);
        }
        (h ^ self.next_id as u64).wrapping_mul(0x0000_0100_0000_01b3)
    }
}

fn choice_id(crisis: &Crisis) -> String {
    format!("crisis-{}-r{}", crisis.id, crisis.rung)
}

/// Options for the ball-holder at the current rung. Option 0 is the
/// deadline default (the cautious one — nobody sleepwalks upward).
/// Rung 6 only offers the firebreak once the taboo is broken; rung 7
/// offers the end of the world, labeled as exactly that.
fn options_for(rung: u8, taboo_broken: bool) -> Vec<String> {
    let escalate = match rung {
        1 => "ESCALATE: SHOW OF FORCE",
        2 => "ESCALATE: MOBILIZE",
        3 => "ESCALATE: BLOCKADE / CONFRONT",
        4 => "ESCALATE: OPEN HOSTILITIES [FIREBREAK]",
        5 => "ESCALATE: NUCLEAR ULTIMATUM",
        6 => "ESCALATE: TACTICAL EMPLOYMENT [FIREBREAK: NUCLEAR USE]",
        _ => "ESCALATE: GENERAL EXCHANGE [THIS ENDS EVERYTHING]",
    };
    let mut v = vec!["BACK DOWN".to_string()];
    // Crossing firebreak B out of a crisis requires a broken taboo;
    // before that, rung 6 is a wall you talk down from.
    if rung < 6 || taboo_broken || rung >= 7 {
        v.push(escalate.to_string());
    }
    if rung <= 3 {
        v.push("SEEK COMPROMISE".to_string());
    }
    v
}

fn post_decision(
    fired: &mut FiredEvents,
    crisis: &Crisis,
    clock: &SimClock,
    deadline: u64,
    taboo_broken: bool,
) {
    fired.dynamic.push(DynamicChoice {
        id: choice_id(crisis),
        title: format!("{} -- RUNG {}", crisis.title, crisis.rung),
        body: format!(
            "STAKE: {}. THE MOVE RESTS WITH {}. STANDING AT THIS ALTITUDE RISKS INCIDENTS NEITHER CAPITAL ORDERS.",
            crisis.stake, crisis.ball.0
        ),
        country: crisis.ball.clone(),
        options: options_for(crisis.rung, taboo_broken),
        deadline_tick: clock.tick + deadline,
    });
}

/// The full crisis state machine. Runs in Politics after deterrence.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
pub fn update_crises(
    mut commands: Commands,
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    deterrence: Res<Deterrence>,
    player: Res<PlayerCountry>,
    programs: Res<crate::nuclear::NuclearPrograms>,
    demo: Res<crate::demography::Demographics>,
    mut rng: ResMut<SimRng>,
    mut crises: ResMut<Crises>,
    mut fired: ResMut<FiredEvents>,
    mut tension: ResMut<GlobalTension>,
) {
    use tuning::*;
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;

    // --- Consume decisions answered through the command log -----------
    let new_resolved: Vec<(String, u8)> = fired
        .resolved
        .iter()
        .skip(crises.resolved_cursor)
        .cloned()
        .collect();
    crises.resolved_cursor = fired.resolved.len();
    for (id, option) in new_resolved {
        let Some(pos) = crises.active.iter().position(|c| id == choice_id(c)) else {
            continue;
        };
        let mut crisis = crises.active.remove(pos);
        let actor = crisis.ball.clone();
        let other = crisis.other(&actor);
        // Interpret the answer by label — the option list varies with
        // the rung and the state of the taboo.
        let opts = options_for(crisis.rung, programs.taboo_broken);
        let action = opts
            .get(option as usize)
            .map(|l| {
                if l.starts_with("ESCALATE") {
                    1u8
                } else if l.starts_with("SEEK") {
                    2u8
                } else {
                    0u8
                }
            })
            .unwrap_or(0);
        match action {
            // 0 = BACK DOWN (also the deadline default).
            0 => {
                let climbed = if actor == crisis.initiator {
                    crisis.initiator_climbed
                } else {
                    crisis.target_climbed
                } as i64;
                let cost = match crisis.rung {
                    0..=3 => BACKDOWN_COST_LOW,
                    4..=5 => BACKDOWN_COST_MID,
                    _ => BACKDOWN_COST_HIGH,
                } * climbed.max(1);
                *crises.resolve.entry(actor.clone()).or_insert(START_RESOLVE) -= cost;
                *crises.resolve.entry(other.clone()).or_insert(START_RESOLVE) += VICTORY_RESOLVE;
                tension.apply(CRISIS_END_TENSION);
                fired.notices.push((
                    format!("{} ENDS", crisis.title),
                    format!(
                        "{} STANDS DOWN. {} PREVAILS ON {}. THE CLIMBDOWN IS NOTED IN EVERY CAPITAL.",
                        actor.0, other.0, crisis.stake
                    ),
                ));
            }
            // 1 = ESCALATE one rung; the ball passes.
            1 => {
                crisis.rung = (crisis.rung + 1).min(8);
                if crisis.rung >= 8 {
                    // The general exchange. Compute the funeral from the
                    // real demography and end the campaign.
                    let mut cities: Vec<(String, u64)> = data
                        .provinces
                        .values()
                        .filter(|p| p.owner == actor || p.owner == other)
                        .filter_map(|p| {
                            demo.provinces
                                .get(&p.id)
                                .map(|c| (p.name.clone(), c.total()))
                        })
                        .filter(|(_, pop)| *pop >= 300_000)
                        .collect();
                    cities.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                    let dead: u64 = cities.iter().map(|(_, pop)| pop * 6 / 10).sum();
                    cities.truncate(10);
                    tension.apply(1000);
                    fired.notices.push((
                        "GENERAL EXCHANGE".into(),
                        "STRATEGIC WARNING CONFIRMED. MULTIPLE LAUNCHES. THIS IS NOT AN EXERCISE. TRANSMISSION CONTINUES AS LONG AS POSSIBLE.".into(),
                    ));
                    commands.insert_resource(GameOver {
                        initiator: actor.clone(),
                        against: other.clone(),
                        tick: clock.tick,
                        dead,
                        cities,
                    });
                    continue;
                }
                if actor == crisis.initiator {
                    crisis.initiator_climbed += 1;
                } else {
                    crisis.target_climbed += 1;
                }
                if crisis.rung == 7 {
                    // Tactical use inside the crisis: firebreak B is
                    // crossed here, cities are next.
                    tension.apply(300);
                } else {
                    tension.apply(TENSION_PER_RUNG * crisis.rung as i32);
                }
                fired.notices.push((
                    format!("{} DEEPENS", crisis.title),
                    format!(
                        "{} ESCALATES TO RUNG {}. {}",
                        actor.0,
                        crisis.rung,
                        match crisis.rung {
                            5 => "OPEN HOSTILITIES. THE FIREBREAK IS BEHIND US.",
                            6 => "A NUCLEAR ULTIMATUM IS ON THE TABLE. ONE RUNG REMAINS.",
                            7 => "A NUCLEAR WEAPON HAS BEEN EMPLOYED IN THEATER. ONE RUNG REMAINS, AND IT IS THE LAST.",
                            _ => "THE OTHER SIDE MUST NOW ANSWER.",
                        }
                    ),
                ));
                crisis.ball = other;
                crisis.deadline_tick = clock.tick + DECISION_HOURS;
                post_decision(
                    &mut fired,
                    &crisis,
                    &clock,
                    DECISION_HOURS,
                    programs.taboo_broken,
                );
                crises.active.push(crisis);
            }
            // 2 = COMPROMISE (only offered at low rungs).
            _ => {
                *crises.resolve.entry(actor.clone()).or_insert(START_RESOLVE) -= 1;
                *crises.resolve.entry(other.clone()).or_insert(START_RESOLVE) -= 1;
                tension.apply(CRISIS_END_TENSION);
                fired.notices.push((
                    format!("{} SETTLED", crisis.title),
                    format!(
                        "QUIET DIPLOMACY DIVIDES {}. NEITHER CAPITAL CLAIMS VICTORY; BOTH CLAIM PEACE.",
                        crisis.stake
                    ),
                ));
            }
        }
    }

    // --- Hourly incident hazard (Schelling's chance) -------------------
    let mut incidents: Vec<u32> = Vec::new();
    // Fork only when a crisis is actually at hazardous altitude — a
    // fork advances the root stream, and quiet months must not perturb
    // the rest of the sim.
    let any_hazard = crises
        .active
        .iter()
        .any(|c| HAZARD_BP[(c.rung as usize).min(8)] > 0);
    if any_hazard {
        let mut stream = rng.fork(b"crisis");
        for c in &crises.active {
            let alert =
                |t: &CountryTag| programs.programs.get(t).map(|p| p.alert).unwrap_or(0) as usize;
            let mult = crate::nuclear::tuning::ALERT_HAZARD_MULT
                [alert(&c.initiator).max(alert(&c.target)).min(3)];
            let bp = HAZARD_BP[(c.rung as usize).min(8)] * mult;
            if bp > 0 && stream.below(10_000) < bp {
                incidents.push(c.id);
            }
        }
    }
    for id in incidents {
        let Some(c) = crises.active.iter_mut().find(|c| c.id == id) else {
            continue;
        };
        c.rung = (c.rung + 1).min(if programs.taboo_broken { 7 } else { 6 });
        tension.apply(INCIDENT_TENSION);
        // The incident preempts whatever question was pending.
        let stale = choice_id(c);
        fired
            .dynamic
            .retain(|d| !(d.id.starts_with(&format!("crisis-{}-", c.id)) && d.id != stale));
        fired.notices.push((
            "INCIDENT AT THE BRINK".into(),
            format!(
                "{} -- A RECONNAISSANCE AIRCRAFT IS DOWN. NOBODY ORDERED IT. THE CRISIS CLIMBS ON ITS OWN: RUNG {}. TIME AT ALTITUDE IS ITSELF THE WEAPON.",
                c.title, c.rung
            ),
        ));
        let deadline = tuning::DECISION_HOURS;
        c.deadline_tick = clock.tick + deadline;
        let crisis_snapshot = c.clone();
        post_decision(
            &mut fired,
            &crisis_snapshot,
            &clock,
            deadline,
            programs.taboo_broken,
        );
    }

    // --- Deadline enforcement + AI decisions ---------------------------
    // The player answers via commands; the AI (and an absent player)
    // decides deterministically. AI answers a day before the deadline.
    let ai_decisions: Vec<(String, u8)> = crises
        .active
        .iter()
        .filter(|c| {
            let is_player = player.0.as_ref() == Some(&c.ball);
            let due = if is_player {
                clock.tick >= c.deadline_tick
            } else {
                clock.tick + 24 >= c.deadline_tick
            };
            due && fired.dynamic.iter().any(|d| d.id == choice_id(c))
        })
        .map(|c| {
            let is_player = player.0.as_ref() == Some(&c.ball);
            if is_player {
                // Deadline default: back down. Nobody sleepwalks upward.
                (choice_id(c), 0u8)
            } else {
                let resolve = crises.resolve_of(&c.ball);
                // An AI only ever launches (rung 7 -> 8) at resolve 95+,
                // reachable through a long run of crisis victories — so a
                // player-facing exchange always follows the player's own
                // labeled choice one rung earlier, while an AI-vs-AI
                // world can still end itself if someone feeds a hardliner.
                let will_escalate = match c.rung {
                    0..=5 => resolve >= (c.rung as i64) * 14 + 6,
                    6 => programs.taboo_broken && resolve >= 85,
                    _ => resolve >= 95,
                };
                let compromise_ok = c.rung <= 3 && resolve < 40;
                let opts = options_for(c.rung, programs.taboo_broken);
                let idx_of = |prefix: &str| {
                    opts.iter()
                        .position(|l| l.starts_with(prefix))
                        .map(|i| i as u8)
                };
                if compromise_ok {
                    (choice_id(c), idx_of("SEEK").unwrap_or(0))
                } else if will_escalate {
                    (choice_id(c), idx_of("ESCALATE").unwrap_or(0))
                } else {
                    (choice_id(c), 0u8)
                }
            }
        })
        .collect();
    for (id, option) in ai_decisions {
        if let Some(pos) = fired.dynamic.iter().position(|d| d.id == id) {
            fired.dynamic.remove(pos);
            fired.resolved.push((id, option));
        }
    }

    // --- Monthly: spawn new crises from the flashpoint table -----------
    if !clock.new_month || crises.active.len() >= MAX_ACTIVE {
        return;
    }
    let mut stream = rng.fork(b"crisis-spawn");
    let mutual_anywhere = deterrence
        .dyads
        .values()
        .any(|d| d.class == DyadClass::Mutual);
    let mut chance = SPAWN_BASE_PERMILLE as u64 * tension.value().max(0) as u64 / 1000;
    if mutual_anywhere {
        chance = chance * MUTUAL_SPAWN_MULT_PERMILLE as u64 / 1000;
    }
    if (stream.below(1000) as u64) >= chance {
        return;
    }
    let Some(fp) = FLASHPOINTS
        .iter()
        .find(|f| !crises.used.iter().any(|u| u == f.slug))
    else {
        return;
    };
    let initiator = CountryTag(fp.initiator.to_string());
    let target = CountryTag(fp.target.to_string());
    if !data.countries.contains_key(&initiator) || !data.countries.contains_key(&target) {
        return;
    }
    // Opening rung is capped by the tension band the world is in.
    let opening = (tension.value().max(0) / 250 + 1).clamp(1, 4) as u8;
    crises.used.push(fp.slug.to_string());
    crises.next_id += 1;
    let crisis = Crisis {
        id: crises.next_id,
        slug: fp.slug.to_string(),
        title: fp.title.to_string(),
        stake: fp.stake.to_string(),
        initiator: initiator.clone(),
        target: target.clone(),
        rung: opening,
        ball: target,
        deadline_tick: clock.tick + DECISION_HOURS,
        initiator_climbed: opening,
        target_climbed: 0,
    };
    tension.apply(TENSION_PER_RUNG * opening as i32);
    fired
        .notices
        .push((format!("CRISIS: {}", fp.title), fp.body.to_string()));
    post_decision(
        &mut fired,
        &crisis,
        &clock,
        DECISION_HOURS,
        programs.taboo_broken,
    );
    crises.active.push(crisis);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::GameDate;
    use crate::command::{PendingCommands, SimCommand};
    use crate::{run_ticks, SimPlugin};
    use bevy_app::App;
    use std::path::Path;
    use std::sync::Arc;

    fn app_with_scenario() -> App {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 1950,
        });
        app.insert_resource(crate::demography::SimScenario(Arc::new(data)));
        app
    }

    /// Drive tension high and watch a crisis spawn, be answered by the
    /// AI on both sides, and end with a resolve transfer.
    #[test]
    fn crises_spawn_run_and_conclude() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 2);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::DebugAdjustTension(500));
        // Years of high tension: flashpoints fire eventually.
        run_ticks(&mut app, 24 * 365 * 3);
        let crises = app.world().resource::<Crises>();
        assert!(
            !crises.used.is_empty(),
            "at least one flashpoint should have fired in 3 years of Crisis-band tension"
        );
        let fired = app.world().resource::<FiredEvents>();
        assert!(
            fired.notices.iter().any(|(t, _)| t.starts_with("CRISIS:")),
            "crisis opening notice"
        );
        assert!(
            fired
                .notices
                .iter()
                .any(|(t, _)| t.contains("ENDS") || t.contains("SETTLED") || t.contains("DEEPENS")),
            "the crisis moved: {:?}",
            fired.notices.iter().map(|(t, _)| t).collect::<Vec<_>>()
        );
        assert!(
            crises.resolve.values().any(|r| *r != tuning::START_RESOLVE),
            "resolve shifted somewhere"
        );
    }

    /// A player-held ball that never answers defaults to backing down
    /// at the deadline — nobody sleepwalks upward.
    #[test]
    fn player_silence_backs_down() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 2);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetPlayerCountry {
                country: Some(CountryTag("USA".into())),
            });
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::DebugAdjustTension(500));
        run_ticks(&mut app, 24 * 365 * 3);
        let crises = app.world().resource::<Crises>();
        // Whatever crises fired at the USA were auto-backed-down (or
        // still live within their deadline window); none may sit past
        // deadline unanswered.
        let clock_tick = app.world().resource::<SimClock>().tick;
        for c in &crises.active {
            assert!(
                c.deadline_tick + 1 >= clock_tick.saturating_sub(1),
                "no crisis lingers past its deadline: {c:?}"
            );
        }
    }
}
