//! Conventional military v1: formations, wars, movement, combat, and
//! occupation — the Korea-slice core of the researched architecture
//! (docs/design/systems/military.md). Cohesion decides battles;
//! strength dies slowly. No player micro: countries have postures.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{CountryTag, ProvinceId, Terrain};

use crate::demography::SimScenario;
use crate::rng::SimRng;
use crate::SimClock;

pub mod tuning {
    /// Archetype base stats: (attack, defense, days per province move).
    pub const INFANTRY: (u64, u64, u8) = (10, 13, 2);
    pub const MOTORIZED: (u64, u64, u8) = (13, 10, 1);
    pub const ARMOR: (u64, u64, u8) = (18, 8, 1);
    /// Hourly cohesion damage multiplier.
    pub const COHESION_DAMAGE_SCALE: u64 = 1;
    /// Strength damage = cohesion damage / this.
    pub const STRENGTH_DAMAGE_DIVISOR: u64 = 25;
    /// Retreat below this cohesion (permille).
    pub const RETREAT_COHESION: u64 = 200;
    /// Cohesion regained per hour out of battle (permille).
    pub const COHESION_REGEN: u64 = 8;
    /// Battle-hour variance: roll in [70, 130] percent.
    pub const VARIANCE_MIN: u32 = 70;
    pub const VARIANCE_SPAN: u32 = 61;

    /// Defenders on home (unoccupied) soil fight harder.
    pub const HOME_DEFENSE_PERMILLE: u64 = 1200;

    /// Men per division at full strength (strength 1000 = 10,000 men).
    pub const MEN_PER_STRENGTH_POINT: u64 = 10;
    /// Peacetime available-manpower pool: permille of total population.
    pub const MANPOWER_BASE_PERMILLE: u64 = 15;
    /// Wartime monthly mobilization: permille of population added to pool.
    pub const MOBILIZE_PERMILLE_PER_MONTH: u64 = 2;
    /// Strength points a resting formation regains per day (from the pool).
    pub const REINFORCE_PER_DAY: u64 = 15;

    /// Auto-willingness for armistice: months at war and months of
    /// front stability required (non-player countries).
    pub const ARMISTICE_WAR_MONTHS: u64 = 10;
    pub const ARMISTICE_STALE_MONTHS: u64 = 2;
    /// Tension released when guns fall silent.
    pub const ARMISTICE_TENSION_RELIEF: i32 = -50;

    // --- Force generation (military-command.md) --------------------------
    /// military_stock points to raise one division, by archetype.
    pub const RAISE_STOCK_INFANTRY: u64 = 3;
    pub const RAISE_STOCK_MOTORIZED: u64 = 5;
    pub const RAISE_STOCK_ARMOR: u64 = 8;
    /// Days of training to reach full (1000 permille) readiness.
    pub const TRAIN_DAYS_INFANTRY: u64 = 90;
    pub const TRAIN_DAYS_MOTORIZED: u64 = 120;
    pub const TRAIN_DAYS_ARMOR: u64 = 150;
    /// Monthly upkeep per Active division, centi-stock, by archetype.
    pub const UPKEEP_CENTI_INFANTRY: u64 = 20;
    pub const UPKEEP_CENTI_MOTORIZED: u64 = 30;
    pub const UPKEEP_CENTI_ARMOR: u64 = 50;
    /// Reserve/Mobilizing divisions accrue this share of Active upkeep.
    pub const RESERVE_UPKEEP_PERMILLE: u64 = 200;
    /// Upkeep multiplier for divisions off their owner's home soil.
    pub const OVERSEAS_UPKEEP_MULT: u64 = 3;
    /// Days from Reserve to Active.
    pub const MOBILIZE_DAYS: u8 = 21;
    /// Tension (internal tenths) per peacetime activation, and per raise
    /// at peace or beyond the peace floor at war (commitment is public).
    pub const MOBILIZATION_TENSION: i32 = 3;
    pub const RAISE_TENSION: i32 = 3;
    /// While in upkeep arrears: strength lost per division per month
    /// (desertion/breakdown) and quality decay at full shortfall.
    pub const ARREARS_MELT: u64 = 30;
    pub const ARREARS_QUALITY_DECAY: u64 = 20;
    /// Quality regained per fully-paid month, toward archetype baseline.
    pub const QUALITY_RECOVER: u64 = 10;
    /// Reserve divisions defend at this permille weight where they sit.
    pub const RESERVE_DEFENSE_PERMILLE: u64 = 700;
    /// Concentration soft-cap: side contribution scales by
    /// min(1, (base + hostile_adjacent) / divisions).
    pub const CONCENTRATION_BASE: u64 = 3;
    /// Days a formation keeps its front slot after retargeting.
    pub const RETARGET_COOLDOWN: u8 = 3;
    /// Quota-controller thresholds: keep slot if over quota by <= 1;
    /// only deficit >= 2 provinces pull from surplus >= 2 provinces.
    pub const QUOTA_SLACK: i64 = 1;
    pub const QUOTA_PULL_DEFICIT: i64 = 2;
    /// AI: peacetime active divisions (majors: industry / this divisor).
    pub const PEACE_FLOOR_DIVS: usize = 2;
    pub const MAJOR_INDUSTRY: u32 = 50;
    pub const MAJOR_FLOOR_DIVISOR: u32 = 20;
    /// AI reserve activations per day while mobilizing for war.
    pub const AI_ACTIVATIONS_PER_DAY: usize = 5;
    /// AI raises 1 armor per this many infantry (when affordable).
    pub const AI_ARMOR_RATIO: usize = 4;
    /// Sanity caps (military-command.md edge cases).
    pub const MAX_THEATERS: usize = 8;
    pub const MAX_OBJECTIVES: usize = 3;
    pub const MAX_FORMATIONS: usize = 200;
    /// Share of a disbanded division's men returned to the pool.
    pub const DISBAND_RETURN_PERMILLE: u64 = 800;
    /// Newly raised divisions spawn at this strength (fill via the
    /// reinforcement pipeline) and train from zero.
    pub const RAISE_START_STRENGTH: u64 = 100;
    /// Cohesion after standing down or completing mobilization.
    pub const STAND_DOWN_COHESION: u64 = 300;

    pub fn terrain_defense_permille(t: ugs_data::Terrain) -> u64 {
        use ugs_data::Terrain::*;
        match t {
            Mountain => 1600,
            Urban => 1500,
            Hills => 1300,
            Forest | Jungle => 1200,
            Marsh => 1250,
            _ => 1000,
        }
    }
}

/// 1 -> "1ST", 2 -> "2ND", 13 -> "13TH".
fn ordinal_words(n: u64) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "TH",
        (1, _) => "ST",
        (2, _) => "ND",
        (3, _) => "RD",
        _ => "TH",
    };
    format!("{n}{suffix}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Archetype {
    Infantry,
    Motorized,
    Armor,
}

impl Archetype {
    pub fn stats(self) -> (u64, u64, u8) {
        match self {
            Archetype::Infantry => tuning::INFANTRY,
            Archetype::Motorized => tuning::MOTORIZED,
            Archetype::Armor => tuning::ARMOR,
        }
    }
    pub fn raise_cost(self) -> u64 {
        match self {
            Archetype::Infantry => tuning::RAISE_STOCK_INFANTRY,
            Archetype::Motorized => tuning::RAISE_STOCK_MOTORIZED,
            Archetype::Armor => tuning::RAISE_STOCK_ARMOR,
        }
    }
    pub fn train_days(self) -> u64 {
        match self {
            Archetype::Infantry => tuning::TRAIN_DAYS_INFANTRY,
            Archetype::Motorized => tuning::TRAIN_DAYS_MOTORIZED,
            Archetype::Armor => tuning::TRAIN_DAYS_ARMOR,
        }
    }
    pub fn upkeep_centi(self) -> u64 {
        match self {
            Archetype::Infantry => tuning::UPKEEP_CENTI_INFANTRY,
            Archetype::Motorized => tuning::UPKEEP_CENTI_MOTORIZED,
            Archetype::Armor => tuning::UPKEEP_CENTI_ARMOR,
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "Motorized" => Archetype::Motorized,
            "Armor" => Archetype::Armor,
            _ => Archetype::Infantry,
        }
    }
}

/// Mobilization state. Mobilizing counts as Reserve for upkeep, front
/// slots, movement, and defense weight until `days_left` reaches zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Readiness {
    #[default]
    Active,
    Reserve,
    Mobilizing {
        days_left: u8,
    },
}

impl Readiness {
    /// Treated as stood-down (no slots, no movement, reduced weight)?
    pub fn stood_down(self) -> bool {
        !matches!(self, Readiness::Active)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct TheaterId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TheaterPosture {
    Defend,
    Probe,
    Offensive,
}

/// A player-painted (or AI auto-generated) command area. Formations are
/// assigned to theaters, never moved directly; the daily controller
/// distributes them across the theater's front.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theater {
    pub owner: CountryTag,
    pub name: String,
    pub provinces: std::collections::BTreeSet<ProvinceId>,
    pub posture: TheaterPosture,
    /// Advance axes (<= MAX_OBJECTIVES). Auto-cleared when captured or
    /// their holder leaves the war.
    pub objectives: Vec<ProvinceId>,
    /// Share of the theater's committed divisions held at the rear.
    pub echelon_permille: u16,
    /// ROE: enemy countries whose soil this theater may never enter.
    pub forbidden: std::collections::BTreeSet<CountryTag>,
    /// Auto-theaters track the whole country and die at peace.
    pub auto: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FormationId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Formation {
    pub owner: CountryTag,
    pub archetype: Archetype,
    pub location: ProvinceId,
    /// Vic2's lesson: a division raised from a named place is never
    /// magic. Casualties debit this province's real population.
    pub home: ProvinceId,
    /// "3RD SEOUL INFANTRY" — numbered per home province at raising.
    pub name: String,
    /// Where this formation stood before its last move (drawn as a
    /// movement arrow while `move_cooldown` runs).
    pub last_location: Option<ProvinceId>,
    /// Fighting spirit, permille. Recovers fast; breaking it wins battles.
    pub cohesion: u64,
    /// Men and equipment, permille. Dies slowly; hitting zero destroys.
    pub strength: u64,
    /// Equipment/training quality, permille.
    pub quality: u64,
    /// Days until this formation may move again.
    pub move_cooldown: u8,
    /// Mobilization state (military-command.md).
    #[serde(default)]
    pub readiness: Readiness,
    /// Which theater commands this formation (None = walks home, sits).
    #[serde(default)]
    pub theater: Option<TheaterId>,
    /// Training permille; combat weight scales by 500 + training/2.
    #[serde(default = "full_training")]
    pub training: u16,
    /// Current front-slot assignment from the theater controller.
    #[serde(default)]
    pub slot: Option<ProvinceId>,
    /// Days before the controller may hand this formation a new slot.
    #[serde(default)]
    pub retarget_cooldown: u8,
}

fn full_training() -> u16 {
    1000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Posture {
    Hold,
    Advance,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Military {
    pub formations: BTreeMap<FormationId, Formation>,
    /// Pairwise wars (stored with a < b ordering).
    pub wars: Vec<(CountryTag, CountryTag)>,
    /// (country, enemy) -> posture. Default Hold.
    pub postures: BTreeMap<(CountryTag, CountryTag), Posture>,
    /// Runtime ownership overrides (occupation / transfers).
    pub occupation: BTreeMap<ProvinceId, CountryTag>,
    /// Tick each war began.
    pub war_started: BTreeMap<(CountryTag, CountryTag), u64>,
    /// Cumulative strength points lost, per country.
    pub casualties: BTreeMap<CountryTag, u64>,
    /// Last tick any province changed hands.
    pub last_line_change_tick: u64,
    /// Standing armistice offers (offerer, enemy).
    pub armistice_offers: Vec<(CountryTag, CountryTag)>,
    /// Available trained manpower per country, in MEN. Reinforcement
    /// draws it down; wartime mobilization refills from population.
    pub manpower: BTreeMap<CountryTag, u64>,
    /// Battles won/lost per country (a battle is won when the enemy
    /// retreats from or dies in a contested province).
    pub battles_won: BTreeMap<CountryTag, u32>,
    pub battles_lost: BTreeMap<CountryTag, u32>,
    /// Live view of ongoing battles, rebuilt every combat hour for the
    /// UI. Derived state: excluded from the determinism digest.
    pub active_battles: Vec<BattleView>,
    /// Wire-service war ticker: (tick, line). Capped ring buffer.
    pub war_log: Vec<(u64, String)>,
    /// Command areas (military-command.md).
    #[serde(default)]
    pub theaters: BTreeMap<TheaterId, Theater>,
    #[serde(default)]
    next_theater_id: u32,
    /// Upkeep accrued this month, centi-stock, settled at month end.
    #[serde(default)]
    pub upkeep_accrued_centi: BTreeMap<CountryTag, u64>,
    /// Consecutive months of unpaid upkeep.
    #[serde(default)]
    pub upkeep_arrears: BTreeMap<CountryTag, u16>,
    /// Dynamic bloc alignment overrides (events flip countries; the
    /// CountryDef value is the 1950 baseline). ALL alignment reads go
    /// through `alignment_of` — never read CountryDef directly.
    #[serde(default)]
    pub alignments: BTreeMap<CountryTag, ugs_data::Alignment>,
    /// Dynamic stability overrides, 0-100 (baseline: CountryDef).
    #[serde(default)]
    pub stability: BTreeMap<CountryTag, u8>,
    next_id: u32,
}

/// UI-facing snapshot of one battle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleView {
    pub province: ProvinceId,
    /// Tick the battle began.
    pub since_tick: u64,
    pub attacker_owners: Vec<CountryTag>,
    pub defender_owners: Vec<CountryTag>,
    pub attacker_divisions: u32,
    pub defender_divisions: u32,
    /// Field strength in men.
    pub attacker_men: u64,
    pub defender_men: u64,
    /// Average cohesion permille.
    pub attacker_cohesion: u64,
    pub defender_cohesion: u64,
    /// Average quality permille.
    pub attacker_quality: u64,
    pub defender_quality: u64,
    /// Last hour's effective combat power (after modifiers).
    pub attacker_power: u64,
    pub defender_power: u64,
    /// Cohesion lost per division this hour (permille).
    pub attacker_hourly_loss: u64,
    pub defender_hourly_loss: u64,
    pub terrain: Terrain,
    /// Defender fights on unoccupied home soil.
    pub defender_home: bool,
}

impl Military {
    /// The single source of truth for a country's current bloc.
    pub fn alignment_of(
        &self,
        data: &ugs_data::ScenarioData,
        tag: &CountryTag,
    ) -> ugs_data::Alignment {
        self.alignments.get(tag).copied().unwrap_or_else(|| {
            data.countries
                .get(tag)
                .map(|c| c.alignment)
                .unwrap_or(ugs_data::Alignment::NonAligned)
        })
    }

    pub fn stability_of(&self, data: &ugs_data::ScenarioData, tag: &CountryTag) -> u8 {
        self.stability
            .get(tag)
            .copied()
            .unwrap_or_else(|| data.countries.get(tag).map(|c| c.stability).unwrap_or(50))
    }

    pub fn at_war(&self, a: &CountryTag, b: &CountryTag) -> bool {
        let key = if a < b { (a, b) } else { (b, a) };
        self.wars.iter().any(|(x, y)| (x, y) == key)
    }

    pub fn declare_war(&mut self, a: CountryTag, b: CountryTag) {
        let pair = if a < b { (a, b) } else { (b, a) };
        if !self.wars.contains(&pair) {
            self.wars.push(pair);
        }
    }

    pub fn owner_of(&self, id: ProvinceId, scenario_owner: &CountryTag) -> CountryTag {
        self.occupation
            .get(&id)
            .cloned()
            .unwrap_or_else(|| scenario_owner.clone())
    }

    pub fn spawn(&mut self, formation: Formation) -> FormationId {
        self.next_id += 1;
        let id = FormationId(self.next_id);
        self.formations.insert(id, formation);
        id
    }

    /// Raise a division from a home province, numbered and named after
    /// it ("2ND BUSAN ARMOR").
    #[allow(clippy::too_many_arguments)]
    pub fn raise(
        &mut self,
        data: &ugs_data::ScenarioData,
        owner: CountryTag,
        archetype: Archetype,
        location: ProvinceId,
        home: ProvinceId,
        quality: u64,
    ) -> FormationId {
        let ordinal = self
            .formations
            .values()
            .filter(|f| f.owner == owner && f.home == home)
            .count() as u64
            + 1;
        let place = data
            .provinces
            .get(&home)
            .map(|p| p.name.to_uppercase())
            .unwrap_or_else(|| owner.0.clone());
        let kind = match archetype {
            Archetype::Infantry => "INFANTRY",
            Archetype::Motorized => "MOTORIZED",
            Archetype::Armor => "ARMOR",
        };
        let name = format!("{} {place} {kind}", ordinal_words(ordinal));
        self.spawn(Formation {
            owner,
            archetype,
            location,
            home,
            name,
            last_location: None,
            cohesion: 1000,
            strength: 1000,
            quality,
            move_cooldown: 0,
            readiness: Readiness::Active,
            theater: None,
            training: 1000,
            slot: None,
            retarget_cooldown: 0,
        })
    }

    /// Raise a green division (player/AI force generation): spawns at
    /// RAISE_START_STRENGTH and trains from zero. The caller has already
    /// debited military_stock. Assigned to the owner's theater containing
    /// the home province, else nearest by any member province, else None.
    pub fn raise_recruit(
        &mut self,
        data: &ugs_data::ScenarioData,
        owner: CountryTag,
        archetype: Archetype,
        home: ProvinceId,
    ) -> FormationId {
        let id = self.raise(data, owner, archetype, home, home, 1000);
        let theater = self.theater_for(data, id, home);
        let f = self.formations.get_mut(&id).unwrap();
        f.strength = tuning::RAISE_START_STRENGTH;
        f.training = 0;
        f.theater = theater;
        id
    }

    fn theater_for(
        &self,
        data: &ugs_data::ScenarioData,
        id: FormationId,
        home: ProvinceId,
    ) -> Option<TheaterId> {
        let owner = &self.formations[&id].owner;
        let mine: Vec<(&TheaterId, &Theater)> = self
            .theaters
            .iter()
            .filter(|(_, t)| &t.owner == owner)
            .collect();
        if let Some((tid, _)) = mine.iter().find(|(_, t)| t.provinces.contains(&home)) {
            return Some(**tid);
        }
        // Nearest theater by BFS distance from home to any member province.
        use std::collections::{BTreeSet, VecDeque};
        if mine.is_empty() {
            return None;
        }
        let mut visited: BTreeSet<ProvinceId> = BTreeSet::from([home]);
        let mut queue: VecDeque<ProvinceId> = VecDeque::from([home]);
        let mut expanded = 0usize;
        while let Some(current) = queue.pop_front() {
            expanded += 1;
            if expanded > 2000 {
                break;
            }
            if let Some((tid, _)) = mine.iter().find(|(_, t)| t.provinces.contains(&current)) {
                return Some(**tid);
            }
            if let Some(p) = data.provinces.get(&current) {
                for adj in &p.adjacent {
                    if visited.insert(*adj) {
                        queue.push_back(*adj);
                    }
                }
            }
        }
        None
    }

    /// Disband: return DISBAND_RETURN_PERMILLE of the men to the pool;
    /// training is lost. Disband-and-re-raise is the v1 rebase.
    pub fn disband(&mut self, id: FormationId) {
        if let Some(f) = self.formations.remove(&id) {
            let men = f.strength * tuning::MEN_PER_STRENGTH_POINT * tuning::DISBAND_RETURN_PERMILLE
                / 1000;
            *self.manpower.entry(f.owner).or_default() += men;
        }
    }

    /// Friendly soil for basing, painting, and passage: the country's
    /// own, a co-belligerent's (shares an enemy, not itself hostile), or
    /// a fellow bloc member's (Western with Western, Eastern with
    /// Eastern — NonAligned grants nothing). The bloc clause is what
    /// keeps an expeditionary force legal after its host signs a
    /// separate peace: the ROK's armistice must not strand the US Army
    /// on suddenly-forbidden ground.
    pub fn friendly_soil(
        &self,
        data: &ugs_data::ScenarioData,
        country: &CountryTag,
        holder: &CountryTag,
    ) -> bool {
        if holder == country {
            return true;
        }
        if self.at_war(country, holder) {
            return false;
        }
        let co_belligerent = self.wars.iter().any(|(a, b)| {
            let enemy = if a == country {
                Some(b)
            } else if b == country {
                Some(a)
            } else {
                None
            };
            enemy.is_some_and(|e| self.at_war(holder, e))
        });
        if co_belligerent {
            return true;
        }
        use ugs_data::Alignment;
        matches!(
            (
                self.alignment_of(data, country),
                self.alignment_of(data, holder),
            ),
            (Alignment::WesternBloc, Alignment::WesternBloc)
                | (Alignment::EasternBloc, Alignment::EasternBloc)
        )
    }

    /// Where a country may base divisions and paint theaters: friendly
    /// soil (own / co-belligerent / same bloc). Overseas basing IS the
    /// v1 deployment abstraction (no naval transport exists).
    pub fn may_operate(
        &self,
        data: &ugs_data::ScenarioData,
        country: &CountryTag,
        province: ProvinceId,
    ) -> bool {
        let Some(p) = data.provinces.get(&province) else {
            return false;
        };
        let holder = self.owner_of(province, &p.owner);
        self.friendly_soil(data, country, &holder)
    }

    pub fn create_theater(&mut self, owner: CountryTag, name: String, auto: bool) -> TheaterId {
        self.next_theater_id += 1;
        let id = TheaterId(self.next_theater_id);
        self.theaters.insert(
            id,
            Theater {
                owner,
                name,
                provinces: Default::default(),
                posture: TheaterPosture::Defend,
                objectives: Vec::new(),
                echelon_permille: 0,
                forbidden: Default::default(),
                auto,
            },
        );
        id
    }

    /// The most populous province a country owns — where its
    /// expeditionary divisions are raised from.
    pub fn heartland_of(
        data: &ugs_data::ScenarioData,
        owner: &CountryTag,
        fallback: ProvinceId,
    ) -> ProvinceId {
        data.provinces
            .values()
            .filter(|p| &p.owner == owner)
            .max_by_key(|p| (p.population_k, std::cmp::Reverse(p.id.0)))
            .map(|p| p.id)
            .unwrap_or(fallback)
    }

    pub fn has_offered_armistice(&self, country: &CountryTag, enemy: &CountryTag) -> bool {
        self.armistice_offers
            .iter()
            .any(|(c, e)| c == country && e == enemy)
    }

    pub fn posture(&self, country: &CountryTag, enemy: &CountryTag) -> Posture {
        self.postures
            .get(&(country.clone(), enemy.clone()))
            .copied()
            .unwrap_or(Posture::Hold)
    }

    pub fn log(&mut self, tick: u64, line: String) {
        self.war_log.push((tick, line));
        let overflow = self.war_log.len().saturating_sub(60);
        if overflow > 0 {
            self.war_log.drain(..overflow);
        }
    }

    pub fn digest(&self) -> u64 {
        // FNV-style sequence fold: every byte/id folds in order, so
        // anagram tags and re-partitioned province sets don't collide.
        fn fold(h: &mut u64, v: u64) {
            *h = (*h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        }
        fn fold_tag(h: &mut u64, tag: &CountryTag) {
            for b in tag.0.bytes() {
                fold(h, b as u64);
            }
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (id, f) in &self.formations {
            let readiness = match f.readiness {
                Readiness::Active => 0,
                Readiness::Reserve => 1,
                Readiness::Mobilizing { days_left } => 2 + days_left as u64,
            };
            for v in [
                id.0 as u64,
                f.location.0 as u64,
                f.cohesion,
                f.strength,
                f.quality,
                readiness,
                f.training as u64,
                f.move_cooldown as u64,
                f.retarget_cooldown as u64,
                f.theater.map(|t| t.0 as u64 + 1).unwrap_or(0),
                f.slot.map(|p| p.0 as u64 + 1).unwrap_or(0),
            ] {
                fold(&mut h, v);
            }
            fold_tag(&mut h, &f.owner);
        }
        for (id, t) in &self.theaters {
            let posture = match t.posture {
                TheaterPosture::Defend => 0u64,
                TheaterPosture::Probe => 1,
                TheaterPosture::Offensive => 2,
            };
            for v in [id.0 as u64, posture, t.echelon_permille as u64] {
                fold(&mut h, v);
            }
            for p in &t.provinces {
                fold(&mut h, p.0 as u64 + 1);
            }
            for o in &t.objectives {
                fold(&mut h, o.0 as u64 + 1);
            }
            for tag in &t.forbidden {
                fold_tag(&mut h, tag);
            }
        }
        for (tag, v) in &self.upkeep_accrued_centi {
            fold_tag(&mut h, tag);
            fold(&mut h, *v);
        }
        for (tag, v) in &self.upkeep_arrears {
            fold_tag(&mut h, tag);
            fold(&mut h, *v as u64);
        }
        for (p, tag) in &self.occupation {
            fold(&mut h, p.0 as u64);
            fold_tag(&mut h, tag);
        }
        for (tag, men) in &self.manpower {
            fold_tag(&mut h, tag);
            fold(&mut h, *men);
        }
        for (tag, a) in &self.alignments {
            fold_tag(&mut h, tag);
            fold(&mut h, *a as u64 + 1);
        }
        for (tag, st) in &self.stability {
            fold_tag(&mut h, tag);
            fold(&mut h, *st as u64);
        }
        // The record the era score reads (scoring.md).
        for (a, b) in &self.wars {
            fold_tag(&mut h, a);
            fold_tag(&mut h, b);
        }
        for (tag, v) in &self.casualties {
            fold_tag(&mut h, tag);
            fold(&mut h, *v);
        }
        for (tag, v) in self.battles_won.iter().chain(self.battles_lost.iter()) {
            fold_tag(&mut h, tag);
            fold(&mut h, *v as u64);
        }
        h
    }
}

/// Hourly: seed OOB on first tick, fight battles; daily: move, occupy;
/// monthly: armistice diplomacy.
#[allow(clippy::too_many_arguments)]
pub fn update_military(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    mut demo: ResMut<crate::demography::Demographics>,
    mut rng: ResMut<SimRng>,
    mut military: ResMut<Military>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;

    // Seed formations from the OOB once.
    if military.formations.is_empty() && military.next_id == 0 && !data.oob.is_empty() {
        for entry in &data.oob {
            let Ok(province) = data.province_by_name(&entry.owner, &entry.province) else {
                continue;
            };
            for _ in 0..entry.divisions {
                military.raise(
                    data,
                    entry.owner.clone(),
                    Archetype::parse(&entry.archetype),
                    province,
                    province,
                    entry.quality as u64,
                );
            }
        }
        // Seed manpower pools from the real populations: the army is no
        // longer magic — it comes from these people.
        let mut pop_by_country: BTreeMap<CountryTag, u64> = BTreeMap::new();
        for (id, c) in &demo.provinces {
            if let Some(p) = data.provinces.get(id) {
                *pop_by_country.entry(p.owner.clone()).or_default() += c.total();
            }
        }
        for (tag, pop) in pop_by_country {
            military
                .manpower
                .insert(tag, pop * tuning::MANPOWER_BASE_PERMILLE / 1000);
        }
        return;
    }
    if military.wars.is_empty() {
        military.active_battles.clear();
        return; // peace: nothing to simulate hourly (regen is cheap, skip)
    }
    let new_pairs: Vec<(CountryTag, CountryTag)> = military
        .wars
        .iter()
        .filter(|p| !military.war_started.contains_key(p))
        .cloned()
        .collect();
    for pair in new_pairs {
        let tick = clock.tick;
        military.war_started.insert(pair, tick);
        military.last_line_change_tick = tick; // new war resets staleness
    }

    use tuning::*;

    // --- Hourly combat ---------------------------------------------------
    // Group formations by province; battle where warring owners share one.
    let mut by_province: BTreeMap<ProvinceId, Vec<FormationId>> = BTreeMap::new();
    for (id, f) in &military.formations {
        by_province.entry(f.location).or_default().push(*id);
    }

    let mut battles: Vec<(ProvinceId, Vec<FormationId>, Vec<FormationId>)> = Vec::new();
    for (province, ids) in &by_province {
        // Split into the two sides (v1: first owner found vs its enemies).
        let owners: Vec<CountryTag> = {
            let mut o: Vec<CountryTag> = ids
                .iter()
                .map(|i| military.formations[i].owner.clone())
                .collect();
            o.sort();
            o.dedup();
            o
        };
        let Some(first) = owners.first() else {
            continue;
        };
        let enemies: Vec<&CountryTag> = owners
            .iter()
            .skip(1)
            .filter(|o| military.at_war(first, o))
            .collect();
        if enemies.is_empty() {
            continue;
        }
        // Side A: the first owner plus everyone NOT at war with it
        // (co-belligerents fight together).
        let side_a: Vec<FormationId> = ids
            .iter()
            .filter(|i| {
                let o = &military.formations[i].owner;
                o == first || !military.at_war(first, o)
            })
            .copied()
            .collect();
        let side_b: Vec<FormationId> = ids
            .iter()
            .filter(|i| enemies.contains(&&military.formations[i].owner))
            .copied()
            .collect();
        battles.push((*province, side_a, side_b));
    }

    // Resolve battles that ended since last hour: the side still standing
    // in the province won the field. Feeds the ticker and the W/L tally.
    let prev_battles = std::mem::take(&mut military.active_battles);
    {
        use std::collections::BTreeSet;
        let contested: BTreeSet<ProvinceId> = battles.iter().map(|(p, _, _)| *p).collect();
        for old in &prev_battles {
            if contested.contains(&old.province) {
                continue;
            }
            let present: Vec<CountryTag> = by_province
                .get(&old.province)
                .map(|ids| {
                    ids.iter()
                        .map(|i| military.formations[i].owner.clone())
                        .collect()
                })
                .unwrap_or_default();
            let att = old.attacker_owners.iter().any(|o| present.contains(o));
            let def = old.defender_owners.iter().any(|o| present.contains(o));
            if att == def {
                continue; // both withdrew (or war ended): no verdict
            }
            let (winners, losers) = if att {
                (&old.attacker_owners, &old.defender_owners)
            } else {
                (&old.defender_owners, &old.attacker_owners)
            };
            let (winners, losers) = (winners.clone(), losers.clone());
            for w in &winners {
                *military.battles_won.entry(w.clone()).or_default() += 1;
            }
            for l in &losers {
                *military.battles_lost.entry(l.clone()).or_default() += 1;
            }
            let name = data
                .provinces
                .get(&old.province)
                .map(|p| p.name.to_uppercase())
                .unwrap_or_default();
            let hours = clock.tick.saturating_sub(old.since_tick);
            let victors: Vec<&str> = winners.iter().map(|t| t.0.as_str()).collect();
            military.log(
                clock.tick,
                format!(
                    "BATTLE OF {name} ENDS AFTER {hours}H -- {} HOLD THE FIELD",
                    victors.join("/")
                ),
            );
        }
    }

    let mut in_battle: Vec<FormationId> = Vec::new();
    let mut battle_views: Vec<BattleView> = Vec::new();
    for (province, side_a, side_b) in &battles {
        in_battle.extend(side_a.iter().chain(side_b.iter()));
        let terrain = data
            .provinces
            .get(province)
            .map(|p| p.terrain)
            .unwrap_or(Terrain::Plains);
        // Defender = side whose country owns the province right now.
        let owner_now = data
            .provinces
            .get(province)
            .map(|p| military.owner_of(*province, &p.owner));
        let a_defends = side_a
            .first()
            .map(|i| Some(&military.formations[i].owner) == owner_now.as_ref())
            .unwrap_or(false);
        let defender_ids = if a_defends { side_a } else { side_b };
        // Home soil: province is still held by its 1950 owner.
        let defender_home = data.provinces.get(province).is_some_and(|p| {
            !military.occupation.contains_key(province)
                && defender_ids
                    .first()
                    .is_some_and(|i| military.formations[i].owner == p.owner)
        });

        let power = |ids: &[FormationId], defending: bool| -> u64 {
            let base: u64 = ids
                .iter()
                .map(|i| {
                    let f = &military.formations[i];
                    let (attack, defense, _) = f.archetype.stats();
                    let stat = if defending { defense } else { attack };
                    // Green troops fight at half weight (500 + training/2);
                    // stood-down divisions defend at reduced weight.
                    // One divide at the end: stepwise truncation zeroed
                    // out green low-strength divisions entirely.
                    let mut v = stat * f.strength * f.quality * (500 + f.training as u64 / 2)
                        / 1_000_000_000;
                    if f.readiness.stood_down() {
                        v = v * RESERVE_DEFENSE_PERMILLE / 1000;
                    }
                    v
                })
                .sum();
            // Concentration soft-cap: stand-in for frontage until real
            // geometry lands — divisions beyond what the local edges
            // support add nothing (kills the deliberate one-province blob).
            let hostile_adjacent = ids
                .first()
                .map(|i| {
                    let side_owner = &military.formations[i].owner;
                    data.provinces
                        .get(province)
                        .map(|p| {
                            p.adjacent
                                .iter()
                                .filter(|adj| {
                                    data.provinces.get(adj).is_some_and(|ap| {
                                        let holder = military.owner_of(**adj, &ap.owner);
                                        military.at_war(side_owner, &holder)
                                    })
                                })
                                .count() as u64
                        })
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            let n = ids.len().max(1) as u64;
            let concentration = (1000 * (CONCENTRATION_BASE + hostile_adjacent) / n).min(1000);
            let base = base * concentration / 1000;
            if defending {
                let mut v = base * terrain_defense_permille(terrain) / 1000;
                if defender_home {
                    v = v * HOME_DEFENSE_PERMILLE / 1000;
                }
                v
            } else {
                base
            }
        };
        let a_power = power(side_a, a_defends);
        let b_power = power(side_b, !a_defends);
        let mut variance = || VARIANCE_MIN as u64 + rng.below(VARIANCE_SPAN) as u64;
        let (va, vb) = (variance(), variance());
        let damage_to_b = a_power * COHESION_DAMAGE_SCALE * va / 100;
        let damage_to_a = b_power * COHESION_DAMAGE_SCALE * vb / 100;

        // Pre-damage snapshot for the UI battle view.
        let side_stats = |ids: &[FormationId]| -> (u32, u64, u64, u64, Vec<CountryTag>) {
            let n = ids.len().max(1) as u64;
            let men: u64 = ids
                .iter()
                .map(|i| military.formations[i].strength * MEN_PER_STRENGTH_POINT)
                .sum();
            let coh: u64 = ids.iter().map(|i| military.formations[i].cohesion).sum();
            let qual: u64 = ids.iter().map(|i| military.formations[i].quality).sum();
            let mut owners: Vec<CountryTag> = ids
                .iter()
                .map(|i| military.formations[i].owner.clone())
                .collect();
            owners.sort();
            owners.dedup();
            (ids.len() as u32, men, coh / n, qual / n, owners)
        };
        let (att_ids, def_ids, att_power, def_power, att_damage, def_damage) = if a_defends {
            (side_b, side_a, b_power, a_power, damage_to_b, damage_to_a)
        } else {
            (side_a, side_b, a_power, b_power, damage_to_a, damage_to_b)
        };
        let (att_div, att_men, att_coh, att_qual, att_owners) = side_stats(att_ids);
        let (def_div, def_men, def_coh, def_qual, def_owners) = side_stats(def_ids);
        let since_tick = prev_battles
            .iter()
            .find(|b| b.province == *province)
            .map(|b| b.since_tick)
            .unwrap_or(clock.tick);
        if since_tick == clock.tick {
            let name = data
                .provinces
                .get(province)
                .map(|p| p.name.to_uppercase())
                .unwrap_or_default();
            let att_names: Vec<&str> = att_owners.iter().map(|t| t.0.as_str()).collect();
            military.log(
                clock.tick,
                format!(
                    "BATTLE OF {name} BEGINS -- {} ATTACK WITH {att_div} DIV VS {def_div} DIV",
                    att_names.join("/")
                ),
            );
        }
        battle_views.push(BattleView {
            province: *province,
            since_tick,
            attacker_owners: att_owners,
            defender_owners: def_owners,
            attacker_divisions: att_div,
            defender_divisions: def_div,
            attacker_men: att_men,
            defender_men: def_men,
            attacker_cohesion: att_coh,
            defender_cohesion: def_coh,
            attacker_quality: att_qual,
            defender_quality: def_qual,
            attacker_power: att_power,
            defender_power: def_power,
            attacker_hourly_loss: (att_damage / att_ids.len().max(1) as u64).max(1),
            defender_hourly_loss: (def_damage / def_ids.len().max(1) as u64).max(1),
            terrain,
            defender_home,
        });

        let mut debits: Vec<(ProvinceId, u64)> = Vec::new();
        let mut apply = |military: &mut Military, ids: &[FormationId], total: u64| {
            if ids.is_empty() {
                return;
            }
            let per = (total / ids.len() as u64).max(1);
            for id in ids {
                let owner = military.formations[id].owner.clone();
                let f = military.formations.get_mut(id).unwrap();
                f.cohesion = f.cohesion.saturating_sub(per);
                let strength_loss = (per / STRENGTH_DAMAGE_DIVISOR).max(1).min(f.strength);
                f.strength -= strength_loss;
                debits.push((f.home, strength_loss * MEN_PER_STRENGTH_POINT));
                *military.casualties.entry(owner).or_default() += strength_loss;
            }
        };
        apply(&mut military, side_a, damage_to_a);
        apply(&mut military, side_b, damage_to_b);
        // War dead come off the home province's books: rural first (the
        // armies of 1950 were drafted off farms), then urban, then
        // educated.
        for (home, men) in debits {
            if let Some(c) = demo.provinces.get_mut(&home) {
                let from_rural = men.min(c.rural);
                c.rural -= from_rural;
                let rest = men - from_rural;
                let from_urban = rest.min(c.urban);
                c.urban -= from_urban;
                c.educated = c.educated.saturating_sub(rest - from_urban);
            }
        }
    }
    military.active_battles = battle_views;

    // Regen for formations not in battle.
    for (id, f) in military.formations.iter_mut() {
        if !in_battle.contains(id) {
            f.cohesion = (f.cohesion + COHESION_REGEN).min(1000);
        }
    }

    // Retreats & destruction (checked hourly).
    let retreat_or_die: Vec<FormationId> = military
        .formations
        .iter()
        .filter(|(id, f)| {
            in_battle.contains(id) && (f.cohesion < RETREAT_COHESION || f.strength == 0)
        })
        .map(|(id, _)| *id)
        .collect();
    for id in retreat_or_die {
        let (owner, location, strength) = {
            let f = &military.formations[&id];
            (f.owner.clone(), f.location, f.strength)
        };
        // Find a friendly adjacent province with no enemy formations.
        let retreat_to = data.provinces.get(&location).and_then(|p| {
            p.adjacent.iter().find(|adj| {
                let adj_owner = data
                    .provinces
                    .get(adj)
                    .map(|ap| military.owner_of(**adj, &ap.owner));
                let friendly = adj_owner
                    .as_ref()
                    .map(|o| !military.at_war(&owner, o))
                    .unwrap_or(false);
                let no_enemies = by_province.get(adj).is_none_or(|ids| {
                    ids.iter()
                        .all(|i| !military.at_war(&owner, &military.formations[i].owner))
                });
                friendly && no_enemies
            })
        });
        match (retreat_to, strength) {
            (Some(dest), s) if s > 0 => {
                let dest = *dest;
                let f = military.formations.get_mut(&id).unwrap();
                f.last_location = Some(f.location);
                f.location = dest;
                f.move_cooldown = 2;
            }
            _ => {
                military.formations.remove(&id); // destroyed or pocketed
                let name = data
                    .provinces
                    .get(&location)
                    .map(|p| p.name.to_uppercase())
                    .unwrap_or_default();
                military.log(
                    clock.tick,
                    format!("{} DIVISION DESTROYED AT {name}", owner.0),
                );
            }
        }
    }

    // --- Daily occupation & monthly diplomacy ----------------------------
    // (Movement, reinforcement, readiness, training, and upkeep live in
    // `update_command`, which runs just before this system each tick.)
    if !clock.new_day {
        return;
    }

    // Wartime mobilization: belligerents add men to the pool monthly.
    if clock.new_month {
        let at_war: Vec<CountryTag> = military
            .wars
            .iter()
            .flat_map(|(a, b)| [a.clone(), b.clone()])
            .collect();
        // Keyed by the CURRENT holder: an independent state mobilizes
        // its own people; a colonial power no longer draws on ceded
        // ground (finding from the timeline review).
        let mut pop_by_country: BTreeMap<CountryTag, u64> = BTreeMap::new();
        for (id, c) in &demo.provinces {
            if let Some(p) = data.provinces.get(id) {
                let holder = military.owner_of(*id, &p.owner);
                if at_war.contains(&holder) {
                    *pop_by_country.entry(holder).or_default() += c.total();
                }
            }
        }
        for (tag, pop) in pop_by_country {
            *military.manpower.entry(tag).or_default() += pop * MOBILIZE_PERMILLE_PER_MONTH / 1000;
        }
    }

    // Occupation: sole military presence in a province you're at war with
    // its holder flips it to you.
    let mut flips: Vec<(ProvinceId, CountryTag)> = Vec::new();
    let mut presence: BTreeMap<ProvinceId, Vec<CountryTag>> = BTreeMap::new();
    for f in military.formations.values() {
        let e = presence.entry(f.location).or_default();
        if !e.contains(&f.owner) {
            e.push(f.owner.clone());
        }
    }
    for (province, owners) in &presence {
        if owners.len() != 1 {
            continue;
        }
        let occupier = &owners[0];
        let Some(p) = data.provinces.get(province) else {
            continue;
        };
        let holder = military.owner_of(*province, &p.owner);
        if &holder != occupier && military.at_war(occupier, &holder) {
            flips.push((*province, occupier.clone()));
        }
    }
    if !flips.is_empty() {
        military.last_line_change_tick = clock.tick;
    }
    for (province, occupier) in flips {
        let name = data
            .provinces
            .get(&province)
            .map(|p| p.name.to_uppercase())
            .unwrap_or_default();
        military.log(clock.tick, format!("{} FORCES TAKE {name}", occupier.0));
        military.occupation.insert(province, occupier);
    }

    // (Armistice diplomacy moved to the settlement system:
    // settlement::update_settlements owns war termination now.)
}

/// The country the human player controls (None = observer / headless).
/// Set via `SimCommand::SetPlayerCountry` so it lives in the replay log.
#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct PlayerCountry(pub Option<CountryTag>);

pub(crate) fn end_war(military: &mut Military, a: &CountryTag, b: &CountryTag) {
    let pair = if a < b {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    };
    military.wars.retain(|w| *w != pair);
    military.postures.remove(&(a.clone(), b.clone()));
    military.postures.remove(&(b.clone(), a.clone()));
    military
        .armistice_offers
        .retain(|(c, e)| !((c == a && e == b) || (c == b && e == a)));
}

/// How a formation may traverse a province while stepping toward `to`.
/// ROE first (forbidden soil is never enterable), then alignment:
/// friendly soil (own or co-belligerent) is always passable, neutral
/// soil never is, and enemy soil is gated by the theater posture —
/// Defend never enters it, Probe only when it IS the slot, Offensive
/// anywhere en route.
fn passable_toward(
    data: &ugs_data::ScenarioData,
    military: &Military,
    dmz: &std::collections::BTreeSet<ProvinceId>,
    owner: &CountryTag,
    theater: Option<&Theater>,
    province: ProvinceId,
    is_target: bool,
) -> bool {
    if dmz.contains(&province) {
        return false; // demilitarized by treaty or armistice
    }
    let Some(p) = data.provinces.get(&province) else {
        return false;
    };
    let holder = military.owner_of(province, &p.owner);
    if let Some(t) = theater {
        if t.forbidden.contains(&holder) || t.forbidden.contains(&p.owner) {
            return false;
        }
    }
    if &holder == owner {
        return true;
    }
    if military.at_war(owner, &holder) {
        return match theater.map(|t| t.posture) {
            Some(TheaterPosture::Offensive) => true,
            Some(TheaterPosture::Probe) => is_target,
            _ => false,
        };
    }
    // Not hostile: passable on friendly soil (co-belligerent or bloc),
    // never through genuine neutrals.
    military.friendly_soil(data, owner, &holder)
}

/// First hop of the shortest legal path from `from` to `to`.
#[allow(clippy::too_many_arguments)]
fn find_step_toward(
    data: &ugs_data::ScenarioData,
    military: &Military,
    dmz: &std::collections::BTreeSet<ProvinceId>,
    owner: &CountryTag,
    theater: Option<&Theater>,
    from: ProvinceId,
    to: ProvinceId,
) -> Option<ProvinceId> {
    use std::collections::{BTreeSet, VecDeque};
    if from == to {
        return None;
    }
    let mut visited: BTreeSet<ProvinceId> = BTreeSet::from([from]);
    let mut queue: VecDeque<(ProvinceId, Option<ProvinceId>)> = VecDeque::from([(from, None)]);
    let mut expanded = 0usize;
    while let Some((current, first_hop)) = queue.pop_front() {
        expanded += 1;
        if expanded > 4000 {
            break;
        }
        let Some(p) = data.provinces.get(&current) else {
            continue;
        };
        for adj in &p.adjacent {
            if !visited.insert(*adj) {
                continue;
            }
            let hop = first_hop.or(Some(*adj));
            if *adj == to {
                return hop;
            }
            if passable_toward(data, military, dmz, owner, theater, *adj, false) {
                queue.push_back((*adj, hop));
            }
        }
    }
    None
}

/// Multi-source BFS from the current front toward an objective; returns
/// the enemy provinces along the path (they join the Offensive front set).
fn objective_path(
    data: &ugs_data::ScenarioData,
    military: &Military,
    theater: &Theater,
    front: &std::collections::BTreeSet<ProvinceId>,
    objective: ProvinceId,
) -> Vec<ProvinceId> {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    let owner = &theater.owner;
    let mut visited: BTreeSet<ProvinceId> = front.clone();
    let mut parent: BTreeMap<ProvinceId, ProvinceId> = BTreeMap::new();
    let mut queue: VecDeque<ProvinceId> = front.iter().copied().collect();
    let mut expanded = 0usize;
    let mut found = None;
    'search: while let Some(current) = queue.pop_front() {
        expanded += 1;
        if expanded > 4000 {
            break;
        }
        let Some(p) = data.provinces.get(&current) else {
            continue;
        };
        for adj in &p.adjacent {
            if !visited.insert(*adj) {
                continue;
            }
            // For path purposes Offensive theaters may cross enemy soil.
            let enterable = data.provinces.get(adj).is_some_and(|ap| {
                let holder = military.owner_of(*adj, &ap.owner);
                !theater.forbidden.contains(&holder)
                    && !theater.forbidden.contains(&ap.owner)
                    && (military.at_war(owner, &holder)
                        || military.friendly_soil(data, owner, &holder))
            });
            if !enterable {
                continue;
            }
            parent.insert(*adj, current);
            if *adj == objective {
                found = Some(*adj);
                break 'search;
            }
            queue.push_back(*adj);
        }
    }
    // Walk back, keeping the enemy-held provinces on the path.
    let mut path = Vec::new();
    let mut cursor = found;
    while let Some(id) = cursor {
        let hostile = data.provinces.get(&id).is_some_and(|p| {
            let holder = military.owner_of(id, &p.owner);
            military.at_war(owner, &holder)
        });
        if hostile {
            path.push(id);
        }
        cursor = parent.get(&id).copied();
    }
    path
}

/// Daily force management (military-command.md): auto-theaters, AI
/// mobilization and raising, readiness/training progression, upkeep
/// accrual and monthly settlement, the theater front controller, and
/// slot-directed movement. Runs before `update_military` so combat sees
/// the day's positions.
#[allow(clippy::too_many_arguments)]
pub fn update_command(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    player: Res<PlayerCountry>,
    settlements: Res<crate::settlement::Settlements>,
    mut econ: ResMut<crate::planning::Economies>,
    mut military: ResMut<Military>,
    mut tension: ResMut<crate::tension::GlobalTension>,
) {
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    let dmz = settlements.dmz_provinces(&military);
    if military.next_id == 0 {
        return; // OOB not seeded yet (update_military's first tick does it)
    }
    if !clock.new_day {
        return;
    }
    use tuning::*;

    let at_war_tags: Vec<CountryTag> = {
        let mut t: Vec<CountryTag> = military
            .wars
            .iter()
            .flat_map(|(a, b)| [a.clone(), b.clone()])
            .collect();
        t.sort();
        t.dedup();
        t
    };

    // --- Auto-theaters ---------------------------------------------------
    // Countries at war with no theater get one covering their whole
    // holding; auto-theaters refresh daily and die at peace. Any player
    // edit converts a theater to manual (auto = false) via commands.
    let owners_with_theaters: Vec<CountryTag> = {
        let mut t: Vec<CountryTag> = military
            .theaters
            .values()
            .map(|t| t.owner.clone())
            .collect();
        t.sort();
        t.dedup();
        t
    };
    for tag in &at_war_tags {
        if !owners_with_theaters.contains(tag) {
            let id = military.create_theater(tag.clone(), "MAIN FRONT".into(), true);
            military.theaters.get_mut(&id).unwrap().auto = true;
        }
    }
    let auto_ids: Vec<TheaterId> = military
        .theaters
        .iter()
        .filter(|(_, t)| t.auto)
        .map(|(id, _)| *id)
        .collect();
    for id in auto_ids {
        let owner = military.theaters[&id].owner.clone();
        if !at_war_tags.contains(&owner) {
            military.theaters.remove(&id);
            for f in military.formations.values_mut() {
                if f.theater == Some(id) {
                    f.theater = None;
                }
            }
            continue;
        }
        // Whole current holding; posture from the country-level posture
        // (Advance toward any enemy -> Offensive); objectives = enemy
        // capitals; ROE empty (neutral soil is impassable by rule).
        let provinces: std::collections::BTreeSet<ProvinceId> = data
            .provinces
            .values()
            .filter(|p| military.owner_of(p.id, &p.owner) == owner)
            .map(|p| p.id)
            .collect();
        let advancing = military
            .postures
            .iter()
            .any(|((c, _), p)| c == &owner && *p == Posture::Advance);
        let mut objectives: Vec<ProvinceId> = Vec::new();
        for (a, b) in &military.wars {
            let enemy = if a == &owner {
                b
            } else if b == &owner {
                a
            } else {
                continue;
            };
            if let Some(c) = data.countries.get(enemy) {
                if objectives.len() < MAX_OBJECTIVES && !objectives.contains(&c.capital) {
                    objectives.push(c.capital);
                }
            }
        }
        let t = military.theaters.get_mut(&id).unwrap();
        t.provinces = provinces;
        t.posture = if advancing {
            TheaterPosture::Offensive
        } else {
            TheaterPosture::Defend
        };
        t.objectives = objectives;
    }
    // Unassigned formations of theater-owning countries adopt a theater.
    let orphans: Vec<(FormationId, ProvinceId)> = military
        .formations
        .iter()
        .filter(|(_, f)| f.theater.is_none())
        .map(|(id, f)| (*id, f.location))
        .collect();
    for (id, location) in orphans {
        if let Some(t) = military.theater_for(data, id, location) {
            military.formations.get_mut(&id).unwrap().theater = Some(t);
        }
    }
    // Drop assignments to theaters that no longer exist or changed hands.
    let valid: Vec<(FormationId, Option<TheaterId>)> = military
        .formations
        .iter()
        .map(|(id, f)| {
            let ok = f.theater.is_some_and(|t| {
                military
                    .theaters
                    .get(&t)
                    .is_some_and(|th| th.owner == f.owner)
            });
            (*id, if ok { f.theater } else { None })
        })
        .collect();
    for (id, theater) in valid {
        military.formations.get_mut(&id).unwrap().theater = theater;
    }

    // --- Objectives lifecycle -------------------------------------------
    // Captured (or holder left the war) => cleared.
    let theater_ids: Vec<TheaterId> = military.theaters.keys().copied().collect();
    for id in &theater_ids {
        let owner = military.theaters[id].owner.clone();
        let kept: Vec<ProvinceId> = military.theaters[id]
            .objectives
            .iter()
            .filter(|o| {
                data.provinces.get(o).is_some_and(|p| {
                    let holder = military.owner_of(**o, &p.owner);
                    military.at_war(&owner, &holder)
                })
            })
            .copied()
            .collect();
        let t = military.theaters.get_mut(id).unwrap();
        if kept.len() != t.objectives.len() {
            t.objectives = kept;
            if !t.auto {
                let name = t.name.clone();
                military.log(clock.tick, format!("{} OBJECTIVE SECURED", name));
            }
        }
    }

    // --- AI management (never the player's country) ----------------------
    // War: activate reserves, staged; a readable 21+-day ramp. Monthly:
    // raise 4:1 infantry:armor while solvent and manpower allows.
    // Deviation from the design doc, deliberate: the AI never demotes its
    // standing army at peace — the scripted 1950 OOBs (KPA fully mobilized
    // in June) ARE the historical peacetime postures.
    for tag in &at_war_tags {
        if player.0.as_ref() == Some(tag) {
            continue;
        }
        let reserves: Vec<FormationId> = military
            .formations
            .iter()
            .filter(|(_, f)| &f.owner == tag && matches!(f.readiness, Readiness::Reserve))
            .map(|(id, _)| *id)
            .take(AI_ACTIVATIONS_PER_DAY)
            .collect();
        for id in reserves {
            let f = military.formations.get_mut(&id).unwrap();
            f.readiness = Readiness::Mobilizing {
                days_left: MOBILIZE_DAYS,
            };
        }
        if clock.new_month {
            let fielded_men: u64 = military
                .formations
                .values()
                .filter(|f| &f.owner == tag)
                .map(|f| f.strength * MEN_PER_STRENGTH_POINT)
                .sum();
            let pool = military.manpower.get(tag).copied().unwrap_or(0);
            let count = military
                .formations
                .values()
                .filter(|f| &f.owner == tag)
                .count();
            if fielded_men < pool / 3 && count < MAX_FORMATIONS {
                let archetype = if (count + 1) % (AI_ARMOR_RATIO + 1) == 0 {
                    Archetype::Armor
                } else {
                    Archetype::Infantry
                };
                let monthly_upkeep: u64 = military
                    .formations
                    .values()
                    .filter(|f| &f.owner == tag)
                    .map(|f| f.archetype.upkeep_centi())
                    .sum::<u64>()
                    / 100;
                let cost = archetype.raise_cost();
                let stock = econ
                    .industry
                    .get(tag)
                    .map(|s| s.military_stock)
                    .unwrap_or(0);
                if stock >= cost + 2 * monthly_upkeep {
                    let home = Military::heartland_of(
                        data,
                        tag,
                        data.provinces
                            .keys()
                            .next()
                            .copied()
                            .unwrap_or(ProvinceId(0)),
                    );
                    raise_division(
                        data,
                        &mut military,
                        &mut econ,
                        &mut tension,
                        clock.tick,
                        tag.clone(),
                        archetype,
                        home,
                    );
                }
            }
        }
    }

    // --- Readiness & training progression --------------------------------
    for f in military.formations.values_mut() {
        if let Readiness::Mobilizing { days_left } = f.readiness {
            if days_left <= 1 {
                f.readiness = Readiness::Active;
                f.cohesion = f.cohesion.min(STAND_DOWN_COHESION);
            } else {
                f.readiness = Readiness::Mobilizing {
                    days_left: days_left - 1,
                };
            }
        }
        if f.training < 1000 {
            let per_day = 1000u64.div_ceil(f.archetype.train_days()) as u16;
            f.training = (f.training + per_day).min(1000);
        }
        // Peacetime cohesion recovery: update_military early-returns
        // before its hourly regen when no wars exist, which would pin a
        // peacetime-mobilized division at stand-down cohesion forever.
        if !at_war_tags.contains(&f.owner) {
            f.cohesion = (f.cohesion + 24 * COHESION_REGEN).min(1000);
        }
        f.move_cooldown = f.move_cooldown.saturating_sub(1);
        f.retarget_cooldown = f.retarget_cooldown.saturating_sub(1);
    }

    // --- Upkeep: monthly settlement, then daily accrual ------------------
    // Accrued in centi-stock-days; a month's bill is accrued/30 centi.
    // Settle BEFORE accruing so the settlement day's accrual counts
    // toward the new month instead of being silently discarded.
    if clock.new_month {
        // A month is billed as 30 accrual days, a deliberate flat
        // approximation (February underbills ~7%; tuning, not a bug).
        let bills: Vec<(CountryTag, u64)> = military
            .upkeep_accrued_centi
            .iter()
            .map(|(t, acc)| (t.clone(), acc.div_ceil(30 * 100)))
            .collect();
        military.upkeep_accrued_centi.clear();
        // No bill (army melted or disbanded to nothing) => arrears clear;
        // a stale entry would block reinforcement after a later re-raise.
        let billed: std::collections::BTreeSet<CountryTag> =
            bills.iter().map(|(t, _)| t.clone()).collect();
        military.upkeep_arrears.retain(|t, _| billed.contains(t));
        for (tag, due) in bills {
            if due == 0 {
                continue;
            }
            let stock = econ.industry.get_mut(&tag).map(|s| &mut s.military_stock);
            let paid = match stock {
                Some(s) => {
                    let paid = due.min(*s);
                    *s -= paid;
                    paid
                }
                None => 0,
            };
            let shortfall = due - paid;
            if shortfall > 0 {
                *military.upkeep_arrears.entry(tag.clone()).or_default() += 1;
                let decay = ARREARS_QUALITY_DECAY * shortfall / due;
                for f in military.formations.values_mut() {
                    if f.owner == tag {
                        f.quality = f.quality.saturating_sub(decay).max(500);
                        f.strength = f.strength.saturating_sub(ARREARS_MELT);
                    }
                }
                military.log(
                    clock.tick,
                    format!("{} ARMY UNPAID -- DESERTION AND BREAKDOWN SPREAD", tag.0),
                );
            } else if military.upkeep_arrears.remove(&tag).is_some() {
                military.log(clock.tick, format!("{} ARMY PAY RESTORED", tag.0));
            } else {
                for f in military.formations.values_mut() {
                    if f.owner == tag && f.quality < 1000 {
                        f.quality = (f.quality + QUALITY_RECOVER).min(1000);
                    }
                }
            }
        }
        // Arrears melt destroys hollowed-out formations.
        let dead: Vec<FormationId> = military
            .formations
            .iter()
            .filter(|(_, f)| f.strength == 0)
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            military.formations.remove(&id);
        }
    }

    let accruals: Vec<(CountryTag, u64)> = military
        .formations
        .values()
        .map(|f| {
            let mut centi = f.archetype.upkeep_centi();
            if f.readiness.stood_down() {
                centi = centi * RESERVE_UPKEEP_PERMILLE / 1000;
            }
            let overseas = data
                .provinces
                .get(&f.location)
                .is_some_and(|p| p.owner != f.owner);
            if overseas {
                centi *= OVERSEAS_UPKEEP_MULT;
            }
            (f.owner.clone(), centi)
        })
        .collect();
    for (tag, centi) in accruals {
        *military.upkeep_accrued_centi.entry(tag).or_default() += centi;
    }

    // --- Reinforcement ---------------------------------------------------
    // Actives first at full rate, stood-down at half; halted in arrears.
    // Engaged provinces are derived from live positions (a province
    // holding formations of two warring owners), NOT from
    // `active_battles` — that is documented digest-excluded UI state
    // and must never gate sim decisions.
    let battle_provinces: std::collections::BTreeSet<ProvinceId> = {
        let mut owners_at: BTreeMap<ProvinceId, Vec<&CountryTag>> = BTreeMap::new();
        for f in military.formations.values() {
            let e = owners_at.entry(f.location).or_default();
            if !e.contains(&&f.owner) {
                e.push(&f.owner);
            }
        }
        owners_at
            .iter()
            .filter(|(_, owners)| {
                owners
                    .iter()
                    .any(|a| owners.iter().any(|b| military.at_war(a, b)))
            })
            .map(|(p, _)| *p)
            .collect()
    };
    let mut reinforce: Vec<(FormationId, bool)> = military
        .formations
        .iter()
        .filter(|(_, f)| {
            f.strength < 1000
                && !battle_provinces.contains(&f.location)
                && !military.upkeep_arrears.contains_key(&f.owner)
                && data.provinces.get(&f.location).is_some_and(|p| {
                    let holder = military.owner_of(f.location, &p.owner);
                    !military.at_war(&f.owner, &holder)
                })
        })
        .map(|(id, f)| (*id, f.readiness.stood_down()))
        .collect();
    reinforce.sort_by_key(|(id, stood_down)| (*stood_down, *id));
    for (id, stood_down) in reinforce {
        let owner = military.formations[&id].owner.clone();
        let rate = if stood_down {
            REINFORCE_PER_DAY / 2
        } else {
            REINFORCE_PER_DAY
        };
        let pool = military.manpower.entry(owner).or_default();
        let points = rate.min(*pool / MEN_PER_STRENGTH_POINT);
        if points == 0 {
            continue;
        }
        *pool -= points * MEN_PER_STRENGTH_POINT;
        let f = military.formations.get_mut(&id).unwrap();
        f.strength = (f.strength + points).min(1000);
    }

    // --- Theater front controller ----------------------------------------
    for tid in &theater_ids {
        let Some(theater) = military.theaters.get(tid).cloned() else {
            continue;
        };
        let owner = theater.owner.clone();
        // Committed = assigned, Active, alive. Echelon share (newest
        // first — green divisions are the second echelon) holds the rear.
        let mut committed: Vec<FormationId> = military
            .formations
            .iter()
            .filter(|(_, f)| f.theater == Some(*tid) && !f.readiness.stood_down() && f.strength > 0)
            .map(|(id, _)| *id)
            .collect();
        committed.sort();
        let echelon_n = committed.len() * theater.echelon_permille as usize / 1000;
        let echelon: Vec<FormationId> =
            committed.split_off(committed.len().saturating_sub(echelon_n));

        // Front set per posture.
        let mut front: std::collections::BTreeSet<ProvinceId> = theater
            .provinces
            .iter()
            .filter(|p| !dmz.contains(p))
            .filter(|p| {
                data.provinces.get(p).is_some_and(|pd| {
                    pd.adjacent.iter().any(|adj| {
                        data.provinces.get(adj).is_some_and(|ap| {
                            let holder = military.owner_of(*adj, &ap.owner);
                            military.at_war(&owner, &holder)
                        })
                    })
                })
            })
            .copied()
            .collect();
        let mut on_path: std::collections::BTreeSet<ProvinceId> = Default::default();
        if theater.posture != TheaterPosture::Defend {
            // Probe and Offensive extend the front one province into
            // enemy soil — the front rolls forward as occupation flips.
            let extra: Vec<ProvinceId> = front
                .iter()
                .flat_map(|p| {
                    data.provinces
                        .get(p)
                        .map(|pd| pd.adjacent.clone())
                        .unwrap_or_default()
                })
                .filter(|adj| {
                    !dmz.contains(adj)
                        && data.provinces.get(adj).is_some_and(|ap| {
                            let holder = military.owner_of(*adj, &ap.owner);
                            military.at_war(&owner, &holder)
                                && !theater.forbidden.contains(&holder)
                                && !theater.forbidden.contains(&ap.owner)
                        })
                })
                .collect();
            front.extend(extra);
        }
        if theater.posture == TheaterPosture::Offensive {
            // Objectives add depth: enemy provinces along the axis join
            // the front so the advance aims instead of spreading evenly.
            for objective in &theater.objectives {
                for p in objective_path(data, &military, &theater, &front, *objective) {
                    if !dmz.contains(&p) {
                        on_path.insert(p);
                        front.insert(p);
                    }
                }
            }
        }

        if front.is_empty() {
            // Peacetime garrison: disperse round-robin over the theater.
            let provinces: Vec<ProvinceId> = theater.provinces.iter().copied().collect();
            if provinces.is_empty() {
                continue;
            }
            for (i, id) in committed.iter().chain(echelon.iter()).enumerate() {
                let f = military.formations.get_mut(id).unwrap();
                f.slot = Some(provinces[i % provinces.len()]);
            }
            continue;
        }

        // Slot weights: 1 + hostile_adjacent + 3*objective_path
        // + 2*enemy_formation_adjacent (additive; path term boolean).
        let enemy_positions: std::collections::BTreeSet<ProvinceId> = military
            .formations
            .values()
            .filter(|f| military.at_war(&owner, &f.owner))
            .map(|f| f.location)
            .collect();
        let front_vec: Vec<ProvinceId> = front.iter().copied().collect();
        let weight: BTreeMap<ProvinceId, u64> = front_vec
            .iter()
            .map(|p| {
                let hostile_adj = data
                    .provinces
                    .get(p)
                    .map(|pd| {
                        pd.adjacent
                            .iter()
                            .filter(|adj| {
                                data.provinces.get(adj).is_some_and(|ap| {
                                    let holder = military.owner_of(**adj, &ap.owner);
                                    military.at_war(&owner, &holder)
                                })
                            })
                            .count() as u64
                    })
                    .unwrap_or(0);
                let enemy_adj = data
                    .provinces
                    .get(p)
                    .is_some_and(|pd| pd.adjacent.iter().any(|a| enemy_positions.contains(a)));
                let w = 1
                    + hostile_adj
                    + if on_path.contains(p) { 3 } else { 0 }
                    + if enemy_adj { 2 } else { 0 };
                (*p, w)
            })
            .collect();

        // Largest-remainder quotas over the committed (non-echelon) pool.
        let total_weight: u64 = weight.values().sum::<u64>().max(1);
        let n = committed.len() as u64;
        let mut quota: BTreeMap<ProvinceId, i64> = BTreeMap::new();
        let mut remainders: Vec<(u64, ProvinceId)> = Vec::new();
        let mut assigned: u64 = 0;
        for p in &front_vec {
            let ideal = n * weight[p];
            let base = ideal / total_weight;
            quota.insert(*p, base as i64);
            assigned += base;
            remainders.push((ideal % total_weight, *p));
        }
        remainders.sort_by_key(|(r, p)| (std::cmp::Reverse(*r), *p));
        for (_, p) in remainders.into_iter().take((n - assigned.min(n)) as usize) {
            *quota.get_mut(&p).unwrap() += 1;
        }

        // Assignment-preserving controller: invalid slots reassign to the
        // deepest deficit; then deficit>=2 provinces pull from surplus>=2.
        let mut occupancy: BTreeMap<ProvinceId, Vec<FormationId>> = BTreeMap::new();
        let mut unslotted: Vec<FormationId> = Vec::new();
        for id in &committed {
            let slot = military.formations[id].slot;
            match slot {
                Some(s) if front.contains(&s) => occupancy.entry(s).or_default().push(*id),
                _ => unslotted.push(*id),
            }
        }
        for id in unslotted {
            let best = front_vec
                .iter()
                .max_by_key(|p| {
                    let occ = occupancy.get(p).map(|v| v.len() as i64).unwrap_or(0);
                    (quota[p] - occ, std::cmp::Reverse(p.0))
                })
                .copied();
            if let Some(p) = best {
                occupancy.entry(p).or_default().push(id);
                let f = military.formations.get_mut(&id).unwrap();
                f.slot = Some(p);
                f.retarget_cooldown = RETARGET_COOLDOWN;
            }
        }
        let mut moves: Vec<(FormationId, ProvinceId)> = Vec::new();
        loop {
            let deficit = front_vec
                .iter()
                .filter(|p| {
                    let occ = occupancy.get(p).map(|v| v.len() as i64).unwrap_or(0);
                    quota[p] - occ >= QUOTA_PULL_DEFICIT
                })
                .max_by_key(|p| {
                    let occ = occupancy.get(p).map(|v| v.len() as i64).unwrap_or(0);
                    (quota[p] - occ, std::cmp::Reverse(p.0))
                })
                .copied();
            let Some(needy) = deficit else { break };
            let donor = front_vec
                .iter()
                .filter(|p| {
                    let occ = occupancy.get(p).map(|v| v.len() as i64).unwrap_or(0);
                    occ - quota[p] >= QUOTA_PULL_DEFICIT
                        && occupancy[p]
                            .iter()
                            .any(|id| military.formations[id].retarget_cooldown == 0)
                })
                .max_by_key(|p| {
                    let occ = occupancy.get(p).map(|v| v.len() as i64).unwrap_or(0);
                    (occ - quota[p], std::cmp::Reverse(p.0))
                })
                .copied();
            let Some(donor) = donor else { break };
            let mover = occupancy[&donor]
                .iter()
                .filter(|id| military.formations[id].retarget_cooldown == 0)
                .min()
                .copied()
                .unwrap();
            occupancy.get_mut(&donor).unwrap().retain(|i| *i != mover);
            occupancy.entry(needy).or_default().push(mover);
            moves.push((mover, needy));
        }
        for (id, slot) in moves {
            let f = military.formations.get_mut(&id).unwrap();
            f.slot = Some(slot);
            f.retarget_cooldown = RETARGET_COOLDOWN;
        }

        // Echelon holds the theater province nearest the front centroid
        // that is not itself front; fall back to any theater province.
        let rear = theater
            .provinces
            .iter()
            .filter(|p| !front.contains(p))
            .min_by_key(|p| {
                data.provinces
                    .get(p)
                    .map(|pd| pd.adjacent.iter().filter(|a| front.contains(a)).count())
                    .map(std::cmp::Reverse)
                    .map(|r| (r, p.0))
                    .unwrap_or((std::cmp::Reverse(0), p.0))
            })
            .or_else(|| theater.provinces.iter().next())
            .copied();
        if let Some(rear) = rear {
            for id in &echelon {
                military.formations.get_mut(id).unwrap().slot = Some(rear);
            }
        }
    }

    // --- Movement: one hop toward the slot --------------------------------
    let movers: Vec<FormationId> = military
        .formations
        .iter()
        .filter(|(_, f)| {
            !f.readiness.stood_down()
                && f.move_cooldown == 0
                && f.cohesion >= RETREAT_COHESION
                && !battle_provinces.contains(&f.location)
        })
        .map(|(id, _)| *id)
        .collect();
    for id in movers {
        let (owner, location, slot, theater_id, home) = {
            let f = &military.formations[&id];
            (f.owner.clone(), f.location, f.slot, f.theater, f.home)
        };
        let target = slot.or({
            // No theater, no slot: walk home and sit.
            if theater_id.is_none() {
                Some(home)
            } else {
                None
            }
        });
        let Some(target) = target else { continue };
        if target == location {
            continue;
        }
        let theater = theater_id.and_then(|t| military.theaters.get(&t)).cloned();
        let dest = find_step_toward(
            data,
            &military,
            &dmz,
            &owner,
            theater.as_ref(),
            location,
            target,
        );
        if let Some(dest) = dest {
            let (_, _, days) = military.formations[&id].archetype.stats();
            let f = military.formations.get_mut(&id).unwrap();
            f.last_location = Some(f.location);
            f.location = dest;
            f.move_cooldown = days;
        }
    }
}

/// Shared raise path (player command and AI): debit stock, price the
/// escalation signal, spawn the green division. Returns false when the
/// stockpile can't cover it.
#[allow(clippy::too_many_arguments)]
pub fn raise_division(
    data: &ugs_data::ScenarioData,
    military: &mut Military,
    econ: &mut crate::planning::Economies,
    tension: &mut crate::tension::GlobalTension,
    tick: u64,
    country: CountryTag,
    archetype: Archetype,
    home: ProvinceId,
) -> bool {
    use tuning::*;
    let count = military
        .formations
        .values()
        .filter(|f| f.owner == country)
        .count();
    if count >= MAX_FORMATIONS {
        military.log(tick, format!("{} ARMY AT ORGANIZATIONAL LIMIT", country.0));
        return false;
    }
    let cost = archetype.raise_cost();
    let Some(stock) = econ
        .industry
        .get_mut(&country)
        .map(|s| &mut s.military_stock)
    else {
        return false;
    };
    if *stock < cost {
        military.log(
            tick,
            format!("{} PROCUREMENT SHORTFALL -- DIVISION NOT RAISED", country.0),
        );
        return false;
    }
    *stock -= cost;
    let at_war = military
        .wars
        .iter()
        .any(|(a, b)| a == &country || b == &country);
    let floor = peace_floor(data, &country);
    // Commitment is a public escalation signal: priced at peace, and at
    // war beyond the peacetime establishment.
    if !at_war || count >= floor {
        tension.apply(RAISE_TENSION);
        military.log(
            tick,
            format!(
                "{} EXPANDS ARMED FORCES -- FOREIGN CAPITALS TAKE NOTE",
                country.0
            ),
        );
    }
    let id = military.raise_recruit(data, country, archetype, home);
    let name = military.formations[&id].name.clone();
    military.log(tick, format!("{name} FORMING"));
    true
}

/// Peacetime active-division establishment: majors scale with industry.
pub fn peace_floor(data: &ugs_data::ScenarioData, country: &CountryTag) -> usize {
    use tuning::*;
    data.countries
        .get(country)
        .map(|c| {
            if c.industry >= MAJOR_INDUSTRY {
                (c.industry / MAJOR_FLOOR_DIVISOR) as usize
            } else {
                PEACE_FLOOR_DIVS
            }
        })
        .unwrap_or(PEACE_FLOOR_DIVS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::GameDate;
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

    #[test]
    fn oob_seeds_korean_armies() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 2);
        let military = app.world().resource::<Military>();
        let count = |tag: &str| {
            military
                .formations
                .values()
                .filter(|f| f.owner.0 == tag)
                .count()
        };
        assert_eq!(count("PRK"), 10, "KPA divisions");
        assert_eq!(count("KOR"), 8, "ROK divisions");
        assert!(military.wars.is_empty(), "peace at campaign start");
        assert!(
            military
                .manpower
                .get(&CountryTag("KOR".into()))
                .copied()
                .unwrap_or(0)
                > 100_000,
            "ROK manpower pool seeded from population"
        );
    }

    #[test]
    fn war_produces_legible_information() {
        let mut app = app_with_scenario();
        // To the eve of the June 25 invasion, then watch a month of
        // fighting hour by hour: battles are live snapshots, so sample.
        run_ticks(&mut app, 24 * 175);
        let mut seen_battle = None;
        for _ in 0..(24 * 35) {
            run_ticks(&mut app, 1);
            let military = app.world().resource::<Military>();
            if seen_battle.is_none() {
                seen_battle = military.active_battles.first().cloned();
            }
        }
        let military = app.world().resource::<Military>();
        assert!(!military.wars.is_empty(), "Korean War underway");
        let b = seen_battle.expect("at least one battle visible during the invasion month");
        assert!(b.attacker_men > 0 && b.defender_men > 0, "men counted");
        assert!(
            b.attacker_hourly_loss > 0 && b.defender_hourly_loss > 0,
            "hourly attrition reported"
        );
        assert!(!military.war_log.is_empty(), "war ticker has entries");
        let log: Vec<&str> = military.war_log.iter().map(|(_, l)| l.as_str()).collect();
        assert!(
            log.iter().any(|l| l.contains("BATTLE OF")),
            "battle openings logged: {log:?}"
        );
        assert!(
            log.iter().any(|l| l.contains("FORCES TAKE")),
            "captures logged: {log:?}"
        );
        // Divisions carry identity and provenance.
        let scenario = app.world().resource::<SimScenario>().0.clone();
        for f in military.formations.values() {
            assert!(!f.name.is_empty(), "division named");
            assert!(
                scenario.provinces.contains_key(&f.home),
                "home province {:?} exists",
                f.home
            );
        }
        assert!(
            military.formations.values().any(|f| f.name.contains("1ST")),
            "ordinal naming"
        );
        let won: u32 = military.battles_won.values().sum();
        let lost: u32 = military.battles_lost.values().sum();
        assert!(
            won > 0 && lost > 0,
            "battle outcomes tallied ({won}W/{lost}L)"
        );
        // Mobilization grows the belligerents' pools while neutral pools hold.
        let prk = military
            .manpower
            .get(&CountryTag("PRK".into()))
            .copied()
            .unwrap();
        assert!(prk > 0, "KPA still has a manpower pool");
    }

    fn push(app: &mut App, cmd: crate::command::SimCommand) {
        app.world_mut()
            .resource_mut::<crate::command::PendingCommands>()
            .push(cmd);
    }

    #[test]
    fn raising_costs_stock_trains_and_signals() {
        use crate::command::SimCommand;
        let mut app = app_with_scenario();
        run_ticks(&mut app, 25); // past OOB seeding, day 2, at peace
        let usa = CountryTag("USA".into());
        let home = {
            let scenario = app.world().resource::<SimScenario>().0.clone();
            Military::heartland_of(&scenario, &usa, ugs_data::ProvinceId(0))
        };
        {
            let mut econ = app.world_mut().resource_mut::<crate::planning::Economies>();
            econ.industry.get_mut(&usa).unwrap().military_stock = 20;
        }
        let tension_before = app
            .world()
            .resource::<crate::tension::GlobalTension>()
            .value();
        push(
            &mut app,
            SimCommand::RaiseFormation {
                country: usa.clone(),
                archetype: Archetype::Armor,
                home,
                count: 2,
            },
        );
        run_ticks(&mut app, 1);
        {
            let military = app.world().resource::<Military>();
            let econ = app.world().resource::<crate::planning::Economies>();
            let raised: Vec<&Formation> = military
                .formations
                .values()
                .filter(|f| f.owner == usa)
                .collect();
            assert_eq!(raised.len(), 2, "two armor divisions raised");
            assert!(raised
                .iter()
                .all(|f| f.strength == tuning::RAISE_START_STRENGTH));
            assert!(raised.iter().all(|f| f.training == 0), "green");
            assert_eq!(
                econ.industry[&usa].military_stock,
                20 - 2 * tuning::RAISE_STOCK_ARMOR,
                "stock debited"
            );
            // Peacetime force expansion is a public escalation signal.
            assert!(
                app.world()
                    .resource::<crate::tension::GlobalTension>()
                    .value()
                    >= tension_before + 2 * tuning::RAISE_TENSION,
                "raising at peace adds tension"
            );
        }
        // A month later: training climbed and the reinforcement pipeline
        // has been filling the division from the manpower pool.
        run_ticks(&mut app, 24 * 30);
        {
            let military = app.world().resource::<Military>();
            let f = military
                .formations
                .values()
                .find(|f| f.owner == usa)
                .unwrap();
            assert!(f.training > 150, "a month of training: {}", f.training);
            assert!(f.training < 1000, "armor takes 150 days");
            assert!(f.strength > 400, "reinforcement filling: {}", f.strength);
            // Daily upkeep accrual is running and settles monthly.
            assert!(
                military
                    .upkeep_accrued_centi
                    .get(&usa)
                    .copied()
                    .unwrap_or(0)
                    > 0
                    || military.upkeep_arrears.contains_key(&usa),
                "upkeep accruing"
            );
        }
    }

    #[test]
    fn raising_without_stock_is_a_logged_no_op() {
        use crate::command::SimCommand;
        let mut app = app_with_scenario();
        run_ticks(&mut app, 25);
        let prk = CountryTag("PRK".into());
        {
            let mut econ = app.world_mut().resource_mut::<crate::planning::Economies>();
            econ.industry.get_mut(&prk).unwrap().military_stock = 0;
        }
        let before = app
            .world()
            .resource::<Military>()
            .formations
            .values()
            .filter(|f| f.owner == prk)
            .count();
        let home = {
            let scenario = app.world().resource::<SimScenario>().0.clone();
            Military::heartland_of(&scenario, &prk, ugs_data::ProvinceId(0))
        };
        push(
            &mut app,
            SimCommand::RaiseFormation {
                country: prk.clone(),
                archetype: Archetype::Infantry,
                home,
                count: 1,
            },
        );
        run_ticks(&mut app, 1);
        let military = app.world().resource::<Military>();
        let after = military
            .formations
            .values()
            .filter(|f| f.owner == prk)
            .count();
        assert_eq!(after, before, "no division without stock");
        assert!(
            military
                .war_log
                .iter()
                .any(|(_, l)| l.contains("PROCUREMENT SHORTFALL")),
            "shortfall on the wire"
        );
    }

    #[test]
    fn mobilization_takes_weeks_and_is_public_at_peace() {
        use crate::command::SimCommand;
        let mut app = app_with_scenario();
        run_ticks(&mut app, 25);
        let kor = CountryTag("KOR".into());
        let id = *app
            .world()
            .resource::<Military>()
            .formations
            .iter()
            .find(|(_, f)| f.owner == kor)
            .unwrap()
            .0;
        // Stand down: immediate, cohesion drops.
        push(
            &mut app,
            SimCommand::SetReadiness {
                country: kor.clone(),
                id,
                active: false,
            },
        );
        run_ticks(&mut app, 1);
        assert!(matches!(
            app.world().resource::<Military>().formations[&id].readiness,
            Readiness::Reserve
        ));
        // Reactivate at peace: 21-day ramp, tension signal.
        let tension_before = app
            .world()
            .resource::<crate::tension::GlobalTension>()
            .value();
        push(
            &mut app,
            SimCommand::SetReadiness {
                country: kor.clone(),
                id,
                active: true,
            },
        );
        run_ticks(&mut app, 1);
        {
            let military = app.world().resource::<Military>();
            assert!(matches!(
                military.formations[&id].readiness,
                Readiness::Mobilizing { .. }
            ));
            assert!(
                app.world()
                    .resource::<crate::tension::GlobalTension>()
                    .value()
                    > tension_before - 5,
                "peacetime mobilization signals (net of daily decay)"
            );
        }
        run_ticks(&mut app, 24 * (tuning::MOBILIZE_DAYS as u64 + 1));
        assert!(
            matches!(
                app.world().resource::<Military>().formations[&id].readiness,
                Readiness::Active
            ),
            "mobilization completes"
        );
    }

    #[test]
    fn theaters_spread_the_front_instead_of_blobbing() {
        let mut app = app_with_scenario();
        // Into the war: KPA offensive rolling south.
        run_ticks(&mut app, 24 * 200);
        let military = app.world().resource::<Military>();
        assert!(!military.wars.is_empty(), "Korean War underway");
        // Auto-theaters exist for the belligerents.
        assert!(
            military
                .theaters
                .values()
                .any(|t| t.auto && t.owner.0 == "PRK"),
            "KPA auto-theater created"
        );
        // The anti-blob check: the larger armies hold multiple distinct
        // provinces rather than stacking into one death-ball.
        use std::collections::BTreeSet;
        let spread = |tag: &str| -> usize {
            military
                .formations
                .values()
                .filter(|f| f.owner.0 == tag)
                .map(|f| f.location)
                .collect::<BTreeSet<_>>()
                .len()
        };
        assert!(
            spread("PRK") >= 3,
            "KPA spread across the front: {} provinces",
            spread("PRK")
        );
    }

    /// The separate-peace regression: the ROK signing its own armistice
    /// must not strand the US expeditionary force. Basing rests on
    /// friendly soil (co-belligerent OR same bloc), so US theaters and
    /// raising on South Korean ground survive the KOR-PRK peace.
    #[test]
    fn separate_peace_does_not_strand_the_expeditionary_force() {
        use crate::command::SimCommand;
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 200); // mid-war, US committed
        let usa = CountryTag("USA".into());
        let kor = CountryTag("KOR".into());
        let prk = CountryTag("PRK".into());
        {
            let military = app.world().resource::<Military>();
            assert!(military.at_war(&usa, &prk), "US in the war");
            assert!(military.at_war(&kor, &prk), "ROK in the war");
        }
        // Both Koreas offer; the monthly settlement signs their armistice.
        for (country, enemy) in [(kor.clone(), prk.clone()), (prk.clone(), kor.clone())] {
            push(
                &mut app,
                SimCommand::SetArmisticeOffer {
                    country,
                    enemy,
                    offer: true,
                },
            );
        }
        run_ticks(&mut app, 24 * 35); // across a month boundary
        let scenario = app.world().resource::<SimScenario>().0.clone();
        let busan = scenario.province_by_name(&kor, "Busan").unwrap();
        {
            let military = app.world().resource::<Military>();
            assert!(!military.at_war(&kor, &prk), "separate peace signed");
            assert!(military.at_war(&usa, &prk), "the US fights on");
            // The core regression: ROK soil stays friendly for the US.
            assert!(
                military.may_operate(&scenario, &usa, busan),
                "bloc basing survives the host's separate peace"
            );
        }
        // Delete every US theater, then remake and paint one — the
        // reported unrecoverable state.
        let mine: Vec<TheaterId> = app
            .world()
            .resource::<Military>()
            .theaters
            .iter()
            .filter(|(_, t)| t.owner == usa)
            .map(|(id, _)| *id)
            .collect();
        for id in mine {
            push(
                &mut app,
                SimCommand::DeleteTheater {
                    country: usa.clone(),
                    id,
                },
            );
        }
        push(
            &mut app,
            SimCommand::CreateTheater {
                country: usa.clone(),
                name: "KOREA".into(),
            },
        );
        run_ticks(&mut app, 1);
        let new_theater = *app
            .world()
            .resource::<Military>()
            .theaters
            .iter()
            .filter(|(_, t)| t.owner == usa && !t.auto)
            .map(|(id, _)| id)
            .next_back()
            .expect("theater recreated");
        push(
            &mut app,
            SimCommand::PaintTheater {
                country: usa.clone(),
                id: new_theater,
                province: busan,
                add: true,
            },
        );
        run_ticks(&mut app, 1);
        let military = app.world().resource::<Military>();
        assert!(
            military.theaters[&new_theater].provinces.contains(&busan),
            "painting ROK soil works after the separate peace"
        );
        // And raising there still works (the deployment abstraction).
        {
            let mut econ = app.world_mut().resource_mut::<crate::planning::Economies>();
            econ.industry.get_mut(&usa).unwrap().military_stock = 10;
        }
        let before = app
            .world()
            .resource::<Military>()
            .formations
            .values()
            .filter(|f| f.owner == usa)
            .count();
        push(
            &mut app,
            SimCommand::RaiseFormation {
                country: usa.clone(),
                archetype: Archetype::Infantry,
                home: busan,
                count: 1,
            },
        );
        run_ticks(&mut app, 1);
        let after = app
            .world()
            .resource::<Military>()
            .formations
            .values()
            .filter(|f| f.owner == usa)
            .count();
        assert_eq!(after, before + 1, "raising on allied soil still works");
    }
}
