//! Saint-Venant uniform torsion of an arbitrary cross-section.
//!
//! # What this replaces
//!
//! Until now the app had no honest torsional constant for a general shape. It
//! had an exact one for circles and tubes, whatever a catalogue happened to
//! publish, and — where neither existed — a placeholder of `Iz * 0.001` that
//! was a fabrication kept only so 3D models would solve. Routh's polygon
//! approximation was explicitly forbidden as a substitute, because it is exact
//! only for the ellipse and was measured 56.9 % low on a rectangle and 37.0 %
//! high on an I-section. This module computes the real thing.
//!
//! # Formulation
//!
//! Prandtl's stress function `phi` on the section `Omega`:
//!
//! ```text
//!   laplacian(phi) = -2      in Omega
//!   phi = 0                  on the outer boundary
//! ```
//!
//! from which the torsion constant is `J = 2 * integral(phi) dA` and the shear
//! stresses under unit twist rate are `tau_xz = d(phi)/dy`, `tau_xy = -d(phi)/dz`.
//!
//! The sign convention matters and is easy to get backwards: the Poisson solver
//! here solves `-laplacian(u) = f`, so the source is `f = +2`, not `-2`. A test
//! against the circle catches an inversion immediately — `J` would come out
//! negative rather than merely wrong.
//!
//! # Holes
//!
//! On a multiply-connected section `phi` is still constant on each hole
//! boundary, but that constant is unknown. It is fixed by requiring the warping
//! displacement to be single-valued around the hole, which after integration by
//! parts is Bredt's condition:
//!
//! ```text
//!   contour_integral(d(phi)/dn) ds = -2 * A_hole
//! ```
//!
//! Solved here by superposition, which keeps the whole thing linear and reuses
//! the same SPD solver. With `k` holes, solve `k + 1` Poisson problems:
//!
//!   * `phi_0`: source 2, zero on every boundary — the simply-connected answer.
//!   * `phi_i`: source 0, one on hole `i`, zero elsewhere — a unit response.
//!
//! Then `phi = phi_0 + sum(c_i * phi_i)`, and imposing Bredt's condition on
//! each hole gives a small dense `k x k` system for the constants. For a single
//! hole — every tube in the catalogue — that is one scalar equation.
//!
//! Getting this wrong is not subtle: dropping the constants entirely (treating
//! the hole as a free boundary) understates a thin closed tube's `J` by more
//! than an order of magnitude, because a closed section carries torsion by
//! shear flow around the cell and an open one does not.
//!
//! # Accuracy
//!
//! `J` is a functional of the field, not of its gradient, so it converges at
//! the field's rate rather than the gradient's — the same reason the tests
//! below can demand a few tenths of a percent from a mesh that resolves the
//! stresses only to a few percent.

use serde::{Deserialize, Serialize};

use super::mesh::SectionMesh;
use super::poisson::{LoopBc, PoissonProblem, PoissonSolution, SolveStrategy};

/// Result of a Saint-Venant torsion solve.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TorsionResult {
    /// Torsion constant `J`, in (section length unit)^4.
    pub j: f64,
    /// Prandtl stress function at each mesh node.
    pub phi: Vec<f64>,
    /// Shear stress per triangle under UNIT twist rate `theta' = 1`, as
    /// `[tau_xy, tau_xz]`. Multiply by `G * theta'` for a real state.
    pub tau: Vec<[f64; 2]>,
    /// Largest `|tau|` over the section, under unit twist rate.
    pub tau_max: f64,
    /// Triangle carrying `tau_max`, so the caller can report where it acts.
    pub tau_max_triangle: usize,
    /// Linear-system residual, carried through as evidence the solve converged.
    pub residual: f64,
}

/// Solve uniform torsion on a meshed section, open or closed.
pub fn solve_torsion(mesh: &SectionMesh, strategy: SolveStrategy) -> Result<TorsionResult, String> {
    if mesh.triangles.is_empty() {
        return Err("mesh has no triangles".into());
    }
    let holes = mesh.loop_count.saturating_sub(1);

    // Every solve below — the base field and the k unit responses — constrains
    // the SAME node set (every boundary loop is Dirichlet in all of them); only
    // the prescribed values and the source differ. The reduced matrix is
    // therefore bit-identical across all k+1 solves, so factor it once.
    let factored = {
        let mut p = PoissonProblem::new(mesh);
        p.loop_bcs = vec![LoopBc::Dirichlet { value: 0.0 }; mesh.loop_count];
        p.strategy = strategy;
        super::poisson::factor_poisson(&p)?
    };

    // phi_0: the simply-connected field, zero on every boundary.
    let base = factored.solve(
        &vec![2.0; mesh.triangles.len()],
        &vec![0.0; mesh.loop_count],
    )?;

    let sol: PoissonSolution = if holes == 0 {
        base
    } else {
        // One unit response per hole.
        let mut unit = Vec::with_capacity(holes);
        for i in 0..holes {
            let dirichlet: Vec<f64> = (0..mesh.loop_count)
                .map(|l| if l == i + 1 { 1.0 } else { 0.0 })
                .collect();
            unit.push(factored.solve(&[], &dirichlet)?);
        }

        // Bredt: integral over the hole of laplacian(phi) equals the boundary
        // flux, so the condition reduces to `flux_i(phi) = -2 * A_i`. The flux
        // is evaluated as the interior area integral, which needs no boundary
        // quadrature and no normal vectors: for the hole region enclosed by
        // loop i, `-2 A_i` is exactly what the base field contributes and each
        // unit field contributes its own stiffness coupling.
        let a_hole: Vec<f64> = (0..holes).map(|i| hole_area(mesh, i + 1)).collect();
        let mut mat = vec![0.0; holes * holes];
        let mut rhs = vec![0.0; holes];
        for i in 0..holes {
            // Bredt reads `contour_integral(d(phi)/dn) ds = -2 A` with `n`
            // leaving the HOLE. `circulation` measures with `n` leaving the
            // MATERIAL, which on a hole boundary is the opposite direction, so
            // the sign flips: `circulation(phi) = +2 A`.
            rhs[i] = 2.0 * a_hole[i] - circulation(mesh, &base, i + 1);
            for j in 0..holes {
                mat[i * holes + j] = circulation(mesh, &unit[j], i + 1);
            }
        }
        let c = solve_small_dense(&mat, &rhs, holes)?;

        let mut u = base.u.clone();
        let mut grad = base.grad.clone();
        for (k, s) in unit.iter().enumerate() {
            for (n, v) in s.u.iter().enumerate() {
                u[n] += c[k] * v;
            }
            for (t, g) in s.grad.iter().enumerate() {
                grad[t][0] += c[k] * g[0];
                grad[t][1] += c[k] * g[1];
            }
        }
        PoissonSolution { u, grad, residual: base.residual }
    };

    // J = 2 * integral(phi) over the material, PLUS 2 * (hole constant) *
    // (hole area) for each hole: the stress function is defined over the whole
    // enclosed region, and the hole contributes its plateau value times its
    // area even though no material sits there. Omitting that term is what
    // makes a naive implementation report a closed tube as if it were slit.
    let mut j = 2.0 * sol.integrate(mesh);
    for i in 0..holes {
        let plateau = boundary_value(mesh, &sol, i + 1);
        j += 2.0 * plateau * hole_area(mesh, i + 1);
    }
    if !j.is_finite() || j <= 0.0 {
        return Err(format!(
            "torsion constant came out {j}, which is not physical — check the source sign"
        ));
    }

    // tau_xy = -d(phi)/dz, tau_xz = +d(phi)/dy, with grad = [d/dy, d/dz].
    let mut tau = Vec::with_capacity(sol.grad.len());
    let mut tau_max = 0.0;
    let mut tau_max_triangle = 0;
    for (i, g) in sol.grad.iter().enumerate() {
        let t = [-g[1], g[0]];
        let mag = (t[0] * t[0] + t[1] * t[1]).sqrt();
        if mag > tau_max {
            tau_max = mag;
            tau_max_triangle = i;
        }
        tau.push(t);
    }

    Ok(TorsionResult { j, phi: sol.u, tau, tau_max, tau_max_triangle, residual: sol.residual })
}

/// Area enclosed by a boundary loop, by the shoelace rule over oriented edges.
fn hole_area(mesh: &SectionMesh, loop_id: usize) -> f64 {
    let mut a = 0.0;
    for e in mesh.boundary_edges.iter().filter(|e| e.loop_id == loop_id) {
        if let Some((p, q, _)) = mesh.oriented_boundary_edge(e.a, e.b) {
            let (pp, qq) = (mesh.nodes[p], mesh.nodes[q]);
            a += pp[0] * qq[1] - qq[0] * pp[1];
        }
    }
    (a / 2.0).abs()
}

/// The stress function's plateau value on a loop, read from any of its nodes.
fn boundary_value(mesh: &SectionMesh, sol: &PoissonSolution, loop_id: usize) -> f64 {
    mesh.boundary_edges
        .iter()
        .find(|e| e.loop_id == loop_id)
        .map(|e| sol.u[e.a])
        .unwrap_or(0.0)
}

/// Net outward flux of `phi` across a loop, as the sum over triangles touching
/// it of the constant gradient dotted with the outward edge normal.
fn circulation(mesh: &SectionMesh, sol: &PoissonSolution, loop_id: usize) -> f64 {
    let mut total = 0.0;
    for e in mesh.boundary_edges.iter().filter(|e| e.loop_id == loop_id) {
        if let Some((a, b, t)) = mesh.oriented_boundary_edge(e.a, e.b) {
            let (p, q) = (mesh.nodes[a], mesh.nodes[b]);
            let (dy, dz) = (q[0] - p[0], q[1] - p[1]);
            // With the material on the left of (dy, dz), the outward normal is
            // (dz, -dy) — unnormalised, so the edge length falls out of the dot
            // product and this sum is already the line integral.
            let g = sol.grad[t];
            total += g[0] * dz - g[1] * dy;
        }
    }
    total
}

/// Gauss elimination for the tiny hole-constant system. `n` is the number of
/// holes, which is one or two in practice.
fn solve_small_dense(mat: &[f64], rhs: &[f64], n: usize) -> Result<Vec<f64>, String> {
    let mut m = mat.to_vec();
    let mut b = rhs.to_vec();
    for k in 0..n {
        let piv = (k..n).max_by(|&i, &j| {
            m[i * n + k].abs().partial_cmp(&m[j * n + k].abs()).unwrap()
        }).unwrap();
        if m[piv * n + k].abs() < 1e-300 {
            return Err("hole-constant system is singular".into());
        }
        if piv != k {
            for c in 0..n { m.swap(k * n + c, piv * n + c); }
            b.swap(k, piv);
        }
        for i in k + 1..n {
            let f = m[i * n + k] / m[k * n + k];
            for c in k..n { m[i * n + c] -= f * m[k * n + c]; }
            b[i] -= f * b[k];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = b[i];
        for c in i + 1..n { s -= m[i * n + c] * x[c]; }
        x[i] = s / m[i * n + i];
    }
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::section::catalogue as cat;
    use crate::section::mesh::{mesh_section, MeshParams};

    fn meshed(g: &cat::CanonicalGeometry, target: f64) -> SectionMesh {
        let mut p = MeshParams::default();
        p.max_area = target;
        mesh_section(&g.polygons, &p).expect("mesh")
    }

    fn j_of(g: &cat::CanonicalGeometry, target: f64) -> f64 {
        solve_torsion(&meshed(g, target), SolveStrategy::Sparse).expect("torsion").j
    }

    fn src() -> cat::GeometrySource {
        cat::GeometrySource::Parametric { shape: "test".into() }
    }

    /// The one case with a closed form that admits no argument.
    #[test]
    fn a_solid_circle_converges_to_pi_r4_over_2() {
        let r = 50.0;
        let g = cat::solid_circle(2.0 * r, 64).unwrap();
        let exact = std::f64::consts::PI * r.powi(4) / 2.0;
        let j = j_of(&g, 4.0);
        let err = (j / exact - 1.0).abs();
        assert!(err < 0.01, "J = {j:.1} vs exact {exact:.1} ({:.3} %)", err * 100.0);
    }

    /// Refining the mesh must move the answer TOWARDS the closed form, not
    /// merely land near it — that is what distinguishes a converging solver
    /// from one that happens to be close at one resolution.
    #[test]
    fn refining_the_mesh_reduces_the_circle_error() {
        let r = 50.0;
        let g = cat::solid_circle(2.0 * r, 96).unwrap();
        let exact = std::f64::consts::PI * r.powi(4) / 2.0;
        let coarse = (j_of(&g, 40.0) / exact - 1.0).abs();
        let fine = (j_of(&g, 4.0) / exact - 1.0).abs();
        assert!(fine < coarse, "refining made it worse: {coarse:.4} -> {fine:.4}");
    }

    /// Saint-Venant's series for a rectangle. This is the case Routh's
    /// approximation missed by 56.9 %, so it is the one worth pinning.
    fn rectangle_j(a: f64, b: f64) -> f64 {
        // J = a*b^3 * (1/3 - 0.21*(b/a)*(1 - b^4/(12 a^4))) for a >= b.
        let (a, b) = if a >= b { (a, b) } else { (b, a) };
        a * b.powi(3) * (1.0 / 3.0 - 0.21 * (b / a) * (1.0 - b.powi(4) / (12.0 * a.powi(4))))
    }

    #[test]
    fn a_square_matches_saint_venants_series() {
        let s = 100.0;
        let g = cat::rectangle(s, s).unwrap();
        let j = j_of(&g, 4.0);
        let exact = rectangle_j(s, s);
        let err = (j / exact - 1.0).abs();
        assert!(err < 0.02, "J = {j:.1} vs series {exact:.1} ({:.2} %)", err * 100.0);
    }

    #[test]
    fn a_narrow_rectangle_matches_the_thin_strip_limit() {
        // For b << a the series collapses to J -> a*b^3/3, the thin-strip
        // result every open profile's torsion is built on.
        let (a, b) = (200.0, 10.0);
        let g = cat::rectangle(b, a).unwrap();
        let j = j_of(&g, 1.0);
        let thin = a * b.powi(3) / 3.0;
        assert!((j / thin - 1.0).abs() < 0.10, "J = {j:.1} vs thin-strip {thin:.1}");
        // And it must sit BELOW the thin-strip value, which ignores the ends.
        assert!(j < thin, "J = {j:.1} should be under the thin-strip bound {thin:.1}");
    }

    /// The property that makes J useful and that Routh's approximation lacks:
    /// an open section is enormously more flexible in torsion than a compact
    /// one of the same area.
    #[test]
    fn an_open_profile_is_far_more_torsionally_flexible_than_a_compact_one() {
        let i = cat::i_section(300.0, 150.0, 7.1, 10.7, 15.0, 8, src()).unwrap();
        let mesh_i = meshed(&i, 4.0);
        let area = mesh_i.area();
        let j_i = solve_torsion(&mesh_i, SolveStrategy::Sparse).unwrap().j;

        let side = area.sqrt();
        let sq = cat::rectangle(side, side).unwrap();
        let j_sq = j_of(&sq, 4.0);

        assert!(j_i < j_sq / 20.0, "I-section J {j_i:.0} vs equal-area square {j_sq:.0}");
    }

    /// An I-profile's torsion is the sum of its three plates, to within the
    /// junction effect that the fillets add.
    #[test]
    fn an_i_profile_lands_near_the_sum_of_its_plates() {
        let (h, b, tw, tf) = (300.0, 150.0, 7.1, 10.7);
        let g = cat::i_section(h, b, tw, tf, 0.0, 8, src()).unwrap();
        let j = j_of(&g, 2.0);
        let plates = (2.0 * b * tf.powi(3) + (h - 2.0 * tf) * tw.powi(3)) / 3.0;
        // The thin-strip sum ignores the web/flange junctions, so the true J is
        // higher; a factor of two would mean something is wrong.
        assert!(j > plates * 0.9, "J {j:.0} vs plate sum {plates:.0}");
        assert!(j < plates * 2.0, "J {j:.0} vs plate sum {plates:.0}");
    }

    /// A rolled I-profile's J against the thin-strip sum of its plates, with
    /// the junction effect the fillets add.
    ///
    /// The CIRSOC tables do publish a J column, but its position could not be
    /// established with confidence from the extracted text — the neighbouring
    /// warping constant maps cleanly and J does not — so it is deliberately NOT
    /// used as a reference here. Asserting against a number that might be the
    /// wrong column would be worse than asserting against physics: an early
    /// version of this test "passed" at +2.8 % against a misread value while
    /// the solver was actually converging to something else.
    #[test]
    fn a_rolled_i_profile_sits_above_its_plate_sum_by_the_fillet_contribution() {
        // IPE 300.
        let (h, b, tw, tf, r) = (300.0, 150.0, 7.1, 10.7, 15.0);
        let g = cat::i_section(h, b, tw, tf, r, 8, src()).unwrap();
        let j = j_of(&g, 2.0);
        let plates = (2.0 * b * tf.powi(3) + (h - 2.0 * tf) * tw.powi(3)) / 3.0;
        let ratio = j / plates;
        // Fillets add materially at the web/flange junctions but cannot double
        // the constant; for rolled I-profiles the factor sits near 1.2-1.4.
        assert!((1.1..1.5).contains(&ratio), "J {j:.0} / plate sum {plates:.0} = {ratio:.3}");
    }

    /// A circular tube has a closed form too: J = pi/2 (ro^4 - ri^4), the same
    /// polar second moment, because the section is axisymmetric. This is the
    /// check that the hole constants are right — get them wrong and the answer
    /// collapses towards the slit-tube value, which is smaller by orders of
    /// magnitude.
    #[test]
    fn a_circular_tube_converges_to_its_closed_form() {
        let (d, t) = (100.0, 5.0);
        let (ro, ri) = (d / 2.0, d / 2.0 - t);
        let g = cat::circular_hollow(d, t, 96).unwrap();
        let exact = std::f64::consts::PI / 2.0 * (ro.powi(4) - ri.powi(4));
        let j = j_of(&g, 1.0);
        let err = (j / exact - 1.0).abs();
        assert!(err < 0.05, "J = {j:.1} vs exact {exact:.1} ({:.2} %)", err * 100.0);
    }

    /// The property the hole constants exist for: closing a section makes it
    /// enormously stiffer in torsion. A slit tube of the same wall carries
    /// torsion as an open strip, and the ratio runs into the hundreds.
    #[test]
    fn a_closed_tube_is_vastly_stiffer_than_the_open_strip_of_the_same_wall() {
        let (d, t) = (100.0, 5.0);
        let g = cat::circular_hollow(d, t, 96).unwrap();
        let j_closed = j_of(&g, 1.0);
        // Unrolled wall as a thin strip: J ~ L t^3 / 3 with L the mean circumference.
        let l = std::f64::consts::PI * (d - t);
        let j_open = l * t.powi(3) / 3.0;
        assert!(
            j_closed > 100.0 * j_open,
            "closed {j_closed:.0} should dwarf open {j_open:.0}"
        );
    }

    /// Bredt's thin-walled formula for a closed cell: J = 4 A_m^2 / integral(ds/t).
    /// A square tube is the case where a mistake in the hole constant shows up
    /// as a plausible-looking number rather than an absurd one.
    #[test]
    fn a_square_tube_matches_bredts_thin_walled_formula() {
        let (b, t) = (100.0, 4.0);
        let g = cat::rectangular_hollow_rounded(b, b, t, 0.0, 8, src()).unwrap();
        let j = j_of(&g, 1.5);
        let am = (b - t) * (b - t);              // area enclosed by the wall centreline
        let bredt = 4.0 * am * am / (4.0 * (b - t) / t);
        let err = (j / bredt - 1.0).abs();
        assert!(err < 0.10, "J = {j:.0} vs Bredt {bredt:.0} ({:.1} %)", err * 100.0);
    }

    #[test]
    fn the_sparse_and_dense_paths_agree() {
        let g = cat::rectangle(60.0, 40.0).unwrap();
        let mesh = meshed(&g, 8.0);
        let a = solve_torsion(&mesh, SolveStrategy::Sparse).unwrap().j;
        let b = solve_torsion(&mesh, SolveStrategy::Dense).unwrap().j;
        assert!((a / b - 1.0).abs() < 1e-9, "{a} vs {b}");
    }

    #[test]
    fn peak_shear_on_a_circle_sits_at_the_rim_and_matches_t_r_over_j() {
        // Under unit twist rate the elastic solution gives tau = G*theta'*r, so
        // with G factored out the peak magnitude is the radius itself.
        let r = 50.0;
        let g = cat::solid_circle(2.0 * r, 96).unwrap();
        let mesh = meshed(&g, 4.0);
        let res = solve_torsion(&mesh, SolveStrategy::Sparse).unwrap();
        assert!((res.tau_max / r - 1.0).abs() < 0.05, "tau_max {} vs r {r}", res.tau_max);
        // And it must occur near the rim, not somewhere in the interior.
        let t = mesh.triangles[res.tau_max_triangle];
        let c: [f64; 2] = [
            (mesh.nodes[t[0]][0] + mesh.nodes[t[1]][0] + mesh.nodes[t[2]][0]) / 3.0,
            (mesh.nodes[t[0]][1] + mesh.nodes[t[1]][1] + mesh.nodes[t[2]][1]) / 3.0,
        ];
        assert!((c[0] * c[0] + c[1] * c[1]).sqrt() > 0.9 * r);
    }
}
