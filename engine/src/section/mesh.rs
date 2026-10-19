//! Cross-section mesh generation.
//!
//! Produces a quality-controlled triangulation of a cross-section defined by
//! canonical polygons (`SectionPolygon`), for use by the Saint-Venant torsion
//! and transverse-shear solvers that follow this checkpoint.
//!
//! # Why a mesh at all
//!
//! Both remaining stress components are boundary-value problems on the section,
//! not closed-form expressions:
//!
//! * Saint-Venant torsion needs the warping function `psi` from `lap(psi) = 0`
//!   with `d(psi)/dn = z*n_y - y*n_z` on the boundary (or Prandtl's stress
//!   function from `lap(phi) = -2`, constant on each boundary loop).
//! * Transverse shear for an arbitrary section needs the Saint-Venant shear
//!   functions from a further pair of Poisson problems.
//!
//! `V*Q/(I*b)` is only meaningful where a single well-defined width `b(y)`
//! exists, which is exactly why the current angle/channel handling is wrong.
//! The replacement is numerical, and numerical needs a mesh.
//!
//! # Method
//!
//! Constrained Delaunay triangulation with Ruppert-style refinement, via the
//! `spade` crate. Boundary loops are inserted as constraint edges so the
//! section outline is preserved exactly; refinement then inserts Steiner points
//! until every triangle satisfies the requested minimum angle and maximum area.
//!
//! `spade` triangulates the convex hull, so faces outside the solid domain (in
//! concave regions and inside holes) are removed afterwards by an
//! even-odd point-in-polygon test on each triangle centroid. This is exact for
//! the polygonal domains we build: a centroid can never lie on a boundary edge
//! of a triangle whose interior is entirely inside or entirely outside, because
//! constraint edges are never crossed by the triangulation.
//!
//! # Determinism
//!
//! Vertices are inserted in canonical polygon order, refinement is deterministic
//! for a given parameter set, and the emitted nodes/triangles are re-indexed in
//! a stable order (see `canonicalize`). The same input therefore always yields
//! byte-identical output, which the tests rely on.

use serde::{Deserialize, Serialize};
use spade::{AngleLimit, ConstrainedDelaunayTriangulation, Point2, RefinementParameters, Triangulation};

use super::SectionPolygon;

/// Refinement controls for [`mesh_section`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshParams {
    /// Upper bound on triangle area, in section units squared.
    ///
    /// This is the primary refinement knob: halving it roughly doubles the
    /// element count and halves the element size, which is what the
    /// convergence studies sweep.
    pub max_area: f64,
    /// Minimum interior angle, degrees. Ruppert's algorithm is only guaranteed
    /// to terminate below ~30 deg; 25 deg leaves headroom on sections with
    /// acute input corners (an angle profile's toe, for instance) where the
    /// input angle itself bounds what is achievable.
    #[serde(default = "default_min_angle")]
    pub min_angle_deg: f64,
    /// Safety valve so a pathological geometry cannot refine without bound.
    /// Exceeding it is reported as an error rather than silently truncating
    /// the mesh — a silently-coarse mesh would quietly degrade the stress field.
    #[serde(default = "default_max_vertices")]
    pub max_vertices: usize,
}

fn default_min_angle() -> f64 {
    25.0
}
fn default_max_vertices() -> usize {
    200_000
}

impl Default for MeshParams {
    fn default() -> Self {
        Self {
            max_area: f64::INFINITY,
            min_angle_deg: default_min_angle(),
            max_vertices: default_max_vertices(),
        }
    }
}

impl MeshParams {
    /// Refinement targeting roughly `n` elements across the section's smaller
    /// bounding-box dimension. Expressing refinement in terms of the geometry
    /// keeps convergence studies meaningful across sections whose absolute
    /// sizes differ by orders of magnitude (a 10 mm angle leg vs a 1 m girder).
    pub fn with_divisions(bbox: [f64; 4], divisions: usize) -> Self {
        let w = (bbox[2] - bbox[0]).abs();
        let h = (bbox[3] - bbox[1]).abs();
        let smaller = if w < h { w } else { h };
        let n = divisions.max(1) as f64;
        let edge = smaller / n;
        Self {
            // Equilateral triangle of side `edge`; the 0.5 keeps the target
            // slightly below that so refinement actually engages.
            max_area: 0.5 * edge * edge,
            ..Default::default()
        }
    }
}

/// A triangulated cross-section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionMesh {
    /// Node coordinates `[y, z]`, in section units.
    pub nodes: Vec<[f64; 2]>,
    /// Triangles as node indices, always counter-clockwise (positive area).
    pub triangles: Vec<[usize; 3]>,
    /// Boundary edges as node index pairs, tagged with the loop they came from.
    ///
    /// Loop 0 is the outer boundary; loops 1.. are holes. The torsion solver
    /// needs this to impose one circulation constraint per hole, so the tag is
    /// carried through rather than recomputed later.
    pub boundary_edges: Vec<BoundaryEdge>,
    /// Number of boundary loops (1 + number of holes).
    pub loop_count: usize,
}

/// A mesh edge lying on a section boundary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryEdge {
    pub a: usize,
    pub b: usize,
    /// 0 = outer boundary, 1.. = hole index.
    pub loop_id: usize,
}

impl SectionMesh {
    /// Signed area of one triangle (positive for counter-clockwise).
    pub fn triangle_area(&self, t: [usize; 3]) -> f64 {
        let [a, b, c] = [self.nodes[t[0]], self.nodes[t[1]], self.nodes[t[2]]];
        0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
    }

    /// Total meshed area — integrating 1 over the domain.
    pub fn area(&self) -> f64 {
        self.triangles.iter().map(|&t| self.triangle_area(t)).sum()
    }

    /// Area-weighted centroid, integrating (y, z) over the domain.
    pub fn centroid(&self) -> [f64; 2] {
        let (mut a, mut sy, mut sz) = (0.0, 0.0, 0.0);
        for &t in &self.triangles {
            let ar = self.triangle_area(t);
            let cy = (self.nodes[t[0]][0] + self.nodes[t[1]][0] + self.nodes[t[2]][0]) / 3.0;
            let cz = (self.nodes[t[0]][1] + self.nodes[t[1]][1] + self.nodes[t[2]][1]) / 3.0;
            a += ar;
            sy += ar * cy;
            sz += ar * cz;
        }
        if a.abs() < 1e-300 {
            return [0.0, 0.0];
        }
        [sy / a, sz / a]
    }

    /// Second moments about the given point, integrated exactly over each
    /// linear triangle.
    ///
    /// For a triangle with vertices `p_i`, the exact integral of `f*g` for
    /// linear `f`, `g` is `A/12 * (sum_i f_i g_i + (sum f)(sum g))`. Using the
    /// exact rule rather than a centroid sample means these recover the
    /// analytic polygon inertias to round-off, which is what makes them a
    /// meaningful check on the mesh rather than on the quadrature.
    pub fn second_moments(&self, about: [f64; 2]) -> (f64, f64, f64) {
        let (mut iy, mut iz, mut iyz) = (0.0, 0.0, 0.0);
        for &t in &self.triangles {
            let ar = self.triangle_area(t);
            let y: [f64; 3] = [
                self.nodes[t[0]][0] - about[0],
                self.nodes[t[1]][0] - about[0],
                self.nodes[t[2]][0] - about[0],
            ];
            let z: [f64; 3] = [
                self.nodes[t[0]][1] - about[1],
                self.nodes[t[1]][1] - about[1],
                self.nodes[t[2]][1] - about[1],
            ];
            let (sy, sz) = (y[0] + y[1] + y[2], z[0] + z[1] + z[2]);
            iy += ar / 12.0 * (z[0] * z[0] + z[1] * z[1] + z[2] * z[2] + sz * sz);
            iz += ar / 12.0 * (y[0] * y[0] + y[1] * y[1] + y[2] * y[2] + sy * sy);
            iyz += ar / 12.0 * (y[0] * z[0] + y[1] * z[1] + y[2] * z[2] + sy * sz);
        }
        (iy, iz, iyz)
    }

    /// Smallest interior angle across the mesh, in degrees. A quality report:
    /// slivers destroy FEM gradient accuracy, so tests assert a floor.
    pub fn min_angle_deg(&self) -> f64 {
        let mut worst = 180.0_f64;
        for &t in &self.triangles {
            let p = [self.nodes[t[0]], self.nodes[t[1]], self.nodes[t[2]]];
            for i in 0..3 {
                let a = p[i];
                let b = p[(i + 1) % 3];
                let c = p[(i + 2) % 3];
                let u = [b[0] - a[0], b[1] - a[1]];
                let v = [c[0] - a[0], c[1] - a[1]];
                let nu = (u[0] * u[0] + u[1] * u[1]).sqrt();
                let nv = (v[0] * v[0] + v[1] * v[1]).sqrt();
                if nu < 1e-300 || nv < 1e-300 {
                    return 0.0;
                }
                let cos = ((u[0] * v[0] + u[1] * v[1]) / (nu * nv)).clamp(-1.0, 1.0);
                let ang = cos.acos().to_degrees();
                if ang < worst {
                    worst = ang;
                }
            }
        }
        worst
    }
}

/// Distance from a point to a segment — used to attribute a mesh boundary edge
/// to the input loop it lies on.
fn point_segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (vy, vz) = (b[0] - a[0], b[1] - a[1]);
    let len2 = vy * vy + vz * vz;
    let t = if len2 < 1e-300 { 0.0 } else { (((p[0] - a[0]) * vy + (p[1] - a[1]) * vz) / len2).clamp(0.0, 1.0) };
    let (qy, qz) = (a[0] + t * vy, a[1] + t * vz);
    ((p[0] - qy).powi(2) + (p[1] - qz).powi(2)).sqrt()
}

/// Even-odd point-in-polygon test.
fn point_in_loop(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let (yi, zi) = (poly[i][0], poly[i][1]);
        let (yj, zj) = (poly[j][0], poly[j][1]);
        if (zi > p[1]) != (zj > p[1]) {
            let y_cross = (yj - yi) * (p[1] - zi) / (zj - zi) + yi;
            if p[0] < y_cross {
                inside = !inside;
            }
        }
    }
    inside
}

/// Triangulate a cross-section.
///
/// `polygons` is the canonical geometry: solids first, then voids. All solid
/// polygons are treated as one homogeneous domain; voids are subtracted.
pub fn mesh_section(polygons: &[SectionPolygon], params: &MeshParams) -> Result<SectionMesh, String> {
    if polygons.is_empty() {
        return Err("No polygons defined".into());
    }
    if !params.max_area.is_nan() && params.max_area <= 0.0 {
        return Err("maxArea must be positive".into());
    }
    if !(1.0..=30.0).contains(&params.min_angle_deg) {
        return Err("minAngleDeg must be between 1 and 30 (Ruppert refinement only terminates below ~30)".into());
    }

    let solids: Vec<&SectionPolygon> = polygons.iter().filter(|p| !p.is_void).collect();
    let voids: Vec<&SectionPolygon> = polygons.iter().filter(|p| p.is_void).collect();
    if solids.is_empty() {
        return Err("Section has no solid region".into());
    }
    for p in polygons.iter() {
        if p.vertices.len() < 3 {
            return Err("Polygon must have at least 3 vertices".into());
        }
        for v in &p.vertices {
            if !v[0].is_finite() || !v[1].is_finite() {
                return Err("Polygon has non-finite coordinates".into());
            }
        }
    }

    // ── Insert every loop as constraint edges ──────────────────────
    let mut cdt: ConstrainedDelaunayTriangulation<Point2<f64>> = ConstrainedDelaunayTriangulation::new();
    for poly in polygons.iter() {
        let handles: Vec<_> = poly
            .vertices
            .iter()
            .map(|v| cdt.insert(Point2::new(v[0], v[1])).map_err(|e| format!("Triangulation insert failed: {e:?}")))
            .collect::<Result<Vec<_>, _>>()?;
        for i in 0..handles.len() {
            let j = (i + 1) % handles.len();
            if handles[i] != handles[j] {
                cdt.add_constraint(handles[i], handles[j]);
            }
        }
    }

    // ── Quality refinement ─────────────────────────────────────────
    if params.max_area.is_finite() {
        let refinement = RefinementParameters::<f64>::new()
            .with_angle_limit(AngleLimit::from_deg(params.min_angle_deg))
            .with_max_allowed_area(params.max_area)
            .with_max_additional_vertices(params.max_vertices)
            .exclude_outer_faces(true);
        let outcome = cdt.refine(refinement);
        if !outcome.refinement_complete {
            return Err(format!(
                "Mesh refinement hit the {} vertex budget before satisfying maxArea={} — \
                 the requested element size is too small for this geometry",
                params.max_vertices, params.max_area
            ));
        }
    }

    // ── Keep only faces inside the solid domain ────────────────────
    let mut kept: Vec<[[f64; 2]; 3]> = Vec::new();
    for face in cdt.inner_faces() {
        let v = face.positions();
        let tri = [[v[0].x, v[0].y], [v[1].x, v[1].y], [v[2].x, v[2].y]];
        let c = [
            (tri[0][0] + tri[1][0] + tri[2][0]) / 3.0,
            (tri[0][1] + tri[1][1] + tri[2][1]) / 3.0,
        ];
        let in_solid = solids.iter().any(|s| point_in_loop(c, &s.vertices));
        let in_void = voids.iter().any(|h| point_in_loop(c, &h.vertices));
        if in_solid && !in_void {
            kept.push(tri);
        }
    }
    if kept.is_empty() {
        return Err("Meshing produced no triangles inside the section — check winding and hole placement".into());
    }

    Ok(canonicalize(kept, polygons))
}

/// Re-index the kept triangles into a stable, deterministic mesh.
///
/// Nodes are deduplicated by exact bit pattern (every coordinate originates
/// from the same triangulation, so equal points are bit-equal) and then sorted
/// lexicographically, which makes node numbering independent of face iteration
/// order. Triangles are oriented counter-clockwise and sorted by their node
/// indices. Both are what allow the determinism tests to compare meshes
/// directly.
fn canonicalize(tris: Vec<[[f64; 2]; 3]>, polygons: &[SectionPolygon]) -> SectionMesh {
    let mut coords: Vec<[f64; 2]> = Vec::new();
    for t in &tris {
        for p in t {
            coords.push(*p);
        }
    }
    coords.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap().then(a[1].partial_cmp(&b[1]).unwrap()));
    coords.dedup_by(|a, b| a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits());

    let index_of = |p: [f64; 2]| -> usize {
        coords
            .binary_search_by(|c| c[0].partial_cmp(&p[0]).unwrap().then(c[1].partial_cmp(&p[1]).unwrap()))
            .expect("every triangle vertex was inserted into the node list")
    };

    let mut triangles: Vec<[usize; 3]> = tris
        .iter()
        .map(|t| {
            let idx = [index_of(t[0]), index_of(t[1]), index_of(t[2])];
            let (a, b, c) = (coords[idx[0]], coords[idx[1]], coords[idx[2]]);
            let signed = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            if signed < 0.0 {
                [idx[0], idx[2], idx[1]]
            } else {
                idx
            }
        })
        .collect();
    triangles.sort();

    // ── Boundary edges, derived from the MESH, not from the input ──
    //
    // Refinement splits boundary segments, so the original polygon edges are
    // not the mesh's boundary. Taking them as such leaves every boundary
    // Steiner node untagged — which silently drops it from Dirichlet sets and
    // mis-measures Neumann edge lengths. A mesh edge is on the boundary iff
    // exactly one triangle uses it; that is exact and refinement-independent.
    let mut edge_use: std::collections::HashMap<(usize, usize), usize> = std::collections::HashMap::new();
    for t in &triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let key = if a < b { (a, b) } else { (b, a) };
            *edge_use.entry(key).or_insert(0) += 1;
        }
    }
    let mut boundary: Vec<(usize, usize)> = edge_use.into_iter().filter(|(_, n)| *n == 1).map(|(e, _)| e).collect();
    boundary.sort();

    // Tag each boundary edge with the loop it lies on, by testing its midpoint
    // against every loop's segments. Loop 0.. are solids, then voids.
    let loops: Vec<&SectionPolygon> = polygons
        .iter()
        .filter(|p| !p.is_void)
        .chain(polygons.iter().filter(|p| p.is_void))
        .collect();
    let loop_count = loops.len();

    let boundary_edges = boundary
        .into_iter()
        .map(|(a, b)| {
            let mid = [(coords[a][0] + coords[b][0]) / 2.0, (coords[a][1] + coords[b][1]) / 2.0];
            let mut best = (0usize, f64::INFINITY);
            for (id, lp) in loops.iter().enumerate() {
                let n = lp.vertices.len();
                for i in 0..n {
                    let p0 = lp.vertices[i];
                    let p1 = lp.vertices[(i + 1) % n];
                    let d = point_segment_distance(mid, p0, p1);
                    if d < best.1 {
                        best = (id, d);
                    }
                }
            }
            BoundaryEdge { a, b, loop_id: best.0 }
        })
        .collect();

    SectionMesh {
        nodes: coords,
        triangles,
        boundary_edges,
        loop_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly(v: &[[f64; 2]]) -> SectionPolygon {
        SectionPolygon { vertices: v.to_vec(), material_id: 0, is_void: false }
    }
    fn void(v: &[[f64; 2]]) -> SectionPolygon {
        SectionPolygon { vertices: v.to_vec(), material_id: 0, is_void: true }
    }
    fn rect(y0: f64, z0: f64, y1: f64, z1: f64) -> SectionPolygon {
        poly(&[[y0, z0], [y1, z0], [y1, z1], [y0, z1]])
    }
    fn rect_void(y0: f64, z0: f64, y1: f64, z1: f64) -> SectionPolygon {
        void(&[[y0, z0], [y1, z0], [y1, z1], [y0, z1]])
    }
    /// Equal-leg angle: concave, acute-cornered, thin legs — the hardest of the
    /// families this mesher has to serve.
    fn angle(l: f64, t: f64) -> SectionPolygon {
        poly(&[[0.0, 0.0], [l, 0.0], [l, t], [t, t], [t, l], [0.0, l]])
    }
    fn circle(r: f64, n: usize) -> SectionPolygon {
        poly(&(0..n)
            .map(|i| {
                let a = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
                [r * a.cos(), r * a.sin()]
            })
            .collect::<Vec<_>>())
    }
    fn rel(got: f64, exp: f64) -> f64 {
        if exp == 0.0 { got.abs() } else { ((got - exp) / exp).abs() }
    }

    #[test]
    fn recovers_rectangle_area_centroid_and_inertia() {
        let (b, h) = (0.2, 0.4);
        let m = mesh_section(&[rect(-b / 2.0, -h / 2.0, b / 2.0, h / 2.0)], &MeshParams { max_area: 1e-4, ..Default::default() }).unwrap();
        assert!(rel(m.area(), b * h) < 1e-12, "area {}", m.area());
        let c = m.centroid();
        assert!(c[0].abs() < 1e-12 && c[1].abs() < 1e-12);
        let (iy, iz, iyz) = m.second_moments(c);
        assert!(rel(iy, b * h.powi(3) / 12.0) < 1e-12, "iy {iy}");
        assert!(rel(iz, h * b.powi(3) / 12.0) < 1e-12, "iz {iz}");
        assert!(iyz.abs() < 1e-16, "iyz {iyz}");
    }

    #[test]
    fn excludes_holes_exactly() {
        // RHS 200x100x8 as outer rectangle minus inner void.
        let m = mesh_section(
            &[rect(-0.05, -0.10, 0.05, 0.10), rect_void(-0.042, -0.092, 0.042, 0.092)],
            &MeshParams { max_area: 2e-5, ..Default::default() },
        )
        .unwrap();
        let exact = 0.1 * 0.2 - 0.084 * 0.184;
        assert!(rel(m.area(), exact) < 1e-12, "area {} vs {exact}", m.area());
        let (iy, _, _) = m.second_moments(m.centroid());
        let iy_exact = (0.1 * 0.2f64.powi(3) - 0.084 * 0.184f64.powi(3)) / 12.0;
        assert!(rel(iy, iy_exact) < 1e-10, "iy {iy} vs {iy_exact}");
        assert_eq!(m.loop_count, 2);
        assert!(m.boundary_edges.iter().any(|e| e.loop_id == 1), "hole boundary must be tagged");
    }

    #[test]
    fn multiple_holes_are_all_excluded() {
        let m = mesh_section(
            &[
                rect(-0.10, -0.10, 0.10, 0.10),
                rect_void(-0.06, -0.06, -0.02, -0.02),
                rect_void(0.02, 0.02, 0.06, 0.06),
            ],
            &MeshParams { max_area: 5e-5, ..Default::default() },
        )
        .unwrap();
        let exact = 0.2 * 0.2 - 2.0 * 0.04 * 0.04;
        assert!(rel(m.area(), exact) < 1e-12, "area {} vs {exact}", m.area());
        assert_eq!(m.loop_count, 3);
    }

    #[test]
    fn concave_angle_section_area_and_no_inverted_triangles() {
        let (l, t) = (0.100, 0.010);
        let m = mesh_section(&[angle(l, t)], &MeshParams { max_area: 2e-6, ..Default::default() }).unwrap();
        let exact = 2.0 * l * t - t * t;
        assert!(rel(m.area(), exact) < 1e-12, "area {} vs {exact}", m.area());
        for &tri in &m.triangles {
            assert!(m.triangle_area(tri) > 0.0, "inverted or zero-area triangle");
        }
    }

    #[test]
    fn angle_section_reproduces_the_analytic_product_of_inertia() {
        // The property that makes an angle asymmetric, and the one the current
        // stress path ignores. Mesh integration must reproduce it.
        let (l, t) = (0.100, 0.010);
        let m = mesh_section(&[angle(l, t)], &MeshParams { max_area: 5e-7, ..Default::default() }).unwrap();
        let props = super::super::analyze_section(&super::super::SectionInput {
            polygons: vec![angle(l, t)],
            modular_ratios: Default::default(),
        })
        .unwrap();
        let c = m.centroid();
        assert!(rel(c[0], props.yc) < 1e-10 && rel(c[1], props.zc) < 1e-10);
        let (iy, iz, iyz) = m.second_moments(c);
        assert!(rel(iy, props.iy) < 1e-10, "iy {iy} vs {}", props.iy);
        assert!(rel(iz, props.iz) < 1e-10, "iz {iz} vs {}", props.iz);
        assert!(rel(iyz, props.iyz) < 1e-10, "iyz {iyz} vs {}", props.iyz);
        assert!(props.iyz.abs() > 1e-9, "an equal-leg angle must have non-zero Iyz");
    }

    /// I/H outline as ONE closed loop.
    ///
    /// Canonical geometry must express a profile as a single boundary, not as
    /// overlapping or edge-sharing rectangles. Three touching rectangles give
    /// the correct area but seed duplicate constraint vertices along the shared
    /// edges, and refinement then produces slivers there (measured: 0.76 deg
    /// minimum angle). One loop is both the correct representation and the one
    /// that meshes cleanly.
    fn i_outline(h: f64, b: f64, tw: f64, tf: f64) -> SectionPolygon {
        let (hb, bb, tb) = (h / 2.0, b / 2.0, tw / 2.0);
        poly(&[
            [-bb, -hb], [bb, -hb], [bb, -hb + tf], [tb, -hb + tf],
            [tb, hb - tf], [bb, hb - tf], [bb, hb], [-bb, hb],
            [-bb, hb - tf], [-tb, hb - tf], [-tb, -hb + tf], [-bb, -hb + tf],
        ])
    }

    #[test]
    fn quality_floor_is_respected_on_thin_and_acute_geometry() {
        // IPE300: 7.1 mm web between 150 mm flanges — the narrow-region case.
        let (h, b, tw, tf) = (0.300, 0.150, 0.0071, 0.0107);
        let m = mesh_section(&[i_outline(h, b, tw, tf)], &MeshParams { max_area: 5e-6, ..Default::default() }).unwrap();
        assert!(m.min_angle_deg() > 20.0, "min angle {} deg", m.min_angle_deg());
        for &tri in &m.triangles {
            assert!(m.triangle_area(tri) > 0.0);
        }
        let exact = 2.0 * b * tf + (h - 2.0 * tf) * tw;
        assert!(rel(m.area(), exact) < 1e-12, "I area {} vs {exact}", m.area());
    }

    #[test]
    fn refinement_converges_monotonically_under_area_control() {
        let (l, t) = (0.100, 0.010);
        let exact = 2.0 * l * t - t * t;
        let mut last = 0usize;
        for &ma in &[1e-5, 2.5e-6, 6.25e-7] {
            let m = mesh_section(&[angle(l, t)], &MeshParams { max_area: ma, ..Default::default() }).unwrap();
            assert!(m.triangles.len() > last, "refinement must add elements: {} then {}", last, m.triangles.len());
            assert!(rel(m.area(), exact) < 1e-11, "area drifted at maxArea={ma}");
            for &tri in &m.triangles {
                assert!(m.triangle_area(tri) > 0.0);
            }
            last = m.triangles.len();
        }
    }

    #[test]
    fn circle_area_converges_with_boundary_discretization() {
        let r = 0.15;
        let exact = std::f64::consts::PI * r * r;
        let mut prev = f64::INFINITY;
        for &n in &[32usize, 128, 512] {
            let m = mesh_section(&[circle(r, n)], &MeshParams { max_area: 1e-4, ..Default::default() }).unwrap();
            let err = rel(m.area(), exact);
            assert!(err < prev, "discretization error must decrease: {err} !< {prev}");
            prev = err;
        }
        assert!(prev < 1e-4, "512-gon should be within 1e-4 relative, got {prev}");
    }

    #[test]
    fn translation_and_rotation_do_not_change_topology_or_invariants() {
        let (l, t) = (0.100, 0.010);
        let base = angle(l, t);
        let m0 = mesh_section(&[base.clone()], &MeshParams { max_area: 2e-6, ..Default::default() }).unwrap();

        // Ruppert refinement is NOT exactly invariant under rigid motion: the
        // circumcentre arithmetic differs in the last bits, so the element
        // count can shift by a fraction of a percent. What must be invariant
        // is the physics, so that is what is asserted.
        let shifted = poly(&base.vertices.iter().map(|v| [v[0] + 3.0, v[1] - 7.0]).collect::<Vec<_>>());
        let m1 = mesh_section(&[shifted], &MeshParams { max_area: 2e-6, ..Default::default() }).unwrap();
        let drift = (m1.triangles.len() as f64 - m0.triangles.len() as f64).abs() / m0.triangles.len() as f64;
        assert!(drift < 0.02, "translation changed the element count by {:.1}%", 100.0 * drift);
        assert!(rel(m1.area(), m0.area()) < 1e-12);
        let (iy0, iz0, _) = m0.second_moments(m0.centroid());
        let (iy1, iz1, _) = m1.second_moments(m1.centroid());
        assert!(rel(iy1, iy0) < 1e-10 && rel(iz1, iz0) < 1e-10, "translation changed inertia");

        let a = std::f64::consts::FRAC_PI_6;
        let rotated = poly(&base.vertices.iter().map(|v| [v[0] * a.cos() - v[1] * a.sin(), v[0] * a.sin() + v[1] * a.cos()]).collect::<Vec<_>>());
        let m2 = mesh_section(&[rotated], &MeshParams { max_area: 2e-6, ..Default::default() }).unwrap();
        assert!(rel(m2.area(), m0.area()) < 1e-10, "rotation changed area");
        // Trace of the inertia tensor is rotation-invariant.
        let (iy0, iz0, _) = m0.second_moments(m0.centroid());
        let (iy2, iz2, _) = m2.second_moments(m2.centroid());
        assert!(rel(iy2 + iz2, iy0 + iz0) < 1e-10, "inertia trace is not rotation-invariant");
    }

    #[test]
    fn vertex_order_does_not_change_the_meshed_domain() {
        // Same loop, rotated starting vertex: identical canonical mesh.
        let (l, t) = (0.100, 0.010);
        let a = angle(l, t);
        let mut shifted = a.vertices.clone();
        shifted.rotate_left(3);
        let m0 = mesh_section(&[a], &MeshParams { max_area: 2e-6, ..Default::default() }).unwrap();
        let m1 = mesh_section(&[poly(&shifted)], &MeshParams { max_area: 2e-6, ..Default::default() }).unwrap();
        // Same caveat as rigid motion: insertion order perturbs refinement in
        // the last bits. The domain it discretizes must be identical.
        let drift = (m1.nodes.len() as f64 - m0.nodes.len() as f64).abs() / m0.nodes.len() as f64;
        assert!(drift < 0.02, "vertex order changed the node count by {:.1}%", 100.0 * drift);
        assert!(rel(m1.area(), m0.area()) < 1e-12, "vertex order changed the meshed area");
        let (iy0, iz0, iyz0) = m0.second_moments(m0.centroid());
        let (iy1, iz1, iyz1) = m1.second_moments(m1.centroid());
        assert!(rel(iy1, iy0) < 1e-9 && rel(iz1, iz0) < 1e-9 && rel(iyz1, iyz0) < 1e-9);
    }

    #[test]
    fn repeated_meshing_is_bit_identical() {
        let (l, t) = (0.100, 0.010);
        let p = MeshParams { max_area: 2e-6, ..Default::default() };
        let m0 = mesh_section(&[angle(l, t)], &p).unwrap();
        let m1 = mesh_section(&[angle(l, t)], &p).unwrap();
        assert_eq!(m0.triangles, m1.triangles);
        assert_eq!(m0.nodes.len(), m1.nodes.len());
        for (a, b) in m0.nodes.iter().zip(m1.nodes.iter()) {
            assert_eq!(a[0].to_bits(), b[0].to_bits());
            assert_eq!(a[1].to_bits(), b[1].to_bits());
        }
    }

    #[test]
    fn channel_and_tee_mesh_cleanly() {
        // U-channel: web plus two flanges.
        let u = vec![poly(&[
            [0.0, 0.0], [0.075, 0.0], [0.075, 0.0115], [0.0085, 0.0115],
            [0.0085, 0.1885], [0.075, 0.1885], [0.075, 0.2], [0.0, 0.2],
        ])];
        let mu = mesh_section(&u, &MeshParams { max_area: 2e-6, ..Default::default() }).unwrap();
        assert!(mu.min_angle_deg() > 20.0);
        for &t in &mu.triangles {
            assert!(mu.triangle_area(t) > 0.0);
        }

        // T-section as ONE outline (see i_outline for why not two rectangles).
        let tee = vec![poly(&[
            [-0.05, 0.0], [0.05, 0.0], [0.05, 0.15], [0.15, 0.15],
            [0.15, 0.30], [-0.15, 0.30], [-0.15, 0.15], [-0.05, 0.15],
        ])];
        let mt = mesh_section(&tee, &MeshParams { max_area: 1e-4, ..Default::default() }).unwrap();
        let exact = 0.30 * 0.15 + 0.10 * 0.15;
        assert!(rel(mt.area(), exact) < 1e-12, "tee area {} vs {exact}", mt.area());
    }

    #[test]
    fn rejects_invalid_input() {
        let p = MeshParams::default();
        assert!(mesh_section(&[], &p).is_err(), "empty");
        assert!(mesh_section(&[poly(&[[0.0, 0.0], [1.0, 0.0]])], &p).is_err(), "two vertices");
        assert!(mesh_section(&[poly(&[[0.0, 0.0], [f64::NAN, 0.0], [1.0, 1.0]])], &p).is_err(), "non-finite");
        assert!(mesh_section(&[rect_void(0.0, 0.0, 1.0, 1.0)], &p).is_err(), "void only");
        assert!(
            mesh_section(&[rect(0.0, 0.0, 1.0, 1.0)], &MeshParams { max_area: -1.0, ..Default::default() }).is_err(),
            "negative area target"
        );
        assert!(
            mesh_section(&[rect(0.0, 0.0, 1.0, 1.0)], &MeshParams { min_angle_deg: 45.0, ..Default::default() }).is_err(),
            "angle limit above Ruppert termination bound"
        );
    }

    #[test]
    fn reports_rather_than_silently_truncating_when_the_budget_is_hit() {
        let r = mesh_section(
            &[rect(0.0, 0.0, 1.0, 1.0)],
            &MeshParams { max_area: 1e-9, max_vertices: 500, ..Default::default() },
        );
        let err = r.expect_err("must not silently return a coarse mesh");
        assert!(err.contains("vertex budget"), "unexpected error: {err}");
    }

    #[test]
    fn with_divisions_scales_refinement_to_the_geometry() {
        let (l, t) = (0.100, 0.010);
        let bbox = [0.0, 0.0, l, l];
        let coarse = mesh_section(&[angle(l, t)], &MeshParams::with_divisions(bbox, 4)).unwrap();
        let fine = mesh_section(&[angle(l, t)], &MeshParams::with_divisions(bbox, 16)).unwrap();
        assert!(fine.triangles.len() > coarse.triangles.len());
    }
}

impl SectionMesh {
    /// A boundary edge oriented so the material lies to its LEFT, with the
    /// triangle that owns it.
    ///
    /// `boundary_edges` is derived from connectivity — an edge used by exactly
    /// one triangle — so its stored vertex order carries NO orientation.
    /// Trusting it makes shoelace sums cancel and sends outward normals half
    /// one way and half the other. The owning triangle is wound
    /// counter-clockwise, so the directed edge appearing in it is the correct
    /// one. Two solvers depend on this, which is why it lives here rather than
    /// in either of them.
    pub fn oriented_boundary_edge(&self, a: usize, b: usize) -> Option<(usize, usize, usize)> {
        let t = self.triangles.iter().position(|t| t.contains(&a) && t.contains(&b))?;
        let tri = self.triangles[t];
        for k in 0..3 {
            let (p, q) = (tri[k], tri[(k + 1) % 3]);
            if (p, q) == (a, b) || (p, q) == (b, a) {
                return Some((p, q, t));
            }
        }
        None
    }

    /// Index of the triangle containing `p`, or the nearest one by centroid.
    ///
    /// Falling back to the nearest rather than returning `None` is deliberate:
    /// a query point comes from a user clicking a fibre on a drawing, and it
    /// can land a hair outside the outline through nothing worse than rounding.
    /// Refusing there would read as a broken panel; snapping to the nearest
    /// element gives the value the user meant and is wrong by at most one
    /// element's width — which the mesh already bounds.
    pub fn locate(&self, p: [f64; 2]) -> Option<usize> {
        let mut best: Option<(f64, usize)> = None;
        for (i, &t) in self.triangles.iter().enumerate() {
            let [a, b, c] = [self.nodes[t[0]], self.nodes[t[1]], self.nodes[t[2]]];
            let d = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            if d.abs() > 0.0 {
                let l1 = ((b[0] - p[0]) * (c[1] - p[1]) - (c[0] - p[0]) * (b[1] - p[1])) / d;
                let l2 = ((c[0] - p[0]) * (a[1] - p[1]) - (a[0] - p[0]) * (c[1] - p[1])) / d;
                let l3 = 1.0 - l1 - l2;
                if l1 >= -1e-9 && l2 >= -1e-9 && l3 >= -1e-9 {
                    return Some(i);
                }
            }
            let cy = (a[0] + b[0] + c[0]) / 3.0 - p[0];
            let cz = (a[1] + b[1] + c[1]) / 3.0 - p[1];
            let dist = cy * cy + cz * cz;
            if best.map_or(true, |(d0, _)| dist < d0) {
                best = Some((dist, i));
            }
        }
        best.map(|(_, i)| i)
    }
}
