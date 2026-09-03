//! Era scoring & the verdict (docs/design/systems/scoring.md). The record
//! is kept in words at four dates; the map is scored as a delta from the
//! 1950 par in the nation's own column; catastrophe is a state above the
//! score, never a term inside it; the exchange has no winner.
//!
//! A pure monthly fold over existing digest-stable resources — the only
//! occupant of `TickSet::Resolve`. Nothing in the sim reads the ledger.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{Alignment, CountryTag, ScenarioData};

use crate::crisis::GameOver;
use crate::demography::SimScenario;
use crate::events::FiredEvents;
use crate::influence::{self, Influence, RegionStanding};
use crate::military::{Military, PlayerCountry};
use crate::nuclear::NuclearPrograms;
use crate::planning::Economies;
use crate::settlement::Settlements;
use crate::tension::{GlobalTension, TensionBand};
use crate::SimClock;

pub mod tuning {
    /// OUTPUT: one point per this many permille of growth over the rival.
    pub const OUTPUT_STEP: i32 = 50;
    pub const OUTPUT_CAP: i32 = 4;
    /// The economy calibration gate (scoring.md, post-slice 1): the
    /// hands-off economy grows Soviet industry per head about ten times
    /// faster than American across all twenty years (+157/+272/+303/+342
    /// permille per era against +16/-31/+49/+93), with no 1960s slowdown.
    /// Until the economy doc calibrates that, OUTPUT is computed and
    /// printed as context but excluded from the era total.
    pub const OUTPUT_GATED: bool = true;
    /// STANDING: legitimacy per band point; band cap; per-era delta cap.
    pub const STANDING_STEP: i32 = 30;
    pub const STANDING_BAND_CAP: i32 = 4;
    pub const STANDING_DELTA_CAP: i32 = 3;
    /// PEACE credits and debits.
    pub const PEACE_SETTLED: i32 = 2;
    pub const PEACE_SETTLED_CAP: i32 = 4;
    /// One point per this many own dead per 10,000 of 1950 population.
    pub const PEACE_DEAD_UNIT: u64 = 2;
    pub const PEACE_DEAD_FLOOR: i32 = 8;
    pub const PEACE_FIRST_USE: i32 = 4;
    pub const PEACE_BRINK_FLOOR: i32 = 4;
    /// SCARRED at this many own dead per 10,000 of 1950 population in one era.
    pub const SCARRED_DEAD: u64 = 30;
    /// Era grade bands on |S x scale|.
    pub const GRADE_NARROW: i32 = 2;
    pub const GRADE_CLEAR: i32 = 6;
    pub const GRADE_DECISIVE: i32 = 12;
    /// Campaign class on C x scale. A decade of contested-world gains
    /// worth one CLEAR era does not win the century: HELD spans -9..+9.
    pub const CLASS_WON: i32 = 10;
    pub const CLASS_LOST: i32 = -10;
    /// Monthly dead band on |provisional S x scale|.
    pub const WORD_DEAD_BAND: i32 = 1;
    pub const HEAD_TO_HEAD_BAND: i32 = 2;
    pub const DEFAULT_SCALE: u8 = 3;
    /// The close of the record.
    pub const FINAL_YEAR: i32 = 1970;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Column {
    West,
    East,
    Denied,
}

impl Column {
    pub fn of(a: Alignment) -> Column {
        match a {
            Alignment::WesternBloc => Column::West,
            Alignment::EasternBloc => Column::East,
            Alignment::NonAligned => Column::Denied,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Column::West => "WEST",
            Column::East => "EAST",
            Column::Denied => "THE FIELD",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Catastrophe {
    #[default]
    Unscarred,
    Scarred,
    Exchange,
}

/// The term that moved a card most, with an id the app can template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Cause {
    #[default]
    None,
    Map(String),
    Output,
    Standing,
    Peace,
}

impl Cause {
    /// The panel key that owns the term.
    pub fn key(&self) -> &'static str {
        match self {
            Cause::None => "",
            Cause::Map(_) => "P",
            Cause::Output => "E",
            Cause::Standing => "R",
            Cause::Peace => "B",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Word {
    Gaining,
    #[default]
    Holding,
    Slipping,
}

impl Word {
    pub fn label(self) -> &'static str {
        match self {
            Word::Gaining => "GAINING",
            Word::Holding => "HOLDING",
            Word::Slipping => "SLIPPING",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Grade {
    Stalemate,
    Narrow,
    Clear,
    Decisive,
}

impl Grade {
    pub fn label(self) -> &'static str {
        match self {
            Grade::Stalemate => "STALEMATE",
            Grade::Narrow => "NARROW",
            Grade::Clear => "CLEAR",
            Grade::Decisive => "DECISIVE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
    Won,
    Held,
    Lost,
    Costly,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Class::Won => "WON",
            Class::Held => "HELD",
            Class::Lost => "LOST",
            Class::Costly => "COSTLY",
        }
    }
}

/// The 1950 baseline (or the birth baseline for a state born later).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Par {
    pub board: i32,
    /// The 1950 board in each column (West, East, Denied), so a nation
    /// that changes column is compared in the column it holds now.
    #[serde(default)]
    pub board_by_column: [i32; 3],
    pub legband: i32,
    /// Industry per thousand people, centi.
    pub ipc: u64,
    pub pop_k: u64,
    pub casualties: u64,
    pub uses: u16,
    pub treaties: usize,
    pub tick: u64,
}

/// One nation's era card: the four terms and the exact inputs the
/// 1970 reveal prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Card {
    pub map: i32,
    pub output: i32,
    pub standing: i32,
    pub peace: i32,
    pub catastrophe: Catastrophe,
    pub cause: Cause,
    pub column: Option<Column>,
    /// Inputs at the freeze (truth).
    pub board: i32,
    pub ipc: u64,
    pub ipc_reported: u64,
    pub growth_permille: i32,
    pub comparator_permille: i32,
    pub legitimacy: i32,
    pub dead: u64,
    pub uses: u16,
    pub treaties: u8,
    /// Cumulative inputs carried to the next era's delta.
    pub casualties_total: u64,
    pub uses_total: u16,
    pub treaties_total: usize,
    /// False for a state with no record yet (born this era).
    pub on_record: bool,
}

impl Card {
    /// The era score. OUTPUT joins the sum only once the economy
    /// calibration gate is lifted (`tuning::OUTPUT_GATED`).
    pub fn total(&self) -> i32 {
        self.map + self.scored_output() + self.standing + self.peace
    }

    pub fn scored_output(&self) -> i32 {
        if tuning::OUTPUT_GATED {
            0
        } else {
            self.output
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Era {
    pub year: i32,
    pub tick: u64,
    pub cards: BTreeMap<CountryTag, Card>,
    pub brink_months: u16,
    /// The prize: the region with the narrowest margin at the freeze.
    pub prize: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CampaignEnd {
    Reckoning { year: i32, tick: u64 },
    Exchange { initiator: CountryTag, tick: u64 },
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Ledger {
    pub par: BTreeMap<CountryTag, Par>,
    pub eras: Vec<Era>,
    /// Since the last freeze, recomputed monthly.
    pub provisional: BTreeMap<CountryTag, Card>,
    pub end: Option<CampaignEnd>,
    /// Months the world sat at Brink this era.
    pub brink_months: u16,
    /// The standing word per country and how many consecutive months
    /// it has agreed with itself (the arrow rule).
    pub last_word: BTreeMap<CountryTag, (Word, u8)>,
    pub seeded: bool,
}

// --- Data helpers ---------------------------------------------------------

pub fn scale_of(data: &ScenarioData, tag: &CountryTag) -> u8 {
    data.influence
        .scorecards
        .iter()
        .find(|s| &s.tag == tag)
        .map(|s| s.scale)
        .unwrap_or(tuning::DEFAULT_SCALE)
}

/// A nation's reach: its scorecard regions, else its own battleground
/// region, else nothing (a minor off the battleground set has no map
/// term, and the paper says so).
pub fn reach_of(data: &ScenarioData, tag: &CountryTag) -> Vec<String> {
    if let Some(s) = data.influence.scorecards.iter().find(|s| &s.tag == tag) {
        return s.reach.clone();
    }
    influence::battleground_region(data, tag)
        .into_iter()
        .collect()
}

fn region_value(data: &ScenarioData, region: &str, v: influence::Verdict) -> i32 {
    let Some(rv) = data
        .influence
        .region_values
        .iter()
        .find(|r| r.region == region)
    else {
        return 0;
    };
    match v {
        influence::Verdict::None => 0,
        influence::Verdict::Presence => rv.presence,
        influence::Verdict::Domination => rv.domination,
        influence::Verdict::Control => rv.control,
    }
}

/// Points for one column in one region: the verdict value plus the
/// battleground count. The denied column faces the stronger pole.
pub fn points(data: &ScenarioData, region: &str, s: &RegionStanding, col: Column) -> i32 {
    let Some(t) = data
        .influence
        .thresholds
        .iter()
        .find(|t| t.region == region)
    else {
        return 0;
    };
    let (mine, rival) = match col {
        Column::West => (s.west, s.east.max(s.denied)),
        Column::East => (s.east, s.west.max(s.denied)),
        Column::Denied => (s.denied, s.west.max(s.east)),
    };
    region_value(data, region, influence::verdict(mine, rival, t)) + mine as i32
}

/// A nation's board over its reach: a pole's points minus the rival
/// pole's (the field denies verdicts through `points`, but a newborn
/// state born non-aligned moves neither pole's board); the field's
/// points minus the stronger pole's.
pub fn board(
    data: &ScenarioData,
    standings: &BTreeMap<String, RegionStanding>,
    reach: &[String],
    col: Column,
) -> i32 {
    let mut total = 0;
    for r in reach {
        let Some(s) = standings.get(r) else { continue };
        let west = points(data, r, s, Column::West);
        let east = points(data, r, s, Column::East);
        total += match col {
            Column::West => west - east,
            Column::East => east - west,
            Column::Denied => points(data, r, s, Column::Denied) - west.max(east),
        };
    }
    total
}

fn ipc_of(econ: &Economies, pop: &BTreeMap<CountryTag, u64>, tag: &CountryTag) -> (u64, u64) {
    let p = pop.get(tag).copied().unwrap_or(0).max(1);
    let st = econ.industry.get(tag);
    let actual = st.map(|s| s.actual_centi).unwrap_or(0);
    let reported = st.map(|s| s.reported_centi).unwrap_or(actual);
    (actual * 1000 / p, reported * 1000 / p)
}

fn legband(legitimacy: i32) -> i32 {
    (legitimacy / tuning::STANDING_STEP)
        .clamp(-tuning::STANDING_BAND_CAP, tuning::STANDING_BAND_CAP)
}

fn growth_permille(now: u64, prev: u64) -> i32 {
    if prev == 0 {
        return 0;
    }
    ((now as i128 - prev as i128) * 1000 / prev as i128).clamp(-5000, 5000) as i32
}

/// The comparator for OUTPUT: the rival pole's leader for a bloc
/// member, the world median for the field.
fn comparator(col: Column, growth: &BTreeMap<CountryTag, i32>) -> i32 {
    let leader = |t: &str| growth.get(&CountryTag(t.into())).copied().unwrap_or(0);
    match col {
        Column::West => leader("SOV"),
        Column::East => leader("USA"),
        Column::Denied => {
            let mut v: Vec<i32> = growth.values().copied().collect();
            if v.is_empty() {
                return 0;
            }
            v.sort_unstable();
            v[v.len() / 2]
        }
    }
}

pub fn grade(card: &Card, scale: u8) -> (Grade, i32) {
    let s = card.total() * scale as i32;
    let g = match s.abs() {
        x if x >= tuning::GRADE_DECISIVE => Grade::Decisive,
        x if x >= tuning::GRADE_CLEAR => Grade::Clear,
        x if x >= tuning::GRADE_NARROW => Grade::Narrow,
        _ => Grade::Stalemate,
    };
    (g, s.signum())
}

impl Ledger {
    /// Campaign sum over the frozen eras.
    pub fn campaign_total(&self, tag: &CountryTag) -> i32 {
        self.eras
            .iter()
            .filter_map(|e| e.cards.get(tag))
            .map(|c| c.total())
            .sum()
    }

    pub fn worst_catastrophe(&self, tag: &CountryTag) -> Catastrophe {
        let mut worst = Catastrophe::Unscarred;
        if matches!(self.end, Some(CampaignEnd::Exchange { .. })) {
            return Catastrophe::Exchange;
        }
        for e in &self.eras {
            if let Some(c) = e.cards.get(tag) {
                worst = worst.max(c.catastrophe);
            }
        }
        worst
    }

    /// The campaign class: none after an exchange, COSTLY when scarred.
    pub fn class_of(&self, tag: &CountryTag, scale: u8) -> Option<Class> {
        match self.worst_catastrophe(tag) {
            Catastrophe::Exchange => None,
            Catastrophe::Scarred => Some(Class::Costly),
            Catastrophe::Unscarred => {
                let c = self.campaign_total(tag) * scale as i32;
                Some(if c >= tuning::CLASS_WON {
                    Class::Won
                } else if c <= tuning::CLASS_LOST {
                    Class::Lost
                } else {
                    Class::Held
                })
            }
        }
    }

    /// What the class would be on the ledger alone (printed with COSTLY).
    pub fn ledger_class(&self, tag: &CountryTag, scale: u8) -> Class {
        let c = self.campaign_total(tag) * scale as i32;
        if c >= tuning::CLASS_WON {
            Class::Won
        } else if c <= tuning::CLASS_LOST {
            Class::Lost
        } else {
            Class::Held
        }
    }

    pub fn word_of(&self, tag: &CountryTag) -> (Word, bool) {
        self.last_word
            .get(tag)
            .map(|(w, n)| (*w, *n >= 2))
            .unwrap_or((Word::Holding, false))
    }

    /// Head to head for the two poles: sign of S(USA) - S(SOV) on the
    /// provisional cards, with a dead band. +1 West ahead, -1 East, 0 even.
    pub fn head_to_head(&self) -> i32 {
        let s = |t: &str| {
            self.provisional
                .get(&CountryTag(t.into()))
                .map(|c| c.total())
                .unwrap_or(0)
        };
        let d = s("USA") - s("SOV");
        if d.abs() < tuning::HEAD_TO_HEAD_BAND {
            0
        } else {
            d.signum()
        }
    }

    /// The three (year, term, value) contributions of largest magnitude
    /// over the frozen eras.
    pub fn decisive_terms(&self, tag: &CountryTag) -> Vec<(i32, &'static str, i32)> {
        let mut v: Vec<(i32, &'static str, i32)> = Vec::new();
        for e in &self.eras {
            if let Some(c) = e.cards.get(tag) {
                v.push((e.year, "MAP", c.map));
                v.push((e.year, "OUTPUT", c.scored_output()));
                v.push((e.year, "STANDING", c.standing));
                v.push((e.year, "PEACE", c.peace));
            }
        }
        v.sort_by(|a, b| {
            b.2.abs()
                .cmp(&a.2.abs())
                .then(a.0.cmp(&b.0))
                .then(a.1.cmp(b.1))
        });
        v.into_iter().filter(|x| x.2 != 0).take(3).collect()
    }

    pub fn digest(&self) -> u64 {
        fn fold(h: &mut u64, v: u64) {
            *h = (*h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        }
        fn fold_i(h: &mut u64, v: i32) {
            fold(h, v.unsigned_abs() as u64 | ((v < 0) as u64) << 32);
        }
        fn fold_tag(h: &mut u64, t: &CountryTag) {
            for b in t.0.bytes() {
                fold(h, b as u64);
            }
        }
        fn fold_card(h: &mut u64, c: &Card) {
            for v in [c.map, c.output, c.standing, c.peace, c.board, c.legitimacy] {
                fold_i(h, v);
            }
            fold(h, c.catastrophe as u64);
            fold(h, c.ipc);
            fold(h, c.dead);
            fold(h, c.uses as u64);
            fold(h, c.treaties as u64);
            fold(h, c.on_record as u64);
            fold(h, c.column.map(|c| c as u64 + 1).unwrap_or(0));
            match &c.cause {
                Cause::None => fold(h, 0),
                Cause::Map(r) => {
                    fold(h, 1);
                    for b in r.bytes() {
                        fold(h, b as u64);
                    }
                }
                Cause::Output => fold(h, 2),
                Cause::Standing => fold(h, 3),
                Cause::Peace => fold(h, 4),
            }
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (t, p) in &self.par {
            fold_tag(&mut h, t);
            fold_i(&mut h, p.board);
            fold_i(&mut h, p.legband);
            fold(&mut h, p.ipc);
            fold(&mut h, p.pop_k);
            fold(&mut h, p.tick);
        }
        for e in &self.eras {
            fold(&mut h, e.year as u64);
            fold(&mut h, e.tick);
            fold(&mut h, e.brink_months as u64);
            if let Some(p) = &e.prize {
                for b in p.bytes() {
                    fold(&mut h, b as u64);
                }
            }
            for (t, c) in &e.cards {
                fold_tag(&mut h, t);
                fold_card(&mut h, c);
            }
        }
        for (t, c) in &self.provisional {
            fold_tag(&mut h, t);
            fold_card(&mut h, c);
        }
        match &self.end {
            None => fold(&mut h, 0),
            Some(CampaignEnd::Reckoning { year, tick }) => {
                fold(&mut h, 1);
                fold(&mut h, *year as u64);
                fold(&mut h, *tick);
            }
            Some(CampaignEnd::Exchange { initiator, tick }) => {
                fold(&mut h, 2);
                fold_tag(&mut h, initiator);
                fold(&mut h, *tick);
            }
        }
        fold(&mut h, self.brink_months as u64);
        for (t, (w, n)) in &self.last_word {
            fold_tag(&mut h, t);
            fold(&mut h, *w as u64 | (*n as u64) << 8);
        }
        fold(&mut h, self.seeded as u64);
        h
    }
}

// --- Seeding --------------------------------------------------------------

/// The 1950 par from static data: standings over the 1950 bands, the
/// authored industry, the 1950 population. Deterministic at tick 0.
fn seed_par(
    ledger: &mut Ledger,
    military: &Military,
    influence: &Influence,
    data: &ScenarioData,
    tick: u64,
) {
    let standings = influence::standings_from(influence, military, data);
    let pop = influence::population_by_holder(data, military);
    for (tag, def) in &data.countries {
        if influence.dormant.contains(tag) {
            continue;
        }
        let col = Column::of(military.alignment_of(data, tag));
        let reach = reach_of(data, tag);
        let p = pop.get(tag).copied().unwrap_or(0).max(1);
        let by_col = [
            board(data, &standings, &reach, Column::West),
            board(data, &standings, &reach, Column::East),
            board(data, &standings, &reach, Column::Denied),
        ];
        ledger.par.insert(
            tag.clone(),
            Par {
                board: board(data, &standings, &reach, col),
                board_by_column: by_col,
                legband: 0,
                ipc: def.industry as u64 * 100 * 1000 / p,
                pop_k: p,
                casualties: 0,
                uses: 0,
                treaties: 0,
                tick,
            },
        );
    }
    ledger.seeded = true;
}

pub fn ensure_seeded(world: &mut World) {
    let Some(scenario) = world.get_resource::<SimScenario>().map(|s| s.0.clone()) else {
        return;
    };
    if world.resource::<Ledger>().seeded || !world.resource::<Influence>().seeded {
        return;
    }
    let tick = world.resource::<SimClock>().tick;
    let military = std::mem::take(&mut *world.resource_mut::<Military>());
    let influence = std::mem::take(&mut *world.resource_mut::<Influence>());
    let mut ledger = std::mem::take(&mut *world.resource_mut::<Ledger>());
    seed_par(&mut ledger, &military, &influence, &scenario, tick);
    *world.resource_mut::<Military>() = military;
    *world.resource_mut::<Influence>() = influence;
    *world.resource_mut::<Ledger>() = ledger;
}

// --- The fold -------------------------------------------------------------

struct Inputs<'a> {
    data: &'a ScenarioData,
    military: &'a Military,
    influence: &'a Influence,
    econ: &'a Economies,
    settlements: &'a Settlements,
    nukes: &'a NuclearPrograms,
    pop: &'a BTreeMap<CountryTag, u64>,
    standings: &'a BTreeMap<String, RegionStanding>,
    tick: u64,
    exchange: bool,
}

/// Compute every nation's card since the previous freeze.
fn compute_cards(
    ledger: &mut Ledger,
    inp: &Inputs,
    brink_months: u16,
) -> BTreeMap<CountryTag, Card> {
    let prev_era = ledger.eras.last().cloned();
    let prev_tick = prev_era.as_ref().map(|e| e.tick).unwrap_or(0);
    // Growth per country first (the comparator needs the whole map).
    let mut growth: BTreeMap<CountryTag, i32> = BTreeMap::new();
    let mut ipcs: BTreeMap<CountryTag, (u64, u64)> = BTreeMap::new();
    for tag in inp.data.countries.keys() {
        if inp.influence.dormant.contains(tag) {
            continue;
        }
        let (ipc, rep) = ipc_of(inp.econ, inp.pop, tag);
        ipcs.insert(tag.clone(), (ipc, rep));
        let prev_ipc = prev_era
            .as_ref()
            .and_then(|e| e.cards.get(tag))
            .filter(|c| c.on_record)
            .map(|c| c.ipc)
            .or_else(|| ledger.par.get(tag).map(|p| p.ipc));
        if let Some(p) = prev_ipc {
            growth.insert(tag.clone(), growth_permille(ipc, p));
        }
    }
    let mut cards: BTreeMap<CountryTag, Card> = BTreeMap::new();
    for tag in inp.data.countries.keys() {
        if inp.influence.dormant.contains(tag) {
            continue;
        }
        let col = Column::of(inp.military.alignment_of(inp.data, tag));
        let reach = reach_of(inp.data, tag);
        let board_now = board(inp.data, inp.standings, &reach, col);
        let (ipc, ipc_reported) = ipcs[tag];
        let legitimacy = inp.settlements.legitimacy_of(tag);
        let casualties_total = inp.military.casualties.get(tag).copied().unwrap_or(0)
            * crate::military::tuning::MEN_PER_STRENGTH_POINT;
        let uses_total = inp.nukes.uses.get(tag).copied().unwrap_or(0);
        let treaties_total = inp
            .settlements
            .treaties
            .iter()
            .filter(|t| t.signatories.contains(tag))
            .count();
        // Previous inputs: the last era's card, else the par (or a par
        // taken now for a state born this era).
        let prev = prev_era
            .as_ref()
            .and_then(|e| e.cards.get(tag))
            .filter(|c| c.on_record);
        let par = ledger.par.get(tag).cloned();
        let col_index = match col {
            Column::West => 0,
            Column::East => 1,
            Column::Denied => 2,
        };
        let (prev_board, prev_legband, prev_cas, prev_uses, prev_treaties, on_record) =
            match (prev, &par) {
                (Some(c), _) => (
                    // The board delta is computed in the column held now,
                    // from the frozen standings of the previous reckoning.
                    board_in_column(inp, ledger.eras.len(), &reach, col, c),
                    legband(c.legitimacy),
                    c.casualties_total,
                    c.uses_total,
                    c.treaties_total,
                    true,
                ),
                (None, Some(p)) => (
                    if p.column_known() {
                        p.board_by_column[col_index]
                    } else {
                        p.board
                    },
                    p.legband,
                    p.casualties,
                    p.uses,
                    p.treaties,
                    true,
                ),
                (None, None) => {
                    // Born this era: take the par now, no record yet.
                    let p = inp.pop.get(tag).copied().unwrap_or(0).max(1);
                    ledger.par.insert(
                        tag.clone(),
                        Par {
                            board: board_now,
                            board_by_column: [
                                board(inp.data, inp.standings, &reach, Column::West),
                                board(inp.data, inp.standings, &reach, Column::East),
                                board(inp.data, inp.standings, &reach, Column::Denied),
                            ],
                            legband: legband(legitimacy),
                            ipc,
                            pop_k: p,
                            casualties: casualties_total,
                            uses: uses_total,
                            treaties: treaties_total,
                            tick: inp.tick,
                        },
                    );
                    (
                        board_now,
                        legband(legitimacy),
                        casualties_total,
                        uses_total,
                        treaties_total,
                        false,
                    )
                }
            };
        let pop1950 = ledger.par.get(tag).map(|p| p.pop_k).unwrap_or(1).max(1);
        let map = if on_record { board_now - prev_board } else { 0 };
        let g = growth.get(tag).copied().unwrap_or(0);
        let comp = comparator(col, &growth);
        let output = if on_record {
            ((g - comp) / tuning::OUTPUT_STEP).clamp(-tuning::OUTPUT_CAP, tuning::OUTPUT_CAP)
        } else {
            0
        };
        let standing = (legband(legitimacy) - prev_legband)
            .clamp(-tuning::STANDING_DELTA_CAP, tuning::STANDING_DELTA_CAP);
        let dead = casualties_total.saturating_sub(prev_cas);
        let uses = uses_total.saturating_sub(prev_uses);
        let treaties = treaties_total.saturating_sub(prev_treaties).min(255) as u8;
        let dead_per_10k = dead * 10 / pop1950; // dead in men, pop in thousands
        let is_pole = tag.0 == "USA" || tag.0 == "SOV";
        let peace = (tuning::PEACE_SETTLED * treaties as i32).min(tuning::PEACE_SETTLED_CAP)
            - ((dead_per_10k / tuning::PEACE_DEAD_UNIT) as i32).min(tuning::PEACE_DEAD_FLOOR)
            - tuning::PEACE_FIRST_USE * uses as i32
            - if is_pole {
                (brink_months as i32).min(tuning::PEACE_BRINK_FLOOR)
            } else {
                0
            };
        let catastrophe = if inp.exchange {
            Catastrophe::Exchange
        } else if uses > 0 || dead_per_10k >= tuning::SCARRED_DEAD {
            Catastrophe::Scarred
        } else {
            Catastrophe::Unscarred
        };
        let mut card = Card {
            map,
            output,
            standing,
            peace,
            catastrophe,
            cause: Cause::None,
            column: Some(col),
            board: board_now,
            ipc,
            ipc_reported,
            growth_permille: g,
            comparator_permille: comp,
            legitimacy,
            dead,
            uses,
            treaties,
            casualties_total,
            uses_total,
            treaties_total,
            on_record,
        };
        card.cause = cause_of(inp, ledger.eras.len(), &reach, col, &card, prev_tick);
        cards.insert(tag.clone(), card);
    }
    cards
}

/// The previous freeze's board recomputed in the column held now, so a
/// defection reads as the board it now faces (scoring.md edge cases).
/// `eras_len` is the number of frozen eras, whose standings live in the
/// influence checkpoints at the same index.
fn board_in_column(
    inp: &Inputs,
    eras_len: usize,
    reach: &[String],
    col: Column,
    prev: &Card,
) -> i32 {
    if prev.column == Some(col) {
        return prev.board;
    }
    match inp.influence.checkpoints.get(eras_len.wrapping_sub(1)) {
        Some(cp) => board(inp.data, &cp.standings, reach, col),
        None => prev.board,
    }
}

impl Par {
    fn column_known(&self) -> bool {
        self.board_by_column != [0, 0, 0] || self.board == 0
    }
}

/// The term with the largest contribution in the card's direction.
fn cause_of(
    inp: &Inputs,
    eras_len: usize,
    reach: &[String],
    col: Column,
    card: &Card,
    _prev_tick: u64,
) -> Cause {
    let sign = card.total().signum();
    let pick = |v: i32| if sign == 0 { v.abs() } else { v * sign };
    let mut best = (pick(card.map), 0u8);
    for (i, v) in [card.scored_output(), card.standing, card.peace]
        .into_iter()
        .enumerate()
    {
        if pick(v) > best.0 {
            best = (pick(v), i as u8 + 1);
        }
    }
    if best.0 <= 0 {
        return Cause::None;
    }
    match best.1 {
        0 => {
            // The region in reach with the largest swing in our column
            // since the previous reckoning, in the card's direction.
            let prev = inp.influence.checkpoints.get(eras_len.wrapping_sub(1));
            let swing = |r: &String| {
                let now = inp
                    .standings
                    .get(r)
                    .map(|_| board(inp.data, inp.standings, std::slice::from_ref(r), col))
                    .unwrap_or(0);
                let then = prev
                    .map(|cp| board(inp.data, &cp.standings, std::slice::from_ref(r), col))
                    .unwrap_or(0);
                pick(now - then)
            };
            let region = reach
                .iter()
                .max_by_key(|r| swing(r))
                .cloned()
                .unwrap_or_default();
            Cause::Map(region)
        }
        1 => Cause::Output,
        2 => Cause::Standing,
        _ => Cause::Peace,
    }
}

fn prize_of(data: &ScenarioData, standings: &BTreeMap<String, RegionStanding>) -> Option<String> {
    standings
        .iter()
        .map(|(r, s)| {
            let w = points(data, r, s, Column::West);
            let e = points(data, r, s, Column::East);
            ((w - e).abs(), r.clone())
        })
        .min()
        .map(|(_, r)| r)
}

#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
pub fn update_score(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    player: Res<PlayerCountry>,
    game_over: Option<Res<GameOver>>,
    influence: Res<Influence>,
    military: Res<Military>,
    econ: Res<Economies>,
    settlements: Res<Settlements>,
    nukes: Res<NuclearPrograms>,
    tension: Res<GlobalTension>,
    mut fired: ResMut<FiredEvents>,
    mut ledger: ResMut<Ledger>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    if !ledger.seeded {
        if !influence.seeded {
            return;
        }
        seed_par(&mut ledger, &military, &influence, data, clock.tick);
    }
    // The exchange ends the record, whenever it comes.
    if let Some(go) = game_over.as_ref() {
        if !matches!(ledger.end, Some(CampaignEnd::Exchange { .. })) {
            ledger.end = Some(CampaignEnd::Exchange {
                initiator: go.initiator.clone(),
                tick: go.tick,
            });
        }
        return;
    }
    if !clock.new_month {
        return;
    }
    if tension.band() == TensionBand::Brink {
        ledger.brink_months = ledger.brink_months.saturating_add(1);
    }
    let pop = influence::population_by_holder(data, &military);
    let standings = influence.standings.clone();
    let inp = Inputs {
        data,
        military: &military,
        influence: &influence,
        econ: &econ,
        settlements: &settlements,
        nukes: &nukes,
        pop: &pop,
        standings: &standings,
        tick: clock.tick,
        exchange: false,
    };
    let brink = ledger.brink_months;
    let cards = compute_cards(&mut ledger, &inp, brink);

    // Standing words with the dead band and the arrow rule.
    for (tag, card) in &cards {
        let scale = scale_of(data, tag) as i32;
        let s = card.total() * scale;
        let word = if s.abs() <= tuning::WORD_DEAD_BAND {
            Word::Holding
        } else if s > 0 {
            Word::Gaining
        } else {
            Word::Slipping
        };
        let entry = ledger
            .last_word
            .entry(tag.clone())
            .or_insert((Word::Holding, 0));
        if entry.0 == word {
            entry.1 = entry.1.saturating_add(1);
        } else {
            *entry = (word, 1);
        }
    }

    // Freeze when the influence checkpoints have moved past the ledger.
    let after_close = matches!(ledger.end, Some(CampaignEnd::Reckoning { .. }));
    if influence.checkpoints.len() > ledger.eras.len() && !after_close {
        let year = influence.checkpoints[ledger.eras.len()].year;
        let prize = prize_of(data, &standings);
        ledger.eras.push(Era {
            year,
            tick: clock.tick,
            cards: cards.clone(),
            brink_months: brink,
            prize: prize.clone(),
        });
        ledger.brink_months = 0;
        ledger.provisional = cards.clone();
        if year >= tuning::FINAL_YEAR {
            ledger.end = Some(CampaignEnd::Reckoning {
                year,
                tick: clock.tick,
            });
        }
        // The moment: a teletype special for the player.
        if let Some(me) = player.0.as_ref() {
            let scale = scale_of(data, me);
            let card = cards.get(me).cloned().unwrap_or_default();
            let (g, sign) = grade(&card, scale);
            let word = format!(
                "{} {}",
                g.label(),
                if g == Grade::Stalemate {
                    ""
                } else if sign > 0 {
                    "GAIN"
                } else {
                    "LOSS"
                }
            );
            let name = |t: &CountryTag| {
                data.nations_meta
                    .get(t)
                    .map(|m| m.display_name.to_uppercase())
                    .unwrap_or_else(|| t.0.clone())
            };
            if year >= tuning::FINAL_YEAR {
                let class = ledger
                    .class_of(me, scale)
                    .map(|c| c.label())
                    .unwrap_or("NO CLASS");
                fired.notices.push((
                    format!("THE FINAL EDITION -- {}: {class}", name(me)),
                    format!(
                        "THE RECORD CLOSES ON 1 JANUARY 1970. THE ERAS SUM TO {:+}: MAP {:+}, OUTPUT {:+}, STANDING {:+}, PEACE {:+}. THE VERDICT IS {class}. THE PAPER (N) CARRIES THE THREE THINGS THAT DECIDED IT AND THE RECORD BESIDE THE BELIEF. PLAY ON IF YOU WISH; NOTHING FURTHER IS SCORED.",
                        ledger.campaign_total(me),
                        ledger.eras.iter().filter_map(|e| e.cards.get(me)).map(|c| c.map).sum::<i32>(),
                        ledger.eras.iter().filter_map(|e| e.cards.get(me)).map(|c| c.scored_output()).sum::<i32>(),
                        ledger.eras.iter().filter_map(|e| e.cards.get(me)).map(|c| c.standing).sum::<i32>(),
                        ledger.eras.iter().filter_map(|e| e.cards.get(me)).map(|c| c.peace).sum::<i32>(),
                    ),
                ));
            } else {
                fired.notices.push((
                    format!("THE {year} RECKONING: {word}"),
                    format!(
                        "STANDINGS FROZEN FOR THE RECORD. THIS ERA: MAP {:+}, OUTPUT {:+}, STANDING {:+}, PEACE {:+}{}. THE PRIZE IS {}. THE ERAS SO FAR SUM TO {:+}. THE PAPER (N) CARRIES THE FULL PAGE.",
                        card.map,
                        card.scored_output(),
                        card.standing,
                        card.peace,
                        if card.catastrophe == Catastrophe::Scarred {
                            " -- SCARRED: THE CLASS IS CAPPED AT COSTLY"
                        } else {
                            ""
                        },
                        prize.as_deref().unwrap_or("NOWHERE").replace('_', " "),
                        ledger.campaign_total(me),
                    ),
                ));
            }
        }
    } else if !after_close {
        ledger.provisional = cards;
    }
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

    fn tag(s: &str) -> CountryTag {
        CountryTag(s.into())
    }

    fn boot(seed: u64) -> App {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed,
        });
        app.insert_resource(SimScenario(Arc::new(data)));
        app
    }

    #[test]
    fn the_par_seeds_before_the_first_command() {
        let mut app = boot(1);
        crate::flush_commands(app.world_mut());
        let ledger = app.world().resource::<Ledger>();
        assert!(ledger.seeded);
        let usa = &ledger.par[&tag("USA")];
        let sov = &ledger.par[&tag("SOV")];
        assert!(usa.board > 0, "the 1950 board is Western: {}", usa.board);
        assert!(sov.board < 0);
        assert!(
            usa.ipc > sov.ipc,
            "industry per head: the level the score never counts"
        );
        assert!(
            !ledger.par.contains_key(&tag("GHA")),
            "dormant states have no par yet"
        );
    }

    #[test]
    fn map_scores_the_delta_and_treaties_credit_peace() {
        let mut app = boot(2);
        run_ticks(&mut app, 24 * 32);
        {
            let ledger = app.world().resource::<Ledger>();
            let usa = &ledger.provisional[&tag("USA")];
            assert_eq!(usa.map, 0, "the map has not moved in a month");
            assert!(usa.on_record);
        }
        // Run through the Korean War's settlement window: a treaty or a
        // frozen conflict; PEACE reflects the dead either way.
        run_ticks(&mut app, 24 * 365 * 4);
        let ledger = app.world().resource::<Ledger>();
        let usa = &ledger.provisional[&tag("USA")];
        let pop = ledger.par[&tag("USA")].pop_k;
        assert!(usa.dead > 0, "Korea cost American lives on the record");
        assert!(usa.peace <= tuning::PEACE_SETTLED_CAP);
        assert_eq!(
            usa.catastrophe,
            Catastrophe::Unscarred,
            "USA dead {} of pop {}k = {} per 10k",
            usa.dead,
            pop,
            usa.dead * 10 / pop
        );
    }

    #[test]
    fn the_first_reckoning_freezes_and_prints() {
        let mut app = boot(3);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetPlayerCountry {
                country: Some(tag("USA")),
            });
        run_ticks(&mut app, 24 * (365 * 5 + 3));
        let world = app.world();
        let ledger = world.resource::<Ledger>();
        assert_eq!(ledger.eras.len(), 1);
        assert_eq!(ledger.eras[0].year, 1955);
        assert!(ledger.eras[0].cards.contains_key(&tag("SOV")));
        let fired = world.resource::<FiredEvents>();
        assert!(
            fired
                .notices
                .iter()
                .any(|(t, _)| t.starts_with("THE 1955 RECKONING")),
            "the freeze is a moment"
        );
        // The identity: the era's MAP is the board delta since the par.
        let usa = &ledger.eras[0].cards[&tag("USA")];
        assert_eq!(usa.map, usa.board - ledger.par[&tag("USA")].board);
        assert!(ledger.end.is_none());
    }

    #[test]
    fn scarring_caps_the_class_and_the_exchange_removes_it() {
        let mut ledger = Ledger::default();
        let mut cards = BTreeMap::new();
        cards.insert(
            tag("USA"),
            Card {
                map: 5,
                output: 3,
                standing: 2,
                peace: -4,
                catastrophe: Catastrophe::Scarred,
                on_record: true,
                ..Default::default()
            },
        );
        ledger.eras.push(Era {
            year: 1955,
            tick: 1,
            cards,
            brink_months: 0,
            prize: None,
        });
        assert_eq!(ledger.class_of(&tag("USA"), 1), Some(Class::Costly));
        assert_eq!(ledger.ledger_class(&tag("USA"), 1), Class::Held);
        ledger.end = Some(CampaignEnd::Exchange {
            initiator: tag("SOV"),
            tick: 2,
        });
        assert_eq!(ledger.class_of(&tag("USA"), 1), None, "no class for anyone");
        assert_eq!(ledger.class_of(&tag("SOV"), 1), None);
    }

    #[test]
    fn grades_and_classes_scale_with_the_nation() {
        let card = Card {
            map: 2,
            ..Default::default()
        };
        assert_eq!(grade(&card, 1), (Grade::Narrow, 1));
        assert_eq!(grade(&card, 3), (Grade::Clear, 1));
        let loss = Card {
            peace: -13,
            ..Default::default()
        };
        assert_eq!(grade(&loss, 1), (Grade::Decisive, -1));
        let flat = Card::default();
        assert_eq!(grade(&flat, 3), (Grade::Stalemate, 0));
    }

    #[test]
    fn the_ledger_is_deterministic() {
        fn run() -> u64 {
            let mut app = boot(1950);
            run_ticks(&mut app, 24 * 400);
            app.world().resource::<Ledger>().digest()
        }
        assert_eq!(run(), run());
    }
}

#[cfg(test)]
mod calibration {
    use super::*;
    use crate::calendar::GameDate;
    use crate::{run_ticks, SimPlugin};
    use bevy_app::App;
    use std::path::Path;
    use std::sync::Arc;

    fn tag(s: &str) -> CountryTag {
        CountryTag(s.into())
    }

    /// The hands-off verdict (scoring.md): bands, never integers.
    #[test]
    fn hands_off_verdict_lands_near_history() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data/scenario/1950");
        let data = ugs_data::ScenarioData::load(&dir).expect("scenario");
        let mut app = App::new();
        app.add_plugins(SimPlugin {
            start_date: GameDate::new(1950, 1, 1, 0),
            seed: 1950,
        });
        app.insert_resource(SimScenario(Arc::new(data)));
        run_ticks(&mut app, 24 * 365 * 20 + 24 * 6);
        let world = app.world();
        let data = world.resource::<SimScenario>().0.clone();
        let ledger = world.resource::<Ledger>();
        assert_eq!(ledger.eras.len(), 4, "four freezes: 1955, 1960, 1965, 1970");
        assert!(matches!(
            ledger.end,
            Some(CampaignEnd::Reckoning { year: 1970, .. })
        ));
        let usa = tag("USA");
        let sov = tag("SOV");
        let mut report = String::new();
        for e in &ledger.eras {
            for t in [&usa, &sov] {
                let c = &e.cards[t];
                let (g, s) = grade(c, 1);
                report.push_str(&format!(
                    "{} {}: MAP {:+} OUT {:+} STAND {:+} PEACE {:+} = {:+} {:?}{:+} {:?} (growth {:+}‰ vs {:+}‰, dead {}, leg {})\n",
                    e.year, t.0, c.map, c.output, c.standing, c.peace, c.total(), g, s, c.catastrophe, c.growth_permille, c.comparator_permille, c.dead, c.legitimacy
                ));
            }
        }
        println!("{report}");
        // 1. Neither pole's 1955 era is CLEAR or DECISIVE.
        for t in [&usa, &sov] {
            let (g, _) = grade(&ledger.eras[0].cards[t], 1);
            assert!(g <= Grade::Narrow, "{} 1955 reads {g:?}\n{report}", t.0);
        }
        // 2. No era DECISIVE; both campaign classes HELD.
        for e in &ledger.eras {
            for t in [&usa, &sov] {
                assert!(
                    grade(&e.cards[t], 1).0 < Grade::Decisive,
                    "{} {} decisive\n{report}",
                    e.year,
                    t.0
                );
            }
        }
        for t in [&usa, &sov] {
            assert_eq!(
                ledger.class_of(t, 1),
                Some(Class::Held),
                "{} class\n{report}",
                t.0
            );
        }
        // 3. The East gains ground on the contested map over the campaign.
        let east_map: i32 = ledger.eras.iter().map(|e| e.cards[&sov].map).sum();
        assert!(
            east_map >= 4,
            "East MAP over the campaign: {east_map}\n{report}"
        );
        // 5. STANDING at 1955 is small for both.
        for t in [&usa, &sov] {
            assert!(
                ledger.eras[0].cards[t].standing.abs() <= 1,
                "{} standing\n{report}",
                t.0
            );
        }
        // 6. Both UNSCARRED in every era.
        for e in &ledger.eras {
            for t in [&usa, &sov] {
                assert_eq!(
                    e.cards[t].catastrophe,
                    Catastrophe::Unscarred,
                    "{} {}\n{report}",
                    e.year,
                    t.0
                );
            }
        }
        // 7. Every nation on the map receives a class; at least one minor
        // reads NARROW or better in some era.
        let mut classed = 0;
        let mut minor_moved = false;
        for t in data.countries.keys() {
            if ledger.eras[3].cards.contains_key(t) {
                classed += 1;
                if ledger.class_of(t, scale_of(&data, t)).is_some() {
                    let scale = scale_of(&data, t);
                    if scale == 3
                        && ledger
                            .eras
                            .iter()
                            .filter_map(|e| e.cards.get(t))
                            .any(|c| grade(c, scale).0 >= Grade::Narrow)
                    {
                        minor_moved = true;
                    }
                }
            }
        }
        assert!(classed >= 86, "{classed} nations classed");
        assert!(
            minor_moved,
            "no minor's ledger moved in twenty years\n{report}"
        );
    }
}
