//! Static game data: scenario definitions, country/province data, and the
//! RON loaders for them. This crate knows nothing about ECS or rendering.
//!
//! All gameplay content lives in `assets/data/` as RON files and is validated
//! at load time. Ids are newtyped so a ProvinceId can never be confused with
//! a CountryId at compile time.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("parse error in {path}: {source}")]
    Parse {
        path: String,
        source: ron::error::SpannedError,
    },
    #[error("validation error: {0}")]
    Validation(String),
}

/// Three-letter tag, e.g. "USA", "SOV", "PRC". Ordered so that iteration over
/// keyed maps is deterministic.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct CountryTag(pub String);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ProvinceId(pub u32);

/// The two poles of the Cold War plus the space in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    WesternBloc,
    EasternBloc,
    NonAligned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryDef {
    pub tag: CountryTag,
    pub name: String,
    pub alignment: Alignment,
    pub capital: ProvinceId,
    /// 0-100. How firmly the government controls the country.
    pub stability: u8,
    /// Starting industrial capacity (abstract units).
    pub industry: u32,
    /// Whether this country has nuclear weapons at scenario start.
    pub nuclear_power: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvinceDef {
    pub id: ProvinceId,
    pub name: String,
    pub owner: CountryTag,
    pub terrain: Terrain,
    /// Population in thousands.
    pub population_k: u32,
    pub adjacent: Vec<ProvinceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Terrain {
    Plains,
    Forest,
    Hills,
    Mountain,
    Desert,
    Jungle,
    Urban,
    Marsh,
    Tundra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioDef {
    pub name: String,
    /// Start date as (year, month, day).
    pub start_date: (i32, u8, u8),
    pub description: String,
}

/// Everything loaded from disk for one scenario, validated and cross-linked.
#[derive(Debug, Clone)]
pub struct ScenarioData {
    pub scenario: ScenarioDef,
    pub countries: BTreeMap<CountryTag, CountryDef>,
    pub provinces: BTreeMap<ProvinceId, ProvinceDef>,
}

impl ScenarioData {
    /// Load a scenario from `assets/data/scenario/<name>/`.
    pub fn load(scenario_dir: &Path) -> Result<Self, DataError> {
        let scenario: ScenarioDef = load_ron(&scenario_dir.join("scenario.ron"))?;

        let mut countries = BTreeMap::new();
        let countries_dir = scenario_dir.join("countries");
        for entry in read_dir_sorted(&countries_dir)? {
            let def: CountryDef = load_ron(&entry)?;
            if countries.insert(def.tag.clone(), def).is_some() {
                return Err(DataError::Validation(format!(
                    "duplicate country tag in {}",
                    entry.display()
                )));
            }
        }

        let mut provinces = BTreeMap::new();
        let provinces_dir = scenario_dir.join("provinces");
        for entry in read_dir_sorted(&provinces_dir)? {
            let defs: Vec<ProvinceDef> = load_ron(&entry)?;
            for def in defs {
                if provinces.insert(def.id, def).is_some() {
                    return Err(DataError::Validation(format!(
                        "duplicate province id in {}",
                        entry.display()
                    )));
                }
            }
        }

        let data = Self {
            scenario,
            countries,
            provinces,
        };
        data.validate()?;
        Ok(data)
    }

    fn validate(&self) -> Result<(), DataError> {
        for (tag, c) in &self.countries {
            if !self.provinces.contains_key(&c.capital) {
                return Err(DataError::Validation(format!(
                    "{:?}: capital {:?} is not a defined province",
                    tag, c.capital
                )));
            }
        }
        for (id, p) in &self.provinces {
            if !self.countries.contains_key(&p.owner) {
                return Err(DataError::Validation(format!(
                    "province {:?}: owner {:?} is not a defined country",
                    id, p.owner
                )));
            }
            for adj in &p.adjacent {
                let neighbor = self.provinces.get(adj).ok_or_else(|| {
                    DataError::Validation(format!(
                        "province {:?}: adjacent {:?} does not exist",
                        id, adj
                    ))
                })?;
                if !neighbor.adjacent.contains(id) {
                    return Err(DataError::Validation(format!(
                        "adjacency not symmetric: {:?} -> {:?}",
                        id, adj
                    )));
                }
            }
        }
        Ok(())
    }
}

fn load_ron<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, DataError> {
    let text = std::fs::read_to_string(path).map_err(|source| DataError::Io {
        path: path.display().to_string(),
        source,
    })?;
    ron::from_str(&text).map_err(|source| DataError::Parse {
        path: path.display().to_string(),
        source,
    })
}

/// Directory listing with a stable order, so load results never depend on
/// filesystem enumeration order.
fn read_dir_sorted(dir: &Path) -> Result<Vec<std::path::PathBuf>, DataError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|source| DataError::Io {
            path: dir.display().to_string(),
            source,
        })?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "ron"))
        .collect();
    entries.sort();
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn scenario_1950_loads_and_validates() {
        let dir = repo_root().join("assets/data/scenario/1950");
        let data = ScenarioData::load(&dir).expect("1950 scenario should load");
        assert_eq!(data.scenario.start_date, (1950, 1, 1));
        assert!(data.countries.contains_key(&CountryTag("USA".into())));
        assert!(data.countries.contains_key(&CountryTag("SOV".into())));
    }
}
