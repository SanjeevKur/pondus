// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Cross-section properties of any polygon by the Stabileo engine's section analysis: area,
//! centroid, second moments and moduli (exact polygon integrals), and the finite-element
//! solutions of Saint-Venant torsion (J), transverse shear (the shear centre) and warping
//! (Cw), on a triangle mesh. Units: m.

use dedaliano_engine::section::mesh::{mesh_section, MeshParams};
use dedaliano_engine::section::poisson::SolveStrategy;
use dedaliano_engine::section::shear::{solve_shear, ShearInertia};
use dedaliano_engine::section::torsion::solve_torsion;
use dedaliano_engine::section::warping::solve_warping;
use dedaliano_engine::section::{analyze_section, SectionInput, SectionPolygon};
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct SectionFe {
    pub a: f64,
    /// Centroid (y, z) and the shear centre relative to it.
    pub centroid: [f64; 2],
    pub shear_centre: [f64; 2],
    /// About the horizontal (y) and vertical (z) centroidal axes of the polygon's frame.
    pub iy: f64,
    pub iz: f64,
    pub iyz: f64,
    pub zy: f64,
    pub zz: f64,
    pub j: f64,
    pub cw: f64,
    pub elements: usize,
}

/// Analyses a solid polygon (counter-clockwise or not) with the mesh fine enough for about
/// `n` triangles.
pub fn analyze(outline: &[[f64; 2]], n: usize) -> Result<SectionFe, String> {
    let poly = SectionPolygon { vertices: outline.to_vec(), material_id: 0, is_void: false };
    let p = analyze_section(&SectionInput { polygons: vec![poly.clone()], modular_ratios: Default::default() })?;
    let mesh = mesh_section(&[poly], &MeshParams { max_area: (p.a / n.max(50) as f64).max(1e-12), ..Default::default() })?;
    let c = [p.yc, p.zc];
    let t = solve_torsion(&mesh, SolveStrategy::Sparse)?;
    // the engine's iy is about the horizontal axis (bending in z)
    let sh = solve_shear(&mesh, c, ShearInertia { iy: p.iy, iz: p.iz, iyz: p.iyz }, SolveStrategy::Sparse)?;
    let w = solve_warping(&mesh, c, sh.shear_centre, SolveStrategy::Sparse)?;
    Ok(SectionFe { a: p.a, centroid: c, shear_centre: sh.shear_centre, iy: p.iy, iz: p.iz, iyz: p.iyz, zy: p.zy, zz: p.zz, j: t.j, cw: w.cw, elements: mesh.triangles.len() })
}

/// The outline of a lipped channel (C with lips), outer dimensions h x b, lip d, thickness t,
/// web on the left, opening to the right.
pub fn lipped_c(h: f64, b: f64, d: f64, t: f64) -> Vec<[f64; 2]> {
    let (y0, z0) = (0.0, 0.0);
    vec![
        [y0, z0],
        [b, z0],
        [b, d],
        [b - t, d],
        [b - t, t],
        [t, t],
        [t, h - t],
        [b - t, h - t],
        [b - t, h - d],
        [b, h - d],
        [b, h],
        [y0, h],
    ]
}

/// The outline of an H (no fillets): depth d, flange bf, web tw, flange tf.
pub fn h_outline(d: f64, bf: f64, tw: f64, tf: f64) -> Vec<[f64; 2]> {
    let (l, r) = (-bf / 2.0, bf / 2.0);
    let (wl, wr) = (-tw / 2.0, tw / 2.0);
    vec![[l, 0.0], [r, 0.0], [r, tf], [wr, tf], [wr, d - tf], [r, d - tf], [r, d], [l, d], [l, d - tf], [wl, d - tf], [wl, tf], [l, tf]]
}

/// The outline of a model section, centred on its centroid's vertical line (None: General).
pub fn outline_of(shape: &crate::model::Shape) -> Option<Vec<[f64; 2]>> {
    use crate::model::Shape;
    match *shape {
        Shape::Rect { b, h } => Some(vec![[-b / 2.0, 0.0], [b / 2.0, 0.0], [b / 2.0, h], [-b / 2.0, h]]),
        Shape::H { d, bf, tw, tf, .. } => Some(h_outline(d, bf, tw, tf)),
        Shape::LippedC { h, b, d, t } => Some(lipped_c(h, b, d, t)),
        Shape::General { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h_shape_j_and_cw() {
        // H 300x150x6.5x9 without fillets: J ≈ (2 bf tf³ + (d − 2tf) tw³)/3 (thin-walled, a little
        // over the exact), Cw = Iz,flanges (d − tf)² / 4 = tf bf³ (d − tf)² / 24
        let (d, bf, tw, tf) = (0.3, 0.15, 0.0065, 0.009);
        let s = analyze(&h_outline(d, bf, tw, tf), 4000).unwrap();
        let j_thin = (2.0 * bf * tf.powi(3) + (d - 2.0 * tf) * tw.powi(3)) / 3.0;
        let cw = tf * bf.powi(3) * (d - tf).powi(2) / 24.0;
        eprintln!("J FE {:.4e} thin {j_thin:.4e}; Cw FE {:.4e} formula {cw:.4e}; {} triangles; shear centre {:?}", s.j, s.cw, s.elements, s.shear_centre);
        assert!(s.j < j_thin && s.j > 0.85 * j_thin);
        assert!((s.cw - cw).abs() / cw < 0.03);
        assert!(s.shear_centre[0].abs() < 1e-4 && s.shear_centre[1].abs() < 1e-4);
    }
}
