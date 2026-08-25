//! Raster readers for the offline data pipeline. All grids are exposed as
//! lon/lat lookups on the WGS84 rectangle.
//!
//! Sources (see docs/research/geodata.md):
//! - HYDE 3.2.1 population, ESRI ASCII grid, 5 arcmin (CC0)
//! - ETOPO5 elevation, raw big-endian i16, 5 arcmin (public domain)
//! - Beck et al. Köppen-Geiger 1931-1960, GeoTIFF, 0.1° (CC BY 4.0)

use std::fs;
use std::path::Path;

/// A north-up global grid; row 0 is the northernmost row.
pub struct Grid<T> {
    pub ncols: usize,
    pub nrows: usize,
    /// Longitude of the west edge, latitude of the SOUTH edge (degrees).
    pub west: f64,
    pub south: f64,
    pub cellsize: f64,
    pub nodata: T,
    pub data: Vec<T>,
}

impl<T: Copy + PartialEq> Grid<T> {
    pub fn value_at(&self, lon: f64, lat: f64) -> Option<T> {
        let mut lon = lon;
        // Grids starting at 0°E wrap the antimeridian differently.
        if lon < self.west {
            lon += 360.0;
        }
        let col = ((lon - self.west) / self.cellsize).floor() as isize;
        let north = self.south + self.nrows as f64 * self.cellsize;
        let row = ((north - lat) / self.cellsize).floor() as isize;
        if col < 0 || row < 0 || col >= self.ncols as isize || row >= self.nrows as isize {
            return None;
        }
        let v = self.data[row as usize * self.ncols + col as usize];
        (v != self.nodata).then_some(v)
    }

    /// Cell center coordinates for iteration.
    pub fn cell_center(&self, row: usize, col: usize) -> (f64, f64) {
        let lon = self.west + (col as f64 + 0.5) * self.cellsize;
        let north = self.south + self.nrows as f64 * self.cellsize;
        let lat = north - (row as f64 + 0.5) * self.cellsize;
        // Normalize to [-180, 180) for grids that start at 0°E.
        (if lon >= 180.0 { lon - 360.0 } else { lon }, lat)
    }
}

/// ESRI ASCII grid (HYDE). Header then rows north-to-south.
pub fn read_asc(path: &Path) -> Grid<f32> {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut lines = text.lines();
    let mut header = std::collections::HashMap::new();
    let mut data_start = String::new();
    for line in lines.by_ref() {
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("").to_lowercase();
        if let Ok(_first_val) = key.parse::<f64>() {
            data_start = line.to_string();
            break;
        }
        let value: f64 = parts.next().unwrap().parse().unwrap();
        header.insert(key, value);
    }
    let ncols = header["ncols"] as usize;
    let nrows = header["nrows"] as usize;
    let nodata = *header.get("nodata_value").unwrap_or(&-9999.0) as f32;
    let mut data = Vec::with_capacity(ncols * nrows);
    for line in data_start
        .split_whitespace()
        .chain(lines.flat_map(str::split_whitespace))
    {
        data.push(line.parse::<f32>().unwrap_or(nodata));
    }
    assert_eq!(
        data.len(),
        ncols * nrows,
        "asc size mismatch in {}",
        path.display()
    );
    Grid {
        ncols,
        nrows,
        west: header["xllcorner"],
        south: header["yllcorner"],
        cellsize: header["cellsize"],
        nodata,
        data,
    }
}

/// ETOPO5.DAT: 2160 rows x 4320 cols of big-endian i16 meters, row 0 at
/// 90°N, col 0 at 0°E (wraps east to 360°).
pub fn read_etopo5(path: &Path) -> Grid<i16> {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    assert_eq!(bytes.len(), 4320 * 2160 * 2, "unexpected ETOPO5 size");
    let data: Vec<i16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_be_bytes(*c))
        .collect();
    Grid {
        ncols: 4320,
        nrows: 2160,
        west: 0.0,
        south: -90.0,
        cellsize: 5.0 / 60.0,
        nodata: i16::MIN, // ETOPO5 has no nodata; sentinel never matches
        data,
    }
}

/// Beck et al. Köppen-Geiger GeoTIFF, global 0.1°: 3600x1800 u8, 0 = ocean,
/// classes 1..=30.
pub fn read_koppen(path: &Path) -> Grid<u8> {
    let mut bytes =
        fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    // The file is palette-color (photometric = 3), which the tiff crate
    // refuses to interpret — but the palette *indices* ARE the Köppen class
    // ids we want. Patch the in-memory header to BlackIsZero (1) so the
    // indices decode as plain 8-bit grayscale.
    assert_eq!(&bytes[0..2], b"II", "expected little-endian classic TIFF");
    let ifd = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let entries = u16::from_le_bytes(bytes[ifd..ifd + 2].try_into().unwrap()) as usize;
    let mut patched = false;
    for i in 0..entries {
        let e = ifd + 2 + i * 12;
        let tag = u16::from_le_bytes(bytes[e..e + 2].try_into().unwrap());
        if tag == 262 {
            bytes[e + 8] = 1; // PhotometricInterpretation: RGBPalette -> BlackIsZero
            patched = true;
        }
    }
    assert!(patched, "photometric tag not found in {}", path.display());
    let mut decoder =
        tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).expect("tiff decoder");
    let (w, h) = decoder.dimensions().expect("tiff dims");
    let data = match decoder.read_image().expect("tiff read") {
        tiff::decoder::DecodingResult::U8(v) => v,
        _ => panic!("koppen should decode as u8 class indices"),
    };
    assert_eq!(data.len(), (w * h) as usize);
    Grid {
        ncols: w as usize,
        nrows: h as usize,
        west: -180.0,
        south: -90.0,
        cellsize: 360.0 / w as f64,
        nodata: 0,
        data,
    }
}
