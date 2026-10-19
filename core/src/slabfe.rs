// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Slabs as plates: each panel meshed into MITC4 shell quads (the Stabileo engine's), joined
//! to the beams and columns at shared nodes, and designed from the element moments and shears.
//!
//! Mesh: the grid lines of a panel are its edges, the x and y of every node of the model lying
//! in it (columns, beam ends, crossing beams) and as many lines between as keep the elements
//! within the mesh size. Nodes are ordered anticlockwise in plan, so each quad's local x is the
//! global x and its normal points up: mx, my, mxy are moments per metre about the global axes.
//!
//! Design (Wood 1968, Armer 1968): the bottom moments mx + |mxy|, my + |mxy| and the top
//! moments mx − |mxy|, my − |mxy| (each corrected when one comes out with the wrong sign),
//! enveloped over the ULS combinations, give the bars of each direction and face: the bottom
//! bars from the middle half of the panel and the top bars from the quarter next to each
//! edge. One-way shear: the transverse shears qx, qy against φVc, outside d from the
//! supports. Punching at columns inside a panel (flat slabs) is not designed and is flagged.

use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::rc::{self, RcMat, SlabDesign, SlabStrip};

/// How slabs are analysed and designed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlabMethod {
    /// Load to the beams by the 45° lines; moments by ACI 318-63 Method 2 / ACI 318-19 6.5.
    #[default]
    Coefficients,
    /// Plate finite elements analysed with the frame; bars from the element moments.
    Plate,
}

/// A panel's mesh: its grid lines and z.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub slab: Id,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub z: f64,
}

impl Mesh {
    pub fn point(&self, i: usize, j: usize) -> [f64; 3] {
        [self.xs[i], self.ys[j], self.z]
    }
    pub fn cells(&self) -> usize {
        (self.xs.len() - 1) * (self.ys.len() - 1)
    }
}

fn lines(lo: f64, hi: f64, mut fixed: Vec<f64>, size: f64) -> Vec<f64> {
    fixed.push(lo);
    fixed.push(hi);
    fixed.retain(|x| *x >= lo - 1e-9 && *x <= hi + 1e-9);
    fixed.sort_by(f64::total_cmp);
    fixed.dedup_by(|a, b| (*a - *b).abs() < 0.02);
    let mut out = vec![fixed[0]];
    for w in fixed.windows(2) {
        let n = ((w[1] - w[0]) / size).ceil().max(1.0) as usize;
        for k in 1..=n {
            out.push(w[0] + (w[1] - w[0]) * k as f64 / n as f64);
        }
    }
    out
}

pub fn meshes(m: &Model) -> Vec<Mesh> {
    let size = m.design.slab_mesh.clamp(0.1, 5.0);
    m.slabs
        .iter()
        .map(|s| {
            let (lo, hi) = (s.lo(), s.hi());
            let inside: Vec<&Node> = m.nodes.iter().filter(|n| (n.z - s.z).abs() < 0.05 && n.x >= lo.0 - 1e-6 && n.x <= hi.0 + 1e-6 && n.y >= lo.1 - 1e-6 && n.y <= hi.1 + 1e-6).collect();
            Mesh { slab: s.id, xs: lines(lo.0, hi.0, inside.iter().map(|n| n.x).collect(), size), ys: lines(lo.1, hi.1, inside.iter().map(|n| n.y).collect(), size), z: s.z }
        })
        .collect()
}

/// The moments and shears of one plate element (kN·m/m, kN/m), at its centre.
#[derive(Clone, Debug, Default, Serialize)]
pub struct PlateResult {
    pub slab: Id,
    /// Centre and size of the element, m.
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
    pub mx: f64,
    pub my: f64,
    pub mxy: f64,
    pub qx: f64,
    pub qy: f64,
    /// Vertical displacement of its centre (mean of its nodes), m.
    pub uz: f64,
}

/// Wood–Armer design moments of (mx, my, mxy): (bottom x, bottom y, top x, top y), each ≥ 0.
pub fn wood_armer(mx: f64, my: f64, mxy: f64) -> (f64, f64, f64, f64) {
    let a = mxy.abs();
    // bottom (sagging positive)
    let (mut bx, mut by) = (mx + a, my + a);
    if bx < 0.0 {
        bx = 0.0;
        by = if mx.abs() > 1e-12 { my + mxy * mxy / mx.abs() } else { my };
    } else if by < 0.0 {
        by = 0.0;
        bx = if my.abs() > 1e-12 { mx + mxy * mxy / my.abs() } else { mx };
    }
    // top (hogging negative)
    let (mut tx, mut ty) = (mx - a, my - a);
    if tx > 0.0 {
        tx = 0.0;
        ty = if mx.abs() > 1e-12 { my - mxy * mxy / mx.abs() } else { my };
    } else if ty > 0.0 {
        ty = 0.0;
        tx = if my.abs() > 1e-12 { mx - mxy * mxy / my.abs() } else { mx };
    }
    (bx.max(0.0), by.max(0.0), (-tx).max(0.0), (-ty).max(0.0))
}

/// Designs a slab from its plate results under every ULS combination (`uls`) and under the
/// service loads (`sls`, for the deflection).
pub fn design(slab: &Slab, uls: &[Vec<&PlateResult>], sls: &[&PlateResult], columns_inside: usize, mat: &RcMat, set: &DesignSettings) -> SlabDesign {
    let (lx, ly) = (slab.lx(), slab.ly());
    let (lo, hi) = (slab.lo(), slab.hi());
    let h = slab.h * 1000.0;
    let cover = set.cover_slab * 1000.0;
    let db0 = set.slab_bars.first().copied().unwrap_or(9.0);
    // the short span's bars in the outer layer
    let short_is_x = lx <= ly;
    let d_outer = h - cover - db0 / 2.0;
    let d_inner = d_outer - db0;
    let (dx, dy) = if short_is_x { (d_outer, d_inner) } else { (d_inner, d_outer) };
    let as_min = 0.0018 * 1000.0 * h;
    let smax = (2.0 * h).min(450.0);
    // zones: the middle half for bottom bars, the quarter next to each edge for top bars
    let mid = |p: &PlateResult| (p.x - lo.0) > 0.25 * lx - 1e-9 && (hi.0 - p.x) > 0.25 * lx - 1e-9 && (p.y - lo.1) > 0.25 * ly - 1e-9 && (hi.1 - p.y) > 0.25 * ly - 1e-9;
    let near_x_edges = |p: &PlateResult| (p.x - lo.0) < 0.25 * lx + 1e-9 || (hi.0 - p.x) < 0.25 * lx + 1e-9;
    let near_y_edges = |p: &PlateResult| (p.y - lo.1) < 0.25 * ly + 1e-9 || (hi.1 - p.y) < 0.25 * ly + 1e-9;
    let (mut bx, mut by, mut tx, mut ty) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let (mut bx_all, mut by_all) = (0.0f64, 0.0f64);
    let mut vmax = 0.0f64;
    for combo in uls {
        for p in combo {
            let (a, b, c, d) = wood_armer(p.mx, p.my, p.mxy);
            if mid(p) {
                bx = bx.max(a);
                by = by.max(b);
            }
            bx_all = bx_all.max(a);
            by_all = by_all.max(b);
            // top x bars resist mx: they are needed where the x-direction moment hogs, at the
            // edges across x (x near lo/hi); likewise for y
            if near_x_edges(p) {
                tx = tx.max(c);
            }
            if near_y_edges(p) {
                ty = ty.max(d);
            }
            // one-way shear outside d (and outside the edge elements) from the panel edges
            let dm = (dx.max(dy) / 1000.0).max(p.dx.max(p.dy) / 2.0 + 1e-6);
            if p.x - lo.0 > dm && hi.0 - p.x > dm && p.y - lo.1 > dm && hi.1 - p.y > dm {
                vmax = vmax.max(p.qx.abs()).max(p.qy.abs());
            }
        }
    }
    let mut strips = vec![];
    let mut ratio: f64 = 0.0;
    let mut governs = String::new();
    let mut notes = vec![];
    let mut push = |dir: &str, loc: &str, mu: f64, d: f64, strips: &mut Vec<SlabStrip>| {
        let (as_req, as_prov, _, bars, fb) = rc::slab_bars(mu, d, as_min, smax, &set.slab_bars, mat.fc, mat.fy, mat.fyt);
        let cap = rc::phi_mn(as_prov, 1000.0, d, mat.fc, fb) / 1e6;
        let r = if mu > 0.0 { mu / cap } else { 0.0 };
        if r > ratio {
            ratio = r;
            governs = format!("flexure, {dir} {loc}");
        }
        strips.push(SlabStrip { direction: dir.into(), location: loc.into(), mu, as_req, as_prov, bars, ratio: r });
    };
    push("x", "middle (bottom)", bx, dx, &mut strips);
    push("y", "middle (bottom)", by, dy, &mut strips);
    if bx_all > bx * 1.05 || by_all > by * 1.05 {
        push("x", "elsewhere (bottom)", bx_all, dx, &mut strips);
        push("y", "elsewhere (bottom)", by_all, dy, &mut strips);
    }
    push("x", "edges across x (top)", tx, dx, &mut strips);
    push("y", "edges across y (top)", ty, dy, &mut strips);
    let phi_vc = 0.75 * 0.17 * mat.fc.min(69.0).sqrt() * 1000.0 * dx.min(dy) / 1000.0;
    let rv = vmax / phi_vc;
    if rv > ratio {
        ratio = rv;
        governs = "one-way shear".into();
    }
    // minimum thickness (Table 8.3.1.2, stiff beams) and the plate's own deflection
    let (s, l) = (lx.min(ly), lx.max(ly));
    let h_min = (l * 1000.0 * (0.8 + mat.fy / 1400.0) / (36.0 + 9.0 * l / s)).max(90.0);
    if h < h_min {
        notes.push(format!("h {h:.0} mm is under the Table 8.3.1.2 minimum {h_min:.0} mm (stiff beams assumed)"));
        if h_min / h > ratio {
            ratio = h_min / h;
            governs = "minimum thickness".into();
        }
    }
    if let (Some(lo_z), Some(hi_z)) = (sls.iter().map(|p| p.uz).reduce(f64::max), sls.iter().map(|p| p.uz).reduce(f64::min)) {
        notes.push(format!("plate deflection under D + L: {:.1} mm below the panel's least deflected point (gross section, elastic; cracking and creep increase it)", (lo_z - hi_z) * 1000.0));
    }
    if columns_inside > 0 {
        notes.push(format!("{columns_inside} column(s) inside the panel: punching shear is NOT checked (ACI 318-19 22.6)"));
    }
    notes.push("bars from the element moments by Wood–Armer, enveloped over the ULS combinations; element centre values (no smoothing across peaks)".into());
    let n_el = uls.first().map(|c| c.len()).unwrap_or(0);
    SlabDesign {
        slab: slab.id,
        bim_id: slab.bim_id.clone(),
        name: slab.name.clone(),
        lx,
        ly,
        h: slab.h,
        method: format!("plate FE (MITC4, {n_el} elements, Wood–Armer)"),
        discontinuous_edges: 0,
        wu: 0.0,
        strips,
        h_min,
        vu: vmax,
        phi_vc,
        ratio,
        governs,
        ok: ratio <= set.target_ratio + 1e-9,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wood_armer_cases() {
        // pure bending: no twist, the moments themselves
        assert_eq!(wood_armer(10.0, 5.0, 0.0), (10.0, 5.0, 0.0, 0.0));
        // pure twist: |mxy| on both faces and both directions
        assert_eq!(wood_armer(0.0, 0.0, 4.0), (4.0, 4.0, 4.0, 4.0));
        // bottom x would be negative: zero, and y takes my + mxy²/|mx|
        let (bx, by, _, _) = wood_armer(-10.0, 6.0, 2.0);
        assert_eq!(bx, 0.0);
        assert!((by - (6.0 + 4.0 / 10.0)).abs() < 1e-12);
    }
}
