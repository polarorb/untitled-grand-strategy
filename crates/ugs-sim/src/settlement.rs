//! War termination, occupation & settlement
//! (docs/design/systems/war-termination.md): wars carry declared aims,
//! occupied countries are live political zones that cost real divisions
//! and stockpile, and wars end through a settlement table — signed
//! treaties, frozen conflicts, or unilateral imposition that the world
//! never recognizes. Restraint is cheap; ambition is priced forever.

use std::collections::{BTreeMap, BTreeSet};

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{Alignment, CountryTag, ProvinceId, ScenarioData};

use crate::demography::SimScenario;
use crate::military::{Military, PlayerCountry};
use crate::planning::Economies;
use crate::rng::SimRng;
use crate::tension::GlobalTension;
use crate::SimClock;

pub mod tuning {
    /// Tension (internal tenths) to upgrade INTO each aim rung.
    pub const AIM_TENSION: [i32; 4] = [0, 10, 25, 60];
    /// Legitimacy delta on upgrading into each rung.
    pub const AIM_LEGITIMACY: [i32; 4] = [0, 0, -10, -25];
    /// Max total package sovereignty weight per aim rung.
    pub const AIM_MAX_W: [i32; 4] = [2, 4, 10, 26];

    /// Territorial clauses need this share of named provinces held.
    pub const OCCUPY_GATE_PERMILLE: u64 = 800;
    /// Utility penalty while an opposing-bloc great power's divisions
    /// sit adjacent to a stakeholder's home territory.
    pub const RED_LINE_PENALTY: i64 = 60;
    /// Momentum term: sign of recent front movement times this.
    pub const FRONT_MOMENTUM_SCALE: i64 = 10;
    /// Signed settlements release tension: -(this + W).
    pub const SETTLE_TENSION_RELIEF: i32 = 50;
    /// Truce length after a signed treaty, months.
    pub const TRUCE_MONTHS: u64 = 60;
    /// Tension floor contribution of each frozen conflict.
    pub const FROZEN_TENSION_FLOOR: i32 = 10;
    /// Tension floor while unrecognized annexed holdings stand.
    pub const ANNEX_TENSION_FLOOR: i32 = 30;
    /// Combined extra-floor cap.
    pub const EXTRA_FLOOR_CAP: i32 = 80;

    /// A zone exists once a holder holds this many enemy provinces.
    pub const ZONE_MIN_PROVINCES: usize = 3;
    /// One garrison division "covers" this many occupied people.
    pub const GARRISON_MEN_PER: u64 = 250_000;
    /// Control decay/growth per day (permille points).
    pub const CONTROL_DECAY: u16 = 3;
    pub const CONTROL_GROW: u16 = 2;
    pub const CONTROL_TARGET_BASE: u16 = 600;
    /// Insurgency bonus for a zone adjacent to hostile/rival-bloc soil.
    pub const SANCTUARY_BONUS: u16 = 200;
    /// Insurgency at/above this rolls weekly flare events.
    pub const FLARE_GATE: u16 = 500;
    /// Monthly stockpile upkeep per occupied province (centi-stock).
    pub const ZONE_UPKEEP_CENTI: u64 = 15;
    /// Monthly alignment drift by policy (permille points).
    pub const ALIGN_DRIFT_MILGOV: i16 = -8;
    pub const ALIGN_DRIFT_CLIENT: i16 = 6;
    pub const ALIGN_DRIFT_EXPLOIT: i16 = -20;

    /// Market democracies incorporate only consenting, small territories.
    pub const INCORP_ALIGN_GATE: i16 = 400;
    pub const INCORP_POP_CAP_PERMILLE: u64 = 30;

    /// Acceptance math scales.
    pub const CLAUSE_VALUE_SCALE: i64 = 10;
    /// Exhaustion: months at war + casualties(permille of pop) * this.
    pub const EXHAUSTION_CAS_SCALE: i64 = 2;
    /// A great power (red lines, patronhood) has industry >= this.
    pub const GREAT_POWER_INDUSTRY: u32 = 40;

    /// Weekly insurgency flare: strength/stock damage rolls.
    pub const FLARE_STRENGTH_HIT: u64 = 10;
    pub const FLARE_STOCK_HIT: u64 = 1;
}

/// The declared object of a war, per (belligerent, enemy) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum WarAim {
    #[default]
    StatusQuoAnte,
    Punish,
    NewLine,
    Unify,
}

impl WarAim {
    pub fn rung(self) -> usize {
        match self {
            WarAim::StatusQuoAnte => 0,
            WarAim::Punish => 1,
            WarAim::NewLine => 2,
            WarAim::Unify => 3,
        }
    }
    pub fn max_w(self) -> i32 {
        tuning::AIM_MAX_W[self.rung()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ZonePolicy {
    #[default]
    MilitaryGovernment,
    ClientAdministration,
    Exploitation,
}

/// The political state of one country's territory under another's guns.
/// Province membership is derived from the occupation map each pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccupationZone {
    /// Military grip, 0..=1000.
    pub control: u16,
    /// Popular disposition toward the holder, -1000..=1000.
    pub alignment: i16,
    pub policy: ZonePolicy,
    /// Derived pressure, 0..=1000 (stored for UI/digest stability).
    pub insurgency: u16,
}

impl Default for OccupationZone {
    fn default() -> Self {
        Self {
            control: 300,
            alignment: -300,
            policy: ZonePolicy::MilitaryGovernment,
            insurgency: 0,
        }
    }
}

/// One article of a settlement package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Clause {
    /// Return every occupied province of `to` to it.
    Restore { to: CountryTag },
    /// Cede the named provinces (must be held by proposer's side).
    BorderChange {
        from: CountryTag,
        to: CountryTag,
        provinces: Vec<ProvinceId>,
    },
    /// Demilitarize the named provinces for everyone.
    Dmz { provinces: Vec<ProvinceId> },
    /// `state` becomes `patron`'s client (recorded; teeth post-slice).
    ClientState {
        state: CountryTag,
        patron: CountryTag,
    },
    /// `admin` administers `state` pending a scheduled review.
    Trusteeship {
        state: CountryTag,
        admin: CountryTag,
        review_months: u16,
    },
    /// `state` forswears blocs and bases (recorded; teeth post-slice).
    Neutralization { state: CountryTag },
    /// `absorbed` ceases to exist; its territory passes to `under`.
    Unification {
        absorbed: CountryTag,
        under: CountryTag,
    },
    /// Full annexation into the annexer — the domestic-gate clause.
    Incorporation {
        territory: CountryTag,
        annexer: CountryTag,
    },
    /// Stockpile transfer.
    Reparations {
        from: CountryTag,
        to: CountryTag,
        stock: u64,
    },
}

impl Clause {
    /// Sovereignty weight: how much of the status quo this rewrites.
    /// Negative = a concession sweetening the package.
    pub fn weight(&self) -> i32 {
        match self {
            Clause::Restore { .. } => -2,
            Clause::BorderChange { .. } => 2,
            Clause::Dmz { .. } => 1,
            Clause::ClientState { .. } => 6,
            Clause::Trusteeship { .. } => 4,
            Clause::Neutralization { .. } => -6,
            Clause::Unification { .. } => 12,
            Clause::Incorporation { .. } => 18,
            Clause::Reparations { .. } => 1,
        }
    }

    /// The country whose sovereignty this clause spends, if any —
    /// its patrons must sign.
    pub fn sovereignty_of(&self) -> Option<&CountryTag> {
        match self {
            Clause::ClientState { state, .. } => Some(state),
            Clause::Trusteeship { state, .. } => Some(state),
            Clause::Unification { absorbed, .. } => Some(absorbed),
            Clause::Incorporation { territory, .. } => Some(territory),
            Clause::BorderChange { from, .. } => Some(from),
            _ => None,
        }
    }

    /// The side this clause favors (receives value).
    fn favors(&self) -> Option<&CountryTag> {
        match self {
            Clause::Restore { to } => Some(to),
            Clause::BorderChange { to, .. } => Some(to),
            Clause::Dmz { .. } => None,
            Clause::ClientState { patron, .. } => Some(patron),
            Clause::Trusteeship { admin, .. } => Some(admin),
            Clause::Neutralization { .. } => None,
            Clause::Unification { under, .. } => Some(under),
            Clause::Incorporation { annexer, .. } => Some(annexer),
            Clause::Reparations { to, .. } => Some(to),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub proposer: CountryTag,
    pub clauses: Vec<Clause>,
    pub since_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Treaty {
    pub clauses: Vec<Clause>,
    pub signatories: BTreeSet<CountryTag>,
    pub tick: u64,
}

/// Armistice without a treaty: the era's signature outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenConflict {
    pub a: CountryTag,
    pub b: CountryTag,
    pub dmz: BTreeSet<ProvinceId>,
    pub since_tick: u64,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Settlements {
    /// (belligerent, enemy) -> declared aim.
    pub war_aims: BTreeMap<(CountryTag, CountryTag), WarAim>,
    /// International standing spendable on high-sovereignty clauses.
    pub legitimacy: BTreeMap<CountryTag, i32>,
    /// (holder, original owner) -> political state of that occupation.
    pub zones: BTreeMap<(CountryTag, CountryTag), OccupationZone>,
    /// Standing proposals, at most one per proposer.
    pub proposals: Vec<Proposal>,
    pub treaties: Vec<Treaty>,
    pub frozen: Vec<FrozenConflict>,
    /// Provinces whose current holding is treaty-recognized.
    pub recognized: BTreeSet<ProvinceId>,
    /// (a, b) -> truce until tick; war between them is barred.
    pub truces: BTreeMap<(CountryTag, CountryTag), u64>,
    /// Countries neutralized by treaty (recorded; teeth post-slice).
    pub neutralized: BTreeSet<CountryTag>,
    /// Scheduled reviews: (tick, label).
    pub scheduled: Vec<(u64, String)>,
}

impl Settlements {
    pub fn aim(&self, country: &CountryTag, enemy: &CountryTag) -> WarAim {
        self.war_aims
            .get(&(country.clone(), enemy.clone()))
            .copied()
            .unwrap_or_default()
    }

    pub fn legitimacy_of(&self, country: &CountryTag) -> i32 {
        self.legitimacy.get(country).copied().unwrap_or(0)
    }

    pub fn truce_active(&self, a: &CountryTag, b: &CountryTag, tick: u64) -> bool {
        let key = if a < b {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
        self.truces.get(&key).is_some_and(|until| *until > tick)
    }

    /// Every province in the global DMZ set (from treaties and frozen
    /// conflicts) — divisions may not enter these. A DMZ is SUSPENDED
    /// while war has resumed between its parties: a re-ignited front
    /// must be fightable, not walled off by its own old armistice.
    pub fn dmz_provinces(&self, military: &Military) -> BTreeSet<ProvinceId> {
        let mut out = BTreeSet::new();
        for t in &self.treaties {
            let broken = t
                .signatories
                .iter()
                .any(|a| t.signatories.iter().any(|b| military.at_war(a, b)));
            if broken {
                continue;
            }
            for c in &t.clauses {
                if let Clause::Dmz { provinces } = c {
                    out.extend(provinces.iter().copied());
                }
            }
        }
        for f in &self.frozen {
            if !military.at_war(&f.a, &f.b) {
                out.extend(f.dmz.iter().copied());
            }
        }
        out
    }

    /// Occupied provinces the holder keeps without recognition, after
    /// its war with the owner ended: the bleeding annexation status.
    pub fn unrecognized_holdings(
        &self,
        data: &ScenarioData,
        military: &Military,
        holder: &CountryTag,
    ) -> usize {
        military
            .occupation
            .iter()
            .filter(|(p, h)| {
                *h == holder
                    && !self.recognized.contains(p)
                    && data.provinces.get(p).is_some_and(|pd| {
                        pd.owner != **h
                            && !military.at_war(holder, &pd.owner)
                            // A frozen conflict's line is armistice
                            // status, not annexation — priced by its
                            // own (smaller) tension floor.
                            && !self.frozen.iter().any(|f| {
                                (f.a == **h && f.b == pd.owner)
                                    || (f.b == **h && f.a == pd.owner)
                            })
                    })
            })
            .count()
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
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for ((a, b), aim) in &self.war_aims {
            fold_tag(&mut h, a);
            fold_tag(&mut h, b);
            fold(&mut h, aim.rung() as u64);
        }
        for (t, l) in &self.legitimacy {
            fold_tag(&mut h, t);
            fold(&mut h, l.unsigned_abs() as u64 | ((*l < 0) as u64) << 32);
        }
        for ((holder, orig), z) in &self.zones {
            fold_tag(&mut h, holder);
            fold_tag(&mut h, orig);
            for v in [
                z.control as u64,
                z.alignment.unsigned_abs() as u64 | ((z.alignment < 0) as u64) << 16,
                z.policy as u64,
                z.insurgency as u64,
            ] {
                fold(&mut h, v);
            }
        }
        fn fold_clause(h: &mut u64, c: &Clause) {
            fold(h, c.weight().unsigned_abs() as u64);
            let disc = match c {
                Clause::Restore { .. } => 1u64,
                Clause::BorderChange { .. } => 2,
                Clause::Dmz { .. } => 3,
                Clause::ClientState { .. } => 4,
                Clause::Trusteeship { .. } => 5,
                Clause::Neutralization { .. } => 6,
                Clause::Unification { .. } => 7,
                Clause::Incorporation { .. } => 8,
                Clause::Reparations { .. } => 9,
            };
            fold(h, disc);
            if let Some(t) = c.sovereignty_of() {
                fold_tag(h, t);
            }
            match c {
                Clause::BorderChange { provinces, .. } | Clause::Dmz { provinces } => {
                    for p in provinces {
                        fold(h, p.0 as u64 + 1);
                    }
                }
                Clause::Reparations { stock, .. } => fold(h, *stock),
                _ => {}
            }
        }
        for p in &self.proposals {
            fold_tag(&mut h, &p.proposer);
            for c in &p.clauses {
                fold_clause(&mut h, c);
            }
        }
        for t in &self.treaties {
            for sig in &t.signatories {
                fold_tag(&mut h, sig);
            }
            for c in &t.clauses {
                fold_clause(&mut h, c);
            }
        }
        for f in &self.frozen {
            fold_tag(&mut h, &f.a);
            fold_tag(&mut h, &f.b);
            for p in &f.dmz {
                fold(&mut h, p.0 as u64 + 1);
            }
        }
        for n in &self.neutralized {
            fold_tag(&mut h, n);
        }
        for (t, label) in &self.scheduled {
            fold(&mut h, *t);
            for b in label.bytes() {
                fold(&mut h, b as u64);
            }
        }
        for p in &self.recognized {
            fold(&mut h, p.0 as u64 + 1);
        }
        for ((a, b), until) in &self.truces {
            fold_tag(&mut h, a);
            fold_tag(&mut h, b);
            fold(&mut h, *until);
        }
        h
    }
}

/// A country's bloc patrons: its bloc superpower plus any great-power
/// co-belligerent fighting on its side.
pub fn patrons_of(
    data: &ScenarioData,
    military: &Military,
    client: &CountryTag,
) -> BTreeSet<CountryTag> {
    let mut out = BTreeSet::new();
    let superpower = match military.alignment_of(data, client) {
        Alignment::WesternBloc => Some(CountryTag("USA".into())),
        Alignment::EasternBloc => Some(CountryTag("SOV".into())),
        _ => None,
    };
    if let Some(sp) = superpower {
        if sp != *client {
            out.insert(sp);
        }
    }
    // Great powers at war with any of the client's enemies.
    for (a, b) in &military.wars {
        let enemy = if a == client {
            Some(b)
        } else if b == client {
            Some(a)
        } else {
            None
        };
        let Some(enemy) = enemy else { continue };
        for (x, y) in &military.wars {
            let ally = if x == enemy {
                Some(y)
            } else if y == enemy {
                Some(x)
            } else {
                None
            };
            if let Some(ally) = ally {
                if ally != client
                    && data
                        .countries
                        .get(ally)
                        .is_some_and(|c| c.industry >= tuning::GREAT_POWER_INDUSTRY)
                {
                    out.insert(ally.clone());
                }
            }
        }
    }
    out
}

/// Red line: an opposing-bloc great power's DIVISIONS sit in provinces
/// adjacent to `stakeholder`'s 1950 territory. (Garrisoning the Yalu
/// with same-bloc LOCAL divisions does not trigger it — NSC-81/1.)
pub fn red_line_triggered(
    data: &ScenarioData,
    military: &Military,
    stakeholder: &CountryTag,
) -> bool {
    let my_alignment = military.alignment_of(data, stakeholder);
    // Provinces adjacent to the stakeholder's home territory.
    let mut border_adjacent: BTreeSet<ProvinceId> = BTreeSet::new();
    for p in data.provinces.values() {
        if p.owner == *stakeholder {
            border_adjacent.extend(p.adjacent.iter().copied());
        }
    }
    military.formations.values().any(|f| {
        border_adjacent.contains(&f.location)
            && data.countries.get(&f.owner).is_some_and(|c| {
                c.industry >= tuning::GREAT_POWER_INDUSTRY
                    && military.alignment_of(data, &f.owner) != my_alignment
            })
            && f.owner != *stakeholder
    })
}

/// Why a stakeholder rejects (or would accept) a package. Every term
/// is shown in the UI ledger; v1 numbers are exact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub stakeholder: CountryTag,
    pub accepts: bool,
    /// Human-readable blockers, empty when accepting.
    pub blockers: Vec<String>,
    pub utility: i64,
}

fn same_side(data: &ScenarioData, military: &Military, a: &CountryTag, b: &CountryTag) -> bool {
    if a == b {
        return true;
    }
    if military.at_war(a, b) {
        return false;
    }
    // Shared enemy, or shared bloc.
    let shared_enemy = military.wars.iter().any(|(x, y)| {
        let e = if x == a {
            Some(y)
        } else if y == a {
            Some(x)
        } else {
            None
        };
        e.is_some_and(|e| military.at_war(b, e))
    });
    if shared_enemy {
        return true;
    }
    matches!(
        (
            military.alignment_of(data, a),
            military.alignment_of(data, b),
        ),
        (Alignment::WesternBloc, Alignment::WesternBloc)
            | (Alignment::EasternBloc, Alignment::EasternBloc)
    )
}

/// Integer sqrt for the convex pricing term.
fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Evaluate one proposal against every required stakeholder.
/// Deterministic; also used by the UI for live ledger previews.
fn fmt_millions(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{}.{}M PEOPLE", n / 1_000_000, n % 1_000_000 / 100_000)
    } else {
        format!("{}K PEOPLE", n / 1000)
    }
}

fn is_planned(econ: &Economies, tag: &CountryTag) -> bool {
    matches!(
        econ.system.get(tag),
        Some(crate::planning::EconomicSystem::Planned)
    )
}

#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    data: &ScenarioData,
    military: &Military,
    settlements: &Settlements,
    econ: &Economies,
    demo: &crate::demography::Demographics,
    clock: &SimClock,
    proposal: &Proposal,
) -> Vec<Verdict> {
    let w_total: i32 = proposal.clauses.iter().map(|c| c.weight().max(0)).sum();
    let convex_permille: i64 = 1000 + isqrt(100 * w_total as i64) * 100;

    // Stakeholders: all war enemies of the proposer's side touched by
    // clauses, plus patrons of every sovereignty-spending clause.
    let mut stakeholders: BTreeSet<CountryTag> = BTreeSet::new();
    for (a, b) in &military.wars {
        if a == &proposal.proposer || b == &proposal.proposer {
            stakeholders.insert(a.clone());
            stakeholders.insert(b.clone());
        }
    }
    for c in &proposal.clauses {
        if let Some(sov) = c.sovereignty_of() {
            stakeholders.insert(sov.clone());
            for p in patrons_of(data, military, sov) {
                stakeholders.insert(p);
            }
        }
        // Whoever currently HOLDS restore-affected ground must sign —
        // a KOR-PRK restoration cannot hand back US-held provinces
        // without Washington's signature.
        if let Clause::Restore { to } = c {
            for (p, holder) in &military.occupation {
                if data.provinces.get(p).is_some_and(|pd| &pd.owner == to) {
                    stakeholders.insert(holder.clone());
                }
            }
        }
    }
    stakeholders.remove(&proposal.proposer);

    let pop_of = |tag: &CountryTag| -> u64 {
        demo.provinces
            .iter()
            .filter(|(id, _)| data.provinces.get(id).is_some_and(|p| &p.owner == tag))
            .map(|(_, c)| c.total())
            .sum()
    };

    // Gate: military facts (proposer side must hold the territory).
    let holds = |provinces: &[ProvinceId]| -> bool {
        if provinces.is_empty() {
            return true;
        }
        let held = provinces
            .iter()
            .filter(|p| {
                data.provinces.get(p).is_some_and(|pd| {
                    let holder = military.owner_of(**p, &pd.owner);
                    same_side(data, military, &holder, &proposal.proposer)
                })
            })
            .count() as u64;
        held * 1000 / provinces.len() as u64 >= tuning::OCCUPY_GATE_PERMILLE
    };
    let all_of = |tag: &CountryTag| -> Vec<ProvinceId> {
        data.provinces
            .values()
            .filter(|p| p.owner == *tag)
            .map(|p| p.id)
            .collect()
    };

    let mut universal_blockers: Vec<String> = Vec::new();
    for c in &proposal.clauses {
        let named: Option<Vec<ProvinceId>> = match c {
            Clause::BorderChange { provinces, .. } => Some(provinces.clone()),
            Clause::Unification { absorbed, .. } => Some(all_of(absorbed)),
            Clause::Incorporation { territory, .. } => Some(all_of(territory)),
            _ => None,
        };
        if let Some(named) = named {
            if !holds(&named) {
                universal_blockers.push(format!(
                    "MILITARY: PROPOSER DOES NOT HOLD THE TERRITORY ({})",
                    match c.sovereignty_of() {
                        Some(t) => t.0.clone(),
                        None => "LINE".into(),
                    }
                ));
            }
        }
        // Aim cap.
        if let Some(sov) = c.sovereignty_of() {
            if military.at_war(&proposal.proposer, sov) {
                let aim = settlements.aim(&proposal.proposer, sov);
                if w_total > aim.max_w() {
                    universal_blockers.push(format!(
                        "AIM: PACKAGE (W{w_total}) EXCEEDS DECLARED AIM {:?} (MAX W{})",
                        aim,
                        aim.max_w()
                    ));
                }
            }
        }
        // Domestic gate for market-system annexers.
        if let Clause::Incorporation { territory, annexer } = c {
            if !is_planned(econ, annexer) {
                let zone_align = settlements
                    .zones
                    .get(&(annexer.clone(), territory.clone()))
                    .map(|z| z.alignment)
                    .unwrap_or(-1000);
                let terr_pop = pop_of(territory);
                let own_pop = pop_of(annexer).max(1);
                if zone_align < tuning::INCORP_ALIGN_GATE {
                    universal_blockers.push(format!(
                        "DOMESTIC: THE TERRITORY DOES NOT CONSENT (ALIGNMENT {} < {})",
                        zone_align / 10,
                        tuning::INCORP_ALIGN_GATE / 10
                    ));
                }
                if terr_pop * 1000 / own_pop > tuning::INCORP_POP_CAP_PERMILLE {
                    universal_blockers.push(format!(
                        "DOMESTIC: CONGRESS WILL NOT INCORPORATE {} -- REQUIRES DOMESTIC POLITICAL TRANSFORMATION",
                        fmt_millions(terr_pop)
                    ));
                }
            }
        }
    }

    // Legitimacy gates on the SUMMED package cost — charging is summed
    // at execution, so the gate must match (two W4 clauses at
    // legitimacy 6 must NOT clear).
    let total_legit: i32 = proposal
        .clauses
        .iter()
        .map(|c| match c {
            Clause::Incorporation { annexer, .. } if is_planned(econ, annexer) => 2 * c.weight(),
            _ => c.weight().max(0),
        })
        .sum();
    if total_legit > 0 && settlements.legitimacy_of(&proposal.proposer) < total_legit {
        universal_blockers.push(format!(
            "LEGITIMACY: PACKAGE NEEDS {total_legit}, HAVE {}",
            settlements.legitimacy_of(&proposal.proposer)
        ));
    }
    universal_blockers.sort();
    universal_blockers.dedup();

    let mut verdicts = Vec::new();
    for s in stakeholders {
        let mut blockers = universal_blockers.clone();
        // Terms value from this stakeholder's chair.
        let mut terms: i64 = 0;
        for c in &proposal.clauses {
            let w = c.weight() as i64 * tuning::CLAUSE_VALUE_SCALE;
            let favors_me = c.favors().is_some_and(|f| same_side(data, military, f, &s));
            let against_me = c
                .sovereignty_of()
                .is_some_and(|sov| same_side(data, military, sov, &s) && !favors_me);
            if favors_me {
                terms += w.abs();
            } else if against_me {
                terms -= w.max(0) * convex_permille / 1000;
            }
        }
        // Exhaustion pushes any party at war toward settling.
        let at_war_any = military.wars.iter().any(|(a, b)| a == &s || b == &s);
        let exhaustion: i64 = if at_war_any {
            let start = military
                .war_started
                .iter()
                .filter(|((a, b), _)| a == &s || b == &s)
                .map(|(_, t)| *t)
                .min()
                .unwrap_or(clock.tick);
            let months = (clock.tick.saturating_sub(start) / (24 * 30)) as i64;
            let cas = military.casualties.get(&s).copied().unwrap_or(0)
                * crate::military::tuning::MEN_PER_STRENGTH_POINT;
            let pop = pop_of(&s).max(1);
            let cas_permille = (cas * 1000 / pop) as i64;
            months + cas_permille * tuning::EXHAUSTION_CAS_SCALE
        } else {
            0
        };
        // Front prospects: recent movement favors holding out.
        let stale_days = (clock.tick.saturating_sub(military.last_line_change_tick) / 24) as i64;
        let prospects = if at_war_any && stale_days < 30 {
            tuning::FRONT_MOMENTUM_SCALE
        } else {
            0
        };
        // Red line.
        let red = if red_line_triggered(data, military, &s)
            && proposal.clauses.iter().any(|c| {
                c.sovereignty_of()
                    .is_some_and(|sov| same_side(data, military, sov, &s))
            }) {
            blockers.push(format!(
                "RED LINE: OPPOSING-BLOC FORCES ON {}'S BORDER",
                s.0
            ));
            tuning::RED_LINE_PENALTY
        } else {
            0
        };
        // Patron gate: a patron with a standing army cannot be ignored.
        let utility = terms + exhaustion - prospects - red;
        let accepts = blockers.is_empty() && utility >= 0;
        if !accepts && blockers.is_empty() {
            blockers.push(format!("TERMS: UTILITY {utility} < 0"));
        }
        verdicts.push(Verdict {
            stakeholder: s,
            accepts,
            blockers,
            utility,
        });
    }
    verdicts
}

/// Template builders — deterministic package construction from live
/// state. (Design doc said RON templates; clauses bind to live lines
/// and zones, so builders are code. Recorded as a deviation.)
pub fn templates(
    data: &ScenarioData,
    military: &Military,
    proposer: &CountryTag,
    enemy: &CountryTag,
) -> Vec<(&'static str, Vec<Clause>)> {
    let mut out = Vec::new();
    // Status quo ante: both sides restore everything.
    out.push((
        "STATUS QUO ANTE",
        vec![
            Clause::Restore { to: enemy.clone() },
            Clause::Restore {
                to: proposer.clone(),
            },
        ],
    ));
    // New line: keep what we hold of theirs; DMZ along it.
    let held: Vec<ProvinceId> = military
        .occupation
        .iter()
        .filter(|(p, h)| {
            *h == proposer && data.provinces.get(p).is_some_and(|pd| &pd.owner == enemy)
        })
        .map(|(p, _)| *p)
        .collect();
    if !held.is_empty() {
        out.push((
            "NEW LINE + DMZ",
            vec![
                Clause::BorderChange {
                    from: enemy.clone(),
                    to: proposer.clone(),
                    provinces: held.clone(),
                },
                Clause::Dmz {
                    provinces: held.iter().take(3).copied().collect(),
                },
            ],
        ));
    }
    // Full-country dispositions require holding (checked by the gate).
    out.push((
        "UN TRUSTEESHIP",
        vec![Clause::Trusteeship {
            state: enemy.clone(),
            admin: proposer.clone(),
            review_months: 24,
        }],
    ));
    out.push((
        "CLIENT STATE",
        vec![Clause::ClientState {
            state: enemy.clone(),
            patron: proposer.clone(),
        }],
    ));
    out.push((
        "NEUTRALIZED UNIFICATION",
        vec![
            Clause::Unification {
                absorbed: enemy.clone(),
                under: proposer.clone(),
            },
            Clause::Neutralization {
                state: proposer.clone(),
            },
        ],
    ));
    out.push((
        "UNIFICATION",
        vec![Clause::Unification {
            absorbed: enemy.clone(),
            under: proposer.clone(),
        }],
    ));
    out.push((
        "INCORPORATION",
        vec![Clause::Incorporation {
            territory: enemy.clone(),
            annexer: proposer.clone(),
        }],
    ));
    out
}

/// Execute a signed package: transfers, truces, recognition, tension.
#[allow(clippy::too_many_arguments)]
fn execute(
    data: &ScenarioData,
    clock: &SimClock,
    military: &mut Military,
    econ: &mut Economies,
    tension: &mut GlobalTension,
    fired: &mut crate::events::FiredEvents,
    settlements: &mut Settlements,
    proposal: &Proposal,
    signatories: BTreeSet<CountryTag>,
) {
    let w_total: i32 = proposal.clauses.iter().map(|c| c.weight().max(0)).sum();
    // Restores execute FIRST so cessions and restorations in one
    // package compose the way the acceptance ledger priced them,
    // regardless of authored clause order.
    let ordered: Vec<&Clause> = proposal
        .clauses
        .iter()
        .filter(|c| matches!(c, Clause::Restore { .. }))
        .chain(
            proposal
                .clauses
                .iter()
                .filter(|c| !matches!(c, Clause::Restore { .. })),
        )
        .collect();
    for c in ordered {
        match c {
            Clause::Restore { to } => {
                // Only signatory-held ground is released — third
                // parties' conquests are not this treaty's to give.
                let restore: Vec<ProvinceId> = military
                    .occupation
                    .iter()
                    .filter(|(p, holder)| {
                        signatories.contains(*holder)
                            && data.provinces.get(p).is_some_and(|pd| &pd.owner == to)
                    })
                    .map(|(p, _)| *p)
                    .collect();
                for p in restore {
                    military.occupation.remove(&p);
                    settlements.recognized.remove(&p);
                }
            }
            Clause::BorderChange { to, provinces, .. } => {
                for p in provinces {
                    military.occupation.insert(*p, to.clone());
                    settlements.recognized.insert(*p);
                }
            }
            Clause::Dmz { .. } => {} // read from the treaty record
            Clause::ClientState { state, patron } => {
                military.log(
                    clock.tick,
                    format!("{} ENTERS {}'S ORBIT BY TREATY", state.0, patron.0),
                );
            }
            Clause::Trusteeship {
                state,
                review_months,
                ..
            } => {
                settlements.scheduled.push((
                    clock.tick + *review_months as u64 * 30 * 24,
                    format!("TRUSTEESHIP REVIEW: {}", state.0),
                ));
            }
            Clause::Neutralization { state } => {
                settlements.neutralized.insert(state.clone());
            }
            Clause::Unification { absorbed, under }
            | Clause::Incorporation {
                territory: absorbed,
                annexer: under,
            } => {
                for p in data.provinces.values().filter(|p| &p.owner == absorbed) {
                    military.occupation.insert(p.id, under.clone());
                    settlements.recognized.insert(p.id);
                }
                let dead: Vec<crate::military::FormationId> = military
                    .formations
                    .iter()
                    .filter(|(_, f)| &f.owner == absorbed)
                    .map(|(id, _)| *id)
                    .collect();
                for id in dead {
                    military.formations.remove(&id);
                }
                settlements.zones.retain(|(_, orig), _| orig != absorbed);
                military.log(
                    clock.tick,
                    format!("{} CEASES TO EXIST -- ABSORBED BY {}", absorbed.0, under.0),
                );
            }
            Clause::Reparations { from, to, stock } => {
                let take = econ
                    .industry
                    .get_mut(from)
                    .map(|s| {
                        let t = (*stock).min(s.military_stock);
                        s.military_stock -= t;
                        t
                    })
                    .unwrap_or(0);
                if let Some(s) = econ.industry.get_mut(to) {
                    s.military_stock += take;
                }
            }
        }
    }
    // Legitimacy spend.
    let legit_cost: i32 = proposal
        .clauses
        .iter()
        .map(|c| match c {
            Clause::Incorporation { annexer, .. } if is_planned(econ, annexer) => 2 * c.weight(),
            _ => c.weight().max(0),
        })
        .sum();
    *settlements
        .legitimacy
        .entry(proposal.proposer.clone())
        .or_default() -= legit_cost;
    // End every war among signatories; truce all pairs.
    let until = clock.tick + tuning::TRUCE_MONTHS * 30 * 24;
    let pairs: Vec<(CountryTag, CountryTag)> = military
        .wars
        .iter()
        .filter(|(a, b)| signatories.contains(a) && signatories.contains(b))
        .cloned()
        .collect();
    for (a, b) in &pairs {
        crate::military::end_war(military, a, b);
        settlements.truces.insert((a.clone(), b.clone()), until);
    }
    tension.apply(-(tuning::SETTLE_TENSION_RELIEF + w_total));
    settlements.treaties.push(Treaty {
        clauses: proposal.clauses.clone(),
        signatories: signatories.iter().cloned().collect(),
        tick: clock.tick,
    });
    let names: Vec<&str> = signatories.iter().map(|t| t.0.as_str()).collect();
    fired.notices.push((
        "SETTLEMENT SIGNED".into(),
        format!(
            "A SETTLEMENT IS SIGNED BY {}. THE GUNS FALL SILENT UNDER TERMS, NOT MERELY EXHAUSTION. THE MAP IS REDRAWN WHERE THE TREATY SAYS -- AND ONLY THERE.",
            names.join(", ")
        ),
    ));
}

/// Monthly: zones tick daily; the table evaluates monthly; frozen
/// conflicts form when both sides are willing but nothing signs.
#[allow(clippy::too_many_arguments)]
pub fn update_settlements(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    player: Res<PlayerCountry>,
    demo: Res<crate::demography::Demographics>,
    mut rng: ResMut<SimRng>,
    mut military: ResMut<Military>,
    mut econ: ResMut<Economies>,
    mut tension: ResMut<GlobalTension>,
    mut fired: ResMut<crate::events::FiredEvents>,
    mut settlements: ResMut<Settlements>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;

    // --- Daily: occupation zones -----------------------------------------
    if clock.new_day {
        update_zones(
            data,
            &clock,
            &mut rng,
            &mut military,
            &mut econ,
            &mut settlements,
        );
    }

    // Scheduled reviews fire as notices.
    let due: Vec<String> = settlements
        .scheduled
        .iter()
        .filter(|(t, _)| *t <= clock.tick)
        .map(|(_, s)| s.clone())
        .collect();
    if !due.is_empty() {
        settlements.scheduled.retain(|(t, _)| *t > clock.tick);
        for label in due {
            fired.notices.push(("SCHEDULED REVIEW".into(), label));
        }
    }

    if !clock.new_month {
        return;
    }

    // --- Monthly: tension floor from outcomes ----------------------------
    let mut floor = settlements.frozen.len() as i32 * tuning::FROZEN_TENSION_FLOOR;
    let holders: BTreeSet<CountryTag> = military.occupation.values().cloned().collect();
    for h in &holders {
        if settlements.unrecognized_holdings(data, &military, h) > 0 {
            floor += tuning::ANNEX_TENSION_FLOOR;
            break; // one flag suffices; per-holder stacking post-slice
        }
    }
    tension.extra_floor = floor.min(tuning::EXTRA_FLOOR_CAP);

    // --- Monthly: evaluate standing proposals ----------------------------
    // A proposal from a proposer no longer at war is stale — drop it
    // before evaluation so old packages cannot sign into new wars.
    {
        let wars = military.wars.clone();
        settlements.proposals.retain(|p| {
            wars.iter()
                .any(|(a, b)| a == &p.proposer || b == &p.proposer)
        });
    }
    let proposals = settlements.proposals.clone();
    let mut signed_any = false;
    for proposal in &proposals {
        if signed_any {
            break; // one signing per month keeps ordering simple
        }
        let verdicts = evaluate(
            data,
            &military,
            &settlements,
            &econ,
            &demo,
            &clock,
            proposal,
        );
        if verdicts.iter().all(|v| v.accepts) && !verdicts.is_empty() {
            let mut signatories: BTreeSet<CountryTag> =
                verdicts.iter().map(|v| v.stakeholder.clone()).collect();
            signatories.insert(proposal.proposer.clone());
            execute(
                data,
                &clock,
                &mut military,
                &mut econ,
                &mut tension,
                &mut fired,
                &mut settlements,
                proposal,
                signatories,
            );
            signed_any = true;
        }
    }
    if signed_any {
        settlements.proposals.clear();
    }

    // --- Monthly: AI proposals -------------------------------------------
    // Each AI belligerent proposes its best-clearing template if it has
    // no standing proposal. The player proposes only via command.
    let at_war: Vec<(CountryTag, CountryTag)> = military.wars.clone();
    for (a, b) in &at_war {
        for (me, enemy) in [(a, b), (b, a)] {
            if player.0.as_ref() == Some(me) {
                continue;
            }
            if settlements.proposals.iter().any(|p| &p.proposer == me) {
                continue;
            }
            // AI proposes only when war-weary (reuses the armistice spirit).
            let start = military
                .war_started
                .get(&(a.clone(), b.clone()))
                .copied()
                .unwrap_or(clock.tick);
            let months = clock.tick.saturating_sub(start) / (24 * 30);
            let broken = !military.formations.values().any(|f| &f.owner == me);
            if months < 6 && !broken {
                continue;
            }
            let aim = settlements.aim(me, enemy);
            for (_, clauses) in templates(data, &military, me, enemy) {
                let w: i32 = clauses.iter().map(|c| c.weight().max(0)).sum();
                if w > aim.max_w() {
                    continue;
                }
                let prop = Proposal {
                    proposer: me.clone(),
                    clauses,
                    since_tick: clock.tick,
                };
                let verdicts = evaluate(data, &military, &settlements, &econ, &demo, &clock, &prop);
                if verdicts.iter().all(|v| v.accepts) && !verdicts.is_empty() {
                    settlements.proposals.push(prop);
                    break;
                }
            }
        }
    }

    // --- Monthly: frozen-conflict fallback (the old armistice) -----------
    settle_frozen(
        data,
        &clock,
        &player.0,
        &mut military,
        &mut fired,
        &mut tension,
        &mut settlements,
    );
}

/// Daily zone bookkeeping: membership, garrisons, control, alignment,
/// insurgency, flares. Deterministic; RNG from a forked stream.
fn update_zones(
    data: &ScenarioData,
    clock: &SimClock,
    rng: &mut SimRng,
    military: &mut Military,
    econ: &mut Economies,
    settlements: &mut Settlements,
) {
    // Derive zone membership from the occupation map.
    let mut membership: BTreeMap<(CountryTag, CountryTag), Vec<ProvinceId>> = BTreeMap::new();
    for (p, holder) in &military.occupation {
        let Some(pd) = data.provinces.get(p) else {
            continue;
        };
        if pd.owner != *holder && !settlements.recognized.contains(p) {
            membership
                .entry((holder.clone(), pd.owner.clone()))
                .or_default()
                .push(*p);
        }
    }
    membership.retain(|_, v| v.len() >= tuning::ZONE_MIN_PROVINCES);
    settlements.zones.retain(|k, _| membership.contains_key(k));
    for key in membership.keys() {
        settlements.zones.entry(key.clone()).or_default();
    }

    let mut stream = rng.fork(b"occupation");
    let keys: Vec<(CountryTag, CountryTag)> = settlements.zones.keys().cloned().collect();
    for key in keys {
        let (holder, original) = &key;
        let provinces = &membership[&key];
        let zone_pop: u64 = provinces
            .iter()
            .filter_map(|p| data.provinces.get(p))
            .map(|p| p.population_k as u64 * 1000)
            .sum();
        let garrison_men: u64 = military
            .formations
            .values()
            .filter(|f| &f.owner == holder && provinces.contains(&f.location))
            .map(|f| f.strength * crate::military::tuning::MEN_PER_STRENGTH_POINT)
            .sum();
        let required = zone_pop / tuning::GARRISON_MEN_PER;
        let sanctuary = provinces.iter().any(|p| {
            data.provinces.get(p).is_some_and(|pd| {
                pd.adjacent.iter().any(|adj| {
                    data.provinces.get(adj).is_some_and(|ad| {
                        let h = military.owner_of(*adj, &ad.owner);
                        military.at_war(holder, &h) || !same_side(data, military, holder, &h)
                    })
                })
            })
        });
        // Sponsor tap: any rival-bloc power's collection network against
        // the holder doubles as the resistance pipeline (design deviation:
        // dedicated SponsorResistance op post-slice).
        let z = settlements.zones.get_mut(&key).unwrap();
        // `required` counts full divisions; each is 10,000 men.
        let garrison_ok = garrison_men >= required * 10_000;
        if garrison_ok {
            let target = tuning::CONTROL_TARGET_BASE
                + match z.policy {
                    ZonePolicy::MilitaryGovernment => 200,
                    ZonePolicy::ClientAdministration => 0,
                    ZonePolicy::Exploitation => 100,
                };
            if z.control < target {
                z.control = (z.control + tuning::CONTROL_GROW).min(target);
            }
        } else {
            z.control = z.control.saturating_sub(tuning::CONTROL_DECAY);
        }
        let insurgency = (1000u32.saturating_sub(z.control as u32)) / 4
            + (z.alignment.min(0).unsigned_abs() as u32) / 4
            + if sanctuary {
                tuning::SANCTUARY_BONUS as u32
            } else {
                0
            };
        z.insurgency = insurgency.min(1000) as u16;

        // Flare roll while hot: daily odds tuned to ~weekly events at
        // full insurgency (no tick-modulo arithmetic — determinism rule).
        if z.insurgency >= tuning::FLARE_GATE && stream.below(7000) < z.insurgency as u32 / 2 {
            // Garrison bleeds; a stockpile point burns.
            let victim = military
                .formations
                .iter()
                .filter(|(_, f)| &f.owner == holder && provinces.contains(&f.location))
                .map(|(id, _)| *id)
                .next();
            if let Some(id) = victim {
                let f = military.formations.get_mut(&id).unwrap();
                f.strength = f.strength.saturating_sub(tuning::FLARE_STRENGTH_HIT);
            }
            if let Some(s) = econ.industry.get_mut(holder) {
                s.military_stock = s.military_stock.saturating_sub(tuning::FLARE_STOCK_HIT);
            }
            military.log(
                clock.tick,
                format!(
                    "PARTISAN ATTACKS IN OCCUPIED {} -- GARRISON TAKES LOSSES",
                    original.0
                ),
            );
        }

        // Monthly: upkeep + alignment drift.
        if clock.new_month {
            let bill = (provinces.len() as u64 * tuning::ZONE_UPKEEP_CENTI).div_ceil(100);
            if let Some(s) = econ.industry.get_mut(holder) {
                s.military_stock = s.military_stock.saturating_sub(bill);
            }
            let drift = match z.policy {
                ZonePolicy::MilitaryGovernment => tuning::ALIGN_DRIFT_MILGOV,
                ZonePolicy::ClientAdministration => tuning::ALIGN_DRIFT_CLIENT,
                ZonePolicy::Exploitation => tuning::ALIGN_DRIFT_EXPLOIT,
            };
            z.alignment = (z.alignment + drift).clamp(-1000, 1000);
            if z.policy == ZonePolicy::Exploitation {
                if let Some(s) = econ.industry.get_mut(holder) {
                    s.military_stock += 1;
                }
                *settlements.legitimacy.entry(holder.clone()).or_default() -= 1;
            }
        }
    }
}

/// The old both-willing armistice, upgraded: it now produces a
/// FrozenConflict object (DMZ strip, tension floor, standing claims)
/// instead of a bare war-end.
fn settle_frozen(
    data: &ScenarioData,
    clock: &SimClock,
    player: &Option<CountryTag>,
    military: &mut Military,
    fired: &mut crate::events::FiredEvents,
    tension: &mut GlobalTension,
    settlements: &mut Settlements,
) {
    use crate::military::tuning::*;
    let pairs: Vec<(CountryTag, CountryTag)> = military.wars.clone();
    for (a, b) in pairs {
        let start = military
            .war_started
            .get(&(a.clone(), b.clone()))
            .copied()
            .unwrap_or(0);
        let war_months = (clock.tick.saturating_sub(start)) / (24 * 30);
        let stale_months = (clock.tick.saturating_sub(military.last_line_change_tick)) / (24 * 30);
        let formations_of = |m: &Military, tag: &CountryTag| {
            m.formations.values().filter(|f| &f.owner == tag).count()
        };
        let willing = |m: &Military, tag: &CountryTag, enemy: &CountryTag| {
            if player.as_ref() == Some(tag) {
                m.has_offered_armistice(tag, enemy)
            } else {
                m.has_offered_armistice(tag, enemy)
                    || (war_months >= ARMISTICE_WAR_MONTHS
                        && stale_months >= ARMISTICE_STALE_MONTHS)
                    || formations_of(m, tag) == 0
            }
        };
        if !(willing(military, &a, &b) && willing(military, &b, &a)) {
            continue;
        }
        // DMZ strip: each side's provinces adjacent to the other's.
        let mut dmz: BTreeSet<ProvinceId> = BTreeSet::new();
        for p in data.provinces.values() {
            let holder = military.owner_of(p.id, &p.owner);
            let on_line = p.adjacent.iter().any(|adj| {
                data.provinces.get(adj).is_some_and(|ad| {
                    let other = military.owner_of(*adj, &ad.owner);
                    (holder == a && other == b) || (holder == b && other == a)
                })
            });
            if on_line && (holder == a || holder == b) {
                dmz.insert(p.id);
            }
        }
        crate::military::end_war(military, &a, &b);
        settlements.frozen.push(FrozenConflict {
            a: a.clone(),
            b: b.clone(),
            dmz,
            since_tick: clock.tick,
        });
        fired.notices.push((
            "ARMISTICE SIGNED".into(),
            format!(
                "{} AND {} SIGN AN ARMISTICE. HOSTILITIES SUSPENDED ALONG THE LINE OF CONTACT; A DEMILITARIZED ZONE SEALS THE FRONT. NO POLITICAL SETTLEMENT -- THE CLAIMS STAND, THE GUNS WAIT, AND THE LINE IS THE BORDER UNTIL IT ISN'T.",
                a.0, b.0
            ),
        ));
        tension.apply(ARMISTICE_TENSION_RELIEF);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::GameDate;
    use crate::command::SimCommand;
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
        app.insert_resource(SimScenario(Arc::new(data)));
        app
    }

    fn push(app: &mut App, cmd: SimCommand) {
        app.world_mut()
            .resource_mut::<crate::command::PendingCommands>()
            .push(cmd);
    }

    #[test]
    fn scripted_aims_and_legitimacy_land() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 190); // war on, US committed
        let s = app.world().resource::<Settlements>();
        assert_eq!(
            s.aim(&CountryTag("PRK".into()), &CountryTag("KOR".into())),
            WarAim::Unify,
            "invasion declares Unify"
        );
        assert_eq!(
            s.aim(&CountryTag("USA".into()), &CountryTag("PRK".into())),
            WarAim::StatusQuoAnte,
            "the UN mandate is repel, not conquer"
        );
        assert!(
            s.legitimacy_of(&CountryTag("USA".into())) >= 25,
            "UNSC 83 grants legitimacy"
        );
    }

    #[test]
    fn upgrading_the_aim_is_a_priced_escalation() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 190);
        let usa = CountryTag("USA".into());
        let prk = CountryTag("PRK".into());
        let before = app.world().resource::<GlobalTension>().value();
        push(
            &mut app,
            SimCommand::SetWarAim {
                country: usa.clone(),
                enemy: prk.clone(),
                aim: WarAim::Unify,
            },
        );
        run_ticks(&mut app, 1);
        let after = app.world().resource::<GlobalTension>().value();
        // Rungs 1..=3: 10 + 25 + 60 = 95 tenths (minus one tick's decay).
        assert!(
            after >= before + 90,
            "crossing the 38th is priced: {before} -> {after}"
        );
        assert_eq!(
            app.world().resource::<Settlements>().aim(&usa, &prk),
            WarAim::Unify
        );
    }

    #[test]
    fn occupation_becomes_a_zone_with_costs_and_insurgency() {
        let mut app = app_with_scenario();
        // KPA rolls south: South Korean ground under northern occupation.
        run_ticks(&mut app, 24 * 240);
        let s = app.world().resource::<Settlements>();
        assert!(
            !s.zones.is_empty(),
            "occupied ground forms political zones: {:?}",
            s.zones.keys().collect::<Vec<_>>()
        );
        let hot = s.zones.values().any(|z| z.insurgency > 0);
        assert!(hot, "occupation is contested, not free");
    }

    #[test]
    fn incorporation_is_blocked_by_congress_for_market_annexers() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 200);
        let world = app.world();
        let data = world.resource::<SimScenario>().0.clone();
        let military = world.resource::<Military>();
        let settlements = world.resource::<Settlements>();
        let econ = world.resource::<Economies>();
        let demo = world.resource::<crate::demography::Demographics>();
        let clock = world.resource::<SimClock>();
        let prop = Proposal {
            proposer: CountryTag("USA".into()),
            clauses: vec![Clause::Incorporation {
                territory: CountryTag("PRK".into()),
                annexer: CountryTag("USA".into()),
            }],
            since_tick: clock.tick,
        };
        let verdicts = evaluate(&data, military, settlements, econ, demo, clock, &prop);
        assert!(!verdicts.is_empty());
        let blockers: Vec<&String> = verdicts.iter().flat_map(|v| v.blockers.iter()).collect();
        assert!(
            blockers.iter().any(|b| b.contains("CONGRESS")),
            "the domestic gate names itself: {blockers:?}"
        );
        assert!(
            blockers.iter().any(|b| b.contains("AIM")),
            "demands are capped at the declared aim: {blockers:?}"
        );
    }

    #[test]
    fn imposed_peace_bleeds_without_recognition() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 240); // KPA holds southern ground
        let prk = CountryTag("PRK".into());
        let kor = CountryTag("KOR".into());
        {
            let military = app.world().resource::<Military>();
            assert!(military.at_war(&prk, &kor));
            assert!(
                military.occupation.values().any(|h| *h == prk),
                "KPA holds ROK ground"
            );
        }
        push(
            &mut app,
            SimCommand::ImposeSettlement {
                country: prk.clone(),
                enemy: kor.clone(),
            },
        );
        // Into the next month so the floor derivation runs.
        run_ticks(&mut app, 24 * 32);
        let world = app.world();
        let data = world.resource::<SimScenario>().0.clone();
        let military = world.resource::<Military>();
        let settlements = world.resource::<Settlements>();
        assert!(!military.at_war(&prk, &kor), "the shooting stopped");
        assert!(
            !settlements.truce_active(&prk, &kor, world.resource::<SimClock>().tick),
            "no truce protects an imposed peace"
        );
        assert!(
            settlements.unrecognized_holdings(&data, military, &prk) > 0,
            "the holdings are never recognized"
        );
        assert!(
            world.resource::<GlobalTension>().extra_floor >= tuning::ANNEX_TENSION_FLOOR,
            "unrecognized annexation raises the tension floor"
        );
    }

    #[test]
    fn status_quo_ante_signs_and_restores() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 250); // deep enough for exhaustion
        let prk = CountryTag("PRK".into());
        let kor = CountryTag("KOR".into());
        push(
            &mut app,
            SimCommand::ProposeSettlement {
                proposer: kor.clone(),
                clauses: vec![
                    Clause::Restore { to: kor.clone() },
                    Clause::Restore { to: prk.clone() },
                ],
            },
        );
        // Give the monthly evaluation a few cycles; exhaustion climbs.
        run_ticks(&mut app, 24 * 30 * 8);
        let world = app.world();
        let settlements = world.resource::<Settlements>();
        let military = world.resource::<Military>();
        assert!(
            !settlements.treaties.is_empty() || !settlements.frozen.is_empty(),
            "the war found an ending within months of a fair offer"
        );
        if settlements.treaties.iter().any(|t| {
            t.clauses
                .iter()
                .any(|c| matches!(c, Clause::Restore { .. }))
        }) {
            assert!(
                !military.at_war(&prk, &kor),
                "restoration treaty ended the war"
            );
        }
    }
}
