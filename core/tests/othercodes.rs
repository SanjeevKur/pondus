//! Timber, masonry and cold-formed members against hand calculations.

use pondus_core::analysis::analyze;
use pondus_core::design::{design_pass, Options};
use pondus_core::model::*;

fn beam(mat: Material, section: &str, w: f64) -> Model {
    let mut m = Model::new("t");
    m.cases[0].self_weight = false;
    m.materials.push(mat.clone());
    let a = m.node_at([0.0, 0.0, 3.0]);
    let b = m.node_at([4.0, 0.0, 3.0]);
    let s = m.section_named(section).unwrap();
    m.members.push(Member { id: 1, i: a, j: b, section: s, material: mat.id, kind: MemberKind::Beam, hinge_i: false, hinge_j: false, lb: Some(0.0), k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    let mut s1 = Support::of_kind(a, "pinned").unwrap();
    s1.fix[3] = true;
    let mut s2 = Support::of_kind(b, "roller").unwrap();
    s2.fix[1] = true;
    s2.fix[3] = true;
    m.supports.push(s1);
    m.supports.push(s2);
    m.loads.push(Load { case: 1, kind: LoadKind::Line { member: 1, wi: [0.0, 0.0, -w], wj: [0.0, 0.0, -w], a: None, b: None } });
    m
}

#[test]
fn timber_bending_by_hand() {
    // 100 x 250 DF-L No.2, 4 m, 2 kN/m service (the SLS D + L): M = 4 kN·m, fb = M/S = 3.84 MPa
    // against Fb' = Fb = 6.21 MPa (braced, all factors 1): 0.618
    let m = beam(Material::timber(3), "100 x 250", 2.0);
    let a = analyze(&m).unwrap();
    let r = design_pass(&m, &a, &Options::default());
    let o = &r.others[0];
    let b = o.checks.iter().find(|c| c.name.starts_with("bending (")).unwrap().ratio;
    let want = 4.0e3 / (0.1 * 0.25f64.powi(2) / 6.0) / 6.21e6;
    eprintln!("timber bending {b:.4}, by hand {want:.4}; {}", o.code);
    assert!((b - want).abs() / want < 0.01, "{b} vs {want}");
}

#[test]
fn cold_formed_effective_area() {
    // C 200x75x20x2.0, Fy 345: web ρ = (1 − 0.22/λ)/λ with λ = 1.052/2 x (196/2) x √(345/203000)
    let (h, b, d, t) = (0.2, 0.075, 0.020, 0.002);
    let lp = pondus_core::othermat::lipped_props(h, b, d, t, 345.0).unwrap();
    let rho = |w: f64, k: f64| {
        let l: f64 = 1.052 / k.sqrt() * (w / 2.0) * (345.0f64 / 203000.0).sqrt();
        if l <= 0.673 { 1.0 } else { (1.0 - 0.22 / l) / l }
    };
    let lost = (196.0 * (1.0 - rho(196.0, 4.0)) + 2.0 * 71.0 * (1.0 - rho(71.0, 4.0)) + 2.0 * 19.0 * (1.0 - rho(19.0, 0.43))) * 2.0;
    let want = lp.fe.a * 1e6 - lost;
    eprintln!("A {:.1} mm², Ae {:.1}, by hand {want:.1}; J {:.1} mm⁴ (thin {:.1}); Cw {:.3e} mm⁶", lp.fe.a * 1e6, lp.ae * 1e6, lp.fe.j * 1e12, (200.0 + 150.0 + 40.0) * 8.0 / 3.0, lp.fe.cw * 1e18);
    assert!((lp.ae * 1e6 - want).abs() < 1.0);
    // a C 200 beam designs without error
    let mut st = Material::steel(3, "G450", 345.0, 450.0);
    st.name = "cold-formed".into();
    let m = beam(st, "C 200x75x20x2.0", 1.0);
    let a = analyze(&m).unwrap();
    let r = design_pass(&m, &a, &Options::default());
    eprintln!("CFS beam: {} ratio {:.2} governs {} {:?}", r.others[0].code, r.others[0].ratio, r.others[0].governs, r.others[0].notes);
    assert!(r.others[0].ratio > 0.0 && r.others[0].ratio.is_finite());
}

#[test]
fn masonry_wall_strip() {
    // a 1 m strip of a 200 mm masonry wall, 3 m tall, 100 kN/m axial with DB12 @ 400 mm
    let mut m = Model::new("wall");
    m.cases[0].self_weight = false;
    m.materials.push(Material::masonry(3, 13.8));
    let a = m.node_at([0.0, 0.0, 0.0]);
    let b = m.node_at([0.0, 0.0, 3.0]);
    let s = m.section_named("1000 x 200").unwrap();
    m.members.push(Member { id: 1, i: a, j: b, section: s, material: 3, kind: MemberKind::Column, hinge_i: false, hinge_j: true, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: Some(2.5 * 113.1), bim_id: None, tension: None, role: Default::default(), group: None });
    m.supports.push(Support::of_kind(a, "fixed").unwrap());
    let mut top = Support::of_kind(b, "roller").unwrap();
    top.fix = [true, true, false, true, false, true];
    m.supports.push(top);
    m.loads.push(Load { case: 1, kind: LoadKind::Nodal { node: b, f: [0.0, 0.0, -100.0, 0.0, 0.0, 0.0] } });
    let an = analyze(&m).unwrap();
    let r = design_pass(&m, &an, &Options::default());
    let o = &r.others[0];
    // TMS 402 9.3.4.1.1: φPn = 0.9 x 0.80 [0.80 f'm (An − As) + fy As] [1 − (h/140r)²], h/r = 3/0.0577
    let an_ = 0.2;
    let as_ = 2.5 * 113.1e-6;
    let r_ = 0.2 / 12f64.sqrt();
    let pn = 0.80 * (0.80 * 13.8e6 * (an_ - as_) + 390e6 * as_) * (1.0 - (3.0 / (140.0 * r_)).powi(2));
    let ax = o.checks.iter().find(|c| c.name.starts_with("axial (")).unwrap().ratio;
    eprintln!("masonry axial ratio {ax:.4} (Pu 140 kN / 0.9 Pn {:.0} kN by hand = {:.4}); {:?}", pn / 1e3 * 0.9, 140.0 / (0.9 * pn / 1e3), o.notes);
    assert!(ax > 0.0 && ax < 1.0);
}

#[test]
fn shear_tab_by_hand() {
    // Vu 150 kN on H 400x200x8x13: M20 Fnv 372: φRn = 0.75 x 372 x 314.2 = 87.6 kN per bolt.
    // With e = 75 mm the elastic method on n bolts at 75: the worst bolt takes
    // √((V/n)² + (M ymax / Σy²)²); Pondus adds bolts until that is under 87.6 kN
    use pondus_core::connections::{shear_tab, ConnSettings};
    let cs = ConnSettings::default();
    let d = shear_tab(1, "start", 150.0, (0.4, 0.2, 0.008, 0.013, 0.013), 400.0, &cs);
    let n: usize = d.bolts.split('-').next().unwrap().parse().unwrap();
    let worst = |n: usize| {
        let ys: Vec<f64> = (0..n).map(|k| (k as f64 - (n - 1) as f64 / 2.0) * 0.075).collect();
        let s2: f64 = ys.iter().map(|y| y * y).sum();
        let m = 150.0 * 0.075;
        ((150.0 / n as f64).powi(2) + (m * ys.last().unwrap() / s2).powi(2)).sqrt()
    };
    let cap = 0.75 * 372e6 * std::f64::consts::PI * 0.01f64.powi(2) / 1e3;
    eprintln!("{} | {} | {}: bolt ratio {:.3} (by hand {:.3}), bearing {:.3}, plate {:.3}, weld {:.3}", d.bolts, d.plate, d.weld, d.bolt_ratio, worst(n) / cap, d.bearing_ratio, d.plate_ratio, d.weld_ratio);
    assert!(d.ok);
    assert!((d.bolt_ratio - worst(n) / cap).abs() < 0.01);
    assert!(worst(n - 1) / cap > 1.0 || d.bearing_ratio > 0.9 || n == 2);
}
