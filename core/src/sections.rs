// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Rolled steel H shapes (JIS G 3192 / TIS 1227, the sizes stocked in Thailand) and
//! reinforcing bars. Shape properties are computed from the plate dimensions and the root
//! radius, so they follow from the name; the four root fillets are included. Checked against
//! the JIS tables: H 300x150x6.5x9 A 46.78 cm² (table 46.78), Ix 7 209 cm⁴ (table 7 210).

use crate::model::{Props, Shape};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct HShape {
    pub name: String,
    /// mm
    pub d: f64,
    pub bf: f64,
    pub tw: f64,
    pub tf: f64,
    pub r: f64,
}

impl HShape {
    pub fn shape(&self) -> Shape {
        Shape::H { d: self.d / 1000.0, bf: self.bf / 1000.0, tw: self.tw / 1000.0, tf: self.tf / 1000.0, r: self.r / 1000.0 }
    }
    /// kg/m (7 850 kg/m³)
    pub fn mass(&self) -> f64 {
        h_props(self.d / 1000.0, self.bf / 1000.0, self.tw / 1000.0, self.tf / 1000.0, self.r / 1000.0).a * 7850.0
    }
}

/// d, bf, tw, tf, r in mm.
const H: [(f64, f64, f64, f64, f64); 33] = [
    (100.0, 50.0, 5.0, 7.0, 8.0),
    (125.0, 60.0, 6.0, 8.0, 8.0),
    (150.0, 75.0, 5.0, 7.0, 8.0),
    (175.0, 90.0, 5.0, 8.0, 8.0),
    (200.0, 100.0, 5.5, 8.0, 8.0),
    (250.0, 125.0, 6.0, 9.0, 8.0),
    (300.0, 150.0, 6.5, 9.0, 13.0),
    (350.0, 175.0, 7.0, 11.0, 13.0),
    (400.0, 200.0, 8.0, 13.0, 13.0),
    (450.0, 200.0, 9.0, 14.0, 13.0),
    (500.0, 200.0, 10.0, 16.0, 13.0),
    (600.0, 200.0, 11.0, 17.0, 13.0),
    (700.0, 300.0, 13.0, 24.0, 18.0),
    (800.0, 300.0, 14.0, 26.0, 18.0),
    (900.0, 300.0, 16.0, 28.0, 18.0),
    (148.0, 100.0, 6.0, 9.0, 8.0),
    (194.0, 150.0, 6.0, 9.0, 8.0),
    (244.0, 175.0, 7.0, 11.0, 13.0),
    (294.0, 200.0, 8.0, 12.0, 13.0),
    (340.0, 250.0, 9.0, 14.0, 13.0),
    (390.0, 300.0, 10.0, 16.0, 13.0),
    (440.0, 300.0, 11.0, 18.0, 13.0),
    (488.0, 300.0, 11.0, 18.0, 13.0),
    (588.0, 300.0, 12.0, 20.0, 13.0),
    (100.0, 100.0, 6.0, 8.0, 8.0),
    (125.0, 125.0, 6.5, 9.0, 8.0),
    (150.0, 150.0, 7.0, 10.0, 8.0),
    (175.0, 175.0, 7.5, 11.0, 13.0),
    (200.0, 200.0, 8.0, 12.0, 13.0),
    (250.0, 250.0, 9.0, 14.0, 13.0),
    (300.0, 300.0, 10.0, 15.0, 13.0),
    (350.0, 350.0, 12.0, 19.0, 13.0),
    (400.0, 400.0, 13.0, 21.0, 22.0),
];

fn fmt(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{x:.0}")
    } else {
        format!("{x}")
    }
}

/// Lipped channels (TIS 1228 cold-formed sections): h, b, lip d, t in mm.
pub const LIPPED: [(f64, f64, f64, f64); 12] = [
    (75.0, 45.0, 15.0, 2.3),
    (100.0, 50.0, 20.0, 2.3),
    (100.0, 50.0, 20.0, 3.2),
    (125.0, 50.0, 20.0, 2.3),
    (125.0, 50.0, 20.0, 3.2),
    (150.0, 50.0, 20.0, 2.3),
    (150.0, 50.0, 20.0, 3.2),
    (150.0, 65.0, 20.0, 3.2),
    (150.0, 75.0, 20.0, 3.2),
    (200.0, 75.0, 20.0, 3.2),
    (200.0, 75.0, 25.0, 4.0),
    (250.0, 75.0, 25.0, 4.5),
];

/// A lipped channel of the catalogue: its name as `Model::section_named` makes it, its
/// dimensions (m) and its mass (kg/m).
#[derive(Clone, Debug)]
pub struct LippedShape {
    pub name: String,
    pub h: f64,
    pub b: f64,
    pub d: f64,
    pub t: f64,
}

impl LippedShape {
    pub fn mass(&self) -> f64 {
        (self.h + 2.0 * self.b + 2.0 * self.d - 4.0 * self.t) * self.t * 7850.0
    }
}

/// The lipped channels, lightest first.
pub fn lipped_catalogue() -> Vec<LippedShape> {
    let mut v: Vec<LippedShape> = LIPPED.iter().map(|&(h, b, d, t)| LippedShape { name: format!("C {}x{}x{}x{}", fmt(h), fmt(b), fmt(d), fmt(t)), h: h / 1e3, b: b / 1e3, d: d / 1e3, t: t / 1e3 }).collect();
    v.sort_by(|a, b| a.mass().total_cmp(&b.mass()));
    v
}

/// The catalogue, lightest first.
pub fn catalogue() -> Vec<HShape> {
    let mut v: Vec<HShape> = H.iter().map(|&(d, bf, tw, tf, r)| HShape { name: format!("H {}x{}x{}x{}", fmt(d), fmt(bf), fmt(tw), fmt(tf)), d, bf, tw, tf, r }).collect();
    v.sort_by(|a, b| a.mass().total_cmp(&b.mass()));
    v
}

/// A catalogue shape by name, forgiving of spaces, dashes and case ("H-300x150x6.5x9").
pub fn find(name: &str) -> Option<HShape> {
    let key = |s: &str| s.to_lowercase().replace(' ', "").replace(['-', '×'], "x").trim_start_matches(['h', 'x']).to_string();
    let k = key(name);
    catalogue().into_iter().find(|s| key(&s.name) == k)
}

/// Properties of a rolled H shape (m): plates plus the four root fillets, each of area
/// (1 − π/4) r² with its centroid 0.2234 r from the corner.
pub fn h_props(d: f64, bf: f64, tw: f64, tf: f64, r: f64) -> Props {
    let af = (1.0 - std::f64::consts::PI / 4.0) * r * r;
    let hw = d - 2.0 * tf;
    let a = 2.0 * bf * tf + hw * tw + 4.0 * af;
    let yf = d / 2.0 - tf / 2.0;
    let yfil = d / 2.0 - tf - 0.2234 * r;
    let zfil = tw / 2.0 + 0.2234 * r;
    let iy = 2.0 * (bf * tf.powi(3) / 12.0 + bf * tf * yf * yf) + tw * hw.powi(3) / 12.0 + 4.0 * af * yfil * yfil;
    let iz = 2.0 * tf * bf.powi(3) / 12.0 + hw * tw.powi(3) / 12.0 + 4.0 * af * zfil * zfil;
    let zy = 2.0 * bf * tf * yf + tw * hw * hw / 4.0 + 4.0 * af * yfil;
    let zz = tf * bf * bf / 2.0 + hw * tw * tw / 4.0 + 4.0 * af * zfil;
    let j = (2.0 * bf * tf.powi(3) + hw * tw.powi(3)) / 3.0;
    let h0 = d - tf;
    let cw = tf * bf.powi(3) * h0 * h0 / 24.0;
    Props { a, iy, iz, j, cw, sy: iy / (d / 2.0), sz: iz / (bf / 2.0), zy, zz, ry: (iy / a).sqrt(), rz: (iz / a).sqrt() }
}

/// Bar area, m², of a diameter in mm.
pub fn bar_area(db_mm: f64) -> f64 {
    std::f64::consts::PI * (db_mm / 1000.0).powi(2) / 4.0
}

/// "DB20" for deformed bars, "RB9" for the round bars of 6 and 9 mm.
pub fn bar_name(db_mm: f64) -> String {
    if db_mm == 6.0 || db_mm == 9.0 {
        format!("RB{db_mm:.0}")
    } else {
        format!("DB{db_mm:.0}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jis_table_values() {
        // (name, A cm², Ix cm⁴) from JIS G 3192
        for (n, a, ix) in [("H 300x150x6.5x9", 46.78, 7210.0), ("H 200x100x5.5x8", 26.67, 1810.0), ("H 400x200x8x13", 83.37, 23500.0), ("H 400x400x13x21", 218.7, 66600.0), ("H 700x300x13x24", 231.5, 197000.0)] {
            let s = find(n).unwrap_or_else(|| panic!("{n}"));
            let p = h_props(s.d / 1e3, s.bf / 1e3, s.tw / 1e3, s.tf / 1e3, s.r / 1e3);
            assert!((p.a * 1e4 - a).abs() / a < 0.002, "{n} A {}", p.a * 1e4);
            assert!((p.iy * 1e8 - ix).abs() / ix < 0.01, "{n} Ix {}", p.iy * 1e8);
        }
        assert!(find("h-300x150x6.5x9").is_some());
        assert!(find("H 300 x 150 x 6.5 x 9").is_some());
    }
}
