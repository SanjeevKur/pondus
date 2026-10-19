//! Settlement, temperature, springs, truss members and diaphragms against hand results.

use pondus_core::analysis::analyze;
use pondus_core::model::*;

fn mem(m: &mut Model, a: [f64; 3], b: [f64; 3], s: Id) -> Id {
    let (i, j) = (m.node_at(a), m.node_at(b));
    let id = m.next_member_id();
    m.members.push(Member { id, i, j, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, group: None });
    id
}

fn bare() -> Model {
    let mut m = Model::new("t");
    m.cases[0].self_weight = false;
    m
}

#[test]
fn settlement_moments() {
    // fixed-fixed beam, L 6, the right support settles 10 mm: M = 6EIδ/L² at both ends
    let mut m = bare();
    let s = m.rect_section(0.3, 0.6);
    mem(&mut m, [0.0, 0.0, 0.0], [6.0, 0.0, 0.0], s);
    m.supports.push(Support::of_kind(1, "fixed").unwrap());
    m.supports.push(Support::of_kind(2, "fixed").unwrap());
    m.loads.push(Load { case: 2, kind: LoadKind::Settlement { node: 2, d: [0.0, 0.0, -0.010, 0.0, 0.0, 0.0] } });
    let a = analyze(&m).unwrap();
    let ei = m.materials[0].e * 1000.0 * 0.3 * 0.6f64.powi(3) / 12.0;
    let want = 6.0 * ei * 0.010 / 36.0;
    let st = &a.case(2).unwrap().members[0].stations;
    eprintln!("settlement end moments {:.2} / {:.2}, 6EIδ/L² {want:.2}", st[0].my, st.last().unwrap().my);
    assert!((st[0].my.abs() - want).abs() / want < 1e-3 && (st.last().unwrap().my.abs() - want).abs() / want < 1e-3);
    // nothing in the other case
    assert!(a.case(1).unwrap().members[0].stations[0].my.abs() < 1e-9);
}

#[test]
fn temperature_force() {
    // a bar fixed at both ends, +30 °C: N = −E A α ΔT
    let mut m = bare();
    let s = m.rect_section(0.3, 0.3);
    mem(&mut m, [0.0, 0.0, 0.0], [5.0, 0.0, 0.0], s);
    m.supports.push(Support::of_kind(1, "fixed").unwrap());
    m.supports.push(Support::of_kind(2, "fixed").unwrap());
    m.loads.push(Load { case: 2, kind: LoadKind::Thermal { member: 1, dt: 30.0, dt_y: 0.0, dt_z: 0.0 } });
    let a = analyze(&m).unwrap();
    let want = -m.materials[0].e * 1000.0 * 0.09 * m.materials[0].alpha() * 30.0;
    let n = a.case(2).unwrap().members[0].stations[1].n;
    eprintln!("thermal N {n:.2}, −EAαΔT {want:.2}");
    assert!((n - want).abs() / want.abs() < 1e-3);
}

#[test]
fn spring_support() {
    // a cantilever L 4 propped by a vertical spring k at its tip, P down at the tip: the
    // spring takes P·k / (k + 3EI/L³)
    let mut m = bare();
    let s = m.rect_section(0.3, 0.6);
    mem(&mut m, [0.0, 0.0, 0.0], [4.0, 0.0, 0.0], s);
    m.supports.push(Support::of_kind(1, "fixed").unwrap());
    let k = 5000.0;
    m.supports.push(Support { node: 2, fix: [false; 6], springs: [0.0, 0.0, k, 0.0, 0.0, 0.0], uplift: false });
    m.loads.push(Load { case: 2, kind: LoadKind::Nodal { node: 2, f: [0.0, 0.0, -100.0, 0.0, 0.0, 0.0] } });
    let a = analyze(&m).unwrap();
    let ei = m.materials[0].e * 1000.0 * 0.3 * 0.6f64.powi(3) / 12.0;
    let kb = 3.0 * ei / 64.0;
    let want = 100.0 * k / (k + kb);
    let r = a.case(2).unwrap().reactions.iter().find(|r| r.0 == 2).map(|r| r.1[2]).unwrap_or(f64::NAN);
    eprintln!("spring force {r:.3}, P·k/(k + 3EI/L³) {want:.3}");
    assert!((r - want).abs() / want < 1e-3, "{r} vs {want}");
}

#[test]
fn truss_and_diaphragm() {
    // two columns 3 m (0.3 x 0.3) and 0.4 x 0.4, tops tied by a rigid diaphragm, 100 kN
    // sideways at one top: they sway equally and share it as their stiffnesses, 1 : (4/3)⁴
    let mut m = bare();
    let c1 = m.rect_section(0.3, 0.3);
    let c2 = m.rect_section(0.4, 0.4);
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, 3.0], c1);
    mem(&mut m, [5.0, 0.0, 0.0], [5.0, 0.0, 3.0], c2);
    for p in [[0.0, 0.0, 0.0], [5.0, 0.0, 0.0]] {
        let n = m.find_node(p).unwrap();
        m.supports.push(Support::of_kind(n, "fixed").unwrap());
    }
    let top = m.find_node([0.0, 0.0, 3.0]).unwrap();
    m.loads.push(Load { case: 2, kind: LoadKind::Nodal { node: top, f: [100.0, 0.0, 0.0, 0.0, 0.0, 0.0] } });
    m.diaphragms = vec![3.0];
    let a = analyze(&m).unwrap();
    let r = a.case(2).unwrap();
    let (u1, u2) = (r.displacements[&top][0], r.displacements[&m.find_node([5.0, 0.0, 3.0]).unwrap()][0]);
    let share = 1.0 / (1.0 + (4.0f64 / 3.0).powi(4));
    let v1 = r.members[0].stations[0].vy.abs().max(r.members[0].stations[0].vz.abs());
    eprintln!("tops {u1:.6} / {u2:.6} m; column 1 shear {v1:.2} kN, share {:.2}", 100.0 * share);
    assert!((u1 - u2).abs() < 1e-9 * u1.abs().max(1.0));
    assert!((v1 - 100.0 * share).abs() < 0.5);
    // a truss member carries no moment
    let mut t = bare();
    let s = t.rect_section(0.2, 0.2);
    let id = mem(&mut t, [0.0, 0.0, 0.0], [4.0, 0.0, 3.0], s);
    t.members[0].truss = true;
    mem(&mut t, [4.0, 0.0, 3.0], [8.0, 0.0, 0.0], s);
    t.members[1].truss = true;
    for p in [[0.0, 0.0, 0.0], [8.0, 0.0, 0.0]] {
        let n = t.find_node(p).unwrap();
        t.supports.push(Support::of_kind(n, "pinned").unwrap());
    }
    let apex = t.find_node([4.0, 0.0, 3.0]).unwrap();
    t.supports.push(Support { node: apex, fix: [false, true, false, false, false, false], springs: [0.0; 6], uplift: false });
    t.loads.push(Load { case: 2, kind: LoadKind::Nodal { node: apex, f: [0.0, 0.0, -60.0, 0.0, 0.0, 0.0] } });
    let a = analyze(&t).unwrap();
    let mr = a.case(2).unwrap().member(id).unwrap();
    // two-bar truss: each bar −P/(2 sin θ), sin θ = 3/5 → −50 kN
    eprintln!("truss N {:.3} kN, max |M| {:.2e}", mr.stations[1].n, mr.stations.iter().map(|s| s.my.abs()).fold(0.0, f64::max));
    assert!((mr.stations[1].n + 50.0).abs() < 1e-6 && mr.stations.iter().all(|s| s.my.abs() < 1e-9));
}
