//! Plastic section moduli, `Z`, about both centroidal axes.
//!
//! # What it is, and why the elastic modulus is not enough
//!
//! `S = I / c` describes a section that has just started to yield at its
//! extreme fibre. `Z` describes one fully plastic — every fibre at yield — and
//! is what a plastic or limit-state check needs. The ratio `Z/S` is the shape
//! factor, about 1.5 for a rectangle and nearer 1.12 for a rolled I-profile,
//! and it is precisely the reserve a design code lets you use.
//!
//! # How it is found
//!
//! `Z` is taken about the PLASTIC neutral axis, which is not the centroid: it
//! is the line that splits the section into two EQUAL AREAS, since a fully
//! plastic section carries equal tension and compression forces. For a
//! doubly-symmetric shape the two coincide; for a T or a channel they do not,
//! and using the centroid instead would understate `Z`.
//!
//! The axis is located by bisection, and the areas and first moments on each
//! side are integrated EXACTLY rather than approximated by counting whole
//! triangles: each triangle straddling the line is clipped against it, which is
//! a triangle-versus-half-plane clip yielding a triangle or a quadrilateral.
//! Assigning whole triangles by their centroid instead would leave an error
//! proportional to the mesh size right where the integrand is largest.

use serde::{Deserialize, Serialize};

use super::mesh::SectionMesh;

/// Plastic moduli and the axes they were taken about.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlasticResult {
    /// Plastic modulus for bending about the horizontal axis.
    pub zy: f64,
    /// Plastic modulus for bending about the vertical axis.
    pub zz: f64,
    /// Plastic neutral axis for `zy`, as a `z` coordinate in mesh units.
    pub pna_z: f64,
    /// Plastic neutral axis for `zz`, as a `y` coordinate in mesh units.
    pub pna_y: f64,
}

/// Area and first moment of the part of a triangle on one side of the line
/// `coord = c`, where `coord` is 0 for `y` and 1 for `z`.
///
/// `above` selects which side. The first moment is taken about the line itself,
/// so it is already the `|distance| * area` that `Z` sums.
fn clipped(
    nodes: [[f64; 2]; 3],
    coord: usize,
    c: f64,
    above: bool,
) -> (f64, f64) {
    // Sutherland-Hodgman against a single half-plane.
    let inside = |p: [f64; 2]| if above { p[coord] >= c } else { p[coord] <= c };
    let mut poly: Vec<[f64; 2]> = Vec::with_capacity(4);
    for i in 0..3 {
        let (p, q) = (nodes[i], nodes[(i + 1) % 3]);
        let (pi, qi) = (inside(p), inside(q));
        if pi {
            poly.push(p);
        }
        if pi != qi {
            // The edge crosses the line; interpolate the crossing point.
            let d = q[coord] - p[coord];
            if d.abs() > 0.0 {
                let t = (c - p[coord]) / d;
                poly.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            }
        }
    }
    if poly.len() < 3 {
        return (0.0, 0.0);
    }
    // Shoelace area and centroid of the clipped polygon.
    let (mut a2, mut cy, mut cz) = (0.0, 0.0, 0.0);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let cr = p[0] * q[1] - q[0] * p[1];
        a2 += cr;
        cy += (p[0] + q[0]) * cr;
        cz += (p[1] + q[1]) * cr;
    }
    let area = a2 / 2.0;
    if area.abs() < 1e-300 {
        return (0.0, 0.0);
    }
    let centroid = [cy / (3.0 * a2), cz / (3.0 * a2)];
    (area.abs(), area.abs() * (centroid[coord] - c).abs())
}

/// Total area and first moment above/below a candidate axis.
fn split(mesh: &SectionMesh, coord: usize, c: f64) -> (f64, f64, f64, f64) {
    let (mut a_up, mut m_up, mut a_dn, mut m_dn) = (0.0, 0.0, 0.0, 0.0);
    for &t in &mesh.triangles {
        let n = [mesh.nodes[t[0]], mesh.nodes[t[1]], mesh.nodes[t[2]]];
        let (a, m) = clipped(n, coord, c, true);
        a_up += a;
        m_up += m;
        let (a2, m2) = clipped(n, coord, c, false);
        a_dn += a2;
        m_dn += m2;
    }
    (a_up, m_up, a_dn, m_dn)
}

/// Locate the equal-area axis and return `Z` about it.
fn modulus_about(mesh: &SectionMesh, coord: usize) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for n in &mesh.nodes {
        lo = lo.min(n[coord]);
        hi = hi.max(n[coord]);
    }
    // Bisection on "area above equals area below". The function is monotone in
    // `c`, so bisection cannot miss and needs no derivative.
    let mut a = lo;
    let mut b = hi;
    for _ in 0..80 {
        let mid = 0.5 * (a + b);
        let (a_up, _, a_dn, _) = split(mesh, coord, mid);
        if a_up > a_dn {
            a = mid;
        } else {
            b = mid;
        }
    }
    let c = 0.5 * (a + b);
    let (_, m_up, _, m_dn) = split(mesh, coord, c);
    // Z is the sum of both first moments about the axis: each half contributes
    // its area times the distance from its own centroid to the axis.
    (m_up + m_dn, c)
}

/// Plastic moduli about both centroidal axes.
pub fn solve_plastic(mesh: &SectionMesh) -> Result<PlasticResult, String> {
    if mesh.triangles.is_empty() {
        return Err("mesh has no triangles".into());
    }
    // Bending about the horizontal axis splits the section by `z`.
    let (zy, pna_z) = modulus_about(mesh, 1);
    let (zz, pna_y) = modulus_about(mesh, 0);
    Ok(PlasticResult { zy, zz, pna_z, pna_y })
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
    fn src() -> cat::GeometrySource {
        cat::GeometrySource::Parametric { shape: "test".into() }
    }

    /// A rectangle's plastic modulus is `b h^2 / 4` exactly.
    #[test]
    fn a_rectangle_matches_b_h_squared_over_four() {
        let (b, h) = (60.0, 100.0);
        let z = solve_plastic(&meshed(&cat::rectangle(b, h).unwrap(), 4.0)).unwrap();
        let exact = b * h * h / 4.0;
        assert!((z.zy / exact - 1.0).abs() < 0.005, "Zy {:.1} vs {exact:.1}", z.zy);
        let exact_z = h * b * b / 4.0;
        assert!((z.zz / exact_z - 1.0).abs() < 0.005, "Zz {:.1} vs {exact_z:.1}", z.zz);
    }

    /// And a circle's is `d^3 / 6`.
    #[test]
    fn a_circle_matches_d_cubed_over_six() {
        let d = 100.0;
        let z = solve_plastic(&meshed(&cat::solid_circle(d, 96).unwrap(), 2.0)).unwrap();
        let exact = d.powi(3) / 6.0;
        assert!((z.zy / exact - 1.0).abs() < 0.01, "Z {:.1} vs {exact:.1}", z.zy);
    }

    /// The shape factor — `Z/S` — is the reserve past first yield, and it is
    /// about 1.5 for a rectangle. Getting it wrong means the plastic check is
    /// wrong by that much.
    #[test]
    fn a_rectangle_has_a_shape_factor_of_three_halves() {
        let (b, h) = (60.0, 100.0);
        let z = solve_plastic(&meshed(&cat::rectangle(b, h).unwrap(), 4.0)).unwrap();
        let s = b * h * h / 6.0; // I/c for a rectangle
        assert!((z.zy / s - 1.5).abs() < 0.02, "shape factor {:.3}", z.zy / s);
    }

    /// A rolled I-profile's shape factor is much smaller — near 1.12 — because
    /// its material already sits near the extreme fibres. A solver that
    /// returned 1.5 for everything would pass the rectangle test and fail here.
    #[test]
    fn a_rolled_i_profile_has_a_much_smaller_shape_factor() {
        let g = cat::i_section(300.0, 150.0, 7.1, 10.7, 15.0, 8, src()).unwrap();
        let mesh = meshed(&g, 3.0);
        let z = solve_plastic(&mesh).unwrap();
        let props = crate::section::analyze_section(&crate::section::SectionInput {
            polygons: g.polygons.clone(),
            modular_ratios: Default::default(),
        })
        .unwrap();
        // I / c with c = h/2; the geometry is in millimetres throughout.
        let s = props.iy / 150.0;
        let factor = z.zy / s;
        assert!((1.05..1.25).contains(&factor), "shape factor {factor:.3}");
    }

    /// For a doubly-symmetric section the plastic neutral axis IS the centroid.
    #[test]
    fn a_symmetric_section_puts_its_plastic_axis_at_the_centroid() {
        let g = cat::i_section(300.0, 150.0, 7.1, 10.7, 15.0, 8, src()).unwrap();
        let mesh = meshed(&g, 3.0);
        let z = solve_plastic(&mesh).unwrap();
        assert!(z.pna_z.abs() < 0.5, "PNA at {:.3}, expected the centroid", z.pna_z);
    }

    /// For a T it is NOT, and that is the case where using the centroid would
    /// quietly understate Z.
    #[test]
    fn a_tee_puts_its_plastic_axis_away_from_the_centroid() {
        let g = cat::tee_section_filleted(200.0, 150.0, 8.0, 12.0, 0.0, 0.0, 8, src()).unwrap();
        let mesh = meshed(&g, 3.0);
        let z = solve_plastic(&mesh).unwrap();
        let centroid = mesh.centroid();
        assert!(
            (z.pna_z - centroid[1]).abs() > 5.0,
            "PNA {:.2} sits on the centroid {:.2}; a tee's should not",
            z.pna_z,
            centroid[1]
        );
        // And the equal-area property must actually hold there.
        let (a_up, _, a_dn, _) = split(&mesh, 1, z.pna_z);
        assert!((a_up / a_dn - 1.0).abs() < 0.01, "areas {a_up:.2} vs {a_dn:.2}");
    }

    /// Against the CIRSOC 301-EL tables. The published `Zx` column is
    /// independent of everything the solver does, and — unlike the torsion
    /// column that tripped me up earlier — its position is confirmed by the
    /// neighbouring `Sx`, which must equal `Ix / c` and does.
    /// Columns: h, b, tw, tf, root radius, published Zx (cm^3).
    const TABULATED_Z: &[(&str, f64, f64, f64, f64, f64, f64)] = &[
        ("IPE 80", 80.0, 46.0, 3.8, 5.2, 5.0, 23.0),
        ("IPE 200", 200.0, 100.0, 5.6, 8.5, 12.0, 220.0),
        ("IPE 240", 240.0, 120.0, 6.2, 9.8, 15.0, 366.0),
        ("IPE 300", 300.0, 150.0, 7.1, 10.7, 15.0, 628.0),
        ("IPE 360", 360.0, 170.0, 8.0, 12.7, 18.0, 1020.0),
    ];

    #[test]
    fn rolled_profiles_land_on_their_tabulated_plastic_modulus() {
        for &(name, h, b, tw, tf, r, want) in TABULATED_Z {
            let g = cat::i_section(h, b, tw, tf, r, 8, src()).expect(name);
            // Millimetre geometry gives mm^3; the tables are cm^3.
            let got = solve_plastic(&meshed(&g, (tw * tw / 2.0).max(1.0))).unwrap().zy / 1e3;
            let err = (got / want - 1.0) * 100.0;
            assert!(err.abs() < 3.0, "{name}: Z {got:.1} vs published {want:.1} ({err:+.2} %)");
        }
    }

    /// Refining the mesh must not move the answer much: the clipping is exact,
    /// so the only error left is the outline's own discretisation.
    #[test]
    fn the_result_is_stable_under_refinement() {
        let g = cat::rectangle(60.0, 100.0).unwrap();
        let coarse = solve_plastic(&meshed(&g, 40.0)).unwrap().zy;
        let fine = solve_plastic(&meshed(&g, 2.0)).unwrap().zy;
        assert!((coarse / fine - 1.0).abs() < 0.01, "{coarse:.2} vs {fine:.2}");
    }
}
