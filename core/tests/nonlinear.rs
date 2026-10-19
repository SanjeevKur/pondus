//! One-way members, lift-off supports, Winkler foundations and cables against statics.

use pondus_core::analysis::analyze;
use pondus_core::model::*;

fn bare() -> Model {
    let mut m = Model::new("t");
    m.cases[0].self_weight = false;
    m
}

fn mem(m: &mut Model, a: [f64; 3], b: [f64; 3], s: Id, mat: Id) -> Id {
    let (i, j) = (m.node_at(a), m.node_at(b));
    let id = m.next_member_id();
    m.members.push(Member { id, i, j, section: s, material: mat, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    id
}

#[test]
fn tension_only_x_brace() {
    // a 4 x 3 m bay in x-z, pinned columns and beam, X braces tension-only, H 30 kN at the top
    let mut m = bare();
    let c = m.section_named("H 200x200x8x12").unwrap();
    let br = m.section_named("H 100x100x6x8").unwrap();
    let (a, b, cc, d) = ([0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 0.0, 3.0], [0.0, 0.0, 3.0]);
    for (p, q) in [(a, d), (b, cc), (d, cc)] {
        let id = mem(&mut m, p, q, c, 2);
        let x = m.members.iter_mut().find(|x| x.id == id).unwrap();
        x.hinge_i = true;
        x.hinge_j = true;
    }
    let t1 = mem(&mut m, a, cc, br, 2);
    let t2 = mem(&mut m, b, d, br, 2);
    for x in m.members.iter_mut().filter(|x| x.id == t1 || x.id == t2) {
        x.behaviour = Behaviour::TensionOnly;
    }
    for p in [a, b] {
        let n = m.find_node(p).unwrap();
        m.supports.push(Support { node: n, fix: [true, true, true, true, false, true], springs: [0.0; 6], uplift: false });
    }
    let top = m.find_node(d).unwrap();
    for n in [m.find_node(d).unwrap(), m.find_node(cc).unwrap()] {
        m.supports.push(Support { node: n, fix: [false, true, false, true, false, true], springs: [0.0; 6], uplift: false });
    }
    m.loads.push(Load { case: 2, kind: LoadKind::Nodal { node: top, f: [30.0, 0.0, 0.0, 0.0, 0.0, 0.0] } });
    let r = analyze(&m).unwrap();
    let l = r.case(2).unwrap();
    let (n1, n2) = (l.member(t1).unwrap().stations[1].n, l.member(t2).unwrap().stations[1].n);
    let want = 30.0 / 0.8;
    eprintln!("braces: tension {n1:.3} kN (H/cosθ {want:.3}), the other {n2:.4} kN");
    assert!((n1 - want).abs() < 0.05 && n2.abs() < 1e-3, "{n1} {n2}");
}

#[test]
fn lift_off_support() {
    // two spans of 5 m on three supports, 20 kN/m on the first span only: the third support
    // would be pulled down (a negative reaction); allowed to lift off it carries nothing and
    // the first span is a simple beam with an overhang
    let mut m = bare();
    let s = m.rect_section(0.3, 0.6);
    let b1 = mem(&mut m, [0.0, 0.0, 0.0], [5.0, 0.0, 0.0], s, 1);
    mem(&mut m, [5.0, 0.0, 0.0], [10.0, 0.0, 0.0], s, 1);
    for (k, x) in [0.0, 5.0, 10.0].iter().enumerate() {
        let n = m.find_node([*x, 0.0, 0.0]).unwrap();
        let mut sp = Support::of_kind(n, if k == 0 { "pinned" } else { "roller" }).unwrap();
        sp.fix[1] = true;
        sp.fix[3] = true;
        sp.uplift = k == 2;
        m.supports.push(sp);
    }
    m.loads.push(Load { case: 2, kind: LoadKind::Line { member: b1, wi: [0.0, 0.0, -20.0], wj: [0.0, 0.0, -20.0], a: None, b: None } });
    let r = analyze(&m).unwrap();
    let l = r.case(2).unwrap();
    let rz = |x: f64| { let n = m.find_node([x, 0.0, 0.0]).unwrap(); l.reactions.iter().find(|r| r.0 == n).map(|r| r.1[2]).unwrap_or(0.0) };
    eprintln!("reactions {:.3} / {:.3} / {:.4} kN", rz(0.0), rz(5.0), rz(10.0));
    assert!(rz(10.0).abs() < 1e-3 && (rz(0.0) - 50.0).abs() < 0.05 && (rz(5.0) - 50.0).abs() < 0.05);
}

#[test]
fn winkler_uniform_settlement() {
    // a free ground beam on a Winkler foundation k = 20 000 kN/m/m, 30 kN/m: settles w/k
    let mut m = bare();
    let s = m.rect_section(0.4, 0.6);
    let b = mem(&mut m, [0.0, 0.0, 0.0], [8.0, 0.0, 0.0], s, 1);
    m.members[0].winkler = Some([20_000.0, 20_000.0]);
    let n0 = m.find_node([0.0, 0.0, 0.0]).unwrap();
    m.supports.push(Support { node: n0, fix: [true, true, false, true, false, false], springs: [0.0; 6], uplift: false });
    m.loads.push(Load { case: 2, kind: LoadKind::Line { member: b, wi: [0.0, 0.0, -30.0], wj: [0.0, 0.0, -30.0], a: None, b: None } });
    let r = analyze(&m).unwrap();
    let l = r.case(2).unwrap();
    let uz: Vec<f64> = l.member(b).unwrap().stations.iter().map(|s| s.u[2]).collect();
    eprintln!("settlement {:.6} .. {:.6} m, w/k {:.6}", uz.iter().cloned().fold(f64::MAX, f64::min), uz.iter().cloned().fold(f64::MIN, f64::max), 30.0 / 20000.0);
    // exactly uniform once the engine's z foundation matrix has the right coupling sign
    assert!(uz.iter().all(|u| (u + 30.0 / 20000.0).abs() < 1e-9), "{uz:?}");
}

#[test]
fn winkler_sideways() {
    // the same beam loaded sideways on y springs (local y is horizontal): uniform w/k?
    let mut m = bare();
    let s = m.rect_section(0.4, 0.6);
    let b = mem(&mut m, [0.0, 0.0, 0.0], [8.0, 0.0, 0.0], s, 1);
    m.members[0].winkler = Some([20_000.0, 20_000.0]);
    let n0 = m.find_node([0.0, 0.0, 0.0]).unwrap();
    m.supports.push(Support { node: n0, fix: [true, false, true, true, false, false], springs: [0.0; 6], uplift: false });
    m.loads.push(Load { case: 2, kind: LoadKind::Line { member: b, wi: [0.0, 30.0, 0.0], wj: [0.0, 30.0, 0.0], a: None, b: None } });
    let r = analyze(&m).unwrap();
    let uy: Vec<f64> = r.case(2).unwrap().member(b).unwrap().stations.iter().map(|s| s.u[1]).collect();
    assert!(uy.iter().all(|u| (u - 30.0 / 20000.0).abs() < 1e-9), "{uy:?}");
}

#[test]
fn guy_cable() {
    // a 6 m pole fixed at its base, a cable from its top to the ground 8 m away, H 10 kN at the
    // top towards away from the anchor: the cable takes most of it (the pole is flexible)
    let mut m = bare();
    let pole = m.section_named("H 150x150x7x10").unwrap();
    let cab = m.rect_section(0.06, 0.06);
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, 6.0], pole, 2);
    let c = mem(&mut m, [0.0, 0.0, 6.0], [-8.0, 0.0, 0.0], cab, 2);
    m.members.iter_mut().find(|x| x.id == c).unwrap().behaviour = Behaviour::Cable;
    let base = m.find_node([0.0, 0.0, 0.0]).unwrap();
    m.supports.push(Support::of_kind(base, "fixed").unwrap());
    let anchor = m.find_node([-8.0, 0.0, 0.0]).unwrap();
    m.supports.push(Support::of_kind(anchor, "pinned").unwrap());
    let top = m.find_node([0.0, 0.0, 6.0]).unwrap();
    m.loads.push(Load { case: 2, kind: LoadKind::Nodal { node: top, f: [10.0, 0.0, 0.0, 0.0, 0.0, 0.0] } });
    let r = analyze(&m).unwrap();
    let t = r.case(2).unwrap().member(c).unwrap().stations[1].n;
    // a sagging cable (0.277 kN/m self-weight, Ernst's modulus) takes less than a weightless
    // one (12.5 kN), and the pole carries the rest: its base moment is (H − T cos θ) x 6 m
    let pole = r.case(2).unwrap().member(1).unwrap();
    let mb = pole.stations[0].my.abs().max(pole.stations[0].mz.abs());
    let want = (10.0 - 0.8 * t) * 6.0;
    eprintln!("cable tension {t:.3} kN (weightless 12.500); pole base moment {mb:.3}, (H − T cos θ) h = {want:.3}");
    assert!(t > 0.0 && t < 12.5);
    assert!((mb - want).abs() < 0.02 * want.abs().max(1.0), "{mb} vs {want}");
}
