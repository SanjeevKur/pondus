//! Analysis against closed-form results: signs, magnitudes and load balance.

use pondus_core::analysis::analyze;
use pondus_core::model::*;

fn beam(len: f64, sup: [&str; 2]) -> Model {
    let mut m = Model::new("t");
    m.cases[0].self_weight = false;
    let a = m.node_at([0.0, 0.0, 3.0]);
    let b = m.node_at([len, 0.0, 3.0]);
    let s = m.rect_section(0.3, 0.6);
    m.members.push(Member { id: 1, i: a, j: b, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: Some("B1".into()), tension: None, role: Default::default(), group: None });
    for (n, k) in [(a, sup[0]), (b, sup[1])] {
        if k != "free" {
            let mut s = Support::of_kind(n, k).unwrap();
            // hold the beam's twist and sway so the pinned/roller beam is stable
            s.fix[0] |= k == "pinned";
            s.fix[1] = true;
            s.fix[3] = true;
            m.supports.push(s);
        }
    }
    m
}

#[test]
fn simply_supported_udl() {
    let (l, w) = (6.0, 10.0);
    let mut m = beam(l, ["pinned", "roller"]);
    m.loads.push(Load { case: 1, kind: LoadKind::Line { member: 1, wi: [0.0, 0.0, -w], wj: [0.0, 0.0, -w], a: None, b: None } });
    let a = analyze(&m).unwrap();
    let r = a.case(1).unwrap();
    let mr = r.member(1).unwrap();
    let mid = mr.stations.iter().min_by(|x, y| (x.x - l / 2.0).abs().total_cmp(&(y.x - l / 2.0).abs())).unwrap();
    // sagging positive: wL²/8 = 45 kN·m
    assert!((mid.my - w * l * l / 8.0).abs() < 1e-6, "My mid {}", mid.my);
    assert!((r.reaction_sum()[2] - w * l).abs() < 1e-6, "{:?}", r.reaction_sum());
    assert!((r.applied[2] + w * l).abs() < 1e-9);
    // shear at the ends: ±wL/2
    let v0 = mr.stations.first().unwrap().vz;
    assert!((v0.abs() - w * l / 2.0).abs() < 1e-6, "V0 {v0}");
    // deflection 5wL⁴/384EI, down
    let ei = m.materials[0].e * 1000.0 * 0.3 * 0.6f64.powi(3) / 12.0;
    let d = -mr.local_deflection([0.0, 0.0, 1.0]).into_iter().fold(0.0f64, f64::min);
    let exact = 5.0 * w * l.powi(4) / (384.0 * ei);
    assert!((d - exact).abs() / exact < 1e-3, "{d} vs {exact}");
}

#[test]
fn fixed_ends_hog() {
    let (l, w) = (6.0, 10.0);
    let mut m = beam(l, ["fixed", "fixed"]);
    m.loads.push(Load { case: 1, kind: LoadKind::Line { member: 1, wi: [0.0, 0.0, -w], wj: [0.0, 0.0, -w], a: None, b: None } });
    let a = analyze(&m).unwrap();
    let mr = a.case(1).unwrap().member(1).unwrap().clone();
    let end = mr.stations[0].my;
    let mid = mr.stations.iter().map(|s| s.my).fold(f64::MIN, f64::max);
    assert!((end + w * l * l / 12.0).abs() < 1e-6, "end {end}");
    assert!((mid - w * l * l / 24.0).abs() < 1e-6, "mid {mid}");
}

#[test]
fn axial_sign_and_point_load() {
    // a column with a downward load at its top is in compression: N < 0
    let mut m = Model::new("c");
    m.cases[0].self_weight = false;
    let a = m.node_at([0.0, 0.0, 0.0]);
    let b = m.node_at([0.0, 0.0, 3.0]);
    let s = m.rect_section(0.4, 0.4);
    m.members.push(Member { id: 1, i: a, j: b, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    m.supports.push(Support::of_kind(a, "fixed").unwrap());
    m.loads.push(Load { case: 1, kind: LoadKind::Nodal { node: b, f: [0.0, 0.0, -100.0, 0.0, 0.0, 0.0] } });
    let r = analyze(&m).unwrap();
    let n = r.case(1).unwrap().member(1).unwrap().stations[1].n;
    assert!((n + 100.0).abs() < 1e-6, "N {n}");
    // a point load mid-span of a cantilever: tip deflection P a²(3L − a)/6EI
    let mut m = beam(4.0, ["fixed", "free"]);
    m.loads.push(Load { case: 1, kind: LoadKind::Point { member: 1, a: 2.0, p: [0.0, 0.0, -20.0] } });
    let r = analyze(&m).unwrap();
    let c = r.case(1).unwrap();
    let tip = c.displacements[&2][2];
    let ei = m.materials[0].e * 1000.0 * 0.3 * 0.6f64.powi(3) / 12.0;
    let exact = -20.0 * 4.0 * (12.0 - 2.0) / (6.0 * ei);
    assert!((tip - exact).abs() / exact.abs() < 1e-6, "{tip} vs {exact}");
    assert!((c.member(1).unwrap().stations[0].my + 40.0).abs() < 1e-6);
}

#[test]
fn slab_load_balance_and_combination() {
    // a 2 x 1 bay floor on beams over columns: every slab kN reaches the beams
    let mut m = Model::new("floor");
    let col = m.rect_section(0.4, 0.4);
    let bm = m.rect_section(0.3, 0.6);
    let xs = [0.0, 6.0, 12.0];
    let ys = [0.0, 5.0];
    let mut id = 0;
    let mut add = |m: &mut Model, p: [f64; 3], q: [f64; 3], s: Id| {
        let (a, b) = (m.node_at(p), m.node_at(q));
        id += 1;
        m.members.push(Member { id, i: a, j: b, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    };
    for &x in &xs {
        for &y in &ys {
            add(&mut m, [x, y, 0.0], [x, y, 3.5], col);
        }
    }
    for &y in &ys {
        add(&mut m, [0.0, y, 3.5], [6.0, y, 3.5], bm);
        add(&mut m, [6.0, y, 3.5], [12.0, y, 3.5], bm);
    }
    for &x in &xs {
        add(&mut m, [x, 0.0, 3.5], [x, 5.0, 3.5], bm);
    }
    for n in m.nodes.clone().iter().filter(|n| n.z == 0.0) {
        m.supports.push(Support::of_kind(n.id, "fixed").unwrap());
    }
    for (k, x) in [0.0, 6.0].iter().enumerate() {
        m.slabs.push(Slab { id: k + 1, name: format!("S{}", k + 1), x0: *x, y0: 0.0, x1: x + 6.0, y1: 5.0, z: 3.5, h: 0.15, dead: 1.5, live: 3.0, bim_id: None });
    }
    let a = analyze(&m).unwrap();
    assert!(a.warnings.is_empty(), "{:?}", a.warnings);
    for k in 0..2 {
        assert!((a.slab_applied[k] - a.slab_expected[k]).abs() < 1e-6, "{:?} {:?}", a.slab_applied, a.slab_expected);
    }
    // live case: reactions = 3 kPa x 60 m²
    let l = a.case(2).unwrap();
    assert!((l.reaction_sum()[2] - 180.0).abs() < 1e-6, "{:?}", l.reaction_sum());
    // dead: slabs (24 x 0.15 + 1.5) x 60 + members' own weight
    let sw: f64 = m.members.iter().map(|mm| 24.0 * m.section(mm.section).unwrap().props().a * m.length(mm)).sum();
    let d = a.case(1).unwrap();
    assert!((d.reaction_sum()[2] - (5.1 * 60.0 + sw)).abs() < 1e-6, "{:?}", d.reaction_sum());
    let c = a.combine("1.2D+1.6L", &[(1, 1.2), (2, 1.6)]);
    assert!((c.reaction_sum()[2] - (1.2 * (5.1 * 60.0 + sw) + 1.6 * 180.0)).abs() < 1e-6);
}

#[test]
fn roll_turns_a_column() {
    // a cantilever column pushed along x: with roll 0 its strong axis (iy) resists, with
    // roll 90 its weak axis (iz) does; tip drift PL³/3EI
    for (roll, strong) in [(0.0, true), (90.0, false)] {
        let mut m = Model::new("c");
        m.cases[0].self_weight = false;
        let a = m.node_at([0.0, 0.0, 0.0]);
        let b = m.node_at([0.0, 0.0, 3.0]);
        let s = m.rect_section(0.3, 0.6);
        m.members.push(Member { id: 1, i: a, j: b, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
        m.supports.push(Support::of_kind(a, "fixed").unwrap());
        m.loads.push(Load { case: 1, kind: LoadKind::Nodal { node: b, f: [10.0, 0.0, 0.0, 0.0, 0.0, 0.0] } });
        let r = analyze(&m).unwrap();
        let ux = r.case(1).unwrap().displacements[&b][0];
        let i = if strong { 0.3 * 0.6f64.powi(3) / 12.0 } else { 0.6 * 0.3f64.powi(3) / 12.0 };
        let exact = 10.0 * 27.0 / (3.0 * m.materials[0].e * 1000.0 * i);
        assert!((ux - exact).abs() / exact < 1e-6, "roll {roll}: {ux} vs {exact}");
    }
}
