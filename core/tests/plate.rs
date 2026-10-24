//! The plate slab against Navier's simply supported square plate, and its load balance.

use pondus_core::analysis::analyze;
use pondus_core::model::*;
use pondus_core::slabfe::SlabMethod;

/// A 4 x 4 m slab, h 0.2, on four edge beams rigid in bending and free in torsion (simple
/// supports), on pinned corners; 10 kPa in the live case.
fn square(mesh: f64) -> Model {
    let mut m = Model::new("plate");
    m.cases[0].self_weight = false;
    m.design.slab_method = SlabMethod::Plate;
    m.design.slab_mesh = mesh;
    m.materials[0].e = 25_000.0;
    m.materials[0].nu = 0.2;
    let sid = m.next_section_id();
    m.sections.push(Section { id: sid, name: "rigid".into(), shape: Shape::General { a: 10.0, iy: 1e3, iz: 1e3, j: 1e-9, shear: None } });
    let c = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 4.0, 0.0], [0.0, 4.0, 0.0]];
    for k in 0..4 {
        let (a, b) = (m.node_at(c[k]), m.node_at(c[(k + 1) % 4]));
        let id = m.next_member_id();
        m.members.push(Member { id, i: a, j: b, section: sid, material: 1, kind: MemberKind::Beam, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    }
    for n in m.nodes.clone() {
        m.supports.push(Support { node: n.id, fix: [true, true, true, false, false, true], springs: [0.0; 6], uplift: false });
    }
    m.slabs.push(Slab { id: 1, name: "S1".into(), x0: 0.0, y0: 0.0, x1: 4.0, y1: 4.0, z: 0.0, h: 0.2, dead: 0.0, live: 10.0, bim_id: None, ..Default::default() });
    m
}

#[test]
fn navier_square_plate() {
    let m = square(0.25);
    let a = analyze(&m).unwrap();
    let l = a.case(2).unwrap();
    // all 160 kN reach the supports
    assert!((l.reaction_sum()[2] - 160.0).abs() < 1e-6, "{:?}", l.reaction_sum());
    assert!((l.applied[2] + 160.0).abs() < 1e-9);
    let c = l.plates.iter().min_by(|p, q| ((p.x - 2.0).hypot(p.y - 2.0)).total_cmp(&(q.x - 2.0).hypot(q.y - 2.0))).unwrap();
    // Navier, ν 0.2: M = (1 + ν) 0.0368 q a² = 7.07 kN·m/m (sagging, positive); w = 0.00406 q a⁴ / D
    let navier = 1.2 * 0.0368 * 10.0 * 16.0;
    eprintln!("centre element at ({:.3}, {:.3}): mx {:.3} my {:.3} mxy {:.3}, Navier {navier:.3}", c.x, c.y, c.mx, c.my, c.mxy);
    assert!((c.mx - navier).abs() / navier < 0.03, "mx {} vs {navier}", c.mx);
    assert!((c.my - navier).abs() / navier < 0.03);
    let d = 25_000e3 * 0.2f64.powi(3) / (12.0 * (1.0 - 0.04));
    let w = 0.00406 * 10.0 * 256.0 / d;
    let wmax = l.plates.iter().map(|p| -p.uz).fold(0.0, f64::max);
    eprintln!("deflection {:.3} mm, Navier {:.3} mm", wmax * 1e3, w * 1e3);
    assert!((wmax - w).abs() / w < 0.04, "{wmax} vs {w}");
    // the corner twist: Navier's |mxy| at a corner = (1 − ν) 0.0464 q a² (0.0325 q a² for ν 0.3),
    // 5.94 here; the corner element's centre sits 0.125 m in, a little lower
    let corner = l.plates.iter().map(|p| p.mxy.abs()).fold(0.0, f64::max);
    let navier_c = 0.8 * 0.0464 * 160.0;
    eprintln!("largest |mxy| {corner:.3} (Navier corner {navier_c:.3})");
    assert!(corner < navier_c && corner > 0.9 * navier_c);
}

#[test]
fn plate_floor_on_frame_balances() {
    // the RC floor of the flow test, slabs as plates: the slab load reaches the supports
    let mut m = Model::new("floor");
    m.design.slab_method = SlabMethod::Plate;
    let col = m.rect_section(0.4, 0.4);
    let bm = m.rect_section(0.3, 0.6);
    let add = |m: &mut Model, p: [f64; 3], q: [f64; 3], s: Id| {
        let (a, b) = (m.node_at(p), m.node_at(q));
        let id = m.next_member_id();
        m.members.push(Member { id, i: a, j: b, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    };
    for x in [0.0, 6.0, 12.0] {
        for y in [0.0, 5.0] {
            add(&mut m, [x, y, 0.0], [x, y, 3.5], col);
        }
        add(&mut m, [x, 0.0, 3.5], [x, 5.0, 3.5], bm);
    }
    for y in [0.0, 5.0] {
        add(&mut m, [0.0, y, 3.5], [6.0, y, 3.5], bm);
        add(&mut m, [6.0, y, 3.5], [12.0, y, 3.5], bm);
    }
    for n in m.nodes.clone().iter().filter(|n| n.z == 0.0) {
        m.supports.push(Support::of_kind(n.id, "fixed").unwrap());
    }
    for (k, x) in [0.0, 6.0].iter().enumerate() {
        m.slabs.push(Slab { id: k + 1, name: format!("S{}", k + 1), x0: *x, y0: 0.0, x1: x + 6.0, y1: 5.0, z: 3.5, h: 0.15, dead: 1.5, live: 3.0, bim_id: None, ..Default::default() });
    }
    let a = analyze(&m).unwrap();
    let l = a.case(2).unwrap();
    assert!((l.reaction_sum()[2] - 180.0).abs() < 1e-6, "{:?}", l.reaction_sum());
    assert!((a.slab_applied[0] - a.slab_expected[0]).abs() < 1e-6);
    let d = pondus_core::design::design_pass(&m, &a, &Default::default());
    for s in &d.slabs {
        eprintln!("slab {}: {} ratio {:.2} governs {}", s.slab, s.method, s.ratio, s.governs);
        for t in &s.strips {
            eprintln!("   {} {}: Mu {:.1} -> {}", t.direction, t.location, t.mu, t.bars);
        }
        for n in &s.notes {
            eprintln!("   note: {n}");
        }
    }
    assert_eq!(d.slabs.len(), 2);
    assert!(d.slabs[0].method.starts_with("plate"));
}
