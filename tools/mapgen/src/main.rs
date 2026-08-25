//! Offline map generator: Natural Earth admin-1 GeoJSON -> game data.
//!
//! Reads (run ./fetch-data.sh first):
//!   data/ne_10m_admin_1.geojson  — ~4,600 worldwide first-level admin units
//!                                  (PUBLIC DOMAIN, naturalearthdata.com)
//!   owners_1950.csv              — adm0_a3 -> 1950 sovereign remapping
//!
//! Writes (committed game data):
//!   assets/data/scenario/1950/provinces/world.ron   — ProvinceDef list
//!   assets/data/scenario/1950/countries/generated.ron — CountryDef list
//!   assets/map/world.geo.ron                        — id -> polygon rings
//!
//! Runs offline only; determinism of the *game* never depends on this tool,
//! but its output is stable given identical inputs (sorted iteration, fixed
//! id assignment by adm1_code order).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use serde_json::Value;
use ugs_data::{Alignment, CountryDef, CountryTag, ProvinceDef, ProvinceId, Terrain};

/// Ramer–Douglas–Peucker tolerance, degrees. ~0.03° ≈ 3 km at the equator.
const SIMPLIFY_TOLERANCE_DEG: f64 = 0.03;
/// Rings reduced below this many distinct points are dropped (islets).
const MIN_RING_POINTS: usize = 4;
/// Quantization for adjacency detection: 0.01° grid.
const ADJ_QUANT: f64 = 100.0;
/// Provinces sharing at least this many quantized boundary points are
/// adjacent (2 could be a corner touch).
const ADJ_MIN_SHARED: usize = 3;

type Ring = Vec<(f64, f64)>;
/// (alignment, capital (lon, lat), stability, industry, nuclear)
type CountryMeta = (Alignment, (f32, f32), u8, u32, bool);

/// Hand-authored 1950 metadata for notable countries. Everyone else gets
/// defaults (NonAligned, first province, 60, 5, false).
/// Sources: see docs/research/geodata.md. Industry values are game-scale
/// estimates, not measurements.
fn notable_countries() -> BTreeMap<&'static str, CountryMeta> {
    use Alignment::*;
    BTreeMap::from([
        ("USA", (WesternBloc, (-77.04, 38.91), 85, 100, true)),
        ("SOV", (EasternBloc, (37.62, 55.75), 80, 70, true)),
        ("GBR", (WesternBloc, (-0.13, 51.51), 75, 40, false)),
        ("FRA", (WesternBloc, (2.35, 48.86), 60, 30, false)),
        ("PRC", (EasternBloc, (116.41, 39.90), 60, 15, false)),
        ("CHT", (WesternBloc, (121.56, 25.03), 50, 3, false)), // ROC on Taiwan
        ("PRK", (EasternBloc, (125.75, 39.03), 70, 8, false)),
        ("KOR", (WesternBloc, (126.98, 37.57), 55, 5, false)),
        ("FRG", (WesternBloc, (7.10, 50.73), 60, 25, false)), // Bonn
        ("GDR", (EasternBloc, (13.40, 52.52), 60, 10, false)),
        ("JAP", (WesternBloc, (139.69, 35.69), 65, 20, false)), // occupied
        ("ITA", (WesternBloc, (12.50, 41.90), 60, 20, false)),
        ("CAN", (WesternBloc, (-75.70, 45.42), 80, 25, false)),
        ("AUS", (WesternBloc, (149.13, -35.28), 80, 12, false)),
        ("NLD", (WesternBloc, (4.90, 52.37), 70, 12, false)),
        ("BEL", (WesternBloc, (4.35, 50.85), 70, 12, false)),
        ("POL", (EasternBloc, (21.01, 52.23), 60, 15, false)),
        ("CZE", (EasternBloc, (14.42, 50.09), 65, 15, false)), // Czechoslovakia
        ("HUN", (EasternBloc, (19.04, 47.50), 60, 8, false)),
        ("ROM", (EasternBloc, (26.10, 44.43), 60, 8, false)),
        ("BUL", (EasternBloc, (23.32, 42.70), 60, 5, false)),
        ("ALB", (EasternBloc, (19.82, 41.33), 55, 1, false)),
        ("MON", (EasternBloc, (106.92, 47.92), 60, 1, false)),
        ("YUG", (NonAligned, (20.46, 44.82), 60, 10, false)), // Tito-Stalin split '48
        ("IND", (NonAligned, (77.21, 28.61), 60, 15, false)),
    ])
}

struct OwnerRow {
    tag: String,
    name: String,
}

/// adm0_a3 -> 1950 sovereign. Unlisted codes default to themselves
/// (tag = adm0_a3, name = Natural Earth admin name).
fn load_owners(path: &Path) -> HashMap<String, OwnerRow> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut map = HashMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || (i == 0 && line.starts_with("iso_a3")) {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 3 {
            panic!("owners_1950.csv line {}: expected >=3 columns", i + 1);
        }
        map.insert(
            cols[0].trim().to_uppercase(),
            OwnerRow {
                tag: cols[1].trim().to_uppercase(),
                name: cols[2].trim().to_string(),
            },
        );
    }
    map
}

/// East German Länder by Natural Earth admin-1 name. Berlin goes East for
/// now — West Berlin needs special handling the province model can't
/// express yet (tracked in docs/design/systems/time-and-map.md).
const GDR_STATES: [&str; 6] = [
    "Brandenburg",
    "Mecklenburg-Vorpommern",
    "Sachsen",
    "Sachsen-Anhalt",
    "Thüringen",
    "Berlin",
];

fn prop<'a>(feature: &'a Value, key: &str) -> Option<&'a str> {
    feature["properties"][key].as_str()
}

fn ring_points(ring: &Value) -> Vec<(f64, f64)> {
    ring.as_array()
        .map(|pts| {
            pts.iter()
                .filter_map(|p| {
                    let a = p.as_array()?;
                    Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// All rings (exterior + holes) of a Polygon/MultiPolygon feature.
/// Returns (exterior_rings, all_rings_for_adjacency).
fn feature_rings(geom: &Value) -> (Vec<Ring>, Vec<Ring>) {
    let mut exterior = Vec::new();
    let mut all = Vec::new();
    let coords = &geom["coordinates"];
    match geom["type"].as_str() {
        Some("Polygon") => {
            if let Some(rings) = coords.as_array() {
                for (i, ring) in rings.iter().enumerate() {
                    let pts = ring_points(ring);
                    if i == 0 {
                        exterior.push(pts.clone());
                    }
                    all.push(pts);
                }
            }
        }
        Some("MultiPolygon") => {
            if let Some(polys) = coords.as_array() {
                for poly in polys {
                    if let Some(rings) = poly.as_array() {
                        for (i, ring) in rings.iter().enumerate() {
                            let pts = ring_points(ring);
                            if i == 0 {
                                exterior.push(pts.clone());
                            }
                            all.push(pts);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    (exterior, all)
}

fn perpendicular_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    ((dy * p.0 - dx * p.1 + b.0 * a.1 - b.1 * a.0).abs()) / len2.sqrt()
}

/// Iterative Ramer–Douglas–Peucker.
fn simplify(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let (mut max_d, mut max_i) = (0.0f64, start);
        for i in (start + 1)..end {
            let d = perpendicular_dist(points[i], points[start], points[end]);
            if d > max_d {
                max_d = d;
                max_i = i;
            }
        }
        if max_d > tolerance {
            keep[max_i] = true;
            stack.push((start, max_i));
            stack.push((max_i, end));
        }
    }
    points
        .iter()
        .zip(&keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

fn quantize(p: (f64, f64)) -> (i64, i64) {
    ((p.0 * ADJ_QUANT).round() as i64, (p.1 * ADJ_QUANT).round() as i64)
}

fn round3(v: f64) -> f32 {
    ((v * 1000.0).round() / 1000.0) as f32
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tool_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let geojson_path = tool_dir.join("data/ne_10m_admin_1.geojson");
    let raw = fs::read_to_string(&geojson_path).unwrap_or_else(|e| {
        panic!(
            "cannot read {} — run tools/mapgen/fetch-data.sh first ({e})",
            geojson_path.display()
        )
    });
    let doc: Value = serde_json::from_str(&raw).expect("invalid GeoJSON");
    let owners = load_owners(&tool_dir.join("owners_1950.csv"));

    let mut features: Vec<&Value> = doc["features"]
        .as_array()
        .expect("FeatureCollection")
        .iter()
        .filter(|f| {
            let adm0 = prop(f, "adm0_a3").unwrap_or("");
            adm0 != "ATA" && !adm0.is_empty() && f["geometry"].is_object()
        })
        .collect();
    // Stable id assignment: sort by adm1_code (unique per unit).
    features.sort_by_key(|f| prop(f, "adm1_code").unwrap_or("").to_string());

    println!("features after filter: {}", features.len());

    // --- Ownership -------------------------------------------------------
    let mut country_names: HashMap<String, String> = HashMap::new(); // tag -> name
    let owner_of = |f: &Value, country_names: &mut HashMap<String, String>| -> String {
        let adm0 = prop(f, "adm0_a3").unwrap().to_uppercase();
        let ne_admin = prop(f, "admin").unwrap_or(&adm0).to_string();
        let (tag, name) = match owners.get(&adm0) {
            Some(row) => (row.tag.clone(), row.name.clone()),
            None => (adm0.clone(), ne_admin),
        };
        // Divided Germany: East German Länder override the CSV remap.
        let tag = if adm0 == "DEU" {
            let n = prop(f, "name").unwrap_or("");
            if GDR_STATES.contains(&n) {
                country_names
                    .entry("GDR".into())
                    .or_insert_with(|| "German Democratic Republic".into());
                "GDR".to_string()
            } else {
                tag
            }
        } else {
            tag
        };
        country_names.entry(tag.clone()).or_insert(name);
        tag
    };

    // --- Provinces -------------------------------------------------------
    struct Prov {
        id: u32,
        name: String,
        owner: String,
        center: (f32, f32),
        rings_raw: Vec<Vec<(f64, f64)>>, // exterior rings, unsimplified
        adj_points: Vec<(i64, i64)>,
    }

    let mut provinces = Vec::new();
    for (idx, f) in features.iter().enumerate() {
        let owner = owner_of(f, &mut country_names);
        let (exterior, all) = feature_rings(&f["geometry"]);
        if exterior.is_empty() {
            continue;
        }
        let name = prop(f, "name")
            .or_else(|| prop(f, "name_en"))
            .or_else(|| prop(f, "adm1_code"))
            .unwrap_or("Unnamed")
            .to_string();
        let center = (
            f["properties"]["longitude"]
                .as_f64()
                .or_else(|| f["properties"]["longitude"].as_str().and_then(|s| s.parse().ok()))
                .unwrap_or_else(|| exterior[0].iter().map(|p| p.0).sum::<f64>() / exterior[0].len() as f64),
            f["properties"]["latitude"]
                .as_f64()
                .or_else(|| f["properties"]["latitude"].as_str().and_then(|s| s.parse().ok()))
                .unwrap_or_else(|| exterior[0].iter().map(|p| p.1).sum::<f64>() / exterior[0].len() as f64),
        );
        let mut adj_points: Vec<(i64, i64)> = all.iter().flatten().map(|&p| quantize(p)).collect();
        adj_points.sort_unstable();
        adj_points.dedup();
        provinces.push(Prov {
            id: (idx + 1) as u32,
            name,
            owner,
            center: (round3(center.0), round3(center.1)),
            rings_raw: exterior,
            adj_points,
        });
    }

    // --- Adjacency: shared quantized boundary points ---------------------
    let mut point_owners: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, p) in provinces.iter().enumerate() {
        for &q in &p.adj_points {
            point_owners.entry(q).or_default().push(i);
        }
    }
    let mut pair_counts: HashMap<(usize, usize), usize> = HashMap::new();
    for owners_at in point_owners.values() {
        if owners_at.len() < 2 || owners_at.len() > 8 {
            continue; // >8 provinces on one grid cell = quantization noise
        }
        for a in 0..owners_at.len() {
            for b in (a + 1)..owners_at.len() {
                let key = (owners_at[a].min(owners_at[b]), owners_at[a].max(owners_at[b]));
                *pair_counts.entry(key).or_insert(0) += 1;
            }
        }
    }
    let mut adjacency: Vec<Vec<u32>> = vec![Vec::new(); provinces.len()];
    let mut edges = 0usize;
    for (&(a, b), &count) in &pair_counts {
        if count >= ADJ_MIN_SHARED {
            adjacency[a].push(provinces[b].id);
            adjacency[b].push(provinces[a].id);
            edges += 1;
        }
    }
    for adj in &mut adjacency {
        adj.sort_unstable();
    }
    println!("adjacency edges: {edges}");

    // --- Geometry: simplify exterior rings -------------------------------
    let mut geometry: BTreeMap<u32, Vec<Vec<(f32, f32)>>> = BTreeMap::new();
    let mut total_pts = 0usize;
    for p in &provinces {
        let mut rings_out = Vec::new();
        // Largest ring by raw point count is always kept.
        let largest = p
            .rings_raw
            .iter()
            .enumerate()
            .max_by_key(|(_, r)| r.len())
            .map(|(i, _)| i)
            .unwrap();
        for (i, ring) in p.rings_raw.iter().enumerate() {
            let simplified = simplify(ring, SIMPLIFY_TOLERANCE_DEG);
            if simplified.len() >= MIN_RING_POINTS || i == largest {
                total_pts += simplified.len();
                rings_out.push(simplified.iter().map(|&(x, y)| (round3(x), round3(y))).collect());
            }
        }
        geometry.insert(p.id, rings_out);
    }
    println!("geometry points after simplify: {total_pts}");

    // --- Countries -------------------------------------------------------
    let notable = notable_countries();
    let mut tags: Vec<&String> = country_names.keys().collect();
    tags.sort();
    let mut country_defs = Vec::new();
    for tag in tags {
        let provs: Vec<&Prov> = provinces.iter().filter(|p| &p.owner == tag).collect();
        if provs.is_empty() {
            continue;
        }
        let meta = notable.get(tag.as_str());
        let capital = match meta {
            Some((_, cap, ..)) => {
                provs
                    .iter()
                    .min_by(|a, b| {
                        let da = (a.center.0 - cap.0).powi(2) + (a.center.1 - cap.1).powi(2);
                        let db = (b.center.0 - cap.0).powi(2) + (b.center.1 - cap.1).powi(2);
                        da.total_cmp(&db)
                    })
                    .unwrap()
                    .id
            }
            None => provs.iter().map(|p| p.id).min().unwrap(),
        };
        let (alignment, stability, industry, nuclear) = match meta {
            Some((al, _, st, ind, nuc)) => (*al, *st, *ind, *nuc),
            None => (Alignment::NonAligned, 60, 5, false),
        };
        country_defs.push(CountryDef {
            tag: CountryTag(tag.clone()),
            name: country_names[tag].clone(),
            alignment,
            capital: ProvinceId(capital),
            stability,
            industry,
            nuclear_power: nuclear,
        });
    }
    println!("countries: {}", country_defs.len());

    // --- Write outputs ---------------------------------------------------
    let province_defs: Vec<ProvinceDef> = provinces
        .iter()
        .map(|p| ProvinceDef {
            id: ProvinceId(p.id),
            name: p.name.clone(),
            owner: CountryTag(p.owner.clone()),
            terrain: Terrain::Plains, // TODO: classify from elevation raster
            center: p.center,
            population_k: 0, // TODO: HYDE 3.2 population raster
            adjacent: adjacency[(p.id - 1) as usize]
                .iter()
                .map(|&i| ProvinceId(i))
                .collect(),
        })
        .collect();

    let pretty = ron::ser::PrettyConfig::new().depth_limit(3);
    let header = "// GENERATED by tools/mapgen — do not hand-edit. Source: Natural Earth\n// 10m admin-1 (public domain) + tools/mapgen/owners_1950.csv.\n";

    let prov_path = root.join("assets/data/scenario/1950/provinces/world.ron");
    fs::write(
        &prov_path,
        format!("{header}{}", ron::ser::to_string_pretty(&province_defs, pretty.clone()).unwrap()),
    )
    .unwrap();

    let country_path = root.join("assets/data/scenario/1950/countries/generated.ron");
    fs::write(
        &country_path,
        format!("{header}{}", ron::ser::to_string_pretty(&country_defs, pretty).unwrap()),
    )
    .unwrap();

    fs::create_dir_all(root.join("assets/map")).unwrap();
    let geo_path = root.join("assets/map/world.geo.ron");
    fs::write(&geo_path, ron::to_string(&geometry).unwrap()).unwrap();

    println!(
        "wrote {} provinces, {} countries, geometry {:.1} MB",
        province_defs.len(),
        country_defs.len(),
        fs::metadata(&geo_path).unwrap().len() as f64 / 1e6
    );
}
