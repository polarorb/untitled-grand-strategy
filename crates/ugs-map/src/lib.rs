//! Province graph: adjacency queries and pathfinding over the loaded map.
//! Built once from `ugs_data::ScenarioData` at game start; immutable afterward.
//! Dynamic state (ownership changes, front lines) lives in the sim, not here.

use std::collections::BTreeMap;
use ugs_data::{ProvinceId, ScenarioData, Terrain};

pub struct MapGraph {
    /// Adjacency lists keyed by province, in stable (sorted) order.
    adjacency: BTreeMap<ProvinceId, Vec<ProvinceId>>,
    terrain: BTreeMap<ProvinceId, Terrain>,
}

impl MapGraph {
    pub fn build(data: &ScenarioData) -> Self {
        let mut adjacency = BTreeMap::new();
        let mut terrain = BTreeMap::new();
        for (id, p) in &data.provinces {
            let mut adj = p.adjacent.clone();
            adj.sort();
            adjacency.insert(*id, adj);
            terrain.insert(*id, p.terrain);
        }
        Self { adjacency, terrain }
    }

    pub fn neighbors(&self, id: ProvinceId) -> &[ProvinceId] {
        self.adjacency.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn terrain(&self, id: ProvinceId) -> Option<Terrain> {
        self.terrain.get(&id).copied()
    }

    /// Breadth-first shortest path by hop count. Deterministic: ties broken
    /// by province id order. Movement-cost-aware pathfinding comes later.
    pub fn path(&self, from: ProvinceId, to: ProvinceId) -> Option<Vec<ProvinceId>> {
        if from == to {
            return Some(vec![from]);
        }
        let mut came_from: BTreeMap<ProvinceId, ProvinceId> = BTreeMap::new();
        let mut frontier = std::collections::VecDeque::from([from]);
        while let Some(current) = frontier.pop_front() {
            for &next in self.neighbors(current) {
                if next != from && !came_from.contains_key(&next) {
                    came_from.insert(next, current);
                    if next == to {
                        let mut path = vec![to];
                        let mut node = to;
                        while node != from {
                            node = came_from[&node];
                            path.push(node);
                        }
                        path.reverse();
                        return Some(path);
                    }
                    frontier.push_back(next);
                }
            }
        }
        None
    }
}
