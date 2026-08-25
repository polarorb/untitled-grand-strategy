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
    fn parse(s: &str) -> Self {
        match s {
            "Motorized" => Archetype::Motorized,
            "Armor" => Archetype::Armor,
            _ => Archetype::Infantry,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct FormationId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Formation {
    pub owner: CountryTag,
    pub archetype: Archetype,
    pub location: ProvinceId,
    /// Fighting spirit, permille. Recovers fast; breaking it wins battles.
    pub cohesion: u64,
    /// Men and equipment, permille. Dies slowly; hitting zero destroys.
    pub strength: u64,
    /// Equipment/training quality, permille.
    pub quality: u64,
    /// Days until this formation may move again.
    pub move_cooldown: u8,
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
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (id, f) in &self.formations {
            for v in [
                id.0 as u64,
                f.location.0 as u64,
                f.cohesion,
                f.strength,
                f.owner.0.bytes().map(u64::from).sum::<u64>(),
            ] {
                h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        for (p, tag) in &self.occupation {
            h = (h ^ p.0 as u64).wrapping_mul(0x0000_0100_0000_01b3);
            h = (h ^ tag.0.bytes().map(u64::from).sum::<u64>())
                .wrapping_mul(0x0000_0100_0000_01b3);
        }
        for (tag, men) in &self.manpower {
            h = (h ^ tag.0.bytes().map(u64::from).sum::<u64>() ^ men)
                .wrapping_mul(0x0000_0100_0000_01b3);
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
    player: Res<PlayerCountry>,
    demo: Res<crate::demography::Demographics>,
    mut rng: ResMut<SimRng>,
    mut military: ResMut<Military>,
    mut fired: ResMut<crate::events::FiredEvents>,
    mut tension: ResMut<crate::tension::GlobalTension>,
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
                military.spawn(Formation {
                    owner: entry.owner.clone(),
                    archetype: Archetype::parse(&entry.archetype),
                    location: province,
                    cohesion: 1000,
                    strength: 1000,
                    quality: entry.quality as u64,
                    move_cooldown: 0,
                });
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
        let Some(first) = owners.first() else { continue };
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
                .map(|ids| ids.iter().map(|i| military.formations[i].owner.clone()).collect())
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
                format!("BATTLE OF {name} ENDS AFTER {hours}H -- {} HOLD THE FIELD", victors.join("/")),
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
                    stat * f.strength / 1000 * f.quality / 1000
                })
                .sum();
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

        let apply = |military: &mut Military, ids: &[FormationId], total: u64| {
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
                *military.casualties.entry(owner).or_default() += strength_loss;
            }
        };
        apply(&mut military, side_a, damage_to_a);
        apply(&mut military, side_b, damage_to_b);
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

    // --- Daily movement & occupation -------------------------------------
    if !clock.new_day {
        return;
    }
    for f in military.formations.values_mut() {
        f.move_cooldown = f.move_cooldown.saturating_sub(1);
    }

    // Wartime mobilization: belligerents add men to the pool monthly.
    if clock.new_month {
        let at_war: Vec<CountryTag> = military
            .wars
            .iter()
            .flat_map(|(a, b)| [a.clone(), b.clone()])
            .collect();
        let mut pop_by_country: BTreeMap<CountryTag, u64> = BTreeMap::new();
        for (id, c) in &demo.provinces {
            if let Some(p) = data.provinces.get(id) {
                if at_war.contains(&p.owner) {
                    *pop_by_country.entry(p.owner.clone()).or_default() += c.total();
                }
            }
        }
        for (tag, pop) in pop_by_country {
            *military.manpower.entry(tag).or_default() +=
                pop * MOBILIZE_PERMILLE_PER_MONTH / 1000;
        }
    }

    // Reinforcement: resting formations on non-enemy soil draw men from
    // the national pool.
    let reinforce: Vec<FormationId> = military
        .formations
        .iter()
        .filter(|(id, f)| {
            f.strength < 1000
                && !in_battle.contains(id)
                && data.provinces.get(&f.location).is_some_and(|p| {
                    let holder = military.owner_of(f.location, &p.owner);
                    !military.at_war(&f.owner, &holder)
                })
        })
        .map(|(id, _)| *id)
        .collect();
    for id in reinforce {
        let owner = military.formations[&id].owner.clone();
        let pool = military.manpower.entry(owner).or_default();
        let points = REINFORCE_PER_DAY.min(*pool / MEN_PER_STRENGTH_POINT);
        if points == 0 {
            continue;
        }
        *pool -= points * MEN_PER_STRENGTH_POINT;
        let f = military.formations.get_mut(&id).unwrap();
        f.strength = (f.strength + points).min(1000);
    }

    // Advancing formations move into adjacent enemy provinces, or march
    // toward the nearest enemy front (BFS first-hop) when none is adjacent.
    let movers: Vec<FormationId> = military
        .formations
        .iter()
        .filter(|(id, f)| {
            f.move_cooldown == 0
                && f.cohesion >= RETREAT_COHESION
                && !in_battle.contains(id)
        })
        .map(|(id, _)| *id)
        .collect();
    for id in movers {
        let (owner, location) = {
            let f = &military.formations[&id];
            (f.owner.clone(), f.location)
        };
        let dest = find_advance_step(data, &military, &owner, location);
        if let Some(dest) = dest {
            let (_, _, days) = military.formations[&id].archetype.stats();
            let f = military.formations.get_mut(&id).unwrap();
            f.location = dest;
            f.move_cooldown = days;
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

    // --- Monthly: armistice diplomacy ------------------------------------
    if clock.new_month {
        settle_wars(&clock, data, &player.0, &mut military, &mut fired, &mut tension);
    }
}

/// The country the human player controls (None = observer / headless).
/// Set via `SimCommand::SetPlayerCountry` so it lives in the replay log.
#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct PlayerCountry(pub Option<CountryTag>);

/// End wars at the line of control. Non-player countries become willing
/// automatically (long war + static front, or a broken army); the player
/// must offer explicitly. Total collapse (no army, no home provinces)
/// ends a war unilaterally.
fn settle_wars(
    clock: &SimClock,
    data: &ugs_data::ScenarioData,
    player: &Option<CountryTag>,
    military: &mut Military,
    fired: &mut crate::events::FiredEvents,
    tension: &mut crate::tension::GlobalTension,
) {
    use tuning::*;
    let pairs: Vec<(CountryTag, CountryTag)> = military.wars.clone();
    for (a, b) in pairs {
        let start = military
            .war_started
            .get(&(a.clone(), b.clone()))
            .copied()
            .unwrap_or(0);
        let war_months = (clock.tick.saturating_sub(start)) / (24 * 30);
        let stale_months =
            (clock.tick.saturating_sub(military.last_line_change_tick)) / (24 * 30);

        let formations_of = |m: &Military, tag: &CountryTag| {
            m.formations.values().filter(|f| &f.owner == tag).count()
        };
        let holds_home = |m: &Military, tag: &CountryTag| {
            data.provinces
                .values()
                .any(|p| p.owner == *tag && m.owner_of(p.id, &p.owner) == *tag)
        };

        // Total collapse: no army and no home soil — resistance ends.
        let collapsed = |m: &Military, tag: &CountryTag| {
            formations_of(m, tag) == 0 && !holds_home(m, tag)
        };
        if collapsed(military, &a) || collapsed(military, &b) {
            let loser = if collapsed(military, &a) { &a } else { &b };
            end_war(military, &a, &b);
            fired.notices.push((
                "RESISTANCE ENDS".into(),
                format!(
                    "ORGANIZED RESISTANCE BY {} FORCES HAS CEASED. OCCUPYING AUTHORITIES ASSUME CONTROL. THE GUNS FALL SILENT OVER A CHANGED MAP.",
                    loser.0
                ),
            ));
            tension.apply(ARMISTICE_TENSION_RELIEF);
            continue;
        }

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
        if willing(military, &a, &b) && willing(military, &b, &a) {
            end_war(military, &a, &b);
            fired.notices.push((
                "ARMISTICE SIGNED".into(),
                format!(
                    "{} AND {} SIGN ARMISTICE AGREEMENT. HOSTILITIES SUSPENDED ALONG THE PRESENT LINE OF CONTACT. DEMARCATION LINE FOLLOWS THE FRONT. NO POLITICAL SETTLEMENT REACHED -- THE LINE IS THE BORDER NOW, UNTIL IT ISN'T.",
                    a.0, b.0
                ),
            ));
            tension.apply(ARMISTICE_TENSION_RELIEF);
        }
    }
}

fn end_war(military: &mut Military, a: &CountryTag, b: &CountryTag) {
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

/// The next province an advancing formation should step into: an adjacent
/// enemy province if one exists, else the first hop of the shortest path
/// (through non-enemy territory) toward the nearest province held by an
/// enemy this country is advancing against.
fn find_advance_step(
    data: &ugs_data::ScenarioData,
    military: &Military,
    owner: &CountryTag,
    location: ProvinceId,
) -> Option<ProvinceId> {
    use std::collections::{BTreeSet, VecDeque};
    let is_target = |id: ProvinceId| {
        data.provinces.get(&id).is_some_and(|p| {
            let holder = military.owner_of(id, &p.owner);
            military.at_war(owner, &holder)
                && military.posture(owner, &holder) == Posture::Advance
        })
    };
    let mut visited: BTreeSet<ProvinceId> = BTreeSet::from([location]);
    let mut queue: VecDeque<(ProvinceId, Option<ProvinceId>)> =
        VecDeque::from([(location, None)]);
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
            if is_target(*adj) {
                return hop;
            }
            // March only through territory we are not at war with.
            let passable = data.provinces.get(adj).is_some_and(|ap| {
                let holder = military.owner_of(*adj, &ap.owner);
                !military.at_war(owner, &holder)
            });
            if passable {
                queue.push_back((*adj, hop));
            }
        }
    }
    None
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
            military.manpower.get(&CountryTag("KOR".into())).copied().unwrap_or(0) > 100_000,
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
        let won: u32 = military.battles_won.values().sum();
        let lost: u32 = military.battles_lost.values().sum();
        assert!(won > 0 && lost > 0, "battle outcomes tallied ({won}W/{lost}L)");
        // Mobilization grows the belligerents' pools while neutral pools hold.
        let prk = military.manpower.get(&CountryTag("PRK".into())).copied().unwrap();
        assert!(prk > 0, "KPA still has a manpower pool");
    }
}
