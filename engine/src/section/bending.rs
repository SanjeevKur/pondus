//! Axial and unsymmetrical bending stress on canonical section geometry.
//!
//! # Method
//!
//! For a homogeneous, linearly elastic prismatic beam under an axial force `N`
//! and centroidal bending moments `My`, `Mz`, the normal stress at a point
//! `(y, z)` measured from the centroid is
//!
//! ```text
//!     sigma(y,z) = N/A
//!                + ( My*Iz + Mz*Iyz) / (Iy*Iz - Iyz^2) * z
//!                - ( Mz*Iy + My*Iyz) / (Iy*Iz - Iyz^2) * y
//! ```
//!
//! This is the *general* unsymmetrical-bending solution: it uses the complete
//! centroidal inertia tensor and reduces to the familiar `My*z/Iy - Mz*y/Iz`
//! only when `Iyz = 0`.
//!
//! # Why the general form matters
//!
//! The previous stress path applied the reduced form to every section. For an
//! angle, a channel or any asymmetric polygon the geometric axes are **not**
//! principal — an equal-leg angle has its principal axes at 45 degrees — so
//! the reduced form is simply the wrong equation, and it produced a
//! plausible-looking number with no warning. Nothing here assumes principality;
//! `Iyz` is carried through and the principal angle is reported rather than
//! presumed to be zero.
//!
//! # Neutral axis
//!
//! `sigma = 0` is a straight line. With `N = 0` it passes through the centroid
//! at angle `atan2(kz, -ky)` where `ky`, `kz` are the bending coefficients
//! above; with `N != 0` it is offset. The line reported here is exactly the
//! zero set of the same expression the stress field uses, so the drawing
//! cannot show a neutral axis inconsistent with its own stress plot.
//!
//! # Domain
//!
//! Homogeneous, linearly elastic, prismatic, small displacement. Shear and
//! torsion are NOT part of this module — they are boundary-value problems on
//! the section and belong to the arbitrary-section solver that follows.

use serde::{Deserialize, Serialize};

use super::{analyze_section, SectionInput, SectionPolygon, SectionProperties};

/// Section resultants, already expressed in section coordinates about the
/// centroid.
///
/// The caller is responsible for transforming element-local resultants into
/// this frame; `SectionForces::from_local` does that for a rotated section.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionForces {
    /// Axial force, positive in tension.
    pub n: f64,
    /// Bending moment about the section y-axis (causes stress varying with z).
    pub my: f64,
    /// Bending moment about the section z-axis (causes stress varying with y).
    pub mz: f64,
}

impl SectionForces {
    /// Rotate element-local resultants into section coordinates.
    ///
    /// A section rotated by `+theta` about the member axis sees the moment
    /// vector rotated by `-theta` in its own frame. `N` is invariant.
    pub fn from_local(n: f64, my_local: f64, mz_local: f64, rotation: f64) -> Self {
        let (s, c) = rotation.sin_cos();
        Self {
            n,
            my: my_local * c + mz_local * s,
            mz: -my_local * s + mz_local * c,
        }
    }
}

/// A stress sample at a point of the section.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StressPoint {
    /// Position relative to the centroid.
    pub y: f64,
    pub z: f64,
    /// Normal stress, same force/length units as the inputs.
    pub sigma: f64,
}

/// The straight line `sigma = 0`, as `a*y + b*z + c = 0` with `(a,b)` unit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeutralAxis {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    /// Angle of the line from the section y-axis, radians.
    pub angle: f64,
    /// True when bending is absent, so no neutral axis exists.
    pub uniform: bool,
}

/// Result of an axial + bending analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BendingResult {
    /// Derived properties the stress field was computed from.
    pub properties: SectionProperties,
    /// Resultants actually used, echoed for traceability.
    pub forces: SectionForces,
    /// Stress sampled at every canonical boundary vertex, centroid-relative.
    pub boundary: Vec<StressPoint>,
    /// Most tensile point.
    pub max: StressPoint,
    /// Most compressive point.
    pub min: StressPoint,
    pub neutral_axis: NeutralAxis,
    /// Bending coefficients, so a consumer can evaluate sigma anywhere:
    /// `sigma = n/a + kz*z - ky*y`.
    pub kz: f64,
    pub ky: f64,
}

/// Relative floor for calling the inertia tensor singular.
///
/// The determinant `Iy*Iz - Iyz^2` scales as inertia squared, so it is
/// compared against `Iy*Iz` rather than an absolute number: a section measured
/// in millimetres would otherwise look singular to an absolute threshold and a
/// section in metres would never trip it.
const SINGULAR_TENSOR_RATIO: f64 = 1e-12;

/// Compute the axial + bending stress field over canonical geometry.
pub fn analyze_bending(
    polygons: &[SectionPolygon],
    forces: SectionForces,
) -> Result<BendingResult, String> {
    for v in [forces.n, forces.my, forces.mz] {
        if !v.is_finite() {
            return Err(format!("section forces must be finite (got {v})"));
        }
    }

    let properties = analyze_section(&SectionInput {
        polygons: polygons.to_vec(),
        modular_ratios: Default::default(),
    })?;

    if !(properties.a > 0.0) || !properties.a.is_finite() {
        return Err(format!("section area must be positive and finite (got {})", properties.a));
    }

    let (iy, iz, iyz) = (properties.iy, properties.iz, properties.iyz);
    let det = iy * iz - iyz * iyz;
    let scale = iy * iz;
    if !det.is_finite() || scale <= 0.0 || det <= SINGULAR_TENSOR_RATIO * scale {
        return Err(format!(
            "inertia tensor is singular or degenerate (Iy={iy:.6e}, Iz={iz:.6e}, Iyz={iyz:.6e}); \
             the section has no well-defined bending response about one axis"
        ));
    }

    // sigma = N/A + kz*z - ky*y
    let kz = (forces.my * iz + forces.mz * iyz) / det;
    let ky = (forces.mz * iy + forces.my * iyz) / det;
    let n_over_a = forces.n / properties.a;
    let sigma_at = |y: f64, z: f64| n_over_a + kz * z - ky * y;

    // Extreme normal stress in a linear field always occurs on the boundary,
    // and for a polygon always at a vertex, so sampling vertices is exact
    // rather than a discretization.
    let (cy, cz) = (properties.yc, properties.zc);
    let mut boundary: Vec<StressPoint> = Vec::new();
    for poly in polygons.iter().filter(|p| !p.is_void) {
        for v in &poly.vertices {
            let (y, z) = (v[0] - cy, v[1] - cz);
            boundary.push(StressPoint { y, z, sigma: sigma_at(y, z) });
        }
    }
    if boundary.is_empty() {
        return Err("section has no solid boundary to evaluate".into());
    }

    let mut max = boundary[0];
    let mut min = boundary[0];
    for p in &boundary {
        if p.sigma > max.sigma {
            max = *p;
        }
        if p.sigma < min.sigma {
            min = *p;
        }
    }

    // Zero set of sigma: (-ky)*y + (kz)*z + N/A = 0.
    let (mut a, mut b, mut c) = (-ky, kz, n_over_a);
    let norm = (a * a + b * b).sqrt();
    let uniform = norm <= f64::EPSILON * (1.0 + n_over_a.abs());
    let angle = if uniform {
        0.0
    } else {
        a /= norm;
        b /= norm;
        c /= norm;
        // Direction of the line is perpendicular to its normal (a, b).
        (-a).atan2(b)
    };

    Ok(BendingResult {
        properties,
        forces,
        boundary,
        max,
        min,
        neutral_axis: NeutralAxis { a, b, c, angle, uniform },
        kz,
        ky,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::section::catalogue::{
        angle_section, channel_section, custom, rectangle, tee_section, DEFAULT_ARC_SEGMENTS,
    };
    use crate::section::mesh::{mesh_section, MeshParams};

    fn rel(got: f64, exp: f64) -> f64 {
        if exp.abs() < 1e-300 { got.abs() } else { ((got - exp) / exp).abs() }
    }

    /// Integrate sigma, sigma*z and -sigma*y over the section to recover the
    /// applied resultants. This is the single strongest check available: it
    /// closes the loop from geometry through the inertia tensor to the stress
    /// field and back, and it fails if any of them is inconsistent.
    fn recover(polys: &[SectionPolygon], r: &BendingResult, max_area: f64) -> (f64, f64, f64) {
        let mesh = mesh_section(polys, &MeshParams { max_area, ..Default::default() }).unwrap();
        let (cy, cz) = (r.properties.yc, r.properties.zc);
        let sigma = |y: f64, z: f64| r.forces.n / r.properties.a + r.kz * z - r.ky * y;
        let (mut n, mut my, mut mz) = (0.0, 0.0, 0.0);
        for &t in &mesh.triangles {
            let area = mesh.triangle_area(t);
            let y: [f64; 3] = [
                mesh.nodes[t[0]][0] - cy, mesh.nodes[t[1]][0] - cy, mesh.nodes[t[2]][0] - cy,
            ];
            let z: [f64; 3] = [
                mesh.nodes[t[0]][1] - cz, mesh.nodes[t[1]][1] - cz, mesh.nodes[t[2]][1] - cz,
            ];
            let s: [f64; 3] = [sigma(y[0], z[0]), sigma(y[1], z[1]), sigma(y[2], z[2])];

            // sigma is linear, so the centroid rule is exact for its integral.
            n += area * (s[0] + s[1] + s[2]) / 3.0;

            // sigma*z and sigma*y are QUADRATIC. A one-point rule leaves an
            // O(h^2) error — it showed up here as a 0.2 % shortfall in My,
            // which is quadrature error in this helper, not solver error. The
            // exact rule for the product of two linear functions over a
            // triangle is A/12 * (sum f_i g_i + (sum f)(sum g)).
            let quad = |f: [f64; 3], g: [f64; 3]| {
                area / 12.0
                    * (f[0] * g[0] + f[1] * g[1] + f[2] * g[2]
                        + (f[0] + f[1] + f[2]) * (g[0] + g[1] + g[2]))
            };
            my += quad(s, z);
            mz += -quad(s, y);
        }
        (n, my, mz)
    }

    // ── Rectangle: the reduced form must still be reproduced ──────

    #[test]
    fn rectangle_axial_only() {
        let g = rectangle(0.2, 0.4).unwrap();
        let r = analyze_bending(&g.polygons, SectionForces { n: 200.0, my: 0.0, mz: 0.0 }).unwrap();
        for p in &r.boundary {
            assert!(rel(p.sigma, 200.0 / 0.08) < 1e-12, "uniform axial stress");
        }
        assert!(r.neutral_axis.uniform, "pure axial has no neutral axis");
    }

    #[test]
    fn rectangle_uniaxial_bending_matches_navier() {
        let (b, h) = (0.2, 0.4);
        let g = rectangle(b, h).unwrap();
        let my = 50.0;
        let r = analyze_bending(&g.polygons, SectionForces { n: 0.0, my, mz: 0.0 }).unwrap();
        let iy = b * h.powi(3) / 12.0;
        assert!(rel(r.max.sigma, my * (h / 2.0) / iy) < 1e-12, "sigma_max = M c / I");
        assert!(rel(r.min.sigma, -my * (h / 2.0) / iy) < 1e-12);
        // Neutral axis is the horizontal centroidal axis.
        assert!(r.neutral_axis.angle.abs() < 1e-12, "angle {}", r.neutral_axis.angle);
        assert!(r.neutral_axis.c.abs() < 1e-15, "passes through the centroid");
    }

    #[test]
    fn rectangle_biaxial_bending_superposes() {
        let (b, h) = (0.2, 0.4);
        let g = rectangle(b, h).unwrap();
        let (my, mz) = (50.0, 30.0);
        let r = analyze_bending(&g.polygons, SectionForces { n: 0.0, my, mz }).unwrap();
        let iy = b * h.powi(3) / 12.0;
        let iz = h * b.powi(3) / 12.0;
        // Corner (+b/2, +h/2): sigma = My*z/Iy - Mz*y/Iz
        let expect = my * (h / 2.0) / iy - mz * (b / 2.0) / iz;
        let corner = r
            .boundary
            .iter()
            .find(|p| (p.y - b / 2.0).abs() < 1e-12 && (p.z - h / 2.0).abs() < 1e-12)
            .expect("corner sample");
        assert!(rel(corner.sigma, expect) < 1e-12, "{} vs {expect}", corner.sigma);
        // The neutral axis is tilted away from both geometric axes.
        assert!(r.neutral_axis.angle.abs() > 1e-3);
    }

    #[test]
    fn sign_reversal_mirrors_the_field() {
        let g = rectangle(0.2, 0.4).unwrap();
        let f = SectionForces { n: 100.0, my: 50.0, mz: -20.0 };
        let a = analyze_bending(&g.polygons, f).unwrap();
        let b = analyze_bending(
            &g.polygons,
            SectionForces { n: -f.n, my: -f.my, mz: -f.mz },
        )
        .unwrap();
        for (p, q) in a.boundary.iter().zip(b.boundary.iter()) {
            assert!((p.sigma + q.sigma).abs() < 1e-12, "reversing every resultant must negate sigma");
        }
        assert!(rel(a.max.sigma, -b.min.sigma) < 1e-12);
    }

    // ── The unsymmetrical cases the old path got wrong ────────────

    #[test]
    fn equal_leg_angle_uses_the_full_tensor() {
        let g = angle_section(0.100, 0.100, 0.010).unwrap();
        let r = analyze_bending(&g.polygons, SectionForces { n: 0.0, my: 10.0, mz: 0.0 }).unwrap();
        let p = &r.properties;
        assert!(p.iyz.abs() > 1e-9, "an angle has a non-zero product of inertia");
        assert!((p.theta_p.to_degrees().abs() - 45.0).abs() < 1e-6);

        // The reduced formula My*z/Iy is what the old path applied. With
        // Iyz != 0 it is a different answer, and this pins the difference so
        // the general form cannot silently regress to it.
        let naive_max = r
            .boundary
            .iter()
            .map(|q| 10.0 * q.z / p.iy)
            .fold(f64::MIN, f64::max);
        assert!(
            rel(r.max.sigma, naive_max) > 0.05,
            "the general solution must differ measurably from the Iyz=0 shortcut"
        );

        // With My alone the neutral axis is NOT the y-axis, precisely because
        // the geometric axes are not principal.
        assert!(r.neutral_axis.angle.abs() > 1e-3, "neutral axis must tilt");
    }

    #[test]
    fn bending_about_a_principal_axis_keeps_the_neutral_axis_aligned() {
        // Physical cross-check on the same angle: resolve a moment onto the
        // principal directions and the neutral axis must line up with the
        // other principal axis. If Iyz were mishandled this would not hold.
        let g = angle_section(0.100, 0.100, 0.010).unwrap();
        let p0 = analyze_bending(&g.polygons, SectionForces::default()).unwrap().properties;
        let th = p0.theta_p;
        let m = 10.0;
        // Moment vector along the first principal axis.
        let r = analyze_bending(
            &g.polygons,
            SectionForces { n: 0.0, my: m * th.cos(), mz: m * th.sin() },
        )
        .unwrap();
        let na = r.neutral_axis.angle;
        // Neutral axis should coincide with the principal axis direction
        // (mod pi).
        let diff = ((na - th).rem_euclid(std::f64::consts::PI)).min(
            (th - na).rem_euclid(std::f64::consts::PI),
        );
        assert!(diff < 1e-6 || (std::f64::consts::PI - diff) < 1e-6, "na {na} vs theta_p {th}");
    }

    #[test]
    fn unequal_angle_channel_and_tee_are_all_unsymmetrical() {
        let cases: Vec<(&str, Vec<SectionPolygon>)> = vec![
            ("unequal angle", angle_section(0.100, 0.065, 0.008).unwrap().polygons),
            ("channel", channel_section(0.20, 0.075, 0.0085, 0.0115).unwrap().polygons),
            ("tee", tee_section(0.30, 0.30, 0.10, 0.15).unwrap().polygons),
        ];
        for (name, polys) in cases {
            let r = analyze_bending(&polys, SectionForces { n: 25.0, my: 12.0, mz: 4.0 }).unwrap();
            assert!(r.max.sigma > r.min.sigma, "{name}: field must vary");
            let (n, my, mz) = recover(&polys, &r, 2e-5);
            assert!(rel(n, 25.0) < 1e-6, "{name}: N recovered {n}");
            assert!(rel(my, 12.0) < 1e-6, "{name}: My recovered {my}");
            assert!(rel(mz, 4.0) < 1e-6, "{name}: Mz recovered {mz}");
        }
    }

    #[test]
    fn arbitrary_asymmetric_polygon_and_polygon_with_a_hole() {
        let asym = custom(
            vec![[0.0, 0.0], [0.30, 0.0], [0.22, 0.09], [0.26, 0.24], [0.05, 0.18]],
            vec![],
        )
        .unwrap();
        let holed = custom(
            vec![[0.0, 0.0], [0.20, 0.0], [0.20, 0.30], [0.0, 0.30]],
            vec![vec![[0.05, 0.06], [0.15, 0.06], [0.15, 0.20], [0.05, 0.20]]],
        )
        .unwrap();
        for (name, g, ma) in [("asymmetric", asym, 5e-5), ("with hole", holed, 5e-5)] {
            let f = SectionForces { n: -40.0, my: 8.0, mz: -6.0 };
            let r = analyze_bending(&g.polygons, f).unwrap();
            assert!(r.properties.iyz.abs() > 0.0, "{name}");
            let (n, my, mz) = recover(&g.polygons, &r, ma);
            assert!(rel(n, f.n) < 1e-6, "{name}: N {n}");
            assert!(rel(my, f.my) < 1e-6, "{name}: My {my}");
            assert!(rel(mz, f.mz) < 1e-6, "{name}: Mz {mz}");
        }
    }

    // ── Rotation ─────────────────────────────────────────────────

    #[test]
    fn rotating_geometry_and_forces_together_leaves_stresses_unchanged() {
        // The physical invariance: rotate the section and rotate the moment
        // vector with it, and every material point must see the same stress.
        let g = angle_section(0.100, 0.100, 0.010).unwrap();
        let th = 0.7_f64;
        let rotated: Vec<SectionPolygon> = g
            .polygons
            .iter()
            .map(|p| SectionPolygon {
                vertices: p
                    .vertices
                    .iter()
                    .map(|v| [v[0] * th.cos() - v[1] * th.sin(), v[0] * th.sin() + v[1] * th.cos()])
                    .collect(),
                material_id: p.material_id,
                is_void: p.is_void,
            })
            .collect();

        let f = SectionForces { n: 30.0, my: 9.0, mz: -5.0 };
        let base = analyze_bending(&g.polygons, f).unwrap();
        // Moment vector rotated by the same angle.
        let f_rot = SectionForces {
            n: f.n,
            my: f.my * th.cos() - f.mz * th.sin(),
            mz: f.my * th.sin() + f.mz * th.cos(),
        };
        let rot = analyze_bending(&rotated, f_rot).unwrap();

        assert!(rel(rot.max.sigma, base.max.sigma) < 1e-9, "max stress must be invariant");
        assert!(rel(rot.min.sigma, base.min.sigma) < 1e-9, "min stress must be invariant");
        // Vertices are in the same order, so stresses correspond point by point.
        for (p, q) in base.boundary.iter().zip(rot.boundary.iter()) {
            assert!((p.sigma - q.sigma).abs() < 1e-9 * (1.0 + p.sigma.abs()));
        }
    }

    #[test]
    fn section_rotation_transforms_the_local_moment_vector() {
        // A section rotated by +theta sees the moment rotated by -theta.
        let f = SectionForces::from_local(10.0, 100.0, 0.0, std::f64::consts::FRAC_PI_2);
        assert!(rel(f.n, 10.0) < 1e-15);
        assert!(f.my.abs() < 1e-12, "my {}", f.my);
        assert!(rel(f.mz, -100.0) < 1e-12, "mz {}", f.mz);
        // Zero rotation is the identity.
        let id = SectionForces::from_local(1.0, 2.0, 3.0, 0.0);
        assert!(rel(id.my, 2.0) < 1e-15 && rel(id.mz, 3.0) < 1e-15);
    }

    // ── Equivalent 2D / 3D ───────────────────────────────────────

    #[test]
    fn a_2d_case_equals_the_equivalent_3d_case() {
        // 2D bending is the Mz = 0 slice of the 3D problem; the same geometry
        // and the same My must give the same field.
        let g = rectangle(0.2, 0.4).unwrap();
        let two_d = analyze_bending(&g.polygons, SectionForces { n: 15.0, my: 40.0, mz: 0.0 }).unwrap();
        let three_d = analyze_bending(
            &g.polygons,
            SectionForces::from_local(15.0, 40.0, 0.0, 0.0),
        )
        .unwrap();
        assert!(rel(three_d.max.sigma, two_d.max.sigma) < 1e-15);
        assert!(rel(three_d.min.sigma, two_d.min.sigma) < 1e-15);
    }

    // ── Resultant recovery on a rolled profile ───────────────────

    #[test]
    fn resultants_are_recovered_on_a_filleted_i_profile() {
        use crate::section::catalogue::{i_section, GeometrySource};
        let g = i_section(
            0.300, 0.150, 0.0071, 0.0107, 0.015, DEFAULT_ARC_SEGMENTS,
            GeometrySource::Catalogue { profile_id: "IPE 300".into(), standard: "EN 10365".into() },
        )
        .unwrap();
        let f = SectionForces { n: 120.0, my: 65.0, mz: 8.0 };
        let r = analyze_bending(&g.polygons, f).unwrap();
        let (n, my, mz) = recover(&g.polygons, &r, 5e-6);
        assert!(rel(n, f.n) < 1e-6, "N {n}");
        assert!(rel(my, f.my) < 1e-6, "My {my}");
        assert!(rel(mz, f.mz) < 1e-6, "Mz {mz}");
    }

    // ── Rejection ────────────────────────────────────────────────

    #[test]
    fn rejects_degenerate_and_non_finite_input() {
        let g = rectangle(0.2, 0.4).unwrap();
        assert!(analyze_bending(&g.polygons, SectionForces { n: f64::NAN, my: 0.0, mz: 0.0 }).is_err());
        assert!(analyze_bending(&g.polygons, SectionForces { n: 0.0, my: f64::INFINITY, mz: 0.0 }).is_err());
        // A zero-area / collinear polygon cannot support a stress field.
        let degenerate = vec![SectionPolygon {
            vertices: vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            material_id: 0,
            is_void: false,
        }];
        assert!(analyze_bending(&degenerate, SectionForces { n: 1.0, my: 0.0, mz: 0.0 }).is_err());
    }

    #[test]
    fn wire_round_trip_is_stable() {
        let g = rectangle(0.2, 0.4).unwrap();
        let r = analyze_bending(&g.polygons, SectionForces { n: 10.0, my: 20.0, mz: 5.0 }).unwrap();
        let json = serde_json::to_string(&r).unwrap();
        let back: BendingResult = serde_json::from_str(&json).unwrap();
        assert!(rel(back.max.sigma, r.max.sigma) < 1e-12);
        assert_eq!(back.boundary.len(), r.boundary.len());
        assert!(rel(back.neutral_axis.angle, r.neutral_axis.angle) < 1e-12);
    }
}
