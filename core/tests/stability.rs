//! P-Delta and buckling against the Euler column.

use pondus_core::analysis::analyze;
use pondus_core::model::*;

/// A 6 m cantilever column, 0.3 x 0.3 concrete, P down and H sideways at its top (live
/// case), no self-weight; one ULS combination of the live case alone.
fn column(p: f64, h: f64) -> Model {
    let mut m = Model::new("euler");
    m.cases[0].self_weight = false;
    m.combos = vec![Combination { id: 1, name: "ULS L".into(), kind: ComboKind::Uls, factors: vec![(2, 1.0)] }];
    let a = m.node_at([0.0, 0.0, 0.0]);
    let b = m.node_at([0.0, 0.0, 6.0]);
    let s = m.rect_section(0.3, 0.3);
    m.members.push(Member { id: 1, i: a, j: b, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, group: None });
    m.supports.push(Support::of_kind(a, "fixed").unwrap());
    m.loads.push(Load { case: 2, kind: LoadKind::Nodal { node: b, f: [h, 0.0, -p, 0.0, 0.0, 0.0] } });
    m
}

fn pcr(m: &Model) -> f64 {
    let ei = m.materials[0].e * 1000.0 * 0.3f64.powi(4) / 12.0;
    std::f64::consts::PI.powi(2) * ei / (4.0 * 36.0)
}

#[test]
fn buckling_factor_is_euler() {
    let m = column(100.0, 0.0);
    let a = analyze(&m).unwrap();
    let modes = pondus_core::stability::buckling(&m, &a, &m.combos[0], 3).unwrap();
    let want = pcr(&m) / 100.0;
    eprintln!("λ1 {:.4}, Euler Pcr/P {want:.4}", modes[0].factor);
    assert!((modes[0].factor - want).abs() / want < 0.01, "{} vs {want}", modes[0].factor);
}

#[test]
fn pdelta_amplifies_the_sway() {
    let mut m = column(0.0, 1.0);
    let p = 0.4 * pcr(&m);
    m.loads[0] = Load { case: 2, kind: LoadKind::Nodal { node: 2, f: [1.0, 0.0, -p, 0.0, 0.0, 0.0] } };
    let lin = analyze(&m).unwrap();
    let u1 = lin.case(2).unwrap().displacements[&2][0];
    m.design.second_order = true;
    let a = analyze(&m).unwrap();
    let (cr, it, b2, ok) = a.second.get(&1).expect("second order result");
    let u2 = cr.displacements[&2][0];
    // the cantilever's sway amplification ≈ 1 / (1 − P/Pcr) (Timoshenko & Gere 1.11: exact
    // 3(tan u − u)/u³ with u = (π/2)√(P/Pcr), here 1.6576 against 1/(1 − 0.4) = 1.667)
    let u = std::f64::consts::FRAC_PI_2 * 0.4f64.sqrt();
    let exact = 3.0 * (u.tan() - u) / u.powi(3);
    eprintln!("amplification {:.4} ({it} iterations, B2 {b2:.3}, ok {ok}), exact {exact:.4}", u2 / u1);
    assert!(*ok && (u2 / u1 - exact).abs() / exact < 0.02, "{} vs {exact}", u2 / u1);
    // and its base moment is larger than the first-order one
    let m1 = lin.combine("L", &[(2, 1.0)]).members[0].stations[0].my.abs().max(lin.combine("L", &[(2, 1.0)]).members[0].stations[0].mz.abs());
    let m2 = cr.members[0].stations[0].my.abs().max(cr.members[0].stations[0].mz.abs());
    assert!(m2 > m1 * 1.3, "{m2} vs {m1}");
}
