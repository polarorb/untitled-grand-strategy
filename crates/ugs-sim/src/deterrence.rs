//! Deterrence & delivery — phase 2 of the escalation pillar. What
//! matters is not the arsenal but what the rival BELIEVES about it,
//! and whether it can physically arrive: range vs basing on the real
//! map. Under mutual deterrence, direct war between the peers stops
//! being declarable — the stability–instability paradox as a rule.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use ugs_data::{CountryTag, ProvinceId, ScenarioData};

use crate::demography::SimScenario;
use crate::nuclear::NuclearPrograms;
use crate::SimClock;

pub mod tuning {
    /// Believed deliverable warheads at which deterrence bites.
    pub const MIN_DETERRENT: u32 = 5;
    /// One-way strike forces count at half weight.
    pub const ONE_WAY_WEIGHT_PERMILLE: u32 = 500;
    /// Estimate bias: rivals overestimate an opaque arsenal by up to
    /// this permille above truth (bomber-gap dynamics); deception adds
    /// its own inflation on top.
    pub const OPACITY_BIAS_PERMILLE: u64 = 1000;
    pub const DECEPTION_BIAS_PERMILLE: u64 = 600;
}

/// Integer great-circle-ish distance (equirectangular, km). Uses only
/// integer math off static province coordinates — deterministic.
pub fn distance_km(a: (f32, f32), b: (f32, f32)) -> u64 {
    // Milli-degrees as integers.
    let (alon, alat) = ((a.0 * 1000.0) as i64, (a.1 * 1000.0) as i64);
    let (blon, blat) = ((b.0 * 1000.0) as i64, (b.1 * 1000.0) as i64);
    let dlat = alat - blat;
    let mut dlon = (alon - blon).abs() % 360_000;
    if dlon > 180_000 {
        dlon = 360_000 - dlon;
    }
    // cos of mean latitude, permille, via a small integer table.
    let mean_lat = ((alat + blat) / 2).unsigned_abs() / 1000; // degrees
    let cos_permille = COS_TABLE[(mean_lat as usize).min(90)] as i64;
    let dlon_eff = dlon * cos_permille / 1000;
    let sq = (dlat * dlat + dlon_eff * dlon_eff) as u64;
    // 1 milli-degree of latitude ~ 0.1112 km; sqrt then scale.
    isqrt(sq) * 1112 / 10_000
}

/// cos(deg) * 1000 for 0..=90.
const COS_TABLE: [u16; 91] = [
    1000, 1000, 999, 999, 998, 996, 995, 993, 990, 988, 985, 982, 978, 974, 970, 966, 961, 956,
    951, 946, 940, 934, 927, 921, 914, 906, 899, 891, 883, 875, 866, 857, 848, 839, 829, 819, 809,
    799, 788, 777, 766, 755, 743, 731, 719, 707, 695, 682, 669, 656, 643, 629, 616, 602, 588, 574,
    559, 545, 530, 515, 500, 485, 469, 454, 438, 423, 407, 391, 375, 358, 342, 326, 309, 292, 276,
    259, 242, 225, 208, 191, 174, 156, 139, 122, 105, 87, 70, 52, 35, 17, 0,
];

fn isqrt(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DyadClass {
    /// Neither side can credibly deliver.
    None,
    /// Exactly one side holds a deliverable arsenal.
    OneSided,
    /// Both sides believe the other can retaliate: war between the
    /// peers is no longer declarable — only crises remain.
    Mutual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DyadAssessment {
    pub class: DyadClass,
    /// What each side BELIEVES the other can land (a's estimate of b,
    /// b's estimate of a) — biased, per the bomber-gap record.
    pub a_believes_b_delivers: u32,
    pub b_believes_a_delivers: u32,
}

#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Deterrence {
    /// Set after the first computation; without it a scenario with a
    /// single program would rerun the full reach scan every tick.
    computed: bool,
    /// Keyed (a, b) with a < b, nuclear-program pairs only.
    pub dyads: BTreeMap<(CountryTag, CountryTag), DyadAssessment>,
    /// Provinces each nuclear power can strike (two-way reach), for the
    /// strategic map. Derived monthly; not part of the digest.
    pub reach: BTreeMap<CountryTag, Vec<ProvinceId>>,
}

impl Deterrence {
    pub fn class(&self, a: &CountryTag, b: &CountryTag) -> DyadClass {
        let key = if a < b {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
        self.dyads
            .get(&key)
            .map(|d| d.class)
            .unwrap_or(DyadClass::None)
    }

    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for ((a, b), d) in &self.dyads {
            for byte in a.0.bytes().chain(b.0.bytes()) {
                h = (h ^ byte as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
            for v in [
                d.class as u64,
                d.a_believes_b_delivers as u64,
                d.b_believes_a_delivers as u64,
            ] {
                h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// Strike-origin provinces: own territory plus basing-rights hosts.
fn origins<'a>(
    data: &'a ScenarioData,
    tag: &CountryTag,
    basing: &[CountryTag],
) -> Vec<&'a ugs_data::ProvinceDef> {
    data.provinces
        .values()
        .filter(|p| p.owner == *tag || basing.contains(&p.owner))
        .collect()
}

/// Combat radius after bomber-generation growth.
fn effective_range_km(p: &crate::nuclear::Program, years_elapsed: u64) -> u64 {
    p.bomber_range_km as u64 + p.range_growth_km_per_year as u64 * years_elapsed
}

/// Can `attacker` put a bomber over `target_center`, and how (weight
/// permille: 1000 two-way, 500 one-way, 0 no)?
fn reach_weight(
    data: &ScenarioData,
    programs: &NuclearPrograms,
    attacker: &CountryTag,
    target_center: (f32, f32),
    years_elapsed: u64,
) -> u32 {
    use tuning::*;
    let Some(p) = programs.programs.get(attacker) else {
        return 0;
    };
    if p.bombers == 0 {
        return 0;
    }
    let range = effective_range_km(p, years_elapsed);
    let mut best = 0u32;
    for origin in origins(data, attacker, &p.basing_rights) {
        let d = distance_km(origin.center, target_center);
        if d <= range {
            return 1000;
        }
        if d <= range + p.one_way_extra_km as u64 {
            best = best.max(ONE_WAY_WEIGHT_PERMILLE);
        }
    }
    best
}

/// What `viewer` believes `subject` could land on it. Estimates run
/// biased HIGH when the subject is opaque (the historical direction),
/// higher still under parade deception.
fn believed_deliverable(
    data: &ScenarioData,
    programs: &NuclearPrograms,
    subject: &CountryTag,
    viewer: &CountryTag,
    years_elapsed: u64,
) -> u32 {
    use tuning::*;
    let Some(sp) = programs.programs.get(subject) else {
        return 0;
    };
    // Counter-value: deterrence needs reach to ANY major city of the
    // viewer, not just the capital (Seattle counted as much as DC).
    let weight = data
        .provinces
        .values()
        .filter(|p| p.owner == *viewer && p.population_k >= 500)
        .map(|p| reach_weight(data, programs, subject, p.center, years_elapsed))
        .max()
        .unwrap_or(0);
    if weight == 0 {
        return 0;
    }
    let opacity = 1000u64.saturating_sub(sp.exposure_permille as u64);
    let mut bias = 1000 + opacity * OPACITY_BIAS_PERMILLE / 1000;
    if sp.deception {
        bias += DECEPTION_BIAS_PERMILLE;
    }
    let believed_arsenal = sp.assembled as u64 * bias / 1000;
    (believed_arsenal * weight as u64 / 1000) as u32
}

/// Monthly, after the nuclear system.
pub fn update_deterrence(
    clock: Res<SimClock>,
    scenario: Option<Res<SimScenario>>,
    programs: Res<NuclearPrograms>,
    mut deterrence: ResMut<Deterrence>,
) {
    use tuning::*;
    let Some(scenario) = scenario else { return };
    let data = &scenario.0;
    if !clock.new_month && deterrence.computed {
        return;
    }
    if programs.programs.is_empty() {
        return;
    }
    deterrence.computed = true;

    let years_elapsed = (clock.date.year - data.scenario.start_date.0).max(0) as u64;
    let tags: Vec<CountryTag> = programs.programs.keys().cloned().collect();
    let mut dyads = BTreeMap::new();
    for (i, a) in tags.iter().enumerate() {
        for b in tags.iter().skip(i + 1) {
            let a_believes = believed_deliverable(data, &programs, b, a, years_elapsed);
            let b_believes = believed_deliverable(data, &programs, a, b, years_elapsed);
            let class = match (
                b_believes >= MIN_DETERRENT, // a's arsenal deters b
                a_believes >= MIN_DETERRENT, // b's arsenal deters a
            ) {
                (true, true) => DyadClass::Mutual,
                (false, false) => DyadClass::None,
                _ => DyadClass::OneSided,
            };
            dyads.insert(
                (a.clone(), b.clone()),
                DyadAssessment {
                    class,
                    a_believes_b_delivers: a_believes,
                    b_believes_a_delivers: b_believes,
                },
            );
        }
    }
    deterrence.dyads = dyads;

    // Strategic-map reach sets (two-way only: the wash shows what a
    // nation can strike and come home from).
    let mut reach = BTreeMap::new();
    for tag in &tags {
        let Some(p) = programs.programs.get(tag) else {
            continue;
        };
        if p.bombers == 0 {
            continue;
        }
        let origin_list = origins(data, tag, &p.basing_rights);
        let range = effective_range_km(p, years_elapsed);
        let mut provinces: Vec<ProvinceId> = Vec::new();
        for target in data.provinces.values() {
            let reachable = origin_list
                .iter()
                .any(|o| distance_km(o.center, target.center) <= range);
            if reachable {
                provinces.push(target.id);
            }
        }
        reach.insert(tag.clone(), provinces);
    }
    deterrence.reach = reach;
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
    fn distances_are_sane() {
        // Moscow to London ~2,500 km; Moscow to Washington ~7,800 km.
        let moscow = (37.6, 55.7);
        let london = (-0.1, 51.5);
        let washington = (-77.0, 38.9);
        let d1 = distance_km(moscow, london);
        let d2 = distance_km(moscow, washington);
        assert!((2000..3100).contains(&d1), "Moscow-London {d1} km");
        assert!((6500..9500).contains(&d2), "Moscow-Washington {d2} km");
    }

    #[test]
    fn deterrence_in_1950_is_one_sided() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 40); // past the first month boundary
        let det = app.world().resource::<Deterrence>();
        let usa = CountryTag("USA".into());
        let sov = CountryTag("SOV".into());
        // The US (UK basing, 60 assembled) can strike Moscow; the
        // Soviet Tu-4 force reaches CONUS only one-way with ~1
        // assembled weapon — deterrence points one way in 1950.
        assert_eq!(
            det.class(&usa, &sov),
            DyadClass::OneSided,
            "dyads: {:?}",
            det.dyads
        );
        // Bomber generations improve (Tu-16, Tu-95) and the arsenal
        // grows; by the mid-50s the dyad turns mutual — the historical
        // window where SAC lost sanctuary.
        run_ticks(&mut app, 24 * 365 * 6);
        let det = app.world().resource::<Deterrence>();
        assert_eq!(
            det.class(&usa, &sov),
            DyadClass::Mutual,
            "by 1956: {:?}",
            det.dyads
        );
    }

    #[test]
    fn reach_covers_the_right_hemispheres() {
        let mut app = app_with_scenario();
        run_ticks(&mut app, 24 * 40);
        let det = app.world().resource::<Deterrence>();
        let data = {
            let s = app.world().resource::<crate::demography::SimScenario>();
            s.0.clone()
        };
        let usa_reach = det.reach.get(&CountryTag("USA".into())).unwrap();
        // With UK basing the US reaches Moscow two-way.
        let moscow_like = data
            .provinces
            .values()
            .find(|p| p.owner.0 == "SOV" && distance_km(p.center, (37.6, 55.7)) < 300)
            .expect("a province near Moscow");
        assert!(
            usa_reach.contains(&moscow_like.id),
            "US strategic reach should cover Moscow via UK basing"
        );
        // The Soviet two-way reach covers Europe but not CONUS.
        let sov_reach = det.reach.get(&CountryTag("SOV".into())).unwrap();
        let berlin_like = data
            .provinces
            .values()
            .find(|p| distance_km(p.center, (13.4, 52.5)) < 300)
            .expect("a province near Berlin");
        assert!(sov_reach.contains(&berlin_like.id), "Tu-4s cover Europe");
        let kansas_like = data
            .provinces
            .values()
            .find(|p| p.owner.0 == "USA" && distance_km(p.center, (-98.0, 38.5)) < 400)
            .expect("a province in the US interior");
        assert!(
            !sov_reach.contains(&kansas_like.id),
            "no two-way Soviet reach into CONUS in 1950"
        );
    }
}
