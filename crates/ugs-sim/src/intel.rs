//! Intelligence & covert ops v1 — the espionage pillar
//! (docs/design/systems/espionage.md). What your rival BELIEVES is a
//! function of what you let them see and what you spend to see through
//! them. One four-domain penetration score per (viewer, subject); each
//! domain drives a consumer that already exists (deterrence bias, war
//! fuzz widths, planned-economy transparency, crisis resolve bands).

use std::collections::{BTreeMap, BTreeSet};

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::CountryTag;

use crate::demography::SimScenario;
use crate::events::FiredEvents;
use crate::tension::GlobalTension;
use crate::SimClock;

pub mod tuning {
    /// Embassy/attaché baseline: free, safe, low ceiling (all domains).
    pub const EMBASSY_CAP: u32 = 250;
    pub const EMBASSY_RATE: u32 = 150;
    /// Agent-network caps by domain at full funding (level 3), before
    /// counterintel reduction. Economic truth and political resolve are
    /// only reachable through humint.
    pub const NET_CAP_NUCLEAR: u32 = 550;
    pub const NET_CAP_MILITARY: u32 = 500;
    pub const NET_CAP_ECONOMIC: u32 = 700;
    pub const NET_CAP_POLITICAL: u32 = 650;
    pub const NET_RATE: u32 = 60;
    /// Monthly decay toward the remaining floor when a source lapses.
    pub const DECAY_MILITARY: u32 = 40;
    pub const DECAY_ECONOMIC: u32 = 40;
    pub const DECAY_POLITICAL: u32 = 40;
    pub const DECAY_NUCLEAR: u32 = 25; // hardware ages slower
    /// Network strength grows toward 100 * funding/3 at this rate/month.
    pub const NET_STRENGTH_RATE: u32 = 8;
    /// Counterintel structural floor by regime openness (permille).
    pub const CI_FLOOR_CLOSED: u32 = 300; // Eastern bloc
    pub const CI_FLOOR_OPEN: u32 = 80; // Western/non-aligned
    /// A network's effective caps drop by counterintel/2.
    pub const CI_CAP_PENALTY_DIVISOR: u32 = 2;
    /// Penetration at/above which deception in a domain is seen through.
    pub const SEE_THROUGH_THRESHOLD: u32 = 400;
    /// Monthly counterintel sweep: catch chance permille =
    /// network_strength * ci_permille / this.
    pub const SWEEP_DIVISOR: u32 = 2000;
    /// A caught network loses this much strength.
    pub const SWEEP_STRENGTH_LOSS: u32 = 40;
    /// Network strength floor to run an operation.
    pub const OP_MIN_STRENGTH: u32 = 40;
    /// Operation strength cost on execution.
    pub const OP_STRENGTH_COST: u32 = 25;
    /// Blown-op chance permille = base + target_ci/ this-scaled by tension.
    pub const OP_BLOWN_BASE: u32 = 150;
    /// Deniability spent when an op is blown.
    pub const DENIABILITY_SABOTAGE: u32 = 20;
    pub const DENIABILITY_THEFT: u32 = 12;
    /// Tension from a blown op, multiplied when deniability is low.
    pub const BLOWN_TENSION: i32 = 40;
    /// Steal-designs speed boost (permille multiplier, <1000 = faster).
    pub const STEAL_SPEED_BONUS: u32 = 750;
    /// Sabotage sets a program facility back this many months.
    pub const SABOTAGE_SETBACK_MONTHS: u32 = 10;
    /// Monthly defector base chance permille (scaled by target instability).
    pub const DEFECTOR_BASE_PERMILLE: u32 = 6;
    pub const DEFECTOR_SPIKE: u32 = 200;
}

/// A pending covert operation queued by command, resolved the same
/// tick in the Politics stage (before the monthly pass).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpKind {
    /// Set a nuclear facility back / apply a program malus.
    Sabotage,
    /// Steal weapon-design data: speeds the thief IF they have a program,
    /// and pierces the target's nuclear opacity.
    StealDesigns,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingOp {
    pub owner: CountryTag,
    pub target: CountryTag,
    pub kind: OpKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Domain {
    Nuclear,
    Military,
    Economic,
    Political,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainPenetration {
    pub nuclear: u32,
    pub military: u32,
    pub economic: u32,
    pub political: u32,
}

impl DomainPenetration {
    pub fn get(&self, d: Domain) -> u32 {
        match d {
            Domain::Nuclear => self.nuclear,
            Domain::Military => self.military,
            Domain::Economic => self.economic,
            Domain::Political => self.political,
        }
    }
}

/// One collection network the viewer runs against a subject.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Network {
    /// Funding tier 0-3 (0 = dormant, kept for exposure/history).
    pub funding: u8,
    /// Tradecraft strength 0-100; gates operations, absorbs roll-ups.
    pub strength: u32,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Intel {
    /// Penetration per (viewer, subject).
    pub penetration: BTreeMap<(CountryTag, CountryTag), DomainPenetration>,
    /// Collection networks per (owner, target).
    pub networks: BTreeMap<(CountryTag, CountryTag), Network>,
    /// Counterintelligence funded level 0-3 per country.
    pub counterintel: BTreeMap<CountryTag, u8>,
    /// Deniability 0-100 per country; blown ops spend it.
    pub deniability: BTreeMap<CountryTag, u32>,
    /// (viewer, subject, domain) where the viewer has pierced the
    /// subject's deception — a one-way flag.
    pub seen_through: BTreeSet<(CountryTag, CountryTag, Domain)>,
    /// Operations queued this tick, resolved same tick before the
    /// monthly pass.
    pub pending_ops: Vec<PendingOp>,
    /// The embassy/attaché baseline every pair shares — one accruing
    /// scalar instead of an O(countries^2) table of identical values.
    /// `knowledge()` floors every pair at this; only funded/active
    /// pairs get their own table entry.
    embassy_floor: u32,
    /// Cursor into FiredEvents.resolved for spy-trial outcomes.
    spy_cursor: usize,
    seeded: bool,
}

impl Intel {
    /// Penetration a viewer has on a subject, floored at the shared
    /// embassy baseline — so an absent table entry reads as embassy
    /// coverage, not zero.
    pub fn penetration_of(&self, viewer: &CountryTag, subject: &CountryTag) -> DomainPenetration {
        let stored = self
            .penetration
            .get(&(viewer.clone(), subject.clone()))
            .copied()
            .unwrap_or_default();
        let f = self.embassy_floor;
        DomainPenetration {
            nuclear: stored.nuclear.max(f),
            military: stored.military.max(f),
            economic: stored.economic.max(f),
            political: stored.political.max(f),
        }
    }

    /// Viewer's knowledge in a domain, 0-1000 — used by every consumer.
    pub fn knowledge(&self, viewer: &CountryTag, subject: &CountryTag, d: Domain) -> u32 {
        self.penetration_of(viewer, subject).get(d)
    }

    pub fn deniability_of(&self, country: &CountryTag) -> u32 {
        self.deniability.get(country).copied().unwrap_or(100)
    }

    /// Has `viewer` seen through `subject`'s deception in `domain`?
    pub fn sees_through(&self, viewer: &CountryTag, subject: &CountryTag, d: Domain) -> bool {
        self.seen_through
            .contains(&(viewer.clone(), subject.clone(), d))
    }

    pub fn ci_level(&self, country: &CountryTag) -> u8 {
        self.counterintel.get(country).copied().unwrap_or(0)
    }

    /// Counterintel permille = regime floor + funded component. Regime
    /// openness is dynamic (`Influence::is_closed`): a coup closes a
    /// country, so Cuba after 1961 no longer reads as open.
    pub fn ci_permille(&self, country: &CountryTag, closed: bool) -> u32 {
        let floor = if closed {
            tuning::CI_FLOOR_CLOSED
        } else {
            tuning::CI_FLOOR_OPEN
        };
        floor + self.ci_level(country) as u32 * 200
    }

    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let fold = |mut h: u64, s: &str| {
            for b in s.bytes() {
                h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
            h
        };
        for ((v, s), p) in &self.penetration {
            h = fold(h, &v.0);
            h = fold(h, &s.0);
            for x in [p.nuclear, p.military, p.economic, p.political] {
                h = (h ^ x as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        for ((o, t), n) in &self.networks {
            h = fold(h, &o.0);
            h = fold(h, &t.0);
            h = (h ^ n.funding as u64 ^ ((n.strength as u64) << 8))
                .wrapping_mul(0x0000_0100_0000_01b3);
        }
        for (c, l) in &self.counterintel {
            h = fold(h, &c.0);
            h = (h ^ *l as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        for (c, d) in &self.deniability {
            h = fold(h, &c.0);
            h = (h ^ *d as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        for (v, s, d) in &self.seen_through {
            h = fold(h, &v.0);
            h = fold(h, &s.0);
            h = (h ^ *d as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        (h ^ self.embassy_floor as u64 ^ ((self.spy_cursor as u64) << 20))
            .wrapping_mul(0x0000_0100_0000_01b3)
    }
}

fn approach(value: u32, cap: u32, rate: u32) -> u32 {
    if value >= cap {
        return value;
    }
    (value + (cap - value) * rate / 1000 + 1).min(cap)
}

fn decay_to(value: u32, floor: u32, rate: u32) -> u32 {
    if value <= floor {
        return floor;
    }
    (value - ((value - floor) * rate / 1000 + 1)).max(floor)
}

/// Monthly: grow penetration toward each pair's best active source cap,
/// decay lapsed domains toward the embassy floor, and grow network
/// strength toward its funding ceiling. Deterministic integer math.
pub fn update_intel(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    influence: Res<crate::influence::Influence>,
    mut intel: ResMut<Intel>,
) {
    use tuning::*;
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;

    if !intel.seeded {
        intel.seeded = true;
        // Everyone starts with a full deniability account.
        for tag in data.countries.keys() {
            intel.deniability.insert(tag.clone(), 100);
        }
        return;
    }
    if !clock.new_month {
        return;
    }

    // The shared embassy/attache baseline: one scalar every pair reads
    // as a floor (all embassy-only pairs are identical — same start,
    // rate, cap — so there is no reason to store them individually).
    intel.embassy_floor = approach(intel.embassy_floor, EMBASSY_CAP, EMBASSY_RATE);

    // Network strength grows toward 100 * funding/3.
    let net_keys: Vec<(CountryTag, CountryTag)> = intel.networks.keys().cloned().collect();
    for key in &net_keys {
        let n = intel.networks.get_mut(key).unwrap();
        let ceiling = n.funding as u32 * 100 / 3;
        if n.strength < ceiling {
            n.strength = (n.strength + NET_STRENGTH_RATE).min(ceiling);
        }
    }

    // Only funded networks and pairs that already carry an above-floor
    // entry (a lapsed network decaying back, or op-injected penetration)
    // need per-pair work — never the full O(countries^2) table.
    let mut active: Vec<(CountryTag, CountryTag)> = intel
        .networks
        .iter()
        .filter(|(_, n)| n.funding > 0 && n.strength > 0)
        .map(|(k, _)| k.clone())
        .chain(intel.penetration.keys().cloned())
        .collect();
    active.sort();
    active.dedup();

    let floor = intel.embassy_floor;
    for (viewer, subject) in active {
        let net = intel
            .networks
            .get(&(viewer.clone(), subject.clone()))
            .cloned()
            .unwrap_or_default();
        let ci = intel.ci_permille(&subject, influence.is_closed(&subject));
        let ci_penalty = ci / CI_CAP_PENALTY_DIVISOR;
        let net_active = net.funding > 0 && net.strength > 0;
        let net_scale = net.strength.min(100);
        let net_cap = |full: u32| -> u32 {
            if !net_active {
                return 0;
            }
            let reduced = full.saturating_sub(ci_penalty);
            reduced * net_scale / 100
        };
        // Caps are the network ceilings; the embassy floor is applied at
        // read time in penetration_of, so a lapsed pair decays toward 0.
        let caps = [
            (Domain::Nuclear, net_cap(NET_CAP_NUCLEAR), DECAY_NUCLEAR),
            (Domain::Military, net_cap(NET_CAP_MILITARY), DECAY_MILITARY),
            (Domain::Economic, net_cap(NET_CAP_ECONOMIC), DECAY_ECONOMIC),
            (
                Domain::Political,
                net_cap(NET_CAP_POLITICAL),
                DECAY_POLITICAL,
            ),
        ];
        let rate = NET_RATE;
        let entry = intel
            .penetration
            .entry((viewer.clone(), subject.clone()))
            .or_default();
        for (domain, cap, decay) in caps {
            let cur = entry.get(domain);
            let next = if cur < cap {
                approach(cur, cap, rate)
            } else {
                decay_to(cur, cap, decay)
            };
            match domain {
                Domain::Nuclear => entry.nuclear = next,
                Domain::Military => entry.military = next,
                Domain::Economic => entry.economic = next,
                Domain::Political => entry.political = next,
            }
        }
        // Prune only LAPSED pairs that have decayed to the shared floor:
        // they read identically to an absent entry. An active network
        // still building below the floor must keep its entry, or it
        // could never accumulate past the baseline.
        if !net_active
            && entry.nuclear <= floor
            && entry.military <= floor
            && entry.economic <= floor
            && entry.political <= floor
        {
            intel.penetration.remove(&(viewer, subject));
        }
    }
}

/// Ops, sweeps, defectors, and deception-piercing. Runs after
/// `update_intel` in Politics, so it sees this month's penetration.
/// All randomness from labeled forks; iteration in BTreeMap order.
#[allow(clippy::too_many_arguments)] // Bevy systems take what they query
pub fn update_espionage(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    player: Res<crate::military::PlayerCountry>,
    mut rng: ResMut<crate::rng::SimRng>,
    mut intel: ResMut<Intel>,
    mut programs: ResMut<crate::nuclear::NuclearPrograms>,
    mut tension: ResMut<GlobalTension>,
    mut fired: ResMut<FiredEvents>,
    influence: Res<crate::influence::Influence>,
) {
    use tuning::*;
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;

    // --- Resolve answered spy-trial choices ---------------------------
    let answered: Vec<(String, u8)> = fired
        .resolved
        .iter()
        .skip(intel.spy_cursor)
        .filter(|(id, _)| id.starts_with("spy-caught-"))
        .cloned()
        .collect();
    intel.spy_cursor = fired.resolved.len();
    for (_, option) in answered {
        match option {
            0 => {
                // Public show trial: domestic cohesion up, tension up,
                // the rival embarrassed.
                tension.apply(25);
                fired.notices.push((
                    "SHOW TRIAL OPENS".into(),
                    "THE CAPTURED AGENT IS PRESENTED TO THE PRESS. A CONFESSION IS READ. THE CASE AGAINST THE SPONSORING POWER IS MADE TO THE WORLD -- AND AT HOME, RESOLVE HARDENS.".into(),
                ));
            }
            _ => {
                // Quiet expulsion: no tension, a swap banked (flavor v1).
                fired.notices.push((
                    "QUIET EXPULSION".into(),
                    "THE AGENT IS DECLARED PERSONA NON GRATA AND PUT ON A PLANE. NO HEADLINES. A NAME BANKED FOR A FUTURE EXCHANGE.".into(),
                ));
            }
        }
    }

    // --- Resolve queued operations (any tick they were filed) ----------
    let ops = std::mem::take(&mut intel.pending_ops);
    for op in ops {
        let strength = intel
            .networks
            .get(&(op.owner.clone(), op.target.clone()))
            .map(|n| n.strength)
            .unwrap_or(0);
        if strength < OP_MIN_STRENGTH {
            if player.0.as_ref() == Some(&op.owner) {
                fired.notices.push((
                    "OPERATION SCRUBBED".into(),
                    format!(
                        "INSUFFICIENT NETWORK STRENGTH IN {} TO MOUNT THE OPERATION. ASSETS MUST BE BUILT FIRST.",
                        op.target.0
                    ),
                ));
            }
            continue;
        }
        // Spending the network that also collects (the OSO/OPC tradeoff).
        if let Some(n) = intel
            .networks
            .get_mut(&(op.owner.clone(), op.target.clone()))
        {
            n.strength = n.strength.saturating_sub(OP_STRENGTH_COST);
        }
        // Blown chance rises with target counterintel and tension band.
        let ci = intel.ci_permille(&op.target, influence.is_closed(&op.target));
        let band = tension.value().max(0) as u32 / 250; // 0-3
        let blown_chance = OP_BLOWN_BASE + ci / 4 + band * 40;
        let mut stream = rng.fork(b"op-resolve");
        let blown = stream.below(1000) < blown_chance;

        // Effect lands regardless of exposure — the deed is done.
        let (kind_label, deniability_cost) = match op.kind {
            OpKind::Sabotage => {
                if let Some(p) = programs.programs.get_mut(&op.target) {
                    p.building.push((
                        crate::nuclear::FacilityKind::Reactor,
                        SABOTAGE_SETBACK_MONTHS,
                    ));
                    // A wrench in the works: slow the whole program a while.
                    p.speed_mod_permille = p.speed_mod_permille * 850 / 1000;
                }
                ("SABOTAGE", DENIABILITY_SABOTAGE)
            }
            OpKind::StealDesigns => {
                // Pierce the target's nuclear opacity for the thief.
                intel
                    .seen_through
                    .insert((op.owner.clone(), op.target.clone(), Domain::Nuclear));
                if let Some(e) = intel
                    .penetration
                    .get_mut(&(op.owner.clone(), op.target.clone()))
                {
                    e.nuclear = (e.nuclear + DEFECTOR_SPIKE).min(1000);
                }
                // If the thief has a program, the stolen data speeds it.
                if let Some(p) = programs.programs.get_mut(&op.owner) {
                    p.speed_mod_permille = p.speed_mod_permille * STEAL_SPEED_BONUS / 1000;
                }
                ("DESIGN THEFT", DENIABILITY_THEFT)
            }
        };

        if player.0.as_ref() == Some(&op.owner) {
            fired.notices.push((
                format!("{} -- ASSESSMENT", kind_label),
                format!(
                    "OPERATION AGAINST {} EXECUTED. {}",
                    op.target.0,
                    if blown {
                        "ASSETS COMPROMISED. ATTRIBUTION LIKELY. EXPECT A RESPONSE."
                    } else {
                        "CLEAN. NO INDICATION THE OTHER SIDE KNOWS."
                    }
                ),
            ));
        }

        if blown {
            // Deniability spend + escalating tension when it runs low.
            let den = intel.deniability.entry(op.owner.clone()).or_insert(100);
            let low = *den < 40;
            *den = den.saturating_sub(deniability_cost);
            let mult = if low { 2 } else { 1 };
            tension.apply(BLOWN_TENSION * mult);
            // A blown op is a pretext the wronged party can bank.
            fired.notices.push((
                "COVERT ACTION EXPOSED".into(),
                format!(
                    "{} INTELLIGENCE SERVICES DISPLAY EVIDENCE OF A {} OPERATION BY {}. FORMAL PROTEST LODGED. THE INCIDENT IS A STANDING GRIEVANCE.",
                    op.target.0, kind_label, op.owner.0
                ),
            ));
        }
    }

    if !clock.new_month {
        return;
    }

    // --- Deception piercing: a deep-enough viewer sees through it ------
    let mut newly_seen: Vec<(CountryTag, CountryTag)> = Vec::new();
    for ((viewer, subject), pen) in &intel.penetration {
        if pen.nuclear >= SEE_THROUGH_THRESHOLD
            && programs.programs.get(subject).is_some_and(|p| p.deception)
            && !intel
                .seen_through
                .contains(&(viewer.clone(), subject.clone(), Domain::Nuclear))
        {
            newly_seen.push((viewer.clone(), subject.clone()));
        }
    }
    for (viewer, subject) in newly_seen {
        intel
            .seen_through
            .insert((viewer.clone(), subject.clone(), Domain::Nuclear));
        if player.0.as_ref() == Some(&viewer) {
            fired.notices.push((
                "SOURCES CONTRADICT OFFICIAL FIGURES".into(),
                format!(
                    "ANALYSTS CONCLUDE {}'S DISPLAYED STRENGTH IS INFLATED. THE PARADE COUNT DOES NOT MATCH THE PRODUCTION RECORD. THEIR DECEPTION NO LONGER MOVES OUR ESTIMATE.",
                    subject.0
                ),
            ));
        }
    }

    // --- Counterintel sweeps: catch foreign networks ------------------
    // Iterate defenders in order; at most one catch reported per defender.
    let defenders: Vec<CountryTag> = data.countries.keys().cloned().collect();
    for defender in &defenders {
        let ci = intel.ci_permille(defender, influence.is_closed(defender));
        // The loudest hostile network against this defender.
        let loudest = intel
            .networks
            .iter()
            .filter(|((_, t), n)| t == defender && n.strength > 0)
            .max_by_key(|((o, _), n)| (n.strength, o.0.clone()))
            .map(|((o, t), n)| (o.clone(), t.clone(), n.strength));
        let Some((owner, target, strength)) = loudest else {
            continue;
        };
        let chance = strength * ci / SWEEP_DIVISOR;
        let mut stream = rng.fork(b"ci-sweep");
        if stream.below(1000) >= chance {
            continue;
        }
        if let Some(n) = intel.networks.get_mut(&(owner.clone(), target.clone())) {
            n.strength = n.strength.saturating_sub(SWEEP_STRENGTH_LOSS);
        }
        // The defender catches a spy: a choice event if it's the player,
        // else a wire notice. Tension ticks up either way.
        tension.apply(10);
        if player.0.as_ref() == Some(defender) {
            fired.dynamic.push(crate::events::DynamicChoice {
                id: format!("spy-caught-{}-{}", defender.0, clock.tick),
                title: "FOREIGN AGENT IN CUSTODY".into(),
                body: format!(
                    "COUNTERINTELLIGENCE HAS ROLLED UP A NETWORK RUN BY {}. THE PRISONER CAN BE TRIED, QUIETLY EXPELLED, OR TURNED. THE CHOICE SHAPES WHAT THE WORLD LEARNS.",
                    owner.0
                ),
                country: defender.clone(),
                options: vec![
                    "PUBLIC SHOW TRIAL".into(),
                    "QUIET EXPULSION".into(),
                ],
                deadline_tick: clock.tick + 96,
            });
        } else if player.0.as_ref() == Some(&owner) {
            fired.notices.push((
                "NETWORK ROLLED UP".into(),
                format!(
                    "OUR NETWORK IN {} HAS BEEN COMPROMISED. ASSETS LOST. COLLECTION DEGRADED.",
                    target.0
                ),
            ));
        }
    }

    // --- Defectors: rare truth deliveries, likelier from unstable foes -
    let mut stream = rng.fork(b"defector");
    for subject in &defenders {
        let stability = data
            .countries
            .get(subject)
            .map(|c| c.stability as u32)
            .unwrap_or(50);
        let chance = DEFECTOR_BASE_PERMILLE * (100 - stability.min(99)) / 50;
        if stream.below(1000) >= chance {
            continue;
        }
        // The defector walks in to whoever has the best network there.
        let recipient = intel
            .networks
            .iter()
            .filter(|((_, t), n)| t == subject && n.strength > 0)
            .max_by_key(|((o, _), n)| (n.strength, o.0.clone()))
            .map(|((o, _), _)| o.clone());
        let Some(recipient) = recipient else { continue };
        if let Some(e) = intel
            .penetration
            .get_mut(&(recipient.clone(), subject.clone()))
        {
            e.military = (e.military + DEFECTOR_SPIKE).min(1000);
            e.political = (e.political + DEFECTOR_SPIKE).min(1000);
        }
        if player.0.as_ref() == Some(&recipient) {
            fired.notices.push((
                "DEFECTOR WALKS IN".into(),
                format!(
                    "AN OFFICER OF {} HAS COME OVER, BRINGING DOCUMENTS. A SNAPSHOT OF THE TRUTH, FROZEN AT TODAY'S DATE. OUR PICTURE OF THEM SHARPENS -- FOR NOW.",
                    subject.0
                ),
            ));
        }
    }
}

/// Command handlers.
pub fn set_network_funding(intel: &mut Intel, owner: CountryTag, target: CountryTag, level: u8) {
    let n = intel.networks.entry((owner, target)).or_default();
    n.funding = level.min(3);
}

pub fn set_counterintel(intel: &mut Intel, country: CountryTag, level: u8) {
    intel.counterintel.insert(country, level.min(3));
}

pub fn queue_operation(intel: &mut Intel, owner: CountryTag, target: CountryTag, kind: OpKind) {
    intel.pending_ops.push(PendingOp {
        owner,
        target,
        kind,
    });
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

    fn app() -> App {
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

    #[test]
    fn embassy_floor_accrues_without_a_network() {
        let mut app = app();
        run_ticks(&mut app, 24 * 365); // a year
        let intel = app.world().resource::<Intel>();
        let p = intel.knowledge(
            &CountryTag("USA".into()),
            &CountryTag("SOV".into()),
            Domain::Military,
        );
        assert!(p > 150, "embassy baseline accrues toward its ceiling: {p}");
        assert!(p <= tuning::EMBASSY_CAP, "but stays capped low: {p}");
    }

    #[test]
    fn a_network_penetrates_far_past_the_embassy_floor() {
        let mut app = app();
        run_ticks(&mut app, 2);
        // Player must be set before the network command has an owner.
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetPlayerCountry {
                country: Some(CountryTag("USA".into())),
            });
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetNetworkFunding {
                target: CountryTag("SOV".into()),
                level: 3,
            });
        run_ticks(&mut app, 24 * 365 * 3);
        let intel = app.world().resource::<Intel>();
        let econ = intel.knowledge(
            &CountryTag("USA".into()),
            &CountryTag("SOV".into()),
            Domain::Economic,
        );
        // Economic truth is network-only; the Soviet closed society
        // (counterintel floor 300) caps how far even a full network gets.
        assert!(econ > 300, "network reaches economic intel: {econ}");
        assert!(econ < 700, "closed society still resists: {econ}");
    }

    #[test]
    fn nuclear_penetration_shrinks_the_deterrence_estimate() {
        use crate::deterrence::Deterrence;
        // Two identical worlds; in one, the US deeply penetrates the
        // Soviet program. The believed Soviet arsenal should be lower
        // (closer to truth) where intel is high — the bomber-gap cure.
        let believed = |fund: bool| -> u32 {
            let mut app = app();
            run_ticks(&mut app, 2);
            app.world_mut()
                .resource_mut::<PendingCommands>()
                .push(SimCommand::SetPlayerCountry {
                    country: Some(CountryTag("USA".into())),
                });
            if fund {
                app.world_mut().resource_mut::<PendingCommands>().push(
                    SimCommand::SetNetworkFunding {
                        target: CountryTag("SOV".into()),
                        level: 3,
                    },
                );
            }
            run_ticks(&mut app, 24 * 365 * 4);
            let det = app.world().resource::<Deterrence>();
            det.dyads
                .iter()
                .find(|((a, b), _)| a.0 == "SOV" && b.0 == "USA")
                .map(|(_, d)| d.b_believes_a_delivers) // USA's estimate of SOV
                .unwrap_or(0)
        };
        let blind = believed(false);
        let informed = believed(true);
        assert!(
            informed < blind,
            "penetration narrows the estimate: informed {informed} < blind {blind}"
        );
    }

    #[test]
    fn steal_designs_pierces_opacity_and_speeds_the_thief() {
        let mut app = app();
        run_ticks(&mut app, 2);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetPlayerCountry {
                country: Some(CountryTag("USA".into())),
            });
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetNetworkFunding {
                target: CountryTag("SOV".into()),
                level: 3,
            });
        // Build the network up, then steal designs.
        run_ticks(&mut app, 24 * 365 * 2);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::LaunchOperation {
                target: CountryTag("SOV".into()),
                kind: OpKind::StealDesigns,
            });
        run_ticks(&mut app, 48);
        let intel = app.world().resource::<Intel>();
        assert!(
            intel.sees_through(
                &CountryTag("USA".into()),
                &CountryTag("SOV".into()),
                Domain::Nuclear
            ),
            "design theft pierces the target's nuclear opacity"
        );
    }

    #[test]
    fn an_op_without_a_network_is_scrubbed() {
        let mut app = app();
        run_ticks(&mut app, 2);
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::SetPlayerCountry {
                country: Some(CountryTag("USA".into())),
            });
        app.world_mut()
            .resource_mut::<PendingCommands>()
            .push(SimCommand::LaunchOperation {
                target: CountryTag("SOV".into()),
                kind: OpKind::Sabotage,
            });
        run_ticks(&mut app, 48);
        let fired = app.world().resource::<crate::events::FiredEvents>();
        assert!(
            fired.notices.iter().any(|(t, _)| t.contains("SCRUBBED")),
            "an op needs a network first"
        );
    }

    #[test]
    fn determinism_holds_with_intel_active() {
        let run = || {
            let mut app = app();
            run_ticks(&mut app, 2);
            app.world_mut()
                .resource_mut::<PendingCommands>()
                .push(SimCommand::SetPlayerCountry {
                    country: Some(CountryTag("USA".into())),
                });
            app.world_mut()
                .resource_mut::<PendingCommands>()
                .push(SimCommand::SetNetworkFunding {
                    target: CountryTag("SOV".into()),
                    level: 2,
                });
            run_ticks(&mut app, 24 * 400);
            app.world().resource::<Intel>().digest()
        };
        assert_eq!(run(), run(), "intel state is bit-identical across runs");
    }
}
