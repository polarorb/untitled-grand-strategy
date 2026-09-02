//! Ideology & influence warfare v1 — the alignment pillar
//! (docs/design/systems/influence.md). Every country carries one signed
//! position between the poles; the blocs move it with capped standing
//! programs and priced covert operations; the timeline, settlements and
//! occupation write into the same number; the era checkpoints score it.
//!
//! The bloc enum stays derived: `project()` is the ONLY writer of
//! `Military.alignments`, so every `alignment_of` consumer (basing,
//! patrons, red lines, the masthead) is untouched.

use std::collections::{BTreeMap, BTreeSet};

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{Alignment, CountryTag, ScenarioData};

use crate::demography::SimScenario;
use crate::events::{DynamicChoice, FiredEvents};
use crate::intel::Intel;
use crate::military::{Military, PlayerCountry};
use crate::rng::SimRng;
use crate::settlement::Settlements;
use crate::tension::{GlobalTension, TensionBand};
use crate::SimClock;

pub mod tuning {
    /// Enter a bloc band at |position| >= this (checked at rollover).
    pub const BAND_ENTER: i16 = 300;
    /// Leave a bloc band at |position| < this.
    pub const BAND_LEAVE: i16 = 150;
    /// Display depth: ALIGNED from BAND_ENTER, TREATY/SATELLITE from here.
    pub const DEPTH_TREATY: i16 = 700;
    /// A scripted or coup-made band change lands at this edge.
    pub const SHOVE_EDGE: i16 = 350;
    /// Aligned-but-untreatied 1950 seed when no data row exists.
    pub const SEED_ALIGNED: i16 = 450;
    /// A country that changed band within this many months moves at
    /// half rate back against the change.
    pub const REVERSAL_MONTHS: u64 = 12;
    /// Aid: construction-pool draw per tier per month (centi).
    pub const AID_CENTI_PER_TIER: u64 = 300;
    /// Aid: the announcement step, landed the month a program starts.
    pub const AID_ANNOUNCE: i16 = 50;
    /// Aid: monthly flow per tier once delivering.
    pub const AID_FLOW: i16 = 10;
    /// Presence (radio + missions): monthly flow per tier.
    pub const PRESENCE_FLOW: i16 = 5;
    /// States below this population (thousands) take double flow.
    pub const SMALL_STATE_K: u64 = 5000;
    /// Sponsor legitimacy below this halves every program it runs.
    pub const LEGIT_MALUS: i32 = -20;
    /// Programs on a target already inside the sponsor's band (at or
    /// past BAND_ENTER) run at half rate: consolidation is slow.
    /// Withdrawing a delivering aid program: shove away + tension.
    pub const WITHDRAW_SHOVE: i16 = 100;
    pub const WITHDRAW_TENSION: i32 = 50;
    /// Program-bought lean decays toward baseline when nobody spends.
    pub const DECAY: i16 = 5;
    /// After Bandung, unlocked neighbours of NAM champions pull to 0.
    pub const NAM_PULL: i16 = 5;
    /// Independence opens a contested window this long.
    pub const CONTEST_MONTHS: u64 = 24;
    /// Independence seeds position at baseline_pole * this... but the
    /// data row carries the value directly; this is the fallback.
    pub const BIRTH_LEAN: i16 = 150;
    /// Election push: bounded nudge on the seeded roll.
    pub const ELECTION_PUSH: i16 = 60;
    /// Election roll is uniform in [-ELECTION_ROLL, ELECTION_ROLL].
    pub const ELECTION_ROLL: i16 = 100;
    /// Incumbency: the election leans the way the country already does.
    pub const ELECTION_INCUMBENCY: i16 = 20;
    /// Offer window: an election within this many days is a valve.
    pub const ELECTION_WINDOW_DAYS: u64 = 183;
    /// Coup gate on dynamic stability.
    pub const COUP_STAB_GATE: u8 = 50;
    /// Coup preparation, launch to resolution.
    pub const COUP_PREP_DAYS: u64 = 90;
    /// Coup ladder: success permille = clamp(base + frontier, min, max).
    pub const COUP_BASE: i32 = 300;
    pub const COUP_MIN: i32 = 50;
    pub const COUP_MAX: i32 = 900;
    /// Coup effects.
    pub const COUP_STABILITY_HIT: i32 = -25;
    pub const COUP_FIZZLE_STABILITY: i32 = -5;
    pub const COUP_TENSION_BATTLEGROUND: i32 = 150;
    pub const COUP_TENSION_ELSEWHERE: i32 = 50;
    pub const COUP_LEGITIMACY: i32 = -5;
    pub const EXPOSED_TENSION: i32 = 40;
    pub const EXPOSED_LEGITIMACY: i32 = -10;
    pub const EXPOSED_DENIABILITY: u32 = 20;
    pub const EXPOSED_BACKLASH: i16 = 150;
    /// Recognising a junta you installed.
    pub const RECOGNISE_LEGITIMACY: i32 = -3;
    pub const RECOGNISE_LEAN: i16 = 50;
    /// Crush: lock months and the position edge.
    pub const CRUSH_LOCK_MONTHS: u64 = 60;
    /// The AI drops a program once the target sits this deep its way.
    pub const AI_DONE: i16 = 600;
    /// AI keeps this much pool before it will fund aid (centi).
    pub const AI_POOL_RESERVE: u64 = 900;
    /// Wire lines emitted per month at most.
    pub const WIRE_PER_MONTH: usize = 6;
    /// Era checkpoints (first tick of January).
    pub const CHECKPOINT_YEARS: [i32; 3] = [1955, 1960, 1965];
    /// Settlement clause teeth: client-state edge and lock, neutrality lock.
    pub const CLIENT_LOCK_MONTHS: u64 = 60;
    pub const NEUTRAL_LOCK_MONTHS: u64 = 120;
    /// A released occupation zone flows alignment/this into position.
    pub const ZONE_RELEASE_DIVISOR: i16 = 10;
}

/// The two poles. Program direction and army patronage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Pole {
    West,
    East,
}

impl Pole {
    pub fn sign(self) -> i16 {
        match self {
            Pole::West => 1,
            Pole::East => -1,
        }
    }
    pub fn of(alignment: Alignment) -> Option<Pole> {
        match alignment {
            Alignment::WesternBloc => Some(Pole::West),
            Alignment::EasternBloc => Some(Pole::East),
            Alignment::NonAligned => None,
        }
    }
    pub fn parse(s: &str) -> Option<Pole> {
        match s {
            "West" => Some(Pole::West),
            "East" => Some(Pole::East),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Pole::West => "WEST",
            Pole::East => "EAST",
        }
    }
    pub fn rival(self) -> Pole {
        match self {
            Pole::West => Pole::East,
            Pole::East => Pole::West,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProgramKind {
    /// Draws from the sponsor's construction pool; announcement step.
    Aid,
    /// Radio and missions; near-free; halved in closed regimes.
    Presence,
}

impl ProgramKind {
    pub fn label(self) -> &'static str {
        match self {
            ProgramKind::Aid => "AID",
            ProgramKind::Presence => "PRESENCE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Program {
    pub kind: ProgramKind,
    pub tier: u8,
    pub started_tick: u64,
    /// Months the program has actually delivered (aid: pool covered).
    pub months_delivered: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InfluenceOpKind {
    ElectionPush,
    SponsorCoup,
}

impl InfluenceOpKind {
    pub fn label(self) -> &'static str {
        match self {
            InfluenceOpKind::ElectionPush => "ELECTION PUSH",
            InfluenceOpKind::SponsorCoup => "COUP",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfluenceOp {
    pub kind: InfluenceOpKind,
    pub launched_tick: u64,
    /// Coups: launch + prep. Election pushes: resolved by the election.
    pub resolve_tick: u64,
    pub election_idx: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    /// `u64::MAX` = open-ended.
    pub until_tick: u64,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Verdict {
    #[default]
    None,
    Presence,
    Domination,
    Control,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::None => "NO PRESENCE",
            Verdict::Presence => "PRESENCE",
            Verdict::Domination => "DOMINATES",
            Verdict::Control => "CONTROLS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RegionStanding {
    pub west: u8,
    pub east: u8,
    pub denied: u8,
    pub west_verdict: Verdict,
    pub east_verdict: Verdict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub year: i32,
    pub standings: BTreeMap<String, RegionStanding>,
    /// Bloc totals: (states, population_k) for West / East / denied.
    pub totals: [(u32, u64); 3],
}

/// One accounted term on a country's monthly PRESSURES ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pressure {
    pub label: String,
    pub delta: i16,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Influence {
    /// -1000 East .. +1000 West.
    pub position: BTreeMap<CountryTag, i16>,
    /// The seed; program-bought lean decays toward it, never past.
    pub baseline: BTreeMap<CountryTag, i16>,
    pub lock: BTreeMap<CountryTag, Lock>,
    /// Independence windows: rates doubled, hysteresis suspended.
    pub contested_until: BTreeMap<CountryTag, u64>,
    pub last_band_change: BTreeMap<CountryTag, u64>,
    /// Who equips and trains the army — the coup gate.
    pub army_patron: BTreeMap<CountryTag, Pole>,
    /// Regime openness: closed regimes halve PRESENCE, raise the CI
    /// floor, and hold no elections.
    pub closed: BTreeSet<CountryTag>,
    /// Never decays; the next rising is worse.
    pub crushed_count: BTreeMap<CountryTag, u8>,
    pub slots: BTreeMap<CountryTag, u8>,
    pub op_slots: BTreeMap<CountryTag, u8>,
    pub presence_unlocked: BTreeSet<CountryTag>,
    pub programs: BTreeMap<(CountryTag, CountryTag), Program>,
    pub ops: BTreeMap<(CountryTag, CountryTag), InfluenceOp>,
    pub elections_fired: BTreeSet<u16>,
    pub standings: BTreeMap<String, RegionStanding>,
    pub checkpoints: Vec<Checkpoint>,
    /// States defined in data but not yet born (own no provinces).
    pub dormant: BTreeSet<CountryTag>,
    /// Seed position applied at birth (the newborn's baseline pole).
    pub birth_position: BTreeMap<CountryTag, i16>,
    pub seeded: bool,
    /// Cursor into FiredEvents.resolved for recognise-the-junta answers.
    recog_cursor: usize,
    // --- Derived narrative, excluded from the digest --------------------
    /// Last month's AI attributions: sponsor -> [(target, kind)].
    pub chequebook: BTreeMap<CountryTag, Vec<(CountryTag, ProgramKind)>>,
    /// Ring of (tick, line), capped at 60.
    pub wire: Vec<(u64, String)>,
    /// Last month's ranked wire lines (the paper reads these).
    pub last_month: Vec<String>,
    /// Last month's accounted terms per country (the dossier ledger).
    pub pressures: BTreeMap<CountryTag, Vec<Pressure>>,
}

impl Influence {
    pub fn position_of(&self, tag: &CountryTag) -> i16 {
        self.position.get(tag).copied().unwrap_or(0)
    }

    pub fn is_locked(&self, tag: &CountryTag, tick: u64) -> bool {
        self.lock.get(tag).is_some_and(|l| l.until_tick > tick)
    }

    pub fn lock_label(&self, tag: &CountryTag, tick: u64) -> Option<&str> {
        self.lock
            .get(tag)
            .filter(|l| l.until_tick > tick)
            .map(|l| l.label.as_str())
    }

    pub fn is_contested(&self, tag: &CountryTag, tick: u64) -> bool {
        self.contested_until.get(tag).is_some_and(|t| *t > tick)
    }

    pub fn is_closed(&self, tag: &CountryTag) -> bool {
        self.closed.contains(tag)
    }

    pub fn slots_of(&self, tag: &CountryTag) -> u8 {
        self.slots.get(tag).copied().unwrap_or(0)
    }

    pub fn op_slots_of(&self, tag: &CountryTag) -> u8 {
        self.op_slots.get(tag).copied().unwrap_or(0)
    }

    pub fn programs_of(&self, sponsor: &CountryTag) -> usize {
        self.programs.keys().filter(|(s, _)| s == sponsor).count()
    }

    pub fn ops_of(&self, sponsor: &CountryTag) -> usize {
        self.ops.keys().filter(|(s, _)| s == sponsor).count()
    }

    /// Display depth for the current position (locks read as treaty).
    pub fn depth_label(&self, tag: &CountryTag, tick: u64) -> &'static str {
        let p = self.position_of(tag).abs();
        if self.is_locked(tag, tick) || p >= tuning::DEPTH_TREATY {
            if self.position_of(tag) < 0 {
                "SATELLITE"
            } else {
                "TREATY"
            }
        } else if p >= tuning::BAND_ENTER {
            "ALIGNED"
        } else if p >= tuning::BAND_LEAVE {
            "LEANING"
        } else {
            "NON-ALIGNED"
        }
    }

    pub fn log(&mut self, tick: u64, line: String) {
        self.wire.push((tick, line));
        let overflow = self.wire.len().saturating_sub(60);
        if overflow > 0 {
            self.wire.drain(..overflow);
        }
    }

    /// The band a position implies, given the current band (hysteresis).
    fn band_for(position: i16, current: Alignment, contested: bool) -> Alignment {
        let enter = tuning::BAND_ENTER;
        let leave = if contested {
            tuning::BAND_ENTER
        } else {
            tuning::BAND_LEAVE
        };
        if position >= enter {
            Alignment::WesternBloc
        } else if position <= -enter {
            Alignment::EasternBloc
        } else if position.abs() < leave {
            Alignment::NonAligned
        } else {
            // Inside the hysteresis band: keep what we had, unless the
            // sign contradicts it.
            match current {
                Alignment::WesternBloc if position > 0 => current,
                Alignment::EasternBloc if position < 0 => current,
                _ => Alignment::NonAligned,
            }
        }
    }

    /// Project one country's position into the derived bloc enum. The
    /// only writer of `Military.alignments`. Returns the band change.
    pub fn project(
        &mut self,
        military: &mut Military,
        data: &ScenarioData,
        tag: &CountryTag,
        tick: u64,
    ) -> Option<(Alignment, Alignment)> {
        if self.dormant.contains(tag) {
            return None;
        }
        let current = military.alignment_of(data, tag);
        let contested = self.is_contested(tag, tick);
        let next = Self::band_for(self.position_of(tag), current, contested);
        military.alignments.insert(tag.clone(), next);
        if next != current {
            self.last_band_change.insert(tag.clone(), tick);
            Some((current, next))
        } else {
            None
        }
    }

    /// Move a position by a program/election/zone delta, honouring the
    /// reversal half-rate. Locks are checked by the caller.
    fn apply_delta(
        &mut self,
        military: &Military,
        data: &ScenarioData,
        tag: &CountryTag,
        delta: i16,
        tick: u64,
    ) -> i16 {
        if delta == 0 {
            return 0;
        }
        let pos = self.position_of(tag);
        let band = military.alignment_of(data, tag);
        let recent = self
            .last_band_change
            .get(tag)
            .is_some_and(|t| tick.saturating_sub(*t) < tuning::REVERSAL_MONTHS * 30 * 24);
        let against = match band {
            Alignment::WesternBloc => delta < 0,
            Alignment::EasternBloc => delta > 0,
            Alignment::NonAligned => false,
        };
        let applied = if recent && against { delta / 2 } else { delta };
        let next = (pos as i32 + applied as i32).clamp(-1000, 1000) as i16;
        self.position.insert(tag.clone(), next);
        next - pos
    }

    /// Scripted band change: a band-edge shove of position AND baseline
    /// (regime change is structural), a no-op if already in the band.
    pub fn shove(
        &mut self,
        military: &mut Military,
        data: &ScenarioData,
        tag: &CountryTag,
        band: Alignment,
        tick: u64,
    ) {
        if self.dormant.contains(tag) {
            // Not born yet: record the intent as the birth position.
            let v = match band {
                Alignment::WesternBloc => tuning::SHOVE_EDGE,
                Alignment::EasternBloc => -tuning::SHOVE_EDGE,
                Alignment::NonAligned => 0,
            };
            self.birth_position.insert(tag.clone(), v);
            return;
        }
        if military.alignment_of(data, tag) == band {
            return;
        }
        let v = match band {
            Alignment::WesternBloc => tuning::SHOVE_EDGE,
            Alignment::EasternBloc => -tuning::SHOVE_EDGE,
            Alignment::NonAligned => 0,
        };
        self.position.insert(tag.clone(), v);
        self.baseline.insert(tag.clone(), v);
        self.project(military, data, tag, tick);
    }

    /// Scripted structural shift: position and baseline move together.
    pub fn shift(&mut self, tag: &CountryTag, delta: i16) {
        let p = (self.position_of(tag) as i32 + delta as i32).clamp(-1000, 1000) as i16;
        self.position.insert(tag.clone(), p);
        let b = self.baseline.get(tag).copied().unwrap_or(0);
        let b = (b as i32 + delta as i32).clamp(-1000, 1000) as i16;
        self.baseline.insert(tag.clone(), b);
    }

    pub fn set_lock(&mut self, tag: &CountryTag, until_tick: u64, label: &str) {
        if until_tick == 0 {
            self.lock.remove(tag);
        } else {
            self.lock.insert(
                tag.clone(),
                Lock {
                    until_tick,
                    label: label.to_string(),
                },
            );
        }
    }

    pub fn crush(
        &mut self,
        military: &mut Military,
        data: &ScenarioData,
        patron: &CountryTag,
        country: &CountryTag,
        tick: u64,
    ) {
        let Some(pole) = Pole::of(military.alignment_of(data, patron)) else {
            return;
        };
        let edge = pole.sign() * tuning::DEPTH_TREATY;
        self.position.insert(country.clone(), edge);
        self.baseline.insert(country.clone(), edge);
        *self.crushed_count.entry(country.clone()).or_default() += 1;
        self.set_lock(
            country,
            tick + tuning::CRUSH_LOCK_MONTHS * 30 * 24,
            "GARRISON",
        );
        self.contested_until.remove(country);
        self.project(military, data, country, tick);
    }

    pub fn open_contest(&mut self, tag: &CountryTag, until_tick: u64) {
        self.contested_until.insert(tag.clone(), until_tick);
    }

    /// Independence: the newborn takes its birth lean (plus whatever
    /// programs accumulated on the announced tag), opens a window.
    pub fn on_independence(
        &mut self,
        military: &mut Military,
        data: &ScenarioData,
        tag: &CountryTag,
        tick: u64,
    ) {
        self.dormant.remove(tag);
        let birth = self.birth_position.get(tag).copied().unwrap_or(0);
        let lean = self.position_of(tag);
        let p = (lean as i32 + birth as i32).clamp(-1000, 1000) as i16;
        self.position.insert(tag.clone(), p);
        self.baseline.insert(tag.clone(), birth);
        self.open_contest(tag, tick + tuning::CONTEST_MONTHS * 30 * 24);
        self.project(military, data, tag, tick);
    }

    /// Settlement teeth: a client state sits at its patron's edge,
    /// locked for the truce term; a neutralized state sits at zero,
    /// locked longer.
    pub fn client_state(
        &mut self,
        military: &mut Military,
        data: &ScenarioData,
        state: &CountryTag,
        patron: &CountryTag,
        tick: u64,
    ) {
        if let Some(pole) = Pole::of(military.alignment_of(data, patron)) {
            let edge = pole.sign() * tuning::DEPTH_TREATY;
            self.position.insert(state.clone(), edge);
            self.baseline.insert(state.clone(), edge);
            self.set_lock(
                state,
                tick + tuning::CLIENT_LOCK_MONTHS * 30 * 24,
                "CLIENT TREATY",
            );
            self.contested_until.remove(state);
            self.project(military, data, state, tick);
        }
    }

    pub fn neutralize(
        &mut self,
        military: &mut Military,
        data: &ScenarioData,
        state: &CountryTag,
        tick: u64,
    ) {
        self.position.insert(state.clone(), 0);
        self.baseline.insert(state.clone(), 0);
        self.set_lock(
            state,
            tick + tuning::NEUTRAL_LOCK_MONTHS * 30 * 24,
            "NEUTRALIZED",
        );
        self.contested_until.remove(state);
        self.project(military, data, state, tick);
    }

    /// A released occupation zone flows its popular disposition toward
    /// the holder into the country position, through the holder's
    /// bloc frame (holder-relative, never a shared unit).
    pub fn zone_released(
        &mut self,
        military: &Military,
        data: &ScenarioData,
        holder: &CountryTag,
        country: &CountryTag,
        zone_alignment: i16,
        tick: u64,
    ) {
        let Some(pole) = Pole::of(military.alignment_of(data, holder)) else {
            return;
        };
        if self.is_locked(country, tick) {
            return;
        }
        let delta = pole.sign() * (zone_alignment / tuning::ZONE_RELEASE_DIVISOR);
        self.apply_delta(military, data, country, delta, tick);
    }

    // --- Verb validation (shared by the command hub and the UI) ---------

    #[allow(clippy::too_many_arguments)]
    pub fn can_start_program(
        &self,
        military: &Military,
        data: &ScenarioData,
        clock: &SimClock,
        sponsor: &CountryTag,
        target: &CountryTag,
        kind: ProgramKind,
        tier: u8,
    ) -> Result<(), &'static str> {
        if sponsor == target {
            return Err("NOT A FOREIGN COUNTRY");
        }
        if !(1..=3).contains(&tier) {
            return Err("TIER 1-3");
        }
        if self.slots_of(sponsor) == 0 {
            return Err("NO STANDING PROGRAMS: MINOR POWER");
        }
        if self
            .programs
            .contains_key(&(sponsor.clone(), target.clone()))
        {
            return Err("PROGRAM ALREADY RUNNING");
        }
        if self.programs_of(sponsor) >= self.slots_of(sponsor) as usize {
            return Err("NO SLOT");
        }
        if kind == ProgramKind::Presence && !self.presence_unlocked.contains(sponsor) {
            return Err("PRESENCE NOT YET AUTHORIZED");
        }
        if military.at_war(sponsor, target) {
            return Err("AT WAR -- USE THE WAR ROOM");
        }
        if self.is_locked(target, clock.tick) {
            return Err("LOCKED");
        }
        if self.dormant.contains(target) {
            let announced = data
                .influence
                .seeds
                .iter()
                .find(|s| &s.tag == target)
                .and_then(|s| s.announced);
            let now = (clock.date.year, clock.date.month, clock.date.day);
            match announced {
                Some(d) if d <= now => {}
                _ => return Err("INDEPENDENCE NOT YET ANNOUNCED"),
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn can_launch_op(
        &self,
        military: &Military,
        intel: &Intel,
        tension: &GlobalTension,
        data: &ScenarioData,
        clock: &SimClock,
        sponsor: &CountryTag,
        target: &CountryTag,
        kind: InfluenceOpKind,
    ) -> Result<(), &'static str> {
        if sponsor == target {
            return Err("NOT A FOREIGN COUNTRY");
        }
        if self.op_slots_of(sponsor) == 0 {
            return Err("NO COVERT ACTION AUTHORITY");
        }
        if self.ops.contains_key(&(sponsor.clone(), target.clone())) {
            return Err("OPERATION ALREADY PREPARING");
        }
        if self.ops_of(sponsor) >= self.op_slots_of(sponsor) as usize {
            return Err("OP SLOT BUSY");
        }
        if self.dormant.contains(target) {
            return Err("NO STATE TO ACT AGAINST");
        }
        if military.at_war(sponsor, target) {
            return Err("AT WAR -- USE THE WAR ROOM");
        }
        let strength = intel
            .networks
            .get(&(sponsor.clone(), target.clone()))
            .map(|n| n.strength)
            .unwrap_or(0);
        if strength < crate::intel::tuning::OP_MIN_STRENGTH {
            return Err("NETWORK TOO WEAK");
        }
        match kind {
            InfluenceOpKind::ElectionPush => {
                if self.closed.contains(target) {
                    return Err("NO ELECTIONS HELD");
                }
                if next_election(data, self, clock, target).is_none() {
                    return Err("NO ELECTION WITHIN SIX MONTHS");
                }
            }
            InfluenceOpKind::SponsorCoup => {
                if self.is_locked(target, clock.tick) {
                    return Err("LOCKED");
                }
                if military.stability_of(data, target) >= tuning::COUP_STAB_GATE {
                    return Err("GOVERNMENT TOO STABLE");
                }
                let sponsor_pole = Pole::of(military.alignment_of(data, sponsor));
                if let (Some(sp), Some(ap)) = (sponsor_pole, self.army_patron.get(target)) {
                    if *ap == sp.rival() {
                        return Err("ARMY EQUIPPED BY THE RIVAL");
                    }
                }
                let region = battleground_region(data, target);
                if coup_region_closed(tension.band(), region.as_deref()) {
                    return Err("CLOSED AT THIS TENSION");
                }
            }
        }
        Ok(())
    }

    /// The printed coup frontier: (success permille, frontier line).
    pub fn coup_frontier(
        &self,
        military: &Military,
        intel: &Intel,
        tension: &GlobalTension,
        data: &ScenarioData,
        sponsor: &CountryTag,
        target: &CountryTag,
    ) -> (u32, String) {
        let stability = military.stability_of(data, target) as i32;
        let tier = intel
            .networks
            .get(&(sponsor.clone(), target.clone()))
            .map(|n| n.funding as i32)
            .unwrap_or(0);
        let sponsor_pole = Pole::of(military.alignment_of(data, sponsor));
        let patron = self.army_patron.get(target).copied();
        let army_bonus = match (sponsor_pole, patron) {
            (Some(sp), Some(ap)) if sp == ap => 200,
            _ => 0,
        };
        let band = tension.value().max(0) / 250;
        let s = (50 - stability) * 10 + tier * 100 + army_bonus - band * 50;
        let p = (tuning::COUP_BASE + s).clamp(tuning::COUP_MIN, tuning::COUP_MAX) as u32;
        let army = match patron {
            Some(Pole::West) => "US-EQUIPPED",
            Some(Pole::East) => "SOVIET-EQUIPPED",
            None => "UNALIGNED",
        };
        let line = format!(
            "STAB {stability} · ARMY: {army} · NETWORK L{tier} · TENSION {}",
            tension.band()
        );
        (p, line)
    }

    pub fn digest(&self) -> u64 {
        fn fold(h: &mut u64, v: u64) {
            *h = (*h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        }
        fn fold_tag(h: &mut u64, tag: &CountryTag) {
            for b in tag.0.bytes() {
                fold(h, b as u64);
            }
        }
        fn fold_i16(h: &mut u64, v: i16) {
            fold(h, v.unsigned_abs() as u64 | ((v < 0) as u64) << 16);
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (t, v) in &self.position {
            fold_tag(&mut h, t);
            fold_i16(&mut h, *v);
        }
        for (t, v) in &self.baseline {
            fold_tag(&mut h, t);
            fold_i16(&mut h, *v);
        }
        for (t, l) in &self.lock {
            fold_tag(&mut h, t);
            fold(&mut h, l.until_tick);
            for b in l.label.bytes() {
                fold(&mut h, b as u64);
            }
        }
        for (t, v) in &self.contested_until {
            fold_tag(&mut h, t);
            fold(&mut h, *v);
        }
        for (t, v) in &self.last_band_change {
            fold_tag(&mut h, t);
            fold(&mut h, *v);
        }
        for (t, p) in &self.army_patron {
            fold_tag(&mut h, t);
            fold(&mut h, *p as u64 + 1);
        }
        for t in &self.closed {
            fold_tag(&mut h, t);
            fold(&mut h, 7);
        }
        for (t, v) in &self.crushed_count {
            fold_tag(&mut h, t);
            fold(&mut h, *v as u64);
        }
        for (t, v) in self.slots.iter().chain(self.op_slots.iter()) {
            fold_tag(&mut h, t);
            fold(&mut h, *v as u64);
        }
        for t in &self.presence_unlocked {
            fold_tag(&mut h, t);
            fold(&mut h, 9);
        }
        for ((s, t), p) in &self.programs {
            fold_tag(&mut h, s);
            fold_tag(&mut h, t);
            fold(&mut h, p.kind as u64);
            fold(&mut h, p.tier as u64);
            fold(&mut h, p.started_tick);
            fold(&mut h, p.months_delivered as u64);
        }
        for ((s, t), o) in &self.ops {
            fold_tag(&mut h, s);
            fold_tag(&mut h, t);
            fold(&mut h, o.kind as u64);
            fold(&mut h, o.resolve_tick);
            fold(&mut h, o.election_idx.map(|i| i as u64 + 1).unwrap_or(0));
        }
        for i in &self.elections_fired {
            fold(&mut h, *i as u64);
        }
        for (r, s) in &self.standings {
            for b in r.bytes() {
                fold(&mut h, b as u64);
            }
            fold(
                &mut h,
                s.west as u64 | (s.east as u64) << 8 | (s.denied as u64) << 16,
            );
            fold(&mut h, s.west_verdict as u64 | (s.east_verdict as u64) << 4);
        }
        for c in &self.checkpoints {
            fold(&mut h, c.year as u64);
            for (states, pop) in c.totals {
                fold(&mut h, states as u64);
                fold(&mut h, pop);
            }
            for (r, s) in &c.standings {
                for b in r.bytes() {
                    fold(&mut h, b as u64);
                }
                fold(
                    &mut h,
                    s.west as u64 | (s.east as u64) << 8 | (s.denied as u64) << 16,
                );
                fold(&mut h, s.west_verdict as u64 | (s.east_verdict as u64) << 4);
            }
        }
        for t in &self.dormant {
            fold_tag(&mut h, t);
            fold(&mut h, 11);
        }
        for (t, v) in &self.birth_position {
            fold_tag(&mut h, t);
            fold_i16(&mut h, *v);
        }
        fold(&mut h, self.seeded as u64);
        fold(&mut h, self.recog_cursor as u64);
        h
    }
}

/// The battleground region of a country, if it is one.
pub fn battleground_region(data: &ScenarioData, tag: &CountryTag) -> Option<String> {
    data.influence
        .battlegrounds
        .iter()
        .find(|b| &b.tag == tag)
        .map(|b| b.region.clone())
}

pub fn battleground_weight(data: &ScenarioData, tag: &CountryTag) -> u8 {
    data.influence
        .battlegrounds
        .iter()
        .find(|b| &b.tag == tag)
        .map(|b| b.weight)
        .unwrap_or(0)
}

/// Tension gates on coups by region: Crisis closes Europe; Brink closes
/// Europe, Asia and the Middle East. Nothing may force a coup at Brink
/// in those theaters.
pub fn coup_region_closed(band: TensionBand, region: Option<&str>) -> bool {
    match band {
        TensionBand::Calm | TensionBand::Wary => false,
        TensionBand::Crisis => region == Some("EUROPE"),
        TensionBand::Brink => matches!(region, Some("EUROPE" | "ASIA" | "MIDDLE_EAST")),
    }
}

fn date_to_tick_offset(clock: &SimClock, date: (i32, u8, u8)) -> Option<u64> {
    // Days from now until the date (None if past). Coarse month math is
    // fine for a window check; the calendar itself fires the election.
    let (y, m, d) = date;
    let now = (clock.date.year, clock.date.month, clock.date.day);
    if date < now {
        return None;
    }
    let days = (y - clock.date.year) as i64 * 365
        + (m as i64 - clock.date.month as i64) * 30
        + (d as i64 - clock.date.day as i64);
    Some(days.max(0) as u64)
}

/// The next unfired calendar election for a country within the offer
/// window: (index, days until, def).
pub fn next_election<'a>(
    data: &'a ScenarioData,
    influence: &Influence,
    clock: &SimClock,
    tag: &CountryTag,
) -> Option<(u16, u64, &'a ugs_data::ElectionDef)> {
    data.influence
        .elections
        .iter()
        .enumerate()
        .filter(|(i, e)| &e.tag == tag && !influence.elections_fired.contains(&(*i as u16)))
        .filter_map(|(i, e)| date_to_tick_offset(clock, e.date).map(|d| (i as u16, d, e)))
        .find(|(_, d, _)| *d <= tuning::ELECTION_WINDOW_DAYS)
}

/// Sherman Kent's estimative words, one legend.
pub fn kent_word(permille: u32) -> &'static str {
    if permille >= 800 {
        "ALMOST CERTAIN"
    } else if permille >= 600 {
        "PROBABLE"
    } else if permille >= 400 {
        "CHANCES ABOUT EVEN"
    } else if permille >= 200 {
        "PROBABLY NOT"
    } else {
        "ALMOST CERTAINLY NOT"
    }
}

// --- Command handlers ----------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn start_program(
    influence: &mut Influence,
    military: &Military,
    data: &ScenarioData,
    clock: &SimClock,
    sponsor: CountryTag,
    target: CountryTag,
    kind: ProgramKind,
    tier: u8,
) {
    match influence.can_start_program(military, data, clock, &sponsor, &target, kind, tier) {
        Ok(()) => {
            influence.programs.insert(
                (sponsor.clone(), target.clone()),
                Program {
                    kind,
                    tier,
                    started_tick: clock.tick,
                    months_delivered: 0,
                },
            );
            influence.log(
                clock.tick,
                format!(
                    "{} {} PROGRAM OPENED ON {} (TIER {tier})",
                    sponsor.0,
                    kind.label(),
                    target.0
                ),
            );
        }
        Err(why) => influence.log(
            clock.tick,
            format!("{} PROGRAM ON {} REFUSED: {why}", sponsor.0, target.0),
        ),
    }
}

/// Stop a program. A delivering aid program withdrawn is Aswan: a shove
/// the other way and a tension spike.
pub fn stop_program(
    influence: &mut Influence,
    military: &mut Military,
    data: &ScenarioData,
    tension: &mut GlobalTension,
    clock: &SimClock,
    sponsor: CountryTag,
    target: CountryTag,
) {
    let Some(p) = influence
        .programs
        .remove(&(sponsor.clone(), target.clone()))
    else {
        return;
    };
    if p.kind == ProgramKind::Aid
        && p.months_delivered >= 1
        && !influence.is_locked(&target, clock.tick)
    {
        if let Some(pole) = Pole::of(military.alignment_of(data, &sponsor)) {
            let applied = influence.apply_delta(
                military,
                data,
                &target,
                -pole.sign() * tuning::WITHDRAW_SHOVE,
                clock.tick,
            );
            influence
                .pressures
                .entry(target.clone())
                .or_default()
                .push(Pressure {
                    label: format!("{} AID WITHDRAWN", sponsor.0),
                    delta: applied,
                });
            tension.apply(tuning::WITHDRAW_TENSION);
            influence.project(military, data, &target, clock.tick);
        }
        influence.log(
            clock.tick,
            format!(
                "{} WITHDRAWS AID FROM {} -- THE OFFER IS DEAD AND THE WORLD KNOWS WHY",
                sponsor.0, target.0
            ),
        );
    } else {
        influence.log(
            clock.tick,
            format!(
                "{} {} PROGRAM ON {} CLOSED",
                sponsor.0,
                p.kind.label(),
                target.0
            ),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn launch_op(
    influence: &mut Influence,
    military: &Military,
    intel: &mut Intel,
    tension: &GlobalTension,
    data: &ScenarioData,
    clock: &SimClock,
    sponsor: CountryTag,
    target: CountryTag,
    kind: InfluenceOpKind,
) {
    match influence.can_launch_op(
        military, intel, tension, data, clock, &sponsor, &target, kind,
    ) {
        Ok(()) => {
            // Spending the network that also collects.
            if let Some(n) = intel.networks.get_mut(&(sponsor.clone(), target.clone())) {
                n.strength = n
                    .strength
                    .saturating_sub(crate::intel::tuning::OP_STRENGTH_COST);
            }
            let (resolve_tick, election_idx) = match kind {
                InfluenceOpKind::SponsorCoup => (clock.tick + tuning::COUP_PREP_DAYS * 24, None),
                InfluenceOpKind::ElectionPush => {
                    let (idx, days, _) = next_election(data, influence, clock, &target)
                        .expect("validated: election within window");
                    (clock.tick + days * 24, Some(idx))
                }
            };
            influence.ops.insert(
                (sponsor.clone(), target.clone()),
                InfluenceOp {
                    kind,
                    launched_tick: clock.tick,
                    resolve_tick,
                    election_idx,
                },
            );
            influence.log(
                clock.tick,
                format!(
                    "{} {} AGAINST {} IN PREPARATION",
                    sponsor.0,
                    kind.label(),
                    target.0
                ),
            );
        }
        Err(why) => influence.log(
            clock.tick,
            format!(
                "{} {} ON {} REFUSED: {why}",
                sponsor.0,
                kind.label(),
                target.0
            ),
        ),
    }
}

pub fn cancel_op(
    influence: &mut Influence,
    clock: &SimClock,
    sponsor: CountryTag,
    target: CountryTag,
) {
    if let Some(op) = influence.ops.remove(&(sponsor.clone(), target.clone())) {
        influence.log(
            clock.tick,
            format!(
                "{} {} AGAINST {} ABORTED -- ASSETS STOOD DOWN",
                sponsor.0,
                op.kind.label(),
                target.0
            ),
        );
    }
}

// --- Effects called from the event engine --------------------------------

/// `SetAlignment` in data: a band-edge shove.
pub fn effect_set_alignment(
    influence: &mut Influence,
    military: &mut Military,
    data: &ScenarioData,
    country: &CountryTag,
    alignment: &str,
    tick: u64,
) {
    let band = match alignment {
        "WesternBloc" => Alignment::WesternBloc,
        "EasternBloc" => Alignment::EasternBloc,
        _ => Alignment::NonAligned,
    };
    influence.shove(military, data, country, band, tick);
}

// --- The system ----------------------------------------------------------

fn seed(influence: &mut Influence, military: &mut Military, data: &ScenarioData, tick: u64) {
    // Which tags own nothing in 1950 (born later by Independence)?
    let mut owners: BTreeSet<&CountryTag> = BTreeSet::new();
    for p in data.provinces.values() {
        owners.insert(&p.owner);
    }
    for (tag, def) in &data.countries {
        let row = data.influence.seeds.iter().find(|s| &s.tag == tag);
        let dormant = !owners.contains(tag);
        if dormant {
            influence.dormant.insert(tag.clone());
            let birth = row.map(|r| r.position).unwrap_or(match def.alignment {
                Alignment::WesternBloc => tuning::BIRTH_LEAN,
                Alignment::EasternBloc => -tuning::BIRTH_LEAN,
                Alignment::NonAligned => 0,
            });
            influence.birth_position.insert(tag.clone(), birth);
            influence.position.insert(tag.clone(), 0);
            influence.baseline.insert(tag.clone(), 0);
        } else {
            let pos = row.map(|r| r.position).unwrap_or(match def.alignment {
                Alignment::WesternBloc => tuning::SEED_ALIGNED,
                Alignment::EasternBloc => -tuning::SEED_ALIGNED,
                Alignment::NonAligned => 0,
            });
            influence.position.insert(tag.clone(), pos);
            influence.baseline.insert(tag.clone(), pos);
        }
        if let Some(r) = row {
            if let Some(l) = &r.lock {
                let until = match l.until_year {
                    Some(y) => ticks_until_year(y) + 1,
                    None => u64::MAX,
                };
                influence.set_lock(tag, until, &l.label);
            }
            if let Some(p) = r.army_patron.as_deref().and_then(Pole::parse) {
                influence.army_patron.insert(tag.clone(), p);
            }
            if !r.open {
                influence.closed.insert(tag.clone());
            }
            if let Some(s) = r.stability {
                military.stability.entry(tag.clone()).or_insert(s);
            }
            if r.slots > 0 {
                influence.slots.insert(tag.clone(), r.slots);
            }
            if r.op_slots > 0 {
                influence.op_slots.insert(tag.clone(), r.op_slots);
            }
        } else if def.alignment == Alignment::EasternBloc {
            // No row: Eastern regimes are closed by default.
            influence.closed.insert(tag.clone());
        }
    }
    // Moscow starts with its presence machinery; Washington earns it
    // (Campaign of Truth, April 1950).
    influence.presence_unlocked.insert(CountryTag("SOV".into()));
    let tags: Vec<CountryTag> = data.countries.keys().cloned().collect();
    for tag in &tags {
        influence.project(military, data, tag, tick);
    }
    influence.last_band_change.clear();
    influence.seeded = true;
}

/// Ticks from the 1950-01-01 campaign start to 1 January of `year`,
/// on the real calendar (1952, 1956, 1960, 1964, 1968 are leap years).
fn ticks_until_year(year: i32) -> u64 {
    (1950..year.max(1950))
        .map(|y| {
            if crate::calendar::is_leap_year(y) {
                366
            } else {
                365
            }
        })
        .sum::<u64>()
        * 24
}

/// Seed the influence state from data if the scenario is present and
/// seeding has not happened. Called before the first tick boundary so
/// the paused boot screen already shows slots, locks and positions,
/// and so tick-one events land on seeded state.
pub fn ensure_seeded(world: &mut bevy_ecs::world::World) {
    let Some(scenario) = world.get_resource::<SimScenario>().map(|s| s.0.clone()) else {
        return;
    };
    if world.resource::<Influence>().seeded {
        return;
    }
    let tick = world.resource::<SimClock>().tick;
    let mut military = std::mem::take(&mut *world.resource_mut::<Military>());
    let mut influence = std::mem::take(&mut *world.resource_mut::<Influence>());
    seed(&mut influence, &mut military, &scenario, tick);
    *world.resource_mut::<Military>() = military;
    *world.resource_mut::<Influence>() = influence;
}

/// Per-country population (thousands) by current holder.
fn population_by_holder(data: &ScenarioData, military: &Military) -> BTreeMap<CountryTag, u64> {
    let mut out: BTreeMap<CountryTag, u64> = BTreeMap::new();
    for p in data.provinces.values() {
        let holder = military.owner_of(p.id, &p.owner);
        *out.entry(holder).or_default() += p.population_k as u64;
    }
    out
}

fn nam_champions(data: &ScenarioData, influence: &Influence) -> Vec<CountryTag> {
    ["IND", "EGY", "YUG", "IDN", "GHA"]
        .iter()
        .map(|t| CountryTag((*t).into()))
        .filter(|t| data.countries.contains_key(t) && !influence.dormant.contains(t))
        .collect()
}

#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
pub fn update_influence(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    player: Res<PlayerCountry>,
    mut rng: ResMut<SimRng>,
    mut influence: ResMut<Influence>,
    mut military: ResMut<Military>,
    mut intel: ResMut<Intel>,
    mut tension: ResMut<GlobalTension>,
    mut fired: ResMut<FiredEvents>,
    mut settlements: ResMut<Settlements>,
    mut construction: ResMut<crate::construction::Construction>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    if !influence.seeded {
        seed(&mut influence, &mut military, data, clock.tick);
    }

    // --- Recognise-the-junta answers ----------------------------------
    let answered: Vec<(String, u8)> = fired
        .resolved
        .iter()
        .skip(influence.recog_cursor)
        .filter(|(id, _)| id.starts_with("coup-recognise-"))
        .cloned()
        .collect();
    if influence.recog_cursor != fired.resolved.len() {
        influence.recog_cursor = fired.resolved.len();
    }
    for (id, option) in answered {
        // id: coup-recognise-<sponsor>-<target>-<tick>
        let parts: Vec<&str> = id.split('-').collect();
        if parts.len() < 5 {
            continue;
        }
        let sponsor = CountryTag(parts[2].into());
        let target = CountryTag(parts[3].into());
        if option == 0 {
            *settlements.legitimacy.entry(sponsor.clone()).or_default() +=
                tuning::RECOGNISE_LEGITIMACY;
            if let Some(pole) = Pole::of(military.alignment_of(data, &sponsor)) {
                let applied = influence.apply_delta(
                    &military,
                    data,
                    &target,
                    pole.sign() * tuning::RECOGNISE_LEAN,
                    clock.tick,
                );
                influence
                    .pressures
                    .entry(target.clone())
                    .or_default()
                    .push(Pressure {
                        label: format!("{} RECOGNISES THE JUNTA", sponsor.0),
                        delta: applied,
                    });
                influence.project(&mut military, data, &target, clock.tick);
            }
        }
    }

    // --- Due coups -----------------------------------------------------
    let due: Vec<(CountryTag, CountryTag)> = influence
        .ops
        .iter()
        .filter(|(_, o)| o.kind == InfluenceOpKind::SponsorCoup && o.resolve_tick <= clock.tick)
        .map(|(k, _)| k.clone())
        .collect();
    for (sponsor, target) in due {
        influence.ops.remove(&(sponsor.clone(), target.clone()));
        resolve_coup(
            &clock,
            data,
            &player,
            &mut rng,
            &mut influence,
            &mut military,
            &mut intel,
            &mut tension,
            &mut fired,
            &mut settlements,
            &sponsor,
            &target,
        );
    }

    // --- Elections from the sourced calendar ---------------------------
    let now = (clock.date.year, clock.date.month, clock.date.day);
    let due: Vec<u16> = data
        .influence
        .elections
        .iter()
        .enumerate()
        .filter(|(i, e)| !influence.elections_fired.contains(&(*i as u16)) && e.date <= now)
        .map(|(i, _)| i as u16)
        .collect();
    for idx in due {
        influence.elections_fired.insert(idx);
        resolve_election(
            &clock,
            data,
            &player,
            &mut rng,
            &mut influence,
            &mut military,
            &mut intel,
            &mut tension,
            &mut fired,
            &mut settlements,
            idx,
        );
    }

    if !clock.new_month {
        return;
    }
    monthly(
        &clock,
        data,
        &player,
        &mut influence,
        &mut military,
        &mut fired,
        &settlements,
        &mut construction,
    );
}

#[allow(clippy::too_many_arguments)]
fn resolve_coup(
    clock: &SimClock,
    data: &ScenarioData,
    player: &PlayerCountry,
    rng: &mut SimRng,
    influence: &mut Influence,
    military: &mut Military,
    intel: &mut Intel,
    tension: &mut GlobalTension,
    fired: &mut FiredEvents,
    settlements: &mut Settlements,
    sponsor: &CountryTag,
    target: &CountryTag,
) {
    // The gates are re-checked at resolution: a lock placed during the
    // ninety days (treaty, crush, neutralization) or a war between the
    // parties stands the operation down before any die is rolled.
    if influence.is_locked(target, clock.tick) || military.at_war(sponsor, target) {
        influence.log(
            clock.tick,
            format!(
                "{} COUP AGAINST {} STOOD DOWN: {}",
                sponsor.0,
                target.0,
                if military.at_war(sponsor, target) {
                    "AT WAR"
                } else {
                    "TARGET LOCKED"
                }
            ),
        );
        if player.0.as_ref() == Some(sponsor) {
            fired.notices.push((
                "OPERATION STOOD DOWN".into(),
                format!(
                    "THE SITUATION IN {} HAS CHANGED SINCE AUTHORIZATION. ASSETS WITHDRAWN WITHOUT INCIDENT.",
                    target.0
                ),
            ));
        }
        return;
    }
    let (p, frontier) = influence.coup_frontier(military, intel, tension, data, sponsor, target);
    let mut stream = rng.fork(b"coup");
    let success = stream.below(1000) < p;
    let ci = intel.ci_permille(target, influence.is_closed(target));
    let band = tension.value().max(0) as u32 / 250;
    let blown_chance = crate::intel::tuning::OP_BLOWN_BASE + ci / 4 + band * 40;
    let mut bstream = rng.fork(b"influence-blown");
    let blown = bstream.below(1000) < blown_chance;
    let sponsor_pole = Pole::of(military.alignment_of(data, sponsor));
    let battleground = battleground_weight(data, target) > 0;
    let name = data
        .countries
        .get(target)
        .map(|c| c.name.to_uppercase())
        .unwrap_or_else(|| target.0.clone());

    let rung;
    if success {
        rung = if blown {
            "FLIP WITH EVIDENCE"
        } else {
            "CLEAN FLIP"
        };
        if let Some(pole) = sponsor_pole {
            let edge = pole.sign() * tuning::SHOVE_EDGE;
            influence.position.insert(target.clone(), edge);
            influence.baseline.insert(target.clone(), edge);
            influence.army_patron.insert(target.clone(), pole);
        }
        influence.closed.insert(target.clone());
        influence.contested_until.remove(target);
        let st = military.stability_of(data, target) as i32;
        military.stability.insert(
            target.clone(),
            (st + tuning::COUP_STABILITY_HIT).clamp(0, 100) as u8,
        );
        tension.apply(if battleground {
            tuning::COUP_TENSION_BATTLEGROUND
        } else {
            tuning::COUP_TENSION_ELSEWHERE
        });
        *settlements.legitimacy.entry(sponsor.clone()).or_default() += tuning::COUP_LEGITIMACY;
        influence.project(military, data, target, clock.tick);
        influence.log(
            clock.tick,
            format!(
                "COUP IN {name}: THE ARMY MOVES AT DAWN. NEW GOVERNMENT LEANS {}",
                sponsor_pole.map(|p| p.label()).unwrap_or("NOWHERE")
            ),
        );
        fired.notices.push((
            format!("COUP D'ETAT IN {name}"),
            format!(
                "TANKS IN THE CAPITAL BEFORE DAWN. THE GOVERNMENT IS DISSOLVED, THE RADIO STATION HELD, MINISTERS UNDER ARREST. A JUNTA FRIENDLY TO {} ANNOUNCES ITSELF TO THE WORLD.{}",
                sponsor.0,
                if blown { " FOREIGN CORRESPONDENTS ARE ALREADY ASKING WHO PAID." } else { "" }
            ),
        ));
        if player.0.as_ref() == Some(sponsor) {
            fired.dynamic.push(DynamicChoice {
                id: format!("coup-recognise-{}-{}-{}", sponsor.0, target.0, clock.tick),
                title: format!("RECOGNISE THE JUNTA IN {name}?"),
                body: format!(
                    "OUR PEOPLE ARE IN. THE NEW GOVERNMENT ASKS FOR IMMEDIATE RECOGNITION. RECOGNISING IT BINDS THEM TO US AND US TO THEM -- AND TELLS THE WORLD WHOSE COUP IT WAS. ASSESSMENT BEFORE THE ACT: {frontier} -- {}.",
                    kent_word(p)
                ),
                country: sponsor.clone(),
                options: vec!["RECOGNISE AT ONCE".into(), "STAY QUIET".into()],
                deadline_tick: clock.tick + 72,
            });
        }
    } else {
        rung = if blown {
            "EXPOSED FAILURE"
        } else {
            "QUIET FIZZLE"
        };
        let st = military.stability_of(data, target) as i32;
        military.stability.insert(
            target.clone(),
            (st + tuning::COUP_FIZZLE_STABILITY).clamp(0, 100) as u8,
        );
        if blown {
            if let Some(pole) = sponsor_pole {
                let applied = influence.apply_delta(
                    military,
                    data,
                    target,
                    -pole.sign() * tuning::EXPOSED_BACKLASH,
                    clock.tick,
                );
                influence
                    .pressures
                    .entry(target.clone())
                    .or_default()
                    .push(Pressure {
                        label: format!("{} PLOT EXPOSED", sponsor.0),
                        delta: applied,
                    });
                if !influence.army_patron.contains_key(target) {
                    influence.army_patron.insert(target.clone(), pole.rival());
                }
                influence.project(military, data, target, clock.tick);
            }
        }
        influence.log(
            clock.tick,
            format!(
                "PLOT IN {name} {}",
                if blown {
                    "EXPOSED -- OFFICERS ARRESTED, DIPLOMATS EXPELLED"
                } else {
                    "FIZZLES -- NOTHING MOVES, NOBODY TALKS"
                }
            ),
        );
    }
    if blown {
        let den = intel.deniability.entry(sponsor.clone()).or_insert(100);
        *den = den.saturating_sub(tuning::EXPOSED_DENIABILITY);
        tension.apply(tuning::EXPOSED_TENSION);
        *settlements.legitimacy.entry(sponsor.clone()).or_default() += tuning::EXPOSED_LEGITIMACY;
        fired.notices.push((
            "COVERT ACTION EXPOSED".into(),
            format!(
                "{name} DISPLAYS EVIDENCE OF A {} PLOT AGAINST ITS GOVERNMENT. FORMAL PROTEST LODGED. THE INCIDENT IS A STANDING GRIEVANCE.",
                sponsor.0
            ),
        ));
    }
    if player.0.as_ref() == Some(sponsor) {
        fired.notices.push((
            format!("COUP -- {rung}"),
            format!(
                "OPERATION AGAINST {name} RESOLVED: {rung}. ASSESSMENT WAS {} ({frontier}).",
                kent_word(p)
            ),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_election(
    clock: &SimClock,
    data: &ScenarioData,
    player: &PlayerCountry,
    rng: &mut SimRng,
    influence: &mut Influence,
    military: &mut Military,
    intel: &mut Intel,
    tension: &mut GlobalTension,
    fired: &mut FiredEvents,
    settlements: &mut Settlements,
    idx: u16,
) {
    let def = &data.influence.elections[idx as usize];
    let tag = def.tag.clone();
    let name = data
        .countries
        .get(&tag)
        .map(|c| c.name.to_uppercase())
        .unwrap_or_else(|| tag.0.clone());
    // Pushes aimed at this election.
    let pushes: Vec<(CountryTag, InfluenceOp)> = influence
        .ops
        .iter()
        .filter(|((_, t), o)| *t == tag && o.election_idx == Some(idx))
        .map(|((s, _), o)| (s.clone(), o.clone()))
        .collect();
    for (s, _) in &pushes {
        influence.ops.remove(&(s.clone(), tag.clone()));
    }
    if influence.dormant.contains(&tag) || influence.is_closed(&tag) {
        influence.log(clock.tick, format!("{name}: NO ELECTION HELD"));
        return;
    }
    let occupied = data
        .provinces
        .values()
        .any(|p| p.owner == tag && military.owner_of(p.id, &p.owner) != tag);
    if occupied
        || influence.is_locked(&tag, clock.tick)
            && influence.position_of(&tag).abs() >= tuning::DEPTH_TREATY
    {
        // Locked-deep or occupied: the vote is held but changes nothing.
        influence.log(clock.tick, format!("{name}: {}", def.result));
        return;
    }
    let mut stream = rng.fork(b"elections");
    let roll = stream.below(tuning::ELECTION_ROLL as u32 * 2 + 1) as i16 - tuning::ELECTION_ROLL;
    let pos = influence.position_of(&tag);
    let mut swing = roll + pos.signum() * tuning::ELECTION_INCUMBENCY;
    let mut terms: Vec<Pressure> = vec![Pressure {
        label: "THE VOTE".into(),
        delta: roll,
    }];
    for (sponsor, _) in &pushes {
        let Some(pole) = Pole::of(military.alignment_of(data, sponsor)) else {
            continue;
        };
        let ci = intel.ci_permille(&tag, influence.is_closed(&tag));
        let band = tension.value().max(0) as u32 / 250;
        let blown_chance = crate::intel::tuning::OP_BLOWN_BASE + ci / 4 + band * 40;
        let mut bstream = rng.fork(b"influence-blown");
        let blown = bstream.below(1000) < blown_chance;
        let push = if blown {
            -pole.sign() * tuning::ELECTION_PUSH
        } else {
            pole.sign() * tuning::ELECTION_PUSH
        };
        swing += push;
        terms.push(Pressure {
            label: format!(
                "{} {}",
                sponsor.0,
                if blown {
                    "MEDDLING EXPOSED"
                } else {
                    "ELECTION PUSH"
                }
            ),
            delta: push,
        });
        if blown {
            let den = intel.deniability.entry(sponsor.clone()).or_insert(100);
            *den = den.saturating_sub(tuning::EXPOSED_DENIABILITY);
            *settlements.legitimacy.entry(sponsor.clone()).or_default() +=
                tuning::EXPOSED_LEGITIMACY;
            tension.apply(tuning::EXPOSED_TENSION / 2);
            fired.notices.push((
                "FOREIGN MONEY IN THE BALLOT BOX".into(),
                format!(
                    "{name} PRESS EXPOSES {} FUNDS BEHIND THE CAMPAIGN. THE ELECTORATE TURNS ON THE BENEFICIARY.",
                    sponsor.0
                ),
            ));
        }
    }
    let applied = if influence.is_locked(&tag, clock.tick) {
        0
    } else {
        influence.apply_delta(military, data, &tag, swing, clock.tick)
    };
    let mut ledger = influence.pressures.entry(tag.clone()).or_default();
    ledger.append(&mut terms);
    let _ = &mut ledger;
    influence.log(
        clock.tick,
        format!(
            "{name} VOTES: {} ({})",
            def.result,
            if applied >= 0 {
                format!("+{applied}")
            } else {
                applied.to_string()
            }
        ),
    );
    let concerned =
        pushes.iter().any(|(s, _)| player.0.as_ref() == Some(s)) || player.0.as_ref() == Some(&tag);
    if concerned {
        fired.notices.push((
            format!("{name} GOES TO THE POLLS"),
            format!("{} -- {}.", def.stake, def.result),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn monthly(
    clock: &SimClock,
    data: &ScenarioData,
    player: &PlayerCountry,
    influence: &mut Influence,
    military: &mut Military,
    fired: &mut FiredEvents,
    settlements: &Settlements,
    construction: &mut crate::construction::Construction,
) {
    let tick = clock.tick;
    // Mid-month terms (elections, coups, withdrawals) are the real cause
    // of a rollover band change: keep them for the cause lookup.
    let carried = std::mem::take(&mut influence.pressures);
    let mut lines: Vec<(u64, String)> = Vec::new(); // (weight, line)
    let pop = population_by_holder(data, military);
    let start_positions: BTreeMap<CountryTag, i16> = influence.position.clone();

    // --- Programs ------------------------------------------------------
    let keys: Vec<(CountryTag, CountryTag)> = influence.programs.keys().cloned().collect();
    let mut touched: BTreeSet<CountryTag> = BTreeSet::new();
    for key in keys {
        let (sponsor, target) = key.clone();
        let prog = influence.programs[&key].clone();
        let sponsor_pole = Pole::of(military.alignment_of(data, &sponsor));
        // Auto-suspend when the target locks (against anyone: a locked
        // position does not move by program).
        if influence.is_locked(&target, tick) {
            influence.programs.remove(&key);
            lines.push((
                5,
                format!(
                    "{} {} PROGRAM ON {} SUSPENDED: {}",
                    sponsor.0,
                    prog.kind.label(),
                    target.0,
                    influence.lock_label(&target, tick).unwrap_or("LOCKED")
                ),
            ));
            continue;
        }
        touched.insert(target.clone());
        let mut rate: i16 = match prog.kind {
            ProgramKind::Aid => {
                let draw = tuning::AID_CENTI_PER_TIER * prog.tier as u64;
                let pool = construction.pool.entry(sponsor.clone()).or_default();
                if *pool < draw {
                    lines.push((
                        3,
                        format!("{} AID TO {} DELAYED: POOL EMPTY", sponsor.0, target.0),
                    ));
                    continue;
                }
                *pool -= draw;
                let first = prog.months_delivered == 0;
                if let Some(p) = influence.programs.get_mut(&key) {
                    p.months_delivered += 1;
                }
                tuning::AID_FLOW * prog.tier as i16 + if first { tuning::AID_ANNOUNCE } else { 0 }
            }
            ProgramKind::Presence => {
                if let Some(p) = influence.programs.get_mut(&key) {
                    p.months_delivered += 1;
                }
                let r = tuning::PRESENCE_FLOW * prog.tier as i16;
                if influence.is_closed(&target) {
                    r / 2
                } else {
                    r
                }
            }
        };
        if pop.get(&target).copied().unwrap_or(0) < tuning::SMALL_STATE_K {
            rate *= 2;
        }
        if influence.is_contested(&target, tick) {
            rate *= 2;
        }
        if settlements.legitimacy_of(&sponsor) < tuning::LEGIT_MALUS {
            rate /= 2;
        }
        // Diminishing returns: a target already inside the sponsor's band
        // consolidates at half rate (an unopposed program must not paint
        // a small state to the edge in five years).
        let pos = influence.position_of(&target);
        if let Some(p) = sponsor_pole {
            if p.sign() * pos >= tuning::BAND_ENTER {
                rate /= 2;
            }
        }
        // Direction: toward the sponsor's pole; neutral sponsors pull to 0.
        let delta = match sponsor_pole {
            Some(p) => p.sign() * rate,
            None => -pos.signum() * rate.min(pos.abs()),
        };
        let applied = influence.apply_delta(military, data, &target, delta, tick);
        influence
            .pressures
            .entry(target.clone())
            .or_default()
            .push(Pressure {
                label: format!(
                    "{} {}{}",
                    sponsor.0,
                    prog.kind.label(),
                    if prog.kind == ProgramKind::Aid && prog.months_delivered == 1 {
                        " (ANNOUNCED)"
                    } else {
                        ""
                    }
                ),
                delta: applied,
            });
    }

    // --- Decay toward baseline where nobody spends ----------------------
    let tags: Vec<CountryTag> = influence.position.keys().cloned().collect();
    for tag in &tags {
        if touched.contains(tag)
            || influence.dormant.contains(tag)
            || influence.is_locked(tag, tick)
        {
            continue;
        }
        let pos = influence.position_of(tag);
        let base = influence.baseline.get(tag).copied().unwrap_or(0);
        if pos == base {
            continue;
        }
        let step = (base - pos).signum() * tuning::DECAY.min((base - pos).abs());
        let applied = influence.apply_delta(military, data, tag, step, tick);
        if applied != 0 {
            influence
                .pressures
                .entry(tag.clone())
                .or_default()
                .push(Pressure {
                    label: "DRIFT TO BASELINE".into(),
                    delta: applied,
                });
        }
    }

    // --- The non-aligned pull after Bandung ------------------------------
    if fired.fired_ticks.contains_key("bandung-conference") {
        let champions = nam_champions(data, influence);
        let mut neighbours: BTreeSet<CountryTag> = BTreeSet::new();
        for p in data.provinces.values() {
            let holder = military.owner_of(p.id, &p.owner);
            if !champions.contains(&holder) {
                continue;
            }
            for adj in &p.adjacent {
                if let Some(ad) = data.provinces.get(adj) {
                    let h = military.owner_of(*adj, &ad.owner);
                    if !champions.contains(&h) {
                        neighbours.insert(h);
                    }
                }
            }
        }
        for tag in neighbours {
            if influence.dormant.contains(&tag) || influence.is_locked(&tag, tick) {
                continue;
            }
            let pos = influence.position_of(&tag);
            if pos == 0 {
                continue;
            }
            let step = -pos.signum() * tuning::NAM_PULL.min(pos.abs());
            let applied = influence.apply_delta(military, data, &tag, step, tick);
            if applied != 0 {
                influence
                    .pressures
                    .entry(tag.clone())
                    .or_default()
                    .push(Pressure {
                        label: "NON-ALIGNED PULL".into(),
                        delta: applied,
                    });
            }
        }
    }

    // --- Windows closing ---------------------------------------------------
    let closing: Vec<CountryTag> = influence
        .contested_until
        .iter()
        .filter(|(_, t)| **t <= tick)
        .map(|(k, _)| k.clone())
        .collect();
    for tag in closing {
        influence.contested_until.remove(&tag);
        let name = data
            .countries
            .get(&tag)
            .map(|c| c.name.to_uppercase())
            .unwrap_or_else(|| tag.0.clone());
        let band = Influence::band_for(
            influence.position_of(&tag),
            military.alignment_of(data, &tag),
            true,
        );
        let verdict = match band {
            Alignment::WesternBloc => "GOES WEST",
            Alignment::EasternBloc => "GOES EAST",
            Alignment::NonAligned => "GOES NON-ALIGNED; HARD TO MOVE",
        };
        lines.push((9, format!("{name} {verdict} -- THE CONTEST CLOSES")));
    }

    // --- Hysteresis and projection ------------------------------------------
    for tag in &tags {
        if let Some((from, to)) = influence.project(military, data, tag, tick) {
            let name = data
                .countries
                .get(tag)
                .map(|c| c.name.to_uppercase())
                .unwrap_or_else(|| tag.0.clone());
            let cause = influence
                .pressures
                .get(tag)
                .into_iter()
                .chain(carried.get(tag))
                .flat_map(|v| v.iter())
                .max_by_key(|p| p.delta.abs())
                .map(|p| p.label.clone())
                .unwrap_or_else(|| "STRUCTURAL".into());
            let w = 8 + battleground_weight(data, tag) as u64 * 2;
            lines.push((
                w,
                format!("{name}: {} -> {} ({cause})", band_word(from), band_word(to)),
            ));
            if player.0.is_some() && battleground_weight(data, tag) > 0 {
                fired.notices.push((
                    format!("{name} CHANGES SIDES"),
                    format!(
                        "{name} IS NOW {}. THE DECISIVE PRESSURE: {cause}.",
                        band_word(to)
                    ),
                ));
            }
        }
    }

    // --- Lock expiry lines ---------------------------------------------------
    let expired: Vec<CountryTag> = influence
        .lock
        .iter()
        .filter(|(_, l)| l.until_tick <= tick)
        .map(|(k, _)| k.clone())
        .collect();
    for tag in expired {
        let label = influence
            .lock
            .remove(&tag)
            .map(|l| l.label)
            .unwrap_or_default();
        lines.push((
            4,
            format!("{}: {label} LAPSES -- THE POSITION IS OPEN AGAIN", tag.0),
        ));
    }

    // --- Standings and checkpoints -----------------------------------------
    compute_standings(influence, military, data);
    if clock.date.month == 1
        && tuning::CHECKPOINT_YEARS.contains(&clock.date.year)
        && !influence
            .checkpoints
            .iter()
            .any(|c| c.year == clock.date.year)
    {
        let totals = bloc_totals(influence, military, data, &pop);
        influence.checkpoints.push(Checkpoint {
            year: clock.date.year,
            standings: influence.standings.clone(),
            totals,
        });
        lines.push((
            20,
            format!(
                "THE {} RECKONING: STANDINGS FROZEN FOR THE RECORD",
                clock.date.year
            ),
        ));
    }

    // --- AI allocator: overt verbs only, published, attributed ------------
    influence.chequebook.clear();
    for leader in ["USA", "SOV"] {
        let leader = CountryTag(leader.into());
        if player.0.as_ref() == Some(&leader) || !data.countries.contains_key(&leader) {
            continue;
        }
        let Some(pole) = Pole::of(military.alignment_of(data, &leader)) else {
            continue;
        };
        // Drop finished work.
        let done: Vec<(CountryTag, CountryTag)> = influence
            .programs
            .keys()
            .filter(|(s, t)| {
                *s == leader
                    && pole.sign() * start_positions.get(t).copied().unwrap_or(0) >= tuning::AI_DONE
            })
            .cloned()
            .collect();
        for key in done {
            influence.programs.remove(&key);
        }
        let free =
            (influence.slots_of(&leader) as usize).saturating_sub(influence.programs_of(&leader));
        if free == 0 {
            continue;
        }
        let now = (clock.date.year, clock.date.month, clock.date.day);
        let mut candidates: Vec<(u64, CountryTag)> = Vec::new();
        for tag in data.countries.keys() {
            if *tag == leader
                || military.at_war(&leader, tag)
                || influence.is_locked(tag, tick)
                || influence
                    .programs
                    .contains_key(&(leader.clone(), tag.clone()))
            {
                continue;
            }
            let p = start_positions.get(tag).copied().unwrap_or(0);
            if p.abs() >= tuning::DEPTH_TREATY {
                continue;
            }
            let weight = battleground_weight(data, tag) as u64;
            let contested = influence.is_contested(tag, tick);
            let announced = influence.dormant.contains(tag)
                && data
                    .influence
                    .seeds
                    .iter()
                    .find(|s| &s.tag == tag)
                    .and_then(|s| s.announced)
                    .is_some_and(|d| d <= now);
            if weight == 0 && !contested && !announced {
                continue;
            }
            if influence.dormant.contains(tag) && !announced {
                continue;
            }
            let base = weight * 1000 + pop.get(tag).copied().unwrap_or(0) / 100;
            let score = base
                * (1000 - p.unsigned_abs() as u64)
                * if contested || announced { 2 } else { 1 };
            candidates.push((score, tag.clone()));
        }
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, target) in candidates.into_iter().take(free) {
            let pool = construction.pool.get(&leader).copied().unwrap_or(0);
            let kind = if pool >= tuning::AID_CENTI_PER_TIER + tuning::AI_POOL_RESERVE {
                ProgramKind::Aid
            } else if influence.presence_unlocked.contains(&leader) {
                ProgramKind::Presence
            } else {
                continue;
            };
            influence.programs.insert(
                (leader.clone(), target.clone()),
                Program {
                    kind,
                    tier: 1,
                    started_tick: tick,
                    months_delivered: 0,
                },
            );
            influence
                .chequebook
                .entry(leader.clone())
                .or_default()
                .push((target.clone(), kind));
        }
        if let Some(list) = influence.chequebook.get(&leader) {
            let names: Vec<String> = list
                .iter()
                .map(|(t, k)| format!("{} ({})", t.0, k.label()))
                .collect();
            lines.push((
                6,
                format!(
                    "{}'S CHEQUEBOOK: {}",
                    if leader.0 == "USA" {
                        "WASHINGTON"
                    } else {
                        "MOSCOW"
                    },
                    names.join(", ")
                ),
            ));
        }
    }

    // --- The wire: ranked, capped -------------------------------------------
    lines.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    influence.last_month = lines
        .iter()
        .take(tuning::WIRE_PER_MONTH)
        .map(|(_, l)| l.clone())
        .collect();
    let emit: Vec<String> = influence.last_month.clone();
    for l in emit {
        influence.log(tick, l);
    }
}

pub fn band_word(a: Alignment) -> &'static str {
    match a {
        Alignment::WesternBloc => "WESTERN",
        Alignment::EasternBloc => "EASTERN",
        Alignment::NonAligned => "NON-ALIGNED",
    }
}

fn compute_standings(influence: &mut Influence, military: &Military, data: &ScenarioData) {
    let mut out: BTreeMap<String, RegionStanding> = BTreeMap::new();
    for t in &data.influence.thresholds {
        out.insert(t.region.clone(), RegionStanding::default());
    }
    for bg in &data.influence.battlegrounds {
        if influence.dormant.contains(&bg.tag) {
            continue;
        }
        let entry = out.entry(bg.region.clone()).or_default();
        match military.alignment_of(data, &bg.tag) {
            Alignment::WesternBloc => entry.west += 1,
            Alignment::EasternBloc => entry.east += 1,
            Alignment::NonAligned => entry.denied += 1,
        }
    }
    for t in &data.influence.thresholds {
        if let Some(s) = out.get_mut(&t.region) {
            s.west_verdict = verdict(s.west, s.east, t);
            s.east_verdict = verdict(s.east, s.west, t);
        }
    }
    influence.standings = out;
}

fn verdict(mine: u8, rival: u8, t: &ugs_data::RegionThresholdDef) -> Verdict {
    if mine >= t.control && rival == 0 {
        Verdict::Control
    } else if mine >= t.domination && mine > rival {
        Verdict::Domination
    } else if mine >= t.presence && mine > 0 {
        Verdict::Presence
    } else {
        Verdict::None
    }
}

/// (states, population_k) for West / East / non-aligned, all countries.
pub fn bloc_totals(
    influence: &Influence,
    military: &Military,
    data: &ScenarioData,
    pop: &BTreeMap<CountryTag, u64>,
) -> [(u32, u64); 3] {
    let mut t = [(0u32, 0u64); 3];
    for tag in data.countries.keys() {
        if influence.dormant.contains(tag) {
            continue;
        }
        let i = match military.alignment_of(data, tag) {
            Alignment::WesternBloc => 0,
            Alignment::EasternBloc => 1,
            Alignment::NonAligned => 2,
        };
        t[i].0 += 1;
        t[i].1 += pop.get(tag).copied().unwrap_or(0);
    }
    t
}

/// The verdict sentence for one country's dossier footer.
pub fn verdict_sentence(
    influence: &Influence,
    military: &Military,
    data: &ScenarioData,
    clock: &SimClock,
    tag: &CountryTag,
) -> String {
    let band = military.alignment_of(data, tag);
    let depth = influence.depth_label(tag, clock.tick);
    let trend = influence
        .pressures
        .get(tag)
        .map(|v| v.iter().map(|p| p.delta as i32).sum::<i32>())
        .unwrap_or(0);
    let trend_word = if trend > 0 {
        "DRIFTING WEST"
    } else if trend < 0 {
        "DRIFTING EAST"
    } else {
        "HOLDING"
    };
    let mut parts = vec![
        format!("{} ({depth})", band_word(band)),
        trend_word.to_string(),
    ];
    if let Some(l) = influence.lock_label(tag, clock.tick) {
        parts.push(format!("LOCKED: {l}"));
    }
    if influence.is_contested(tag, clock.tick) {
        let months = influence
            .contested_until
            .get(tag)
            .map(|t| t.saturating_sub(clock.tick) / (30 * 24))
            .unwrap_or(0);
        parts.push(format!("CONTESTED, {months} MONTHS LEFT"));
    }
    if let Some((_, days, e)) = next_election(data, influence, clock, tag) {
        parts.push(format!(
            "ELECTION IN {} MONTHS: {}",
            (days / 30).max(1),
            e.stake
        ));
    }
    match influence.army_patron.get(tag) {
        Some(Pole::West) => parts.push("ARMY: US-EQUIPPED".into()),
        Some(Pole::East) => parts.push("ARMY: SOVIET-EQUIPPED".into()),
        None => {}
    }
    parts.join(" · ")
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
    use ugs_data::{BattlegroundDef, ElectionDef, InfluenceSeedDef, LockDef, RegionThresholdDef};

    fn tag(s: &str) -> CountryTag {
        CountryTag(s.into())
    }

    fn seed(t: &str, position: i16, slots: u8, op_slots: u8) -> InfluenceSeedDef {
        InfluenceSeedDef {
            tag: tag(t),
            position,
            lock: None,
            army_patron: None,
            open: true,
            stability: None,
            slots,
            op_slots,
            announced: None,
        }
    }

    /// The 1950 scenario with a synthetic influence table: superpowers
    /// with slots, a few open battlegrounds, one election.
    fn synthetic_data() -> ugs_data::ScenarioData {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let mut data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        data.influence = ugs_data::InfluenceData {
            seeds: vec![
                seed("USA", 1000, 3, 1),
                seed("SOV", -1000, 3, 1),
                seed("IRN", 100, 0, 0),
                seed("ITA", 450, 0, 0),
                InfluenceSeedDef {
                    stability: Some(30),
                    ..seed("GTM", 200, 0, 0)
                },
                InfluenceSeedDef {
                    lock: Some(LockDef {
                        label: "WARSAW PACT".into(),
                        until_year: None,
                    }),
                    open: false,
                    ..seed("POL", -700, 0, 0)
                },
            ],
            battlegrounds: vec![
                BattlegroundDef {
                    tag: tag("IRN"),
                    region: "MIDDLE_EAST".into(),
                    weight: 3,
                },
                BattlegroundDef {
                    tag: tag("ITA"),
                    region: "EUROPE".into(),
                    weight: 3,
                },
                BattlegroundDef {
                    tag: tag("GTM"),
                    region: "CENTRAL_AMERICA".into(),
                    weight: 2,
                },
            ],
            thresholds: vec![
                RegionThresholdDef {
                    region: "MIDDLE_EAST".into(),
                    presence: 1,
                    domination: 1,
                    control: 1,
                },
                RegionThresholdDef {
                    region: "EUROPE".into(),
                    presence: 1,
                    domination: 1,
                    control: 1,
                },
                RegionThresholdDef {
                    region: "CENTRAL_AMERICA".into(),
                    presence: 1,
                    domination: 1,
                    control: 1,
                },
            ],
            elections: vec![ElectionDef {
                date: (1950, 2, 15),
                tag: tag("ITA"),
                stake: "A TEST BALLOT".into(),
                result: "THE COALITION HOLDS".into(),
            }],
        };
        data
    }

    fn app_with(data: ugs_data::ScenarioData, seed_value: u64) -> App {
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: seed_value,
        });
        app.insert_resource(SimScenario(Arc::new(data)));
        app
    }

    fn boot() -> App {
        app_with(synthetic_data(), 7)
    }

    fn push(app: &mut App, cmd: SimCommand) {
        app.world_mut().resource_mut::<PendingCommands>().push(cmd);
    }

    #[test]
    fn seeds_project_the_1950_bands() {
        let mut app = boot();
        run_ticks(&mut app, 1);
        let world = app.world();
        let data = world.resource::<SimScenario>().0.clone();
        let inf = world.resource::<Influence>();
        let mil = world.resource::<Military>();
        assert!(inf.seeded);
        assert_eq!(mil.alignment_of(&data, &tag("USA")), Alignment::WesternBloc);
        assert_eq!(mil.alignment_of(&data, &tag("POL")), Alignment::EasternBloc);
        assert_eq!(mil.alignment_of(&data, &tag("IRN")), Alignment::NonAligned);
        assert_eq!(mil.alignment_of(&data, &tag("ITA")), Alignment::WesternBloc);
        assert!(inf.is_locked(&tag("POL"), 1), "seed lock holds");
        assert!(inf.is_closed(&tag("POL")));
        assert_eq!(
            mil.stability_of(&data, &tag("GTM")),
            30,
            "sourced stability overrides"
        );
        // A state born later is dormant and carries no band yet.
        assert!(inf.dormant.contains(&tag("GHA")));
        // Countries without a row seed from CountryDef.
        let jap = tag("JAP");
        if data.countries.contains_key(&jap) {
            assert_eq!(inf.position_of(&jap).abs(), tuning::SEED_ALIGNED);
        }
    }

    #[test]
    fn band_hysteresis_holds_the_line() {
        // Inside the band, keep what you had if the sign agrees.
        assert_eq!(
            Influence::band_for(200, Alignment::WesternBloc, false),
            Alignment::WesternBloc
        );
        assert_eq!(
            Influence::band_for(100, Alignment::WesternBloc, false),
            Alignment::NonAligned
        );
        assert_eq!(
            Influence::band_for(200, Alignment::NonAligned, false),
            Alignment::NonAligned
        );
        assert_eq!(
            Influence::band_for(300, Alignment::NonAligned, false),
            Alignment::WesternBloc
        );
        assert_eq!(
            Influence::band_for(-300, Alignment::NonAligned, false),
            Alignment::EasternBloc
        );
        // Contested windows have no hysteresis.
        assert_eq!(
            Influence::band_for(200, Alignment::WesternBloc, true),
            Alignment::NonAligned
        );
        // Sign contradiction always leaves.
        assert_eq!(
            Influence::band_for(-200, Alignment::WesternBloc, false),
            Alignment::NonAligned
        );
    }

    #[test]
    fn presence_moves_a_target_and_decay_pulls_it_back() {
        let mut app = boot();
        run_ticks(&mut app, 1);
        push(
            &mut app,
            SimCommand::StartProgram {
                sponsor: tag("SOV"),
                target: tag("IRN"),
                kind: ProgramKind::Presence,
                tier: 3,
            },
        );
        run_ticks(&mut app, 24 * 62); // two rollovers
        let before = app.world().resource::<Influence>().position_of(&tag("IRN"));
        assert!(
            before <= 100 - 30,
            "two months of tier-3 presence moved Iran east: {before}"
        );
        push(
            &mut app,
            SimCommand::StopProgram {
                sponsor: tag("SOV"),
                target: tag("IRN"),
            },
        );
        run_ticks(&mut app, 24 * 31);
        let after = app.world().resource::<Influence>().position_of(&tag("IRN"));
        assert!(
            after > before,
            "with nobody spending, the lean decays toward baseline"
        );
        assert!(after <= 100, "never past the baseline");
    }

    #[test]
    fn aid_draws_the_pool_and_withdrawal_is_aswan() {
        let mut app = boot();
        run_ticks(&mut app, 1);
        app.world_mut()
            .resource_mut::<crate::construction::Construction>()
            .pool
            .insert(tag("USA"), 5000);
        push(
            &mut app,
            SimCommand::StartProgram {
                sponsor: tag("USA"),
                target: tag("IRN"),
                kind: ProgramKind::Aid,
                tier: 2,
            },
        );
        run_ticks(&mut app, 24 * 31);
        let pool = app
            .world()
            .resource::<crate::construction::Construction>()
            .pool[&tag("USA")];
        assert!(pool < 5000, "aid draws the sponsor's construction pool");
        let announced = app.world().resource::<Influence>().position_of(&tag("IRN"));
        assert!(
            announced >= 100 + tuning::AID_ANNOUNCE,
            "the announcement step lands: {announced}"
        );
        let t0 = app.world().resource::<GlobalTension>().value();
        push(
            &mut app,
            SimCommand::StopProgram {
                sponsor: tag("USA"),
                target: tag("IRN"),
            },
        );
        run_ticks(&mut app, 1);
        let withdrawn = app.world().resource::<Influence>().position_of(&tag("IRN"));
        assert!(withdrawn < announced, "withdrawal shoves the other way");
        assert!(
            app.world().resource::<GlobalTension>().value() > t0,
            "and the world notices"
        );
    }

    #[test]
    fn slots_are_a_hard_budget() {
        let mut app = boot();
        run_ticks(&mut app, 1);
        for t in ["IRN", "ITA", "GTM", "EGY"] {
            push(
                &mut app,
                SimCommand::StartProgram {
                    sponsor: tag("SOV"),
                    target: tag(t),
                    kind: ProgramKind::Presence,
                    tier: 1,
                },
            );
        }
        run_ticks(&mut app, 1);
        let inf = app.world().resource::<Influence>();
        assert_eq!(
            inf.programs_of(&tag("SOV")),
            3,
            "three slots, four requests"
        );
        assert!(inf.wire.iter().any(|(_, l)| l.contains("REFUSED: NO SLOT")));
        // Locked targets are refused outright.
        let mut app2 = boot();
        run_ticks(&mut app2, 1);
        push(
            &mut app2,
            SimCommand::StartProgram {
                sponsor: tag("USA"),
                target: tag("POL"),
                kind: ProgramKind::Aid,
                tier: 1,
            },
        );
        run_ticks(&mut app2, 1);
        let inf2 = app2.world().resource::<Influence>();
        assert_eq!(inf2.programs_of(&tag("USA")), 0);
        assert!(inf2.wire.iter().any(|(_, l)| l.contains("REFUSED: LOCKED")));
    }

    #[test]
    fn set_alignment_is_a_band_edge_shove() {
        let mut app = boot();
        run_ticks(&mut app, 1);
        {
            let world = app.world_mut();
            let data = world.resource::<SimScenario>().0.clone();
            let mut mil = std::mem::take(&mut *world.resource_mut::<Military>());
            let mut inf = std::mem::take(&mut *world.resource_mut::<Influence>());
            let mil = &mut mil;
            // Already Western at 450: a no-op.
            effect_set_alignment(&mut inf, mil, &data, &tag("ITA"), "WesternBloc", 1);
            assert_eq!(inf.position_of(&tag("ITA")), 450);
            // Flipping East: lands at the edge and the enum follows.
            effect_set_alignment(&mut inf, mil, &data, &tag("ITA"), "EasternBloc", 1);
            assert_eq!(inf.position_of(&tag("ITA")), -tuning::SHOVE_EDGE);
            assert_eq!(
                inf.baseline[&tag("ITA")],
                -tuning::SHOVE_EDGE,
                "structural: baseline moves"
            );
            assert_eq!(mil.alignment_of(&data, &tag("ITA")), Alignment::EasternBloc);
            *world.resource_mut::<Military>() = std::mem::take(mil);
            *world.resource_mut::<Influence>() = inf;
        }
        // And the shove survives the monthly pass (no decay away from it).
        run_ticks(&mut app, 24 * 40);
        let world = app.world();
        let data = world.resource::<SimScenario>().0.clone();
        assert_eq!(
            world
                .resource::<Military>()
                .alignment_of(&data, &tag("ITA")),
            Alignment::EasternBloc
        );
    }

    #[test]
    fn the_election_calendar_fires_once() {
        let mut app = boot();
        run_ticks(&mut app, 24 * 50);
        let inf = app.world().resource::<Influence>();
        assert!(inf.elections_fired.contains(&0), "Italy voted on the 15th");
        assert!(inf.wire.iter().any(|(_, l)| l.contains("ITALY VOTES")));
        let count = inf
            .wire
            .iter()
            .filter(|(_, l)| l.contains("ITALY VOTES"))
            .count();
        assert_eq!(count, 1);
        // Italy stays Western: a +/-100 swing cannot clear the hysteresis
        // gate from 450 (an election is a nudge, not a flip verb).
        let data = app.world().resource::<SimScenario>().0.clone();
        assert_eq!(
            app.world()
                .resource::<Military>()
                .alignment_of(&data, &tag("ITA")),
            Alignment::WesternBloc
        );
    }

    #[test]
    fn an_election_push_needs_a_network_and_a_window() {
        let mut app = boot();
        run_ticks(&mut app, 1);
        push(
            &mut app,
            SimCommand::SetPlayerCountry {
                country: Some(tag("USA")),
            },
        );
        push(
            &mut app,
            SimCommand::LaunchInfluenceOp {
                sponsor: tag("USA"),
                target: tag("ITA"),
                kind: InfluenceOpKind::ElectionPush,
            },
        );
        run_ticks(&mut app, 1);
        {
            let inf = app.world().resource::<Influence>();
            assert!(inf.ops.is_empty());
            assert!(inf.wire.iter().any(|(_, l)| l.contains("NETWORK TOO WEAK")));
        }
        app.world_mut().resource_mut::<Intel>().networks.insert(
            (tag("USA"), tag("ITA")),
            crate::intel::Network {
                funding: 3,
                strength: 80,
            },
        );
        push(
            &mut app,
            SimCommand::LaunchInfluenceOp {
                sponsor: tag("USA"),
                target: tag("ITA"),
                kind: InfluenceOpKind::ElectionPush,
            },
        );
        run_ticks(&mut app, 1);
        {
            let inf = app.world().resource::<Influence>();
            assert_eq!(
                inf.ops.len(),
                1,
                "within six months of the ballot: accepted"
            );
            let strength =
                app.world().resource::<Intel>().networks[&(tag("USA"), tag("ITA"))].strength;
            assert_eq!(strength, 80 - crate::intel::tuning::OP_STRENGTH_COST);
        }
        run_ticks(&mut app, 24 * 50);
        let inf = app.world().resource::<Influence>();
        assert!(inf.ops.is_empty(), "the ballot consumed the push");
        assert!(
            inf.wire.iter().any(|(_, l)| l.contains("ITALY VOTES")),
            "and the result printed"
        );
    }

    #[test]
    fn a_coup_is_gated_prepared_and_resolved() {
        let mut app = boot();
        run_ticks(&mut app, 1);
        push(
            &mut app,
            SimCommand::SetPlayerCountry {
                country: Some(tag("USA")),
            },
        );
        for t in ["GTM", "ITA"] {
            app.world_mut().resource_mut::<Intel>().networks.insert(
                (tag("USA"), tag(t)),
                crate::intel::Network {
                    funding: 2,
                    strength: 90,
                },
            );
        }
        // Italy is too stable (60): refused. Guatemala at 30: accepted.
        push(
            &mut app,
            SimCommand::LaunchInfluenceOp {
                sponsor: tag("USA"),
                target: tag("ITA"),
                kind: InfluenceOpKind::SponsorCoup,
            },
        );
        push(
            &mut app,
            SimCommand::LaunchInfluenceOp {
                sponsor: tag("USA"),
                target: tag("GTM"),
                kind: InfluenceOpKind::SponsorCoup,
            },
        );
        run_ticks(&mut app, 1);
        {
            let world = app.world();
            let inf = world.resource::<Influence>();
            assert!(inf
                .wire
                .iter()
                .any(|(_, l)| l.contains("GOVERNMENT TOO STABLE")));
            assert_eq!(inf.ops.len(), 1);
            let data = world.resource::<SimScenario>().0.clone();
            let (p, line) = inf.coup_frontier(
                world.resource::<Military>(),
                world.resource::<Intel>(),
                world.resource::<GlobalTension>(),
                &data,
                &tag("USA"),
                &tag("GTM"),
            );
            assert!(line.contains("STAB 30"), "{line}");
            assert!(
                p >= 600,
                "stab 30 + L2 network: PROBABLE, got {p} ({})",
                kent_word(p)
            );
        }
        let stab_before = 30;
        run_ticks(&mut app, 24 * 91);
        let world = app.world();
        let data = world.resource::<SimScenario>().0.clone();
        let inf = world.resource::<Influence>();
        let fired = world.resource::<FiredEvents>();
        assert!(inf.ops.is_empty(), "resolved after ninety days");
        assert!(
            fired.notices.iter().any(|(t, _)| t.starts_with("COUP -- ")),
            "the sponsor is told the rung"
        );
        let stab = world
            .resource::<Military>()
            .stability_of(&data, &tag("GTM"));
        assert!(stab < stab_before, "every rung costs the target stability");
        let succeeded = fired
            .notices
            .iter()
            .any(|(t, _)| t.starts_with("COUP D'ETAT"));
        if succeeded {
            assert_eq!(inf.position_of(&tag("GTM")), tuning::SHOVE_EDGE);
            assert!(inf.is_closed(&tag("GTM")), "a junta is a closed regime");
            assert_eq!(inf.army_patron.get(&tag("GTM")), Some(&Pole::West));
        }
    }

    #[test]
    fn kent_words_have_one_legend() {
        assert_eq!(kent_word(850), "ALMOST CERTAIN");
        assert_eq!(kent_word(600), "PROBABLE");
        assert_eq!(kent_word(450), "CHANCES ABOUT EVEN");
        assert_eq!(kent_word(250), "PROBABLY NOT");
        assert_eq!(kent_word(50), "ALMOST CERTAINLY NOT");
    }

    #[test]
    fn the_allocator_spends_attributed_and_drops_finished_work() {
        // Hands off: both leaders fill their slots on the battlegrounds.
        let mut app = boot();
        for t in ["SOV", "USA"] {
            app.world_mut()
                .resource_mut::<crate::construction::Construction>()
                .pool
                .insert(tag(t), 5000);
        }
        run_ticks(&mut app, 24 * 32);
        let inf = app.world().resource::<Influence>();
        assert!(
            inf.programs_of(&tag("SOV")) > 0,
            "Moscow opened its chequebook"
        );
        assert!(inf.programs_of(&tag("USA")) > 0, "so did Washington");
        assert!(
            inf.wire
                .iter()
                .any(|(_, l)| l.contains("MOSCOW'S CHEQUEBOOK")),
            "and the paper can print it"
        );
        // Nobody targets a locked satellite.
        assert!(!inf.programs.contains_key(&(tag("USA"), tag("POL"))));
        // Standings computed over the battleground set.
        assert!(inf.standings.contains_key("EUROPE"));
        assert_eq!(inf.standings["EUROPE"].west, 1, "Italy counts West");
    }

    #[test]
    fn independence_opens_a_window_that_closes() {
        let mut data = synthetic_data();
        // Re-date Ghana's real independence to next month.
        let mut ghana = data
            .events
            .iter()
            .find(|e| e.id == "ghana-independence")
            .cloned()
            .expect("ghana event");
        ghana.id = "test-ghana".into();
        ghana.trigger = ugs_data::EventTrigger::Date((1950, 2, 1, 0));
        ghana.country = None;
        ghana.options.clear();
        data.events.retain(|e| !e.id.contains("ghana"));
        data.events.push(ghana);
        data.influence.seeds.push(InfluenceSeedDef {
            announced: Some((1950, 1, 15)),
            ..seed("GHA", -150, 0, 0)
        });
        let mut app = app_with(data, 3);
        run_ticks(&mut app, 24 * 10);
        // Before the announcement: refused. After: a program may target
        // the dormant tag and its lean carries into the birth.
        push(
            &mut app,
            SimCommand::StartProgram {
                sponsor: tag("SOV"),
                target: tag("GHA"),
                kind: ProgramKind::Presence,
                tier: 3,
            },
        );
        run_ticks(&mut app, 24 * 6);
        push(
            &mut app,
            SimCommand::StartProgram {
                sponsor: tag("SOV"),
                target: tag("GHA"),
                kind: ProgramKind::Presence,
                tier: 3,
            },
        );
        run_ticks(&mut app, 24 * 20); // birth on Feb 1
        {
            let world = app.world();
            let inf = world.resource::<Influence>();
            let data = world.resource::<SimScenario>().0.clone();
            assert!(inf
                .wire
                .iter()
                .any(|(_, l)| l.contains("NOT YET ANNOUNCED")));
            assert!(inf.programs.contains_key(&(tag("SOV"), tag("GHA"))));
            assert!(!inf.dormant.contains(&tag("GHA")), "born");
            assert!(inf.is_contested(&tag("GHA"), world.resource::<SimClock>().tick));
            assert_eq!(
                inf.baseline[&tag("GHA")],
                -150,
                "the birth lean is the baseline"
            );
            assert!(
                world
                    .resource::<Military>()
                    .alignment_of(&data, &tag("GHA"))
                    == Alignment::NonAligned,
                "born inside the non-aligned band"
            );
        }
        run_ticks(&mut app, 24 * 30 * 25);
        let world = app.world();
        let inf = world.resource::<Influence>();
        assert!(!inf.is_contested(&tag("GHA"), world.resource::<SimClock>().tick));
        assert!(inf
            .wire
            .iter()
            .any(|(_, l)| l.contains("THE CONTEST CLOSES")));
        // Doubled presence inside the window moved Ghana east of its cradle.
        assert!(inf.position_of(&tag("GHA")) < -150);
    }

    #[test]
    fn checkpoints_freeze_at_the_era_dates() {
        let mut app = boot();
        run_ticks(&mut app, 24 * (365 * 5 + 3));
        let inf = app.world().resource::<Influence>();
        assert_eq!(inf.checkpoints.len(), 1);
        assert_eq!(inf.checkpoints[0].year, 1955);
        let totals = inf.checkpoints[0].totals;
        assert!(totals[0].0 > 0 && totals[1].0 > 0 && totals[2].0 > 0);
    }

    #[test]
    fn determinism_holds_with_programs_and_ops_active() {
        fn run() -> u64 {
            let mut app = boot();
            run_ticks(&mut app, 1);
            push(
                &mut app,
                SimCommand::SetPlayerCountry {
                    country: Some(tag("USA")),
                },
            );
            app.world_mut()
                .resource_mut::<crate::construction::Construction>()
                .pool
                .insert(tag("USA"), 9000);
            app.world_mut().resource_mut::<Intel>().networks.insert(
                (tag("USA"), tag("GTM")),
                crate::intel::Network {
                    funding: 3,
                    strength: 90,
                },
            );
            push(
                &mut app,
                SimCommand::StartProgram {
                    sponsor: tag("USA"),
                    target: tag("IRN"),
                    kind: ProgramKind::Aid,
                    tier: 3,
                },
            );
            push(
                &mut app,
                SimCommand::LaunchInfluenceOp {
                    sponsor: tag("USA"),
                    target: tag("GTM"),
                    kind: InfluenceOpKind::SponsorCoup,
                },
            );
            run_ticks(&mut app, 24 * 200);
            let w = app.world();
            w.resource::<Influence>().digest()
                ^ w.resource::<Military>().digest().rotate_left(7)
                ^ w.resource::<Intel>().digest().rotate_left(13)
        }
        assert_eq!(run(), run());
    }
}

#[cfg(test)]
mod calibration {
    use crate::calendar::GameDate;
    use crate::{run_ticks, SimPlugin};
    use bevy_app::App;
    use std::path::Path;
    use std::sync::Arc;
    use ugs_data::{Alignment, CountryTag};

    pub(super) const ANCHORS: [&str; 30] = [
        "JAP", "FRG", "ITA", "FRA", "GRC", "TUR", "AUT", "FIN", "YUG", "ISL", "IRN", "IRQ", "EGY",
        "SYR", "AFG", "IND", "PAK", "IDN", "MMR", "KOR", "CUB", "GTM", "BRA", "CHL", "GHA", "GIN",
        "COD", "ALB", "POL", "PRC",
    ];

    /// The calibration harness (influence.md, principle 12): hands off,
    /// the anchor countries must sit in their sourced bands at the era
    /// checkpoints, with at most three mismatches across the whole set.
    #[test]
    fn hands_off_anchors_land_near_history() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 1950,
        });
        app.insert_resource(crate::demography::SimScenario(Arc::new(data)));
        // (year, tag, expected band) — sourced in influence.ron's comments.
        let expected: [(i32, &str, Alignment); 40] = [
            (1955, "JAP", Alignment::WesternBloc),
            (1955, "FRG", Alignment::WesternBloc),
            (1955, "ITA", Alignment::WesternBloc),
            (1955, "FRA", Alignment::WesternBloc),
            (1955, "GRC", Alignment::WesternBloc),
            (1955, "TUR", Alignment::WesternBloc),
            (1955, "ISL", Alignment::WesternBloc),
            (1955, "KOR", Alignment::WesternBloc),
            (1955, "IRN", Alignment::WesternBloc),
            (1955, "AUT", Alignment::NonAligned),
            (1955, "FIN", Alignment::NonAligned),
            (1955, "YUG", Alignment::NonAligned),
            (1955, "IND", Alignment::NonAligned),
            (1955, "IDN", Alignment::NonAligned),
            (1955, "EGY", Alignment::NonAligned),
            (1955, "ALB", Alignment::EasternBloc),
            (1955, "POL", Alignment::EasternBloc),
            (1955, "PRC", Alignment::EasternBloc),
            (1960, "EGY", Alignment::EasternBloc),
            (1960, "IRQ", Alignment::NonAligned),
            (1960, "GIN", Alignment::EasternBloc),
            (1960, "GHA", Alignment::NonAligned),
            (1960, "PAK", Alignment::WesternBloc),
            (1960, "AUT", Alignment::NonAligned),
            (1960, "IND", Alignment::NonAligned),
            (1960, "JAP", Alignment::WesternBloc),
            (1960, "POL", Alignment::EasternBloc),
            (1960, "CUB", Alignment::WesternBloc),
            (1965, "CUB", Alignment::EasternBloc),
            (1965, "COD", Alignment::WesternBloc),
            (1965, "EGY", Alignment::EasternBloc),
            (1965, "FIN", Alignment::NonAligned),
            (1965, "YUG", Alignment::NonAligned),
            (1965, "IND", Alignment::NonAligned),
            (1965, "POL", Alignment::EasternBloc),
            (1965, "ITA", Alignment::WesternBloc),
            (1965, "FRG", Alignment::WesternBloc),
            (1965, "KOR", Alignment::WesternBloc),
            (1965, "BRA", Alignment::WesternBloc),
            (1965, "CHL", Alignment::WesternBloc),
        ];
        let mut last = 0u64;
        let mut mismatches: Vec<String> = Vec::new();
        for year in [1955, 1960, 1965] {
            let ticks = (year - 1950) as u64 * 365 * 24 + ((year - 1950) as u64 / 4 + 1) * 24 + 48;
            run_ticks(&mut app, ticks - last);
            last = ticks;
            let world = app.world();
            let data = world.resource::<crate::demography::SimScenario>().0.clone();
            let mil = world.resource::<crate::military::Military>();
            let inf = world.resource::<super::Influence>();
            for (y, tag, band) in expected.iter().filter(|(y, _, _)| *y == year) {
                let t = CountryTag((*tag).into());
                let got = mil.alignment_of(&data, &t);
                if got != *band {
                    mismatches.push(format!(
                        "{y} {tag}: expected {band:?}, got {got:?} ({:+})",
                        inf.position_of(&t)
                    ));
                }
            }
            // The checkpoint froze and every anchor still exists.
            assert!(
                inf.checkpoints.iter().any(|c| c.year == year),
                "{year} checkpoint frozen"
            );
        }
        assert!(
            mismatches.len() <= 3,
            "too many anchor mismatches ({}): {:?}",
            mismatches.len(),
            mismatches
        );
        // Every 1950 treaty lock still holds its band in 1965 (locks only
        // lapse by event).
        let world = app.world();
        let data = world.resource::<crate::demography::SimScenario>().0.clone();
        let mil = world.resource::<crate::military::Military>();
        for tag in ["GBR", "CAN", "BEL", "NLD", "NOR", "DNK", "POR", "LUX"] {
            assert_eq!(
                mil.alignment_of(&data, &CountryTag(tag.into())),
                Alignment::WesternBloc,
                "{tag} holds NATO"
            );
        }
        for tag in ["CSK", "HUN", "ROU", "BGR", "GDR"] {
            assert_eq!(
                mil.alignment_of(&data, &CountryTag(tag.into())),
                Alignment::EasternBloc,
                "{tag} stays a satellite"
            );
        }
    }

    /// Diagnostic: print the hands-off bands at the era checkpoints.
    /// `cargo test -p ugs-sim hands_off_bands -- --ignored --nocapture`
    #[test]
    #[ignore = "diagnostic: prints anchor bands at 1955/1960/1965"]
    fn hands_off_bands() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 1950,
        });
        app.insert_resource(crate::demography::SimScenario(Arc::new(data)));
        let mut last = 0u64;
        for year in [1955, 1960, 1965] {
            let target = crate::calendar::GameDate::new(1950, 1, 1, 0);
            let _ = target;
            let ticks = (year - 1950) as u64 * 365 * 24 + ((year - 1950) as u64 / 4 + 1) * 24 + 48;
            run_ticks(&mut app, ticks - last);
            last = ticks;
            let world = app.world();
            let data = world.resource::<crate::demography::SimScenario>().0.clone();
            let mil = world.resource::<crate::military::Military>();
            let inf = world.resource::<super::Influence>();
            let clock = world.resource::<crate::SimClock>();
            println!("=== {} ({})", year, clock.date);
            for a in ANCHORS {
                let t = CountryTag(a.into());
                let band = match mil.alignment_of(&data, &t) {
                    Alignment::WesternBloc => "W",
                    Alignment::EasternBloc => "E",
                    Alignment::NonAligned => "N",
                };
                println!(
                    "{a} {band} {:+} {}{}",
                    inf.position_of(&t),
                    if inf.dormant.contains(&t) {
                        "dormant "
                    } else {
                        ""
                    },
                    inf.lock_label(&t, clock.tick).unwrap_or("")
                );
            }
            for (r, s) in &inf.standings {
                println!(
                    "  {r}: W{} E{} N{} -> W {:?} / E {:?}",
                    s.west, s.east, s.denied, s.west_verdict, s.east_verdict
                );
            }
            println!("  programs: {}", inf.programs.len());
            for ((s, t), p) in &inf.programs {
                println!("    {} -> {} {:?} t{}", s.0, t.0, p.kind, p.tier);
            }
        }
    }
}
