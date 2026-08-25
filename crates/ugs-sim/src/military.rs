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
    next_id: u32,
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

    pub fn posture(&self, country: &CountryTag, enemy: &CountryTag) -> Posture {
        self.postures
            .get(&(country.clone(), enemy.clone()))
            .copied()
            .unwrap_or(Posture::Hold)
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
        h
    }
}

/// Hourly: seed OOB on first tick, fight battles; daily: move, occupy.
pub fn update_military(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
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
        return;
    }
    if military.wars.is_empty() {
        return; // peace: nothing to simulate hourly (regen is cheap, skip)
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
        let side_a: Vec<FormationId> = ids
            .iter()
            .filter(|i| &military.formations[i].owner == first)
            .copied()
            .collect();
        let side_b: Vec<FormationId> = ids
            .iter()
            .filter(|i| enemies.contains(&&military.formations[i].owner))
            .copied()
            .collect();
        battles.push((*province, side_a, side_b));
    }

    let mut in_battle: Vec<FormationId> = Vec::new();
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
                // Home soil: province is still held by its 1950 owner.
                if data
                    .provinces
                    .get(province)
                    .is_some_and(|p| !military.occupation.contains_key(province) && ids.first().is_some_and(|i| military.formations[i].owner == p.owner))
                {
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

        let apply = |military: &mut Military, ids: &[FormationId], total: u64| {
            if ids.is_empty() {
                return;
            }
            let per = (total / ids.len() as u64).max(1);
            for id in ids {
                let f = military.formations.get_mut(id).unwrap();
                f.cohesion = f.cohesion.saturating_sub(per);
                f.strength = f
                    .strength
                    .saturating_sub((per / STRENGTH_DAMAGE_DIVISOR).max(1));
            }
        };
        apply(&mut military, side_a, damage_to_a);
        apply(&mut military, side_b, damage_to_b);
    }

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
    for (province, occupier) in flips {
        military.occupation.insert(province, occupier);
    }
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
    }
}
