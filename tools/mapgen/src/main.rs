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

mod rasters;

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
        ("ROC", (WesternBloc, (121.56, 25.03), 50, 3, false)), // Nationalists on Taiwan
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
        ("CSK", (EasternBloc, (14.42, 50.09), 65, 15, false)), // Czechoslovakia
        ("HUN", (EasternBloc, (19.04, 47.50), 60, 8, false)),
        ("ROU", (EasternBloc, (26.10, 44.43), 60, 8, false)),
        ("BGR", (EasternBloc, (23.32, 42.70), 60, 5, false)),
        ("ALB", (EasternBloc, (19.82, 41.33), 55, 1, false)),
        ("MNG", (EasternBloc, (106.92, 47.92), 60, 1, false)),
        ("YUG", (NonAligned, (20.46, 44.82), 60, 10, false)), // Tito-Stalin split '48
        ("IND", (NonAligned, (77.21, 28.61), 60, 15, false)),
    ])
}

/// Hand-picked national map colors (sRGB), vintage-atlas flavored:
/// British imperial pink, French blue, Soviet red, Portuguese green...
/// Countries not listed get a procedural golden-angle hue.
fn national_colors() -> BTreeMap<&'static str, (u8, u8, u8)> {
    BTreeMap::from([
        ("USA", (70, 105, 170)),  // union blue
        ("SOV", (178, 34, 41)),   // soviet red
        ("GBR", (219, 112, 130)), // imperial pink
        ("FRA", (85, 110, 190)),  // royal blue
        ("PRC", (196, 78, 46)),   // vermilion
        ("ROC", (98, 88, 160)),   // KMT violet-blue
        ("PRK", (146, 36, 60)),   // dark crimson
        ("KOR", (88, 148, 180)),  // taegukgi cyan
        ("FRG", (108, 118, 126)), // field grey
        ("GDR", (130, 60, 66)),   // rust red
        ("JAP", (214, 184, 96)),  // imperial gold
        ("ITA", (96, 158, 102)),  // verde
        ("ESP", (208, 176, 70)),  // spanish yellow
        ("POR", (36, 130, 104)),  // portuguese green
        ("NLD", (228, 132, 52)),  // orange
        ("BEL", (176, 138, 66)),  // brabant gold
        ("CAN", (204, 102, 92)),  // dominion red
        ("AUS", (150, 168, 96)),  // eucalypt
        ("NZL", (112, 160, 136)),
        ("YUG", (104, 116, 148)), // partisan slate
        ("POL", (188, 82, 100)),
        ("CSK", (140, 92, 146)),
        ("HUN", (120, 156, 82)),
        ("ROU", (156, 132, 176)),
        ("BGR", (110, 140, 110)),
        ("ALB", (150, 76, 62)),
        ("MNG", (168, 118, 152)),
        ("IND", (222, 146, 70)),  // saffron
        ("PAK", (66, 122, 94)),   // pakistan green
        ("IDN", (172, 84, 78)),
        ("TUR", (158, 104, 72)),
        ("IRN", (140, 152, 110)),
        ("EGY", (192, 162, 104)),
        ("ETH", (118, 96, 64)),
        ("BRA", (94, 162, 118)),
        ("ARG", (134, 172, 200)),
        ("MEX", (124, 142, 86)),
        ("DNK", (196, 110, 100)),
        ("NOR", (110, 130, 180)),
        ("SWE", (120, 160, 190)),
        ("FIN", (160, 170, 190)),
        ("CHE", (190, 120, 110)),
        ("AUT", (170, 170, 150)),
        ("GRC", (120, 150, 200)),
        ("THA", (180, 120, 140)),
        ("PHL", (110, 140, 180)),
        ("SAU", (130, 150, 80)),
        ("ISR", (130, 160, 200)),
        ("ZAF", (190, 140, 80)),
    ])
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
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

struct Prov {
    id: u32,
    name: String,
    owner: String,
    center: (f32, f32),
    rings_raw: Vec<Ring>, // exterior rings, unsimplified
    adj_points: Vec<(i64, i64)>,
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

    // --- Population & terrain from rasters -------------------------------
    let enriched = enrich_provinces(&provinces, tool_dir);

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
    let colors = national_colors();
    let mut tags: Vec<&String> = country_names.keys().collect();
    tags.sort();
    let mut country_defs = Vec::new();
    for (tag_idx, tag) in tags.into_iter().enumerate() {
        // Golden-angle hue walk for countries without a hand color, muted
        // saturation so hand-picked majors stay visually dominant.
        let color = colors.get(tag.as_str()).copied().unwrap_or_else(|| {
            hsl_to_rgb(tag_idx as f64 * 137.508, 0.32, 0.55)
        });
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
            color,
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
        .zip(&enriched)
        .map(|(p, e)| ProvinceDef {
            id: ProvinceId(p.id),
            name: p.name.clone(),
            owner: CountryTag(p.owner.clone()),
            terrain: e.terrain,
            center: p.center,
            population_k: e.population_k,
            urban_k: e.urban_k,
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

    // --- Country borders --------------------------------------------------
    // Precise inter-country boundary polylines from the RAW shared-edge
    // topology (Natural Earth neighbors share identical vertices), chained
    // and simplified. Rendered as emphasized border lines by the app.
    let borders = extract_country_borders(&provinces);
    let border_path = root.join("assets/map/country_borders.ron");
    fs::write(&border_path, ron::to_string(&borders).unwrap()).unwrap();
    println!(
        "country borders: {} polylines, {} points",
        borders.len(),
        borders.iter().map(Vec::len).sum::<usize>()
    );

    println!(
        "wrote {} provinces, {} countries, geometry {:.1} MB",
        province_defs.len(),
        country_defs.len(),
        fs::metadata(&geo_path).unwrap().len() as f64 / 1e6
    );
}

// --- Population & terrain enrichment -----------------------------------

pub struct Enriched {
    pub population_k: u32,
    pub urban_k: u32,
    pub terrain: Terrain,
}

fn point_in_province(lon: f64, lat: f64, p: &Prov) -> bool {
    let mut inside = false;
    for ring in &p.rings_raw {
        let n = ring.len();
        if n < 3 {
            continue;
        }
        let mut j = n - 1;
        for i in 0..n {
            let (a, b) = (ring[i], ring[j]);
            if (a.1 > lat) != (b.1 > lat)
                && lon < (b.0 - a.0) * (lat - a.1) / (b.1 - a.1) + a.0
            {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

/// Bucket provinces by 1°x1° cells of their bounding boxes, candidates
/// sorted smallest-bbox-first so enclaves (Lesotho) win over enclosers.
struct SpatialIndex {
    buckets: HashMap<(i32, i32), Vec<usize>>,
    bboxes: Vec<(f64, f64, f64, f64)>, // west, south, east, north
}

impl SpatialIndex {
    fn build(provinces: &[Prov]) -> Self {
        let mut bboxes = Vec::with_capacity(provinces.len());
        for p in provinces {
            let (mut w, mut s, mut e, mut n) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for pt in p.rings_raw.iter().flatten() {
                w = w.min(pt.0);
                s = s.min(pt.1);
                e = e.max(pt.0);
                n = n.max(pt.1);
            }
            bboxes.push((w, s, e, n));
        }
        let mut buckets: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, &(w, s, e, n)) in bboxes.iter().enumerate() {
            for bx in (w.floor() as i32)..=(e.floor() as i32) {
                for by in (s.floor() as i32)..=(n.floor() as i32) {
                    buckets.entry((bx, by)).or_default().push(i);
                }
            }
        }
        for list in buckets.values_mut() {
            list.sort_by(|a, b| {
                let area = |i: usize| {
                    let (w, s, e, n) = bboxes[i];
                    (e - w) * (n - s)
                };
                area(*a).total_cmp(&area(*b))
            });
        }
        Self { buckets, bboxes }
    }

    fn find(&self, provinces: &[Prov], lon: f64, lat: f64) -> Option<usize> {
        let key = (lon.floor() as i32, lat.floor() as i32);
        for &i in self.buckets.get(&key)? {
            let (w, s, e, n) = self.bboxes[i];
            if lon >= w && lon <= e && lat >= s && lat <= n
                && point_in_province(lon, lat, &provinces[i])
            {
                return Some(i);
            }
        }
        None
    }
}

/// Beck et al. class ids: 1-2 tropical rainforest/monsoon, 3 savanna,
/// 4-5 hot/cold desert, 6-7 steppe, 8-16 temperate C, 17-28 continental D
/// (suffix c/d = subarctic), 29 tundra, 30 ice cap.
fn classify(
    elev_mean: f64,
    elev_std: f64,
    koppen_major: u8,
    pop_density_km2: f64,
    urban_share: f64,
) -> Terrain {
    // Urban by three routes, because HYDE's 1950 urban fractions run low
    // outside the West: strongly-urbanized (Western capitals), dense with
    // some urbanization (Shanghai at ~920/km²), or overwhelming raw
    // density (Seoul >4000/km²). Densest purely-rural provinces (Nile,
    // Java, Bengal) run 300-1000/km² with near-zero urban share.
    if (pop_density_km2 > 400.0 && urban_share > 0.45)
        || (pop_density_km2 > 900.0 && urban_share > 0.15)
        || pop_density_km2 > 1500.0
    {
        return Terrain::Urban;
    }
    if elev_mean > 2000.0 || elev_std > 650.0 {
        return Terrain::Mountain;
    }
    if elev_mean > 900.0 || elev_std > 280.0 {
        return Terrain::Hills;
    }
    match koppen_major {
        1 | 2 => Terrain::Jungle,
        4 | 5 => Terrain::Desert,
        29 | 30 => Terrain::Tundra,
        // Subarctic: taiga forest regardless of population.
        19 | 20 | 23 | 24 | 27 | 28 => Terrain::Forest,
        // Humid continental & oceanic: forest where thinly settled,
        // farmland plains where people actually live.
        15 | 16 | 17 | 18 | 21 | 22 | 25 | 26 => {
            if pop_density_km2 < 15.0 {
                Terrain::Forest
            } else {
                Terrain::Plains
            }
        }
        _ => Terrain::Plains,
    }
}

/// Hand overrides for provinces the heuristic gets wrong, by NE admin-1
/// name. Keep short; justify each entry.
fn terrain_overrides() -> &'static [(&'static str, Terrain)] {
    &[
        // Chinese municipalities: HYDE 1950 urban shares for China run
        // near zero, and Beijing's NW mountains skew elevation sigma.
        ("Shanghai", Terrain::Urban),
        ("Beijing", Terrain::Urban),
        ("Tianjin", Terrain::Urban),
        // Cairo governorate: ~2M in 1950, reads Desert from surrounding
        // Köppen cells.
        ("Al Qahirah", Terrain::Urban),
        // Nile delta farmland governorate that trips the density gate.
        ("Al Gharbiyah", Terrain::Plains),
    ]
}

fn enrich_provinces(provinces: &[Prov], tool_dir: &Path) -> Vec<Enriched> {
    let data_dir = tool_dir.join("data");
    let popc = rasters::read_asc(&data_dir.join("popc_1950AD.asc"));
    let urbc = rasters::read_asc(&data_dir.join("urbc_1950AD.asc"));
    let elev = rasters::read_etopo5(&data_dir.join("ETOPO5.DAT"));
    let koppen = rasters::read_koppen(&data_dir.join("1931_1960/koppen_geiger_0p1.tif"));
    let index = SpatialIndex::build(provinces);

    // Population pass: assign every populated HYDE cell to a province.
    let mut pop_sum = vec![0.0f64; provinces.len()];
    let mut urb_sum = vec![0.0f64; provinces.len()];
    let mut unassigned: Vec<(f64, f64, f64, f64)> = Vec::new(); // lon, lat, pop, urb
    for row in 0..popc.nrows {
        for col in 0..popc.ncols {
            let v = popc.data[row * popc.ncols + col];
            if v <= 0.0 || v == popc.nodata {
                continue;
            }
            let (lon, lat) = popc.cell_center(row, col);
            let urb = urbc.value_at(lon, lat).filter(|u| *u > 0.0).unwrap_or(0.0);
            match index.find(provinces, lon, lat) {
                Some(i) => {
                    pop_sum[i] += v as f64;
                    urb_sum[i] += urb as f64;
                }
                None => unassigned.push((lon, lat, v as f64, urb as f64)),
            }
        }
    }
    // Coastal cells that missed every polygon: nearest province center
    // within ~3 degrees.
    let mut dropped = 0.0f64;
    for &(lon, lat, v, u) in &unassigned {
        let mut best: Option<(usize, f64)> = None;
        for dx in -3i32..=3 {
            for dy in -3i32..=3 {
                let key = (lon.floor() as i32 + dx, lat.floor() as i32 + dy);
                if let Some(list) = index.buckets.get(&key) {
                    for &i in list {
                        let c = provinces[i].center;
                        let d = (c.0 as f64 - lon).powi(2) + (c.1 as f64 - lat).powi(2);
                        if best.is_none_or(|(_, bd)| d < bd) {
                            best = Some((i, d));
                        }
                    }
                }
            }
        }
        match best {
            Some((i, d)) if d < 9.0 => {
                pop_sum[i] += v;
                urb_sum[i] += u;
            }
            _ => dropped += v,
        }
    }
    println!(
        "population: {:.0}M assigned, {:.1}M in coastal fallback, {:.2}M dropped",
        pop_sum.iter().sum::<f64>() / 1e6,
        unassigned.iter().map(|c| c.2).sum::<f64>() / 1e6,
        dropped / 1e6
    );

    // Terrain pass: sample each province interior at 0.1°.
    let mut out = Vec::with_capacity(provinces.len());
    for (i, p) in provinces.iter().enumerate() {
        let (w, s, e, n) = index.bboxes[i];
        let step = 0.1;
        let mut samples: Vec<(f64, f64)> = Vec::new();
        let mut lat = s + step / 2.0;
        while lat < n {
            let mut lon = w + step / 2.0;
            while lon < e {
                if point_in_province(lon, lat, p) {
                    samples.push((lon, lat));
                }
                lon += step;
            }
            lat += step;
        }
        if samples.is_empty() {
            samples.push((p.center.0 as f64, p.center.1 as f64));
        }
        let elevs: Vec<f64> = samples
            .iter()
            .filter_map(|&(lon, lat)| elev.value_at(lon, lat).map(|v| v.max(0) as f64))
            .collect();
        let elev_mean = elevs.iter().sum::<f64>() / elevs.len().max(1) as f64;
        let elev_std = (elevs
            .iter()
            .map(|v| (v - elev_mean).powi(2))
            .sum::<f64>()
            / elevs.len().max(1) as f64)
            .sqrt();
        let mut histogram = [0u32; 31];
        for &(lon, lat) in &samples {
            if let Some(k) = koppen.value_at(lon, lat) {
                histogram[(k as usize).min(30)] += 1;
            }
        }
        let koppen_major = histogram
            .iter()
            .enumerate()
            .skip(1)
            .max_by_key(|(_, c)| **c)
            .map(|(k, c)| if *c > 0 { k as u8 } else { 0 })
            .unwrap_or(0);
        // Sample count -> approximate area (0.1° cell shrinks with latitude).
        let mid_lat = (s + n) / 2.0;
        let cell_km2 = 11.132 * 11.132 * mid_lat.to_radians().cos().max(0.05);
        let area_km2 = samples.len() as f64 * cell_km2;
        let density = pop_sum[i] / area_km2.max(1.0);
        let urban_share = if pop_sum[i] > 0.0 { urb_sum[i] / pop_sum[i] } else { 0.0 };
        out.push(Enriched {
            population_k: (pop_sum[i] / 1000.0).round() as u32,
            urban_k: (urb_sum[i].min(pop_sum[i]) / 1000.0).round() as u32,
            terrain: terrain_overrides()
                .iter()
                .find(|(name, _)| *name == p.name)
                .map(|(_, t)| *t)
                .unwrap_or_else(|| {
                    classify(elev_mean, elev_std, koppen_major, density, urban_share)
                }),
        });
    }
    let mut terrain_counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in &out {
        *terrain_counts.entry(format!("{:?}", e.terrain)).or_default() += 1;
    }
    println!("terrain distribution: {terrain_counts:?}");
    out
}

// --- Country border extraction ------------------------------------------

/// Segments of raw ring geometry shared by provinces of DIFFERENT owners,
/// chained into polylines and simplified. Quantized endpoints are the
/// matching keys; emitted coordinates are the actual raw vertices.
fn extract_country_borders(provinces: &[Prov]) -> Vec<Vec<(f32, f32)>> {
    type Q = (i64, i64);
    type Seg = ((f64, f64), (f64, f64));
    // Segment key -> (owners seen, one representative raw segment).
    let mut segments: HashMap<(Q, Q), (Vec<&str>, Seg)> = HashMap::new();
    for p in provinces {
        for ring in &p.rings_raw {
            for w in ring.windows(2) {
                let (a, b) = (w[0], w[1]);
                let (qa, qb) = (quantize(a), quantize(b));
                if qa == qb {
                    continue;
                }
                let key = if qa < qb { (qa, qb) } else { (qb, qa) };
                let entry = segments.entry(key).or_insert_with(|| (Vec::new(), (a, b)));
                if !entry.0.contains(&p.owner.as_str()) {
                    entry.0.push(&p.owner);
                }
            }
        }
    }
    // Border segments: shared by at least two different owners.
    let border: HashMap<(Q, Q), Seg> = segments
        .into_iter()
        .filter(|(_, (owners, _))| owners.len() >= 2)
        .map(|(k, (_, seg))| (k, seg))
        .collect();

    // Endpoint -> attached segment keys, for chaining.
    let mut at_point: HashMap<Q, Vec<(Q, Q)>> = HashMap::new();
    for &(qa, qb) in border.keys() {
        at_point.entry(qa).or_default().push((qa, qb));
        at_point.entry(qb).or_default().push((qa, qb));
    }

    let mut visited: std::collections::HashSet<(Q, Q)> = Default::default();
    let mut polylines = Vec::new();
    let mut keys: Vec<&(Q, Q)> = border.keys().collect();
    keys.sort(); // deterministic output
    for &start in &keys {
        if visited.contains(start) {
            continue;
        }
        // Walk both directions from this segment while the path is
        // unbranched (endpoint touches exactly 2 border segments).
        let mut chain: std::collections::VecDeque<Q> = [start.0, start.1].into_iter().collect();
        visited.insert(*start);
        for forward in [true, false] {
            loop {
                let tip = if forward {
                    *chain.back().unwrap()
                } else {
                    *chain.front().unwrap()
                };
                let candidates = &at_point[&tip];
                if candidates.len() != 2 {
                    break; // junction or dead end
                }
                let Some(next) = candidates.iter().find(|k| !visited.contains(*k)) else {
                    break;
                };
                visited.insert(*next);
                let far = if next.0 == tip { next.1 } else { next.0 };
                if forward {
                    chain.push_back(far);
                } else {
                    chain.push_front(far);
                }
            }
        }
        // Quantized chain -> raw coordinates via representative segments.
        let pts: Vec<(f64, f64)> = chain
            .iter()
            .map(|q| (q.0 as f64 / ADJ_QUANT, q.1 as f64 / ADJ_QUANT))
            .collect();
        let simplified = simplify(&pts, 0.02);
        if simplified.len() >= 2 {
            polylines.push(simplified.iter().map(|&(x, y)| (round3(x), round3(y))).collect());
        }
    }
    polylines
}
