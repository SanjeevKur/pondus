//! Pondus's design routines against the Stabileo engine's own, independently written checks
//! (ACI 318-19: engine postprocess::rc_check; AISC 360-16: postprocess::steel_check).

use dedaliano_engine::postprocess::rc_check::*;
use dedaliano_engine::postprocess::steel_check::*;
use pondus_core::sections::{find, h_props};
use pondus_core::steel::{capacities, check, HDims, SteelDemand};

#[test]
fn rc_flexure_and_shear_match_engine() {
    // 300 x 600, d 540, f'c 28, fy 420, 4-DB25 (1 963 mm²), stirrups 2-DB10 @ 200 (fyt 420)
    let as_ = 4.0 * std::f64::consts::PI * 25.0f64.powi(2) / 4.0;
    let av = 2.0 * std::f64::consts::PI * 10.0f64.powi(2) / 4.0;
    let ours = pondus_core::rc::phi_mn(as_, 300.0, 540.0, 28.0, 420.0) / 1e6;
    let ours_v = 0.75 * (0.17 * 28f64.sqrt() * 300.0 * 540.0 + av * 420.0 * 540.0 / 200.0) / 1e3;
    let mem = RCMemberData { element_id: 1, fc: 28e6, fy: 420e6, es: None, b: 0.3, h: 0.6, d: 0.54, d_prime: None, as_tension: as_ * 1e-6, as_compression: None, section_type: RCSectionType::Rectangular, bf: None, hf: None, av: Some(av * 1e-6), s_stirrup: Some(0.2), lambda: None };
    let r = check_rc_members(&RCCheckInput { members: vec![mem], forces: vec![RCDesignForces { element_id: 1, mu: 300e3, vu: Some(200e3), nu: None }] });
    let eng = r[0].phi_mn / 1e3;
    let eng_v = r[0].phi_vn / 1e3;
    eprintln!("φMn ours {ours:.2} engine {eng:.2} kN·m; φVn ours {ours_v:.2} engine {eng_v:.2} kN");
    assert!((ours - eng).abs() / eng < 0.005, "φMn {ours} vs {eng}");
    assert!((ours_v - eng_v).abs() / eng_v < 0.01, "φVn {ours_v} vs {eng_v}");
}

/// AISC G2.1(a): φv = 1.0 for rolled I-shape webs with h/tw ≤ 2.24 √(E/Fy); the engine
/// takes 0.9 for every web, so the nominal strengths are compared.
fn phi_v(s: &pondus_core::sections::HShape) -> f64 {
    if (s.d - 2.0 * s.tf - 2.0 * s.r) / s.tw <= 2.24 * (200_000.0f64 / 245.0).sqrt() { 1.0 } else { 0.9 }
}

#[test]
fn steel_capacities_match_engine() {
    for (name, l) in [("H 300x150x6.5x9", 6.0), ("H 400x200x8x13", 4.0), ("H 200x200x8x12", 3.5), ("H 600x200x11x17", 9.0)] {
        let s = find(name).unwrap();
        let p = h_props(s.d / 1e3, s.bf / 1e3, s.tw / 1e3, s.tf / 1e3, s.r / 1e3);
        let (fy, fu) = (245.0, 400.0);
        let c = capacities(&p, HDims { d: s.d, bf: s.bf, tw: s.tw, tf: s.tf, r: s.r }, fy, fu, l, l, 1.0);
        let hw = (s.d - 2.0 * s.tf - 2.0 * s.r) / 1e3;
        let mem = SteelMemberData {
            element_id: 1, fy: fy * 1e6, fu: Some(fu * 1e6), ag: p.a, an: None, u_factor: None, aw: Some(s.d * s.tw / 1e6), cv1: None,
            lby: l, lbz: l, ky: Some(1.0), kz: Some(1.0), iy: p.iy, iz: p.iz, ry: p.ry, rz: p.rz, zy: p.zy, zz: p.zz, sy: p.sy, sz: p.sz, j: p.j, cw: Some(p.cw),
            lb: Some(l), cb: Some(1.0), e: 200e9, g: Some(77e9), depth: Some(s.d / 1e3), flange_slenderness: Some(s.bf / (2.0 * s.tf)),
            compression_elements: Some(vec![
                AiscElement { kind: AiscElementKind::RolledFlange, b: s.bf / 2e3, t: s.tf / 1e3, count: Some(4.0), kc: None },
                AiscElement { kind: AiscElementKind::Web, b: hw, t: s.tw / 1e3, count: Some(1.0), kc: None },
            ]),
            // Pondus applies the top-flange factor (Cb / 1.4) in `check`, not in `capacities`
            load_position: Some(AiscLoadPosition::ShearCentre),
        };
        let r = check_steel_members(&SteelCheckInput { members: vec![mem], forces: vec![ElementDesignForces { element_id: 1, n: -100e3, my: 50e3, mz: None, vy: Some(50e3) }] });
        let e = &r[0];
        let pairs = [("φPn,c", c.phi_pc / 1e3, e.phi_pn_compression / 1e3), ("φPn,t", c.phi_pt / 1e3, e.phi_pn_tension / 1e3), ("φMn,x", c.phi_mx1 / 1e6, e.phi_mn_y / 1e3), ("φMn,y", c.phi_mz / 1e6, e.phi_mn_z / 1e3), ("Vn", c.phi_vz / 1e3 / phi_v(&s), e.phi_vn / 1e3 / 0.9)];
        for (what, ours, eng) in pairs {
            eprintln!("{name} L {l}: {what} ours {ours:.1} engine {eng:.1}");
            assert!((ours - eng).abs() / eng.abs().max(1e-9) < 0.02, "{name} {what}: {ours} vs {eng}");
        }
    }
}

#[test]
fn top_flange_load_matches_engine_default() {
    // an unbraced 6 m H 300x150 under a uniform moment (Cb = 1), load on the top flange
    let s = find("H 300x150x6.5x9").unwrap();
    let p = h_props(s.d / 1e3, s.bf / 1e3, s.tw / 1e3, s.tf / 1e3, s.r / 1e3);
    let st: Vec<(f64, f64, f64, f64, f64, f64)> = (0..=8).map(|k| (k as f64 * 0.75, 0.0, 0.0, 0.0, 30.0, 0.0)).collect();
    let dem = SteelDemand { length: 6.0, lb: 6.0, k: 1.0, uls: vec![("M".into(), st)], top_flange_load: true, ..Default::default() };
    let d = check(1, None, &s.name, 36.7, &p, HDims { d: s.d, bf: s.bf, tw: s.tw, tf: s.tf, r: s.r }, 245.0, 400.0, &dem, &Default::default());
    let r = d.checks.iter().find(|c| c.name.starts_with("flexure major")).unwrap().ratio;
    // engine default (top flange): φMn = 39.4 kN·m
    assert!((r - 30.0 / 39.4).abs() < 0.01, "{r} {:?}", d.checks);
}

#[test]
fn ec3_column_buckling_by_hand() {
    // H 200x200x8x12, fy 275, pinned 4 m, NEd 300 kN: χ about the minor axis by EN 1993-1-1
    // 6.3.1.2 by hand, curve c (h/b ≤ 1.2), against Pondus's EC3 check through the engine
    use pondus_core::steel::SteelDemand;
    let s = find("H 200x200x8x12").unwrap();
    let p = h_props(s.d / 1e3, s.bf / 1e3, s.tw / 1e3, s.tf / 1e3, s.r / 1e3);
    let hd = HDims { d: s.d, bf: s.bf, tw: s.tw, tf: s.tf, r: s.r };
    let st: Vec<(f64, f64, f64, f64, f64, f64)> = (0..=2).map(|k| (k as f64 * 2.0, -300.0, 0.0, 0.0, 0.0, 0.0)).collect();
    let dem = SteelDemand { length: 4.0, lb: 4.0, k: 1.0, uls: vec![("N".into(), st)], ..Default::default() };
    let d = pondus_core::codes::check_ec3(1, None, &s.name, 0.0, &p, hd, 275.0, 430.0, &dem, &Default::default());
    let ncr = std::f64::consts::PI.powi(2) * 210e9 * p.iz / 16.0;
    let lam = (p.a * 275e6 / ncr).sqrt();
    let phi = 0.5 * (1.0 + 0.49 * (lam - 0.2) + lam * lam);
    let chi = 1.0 / (phi + (phi * phi - lam * lam).sqrt());
    let want = 300e3 / (chi * p.a * 275e6);
    let got = d.checks.iter().find(|c| c.name.starts_with("axial")).unwrap().ratio;
    eprintln!("EC3 axial ratio {got:.4}, by hand {want:.4} (λ̄ {lam:.3}, χ {chi:.3})");
    assert!((got - want).abs() / want < 0.005, "{got} vs {want}");
}
