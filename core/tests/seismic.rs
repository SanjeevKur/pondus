//! Modal and response spectrum analysis against closed-form results.

use pondus_core::analysis::analyze;
use pondus_core::model::*;
use pondus_core::seismic::Spectrum;

fn column(h: f64) -> Model {
    let mut m = Model::new("cantilever");
    let a = m.node_at([0.0, 0.0, 0.0]);
    let b = m.node_at([0.0, 0.0, h]);
    let s = m.rect_section(0.4, 0.4);
    m.members.push(Member { id: 1, i: a, j: b, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, group: None });
    m.supports.push(Support::of_kind(a, "fixed").unwrap());
    m.seismic.enabled = true;
    m
}

#[test]
fn cantilever_first_frequency() {
    let m = column(4.0);
    let a = analyze(&m).unwrap();
    let s = a.seismic.as_ref().unwrap();
    // f1 = 1.8751² / (2π) √(EI / (m L⁴)), m from 24 kN/m³ as mass (x 1000 / g)
    let ei = m.materials[0].e * 1e6 * 0.4f64.powi(4) / 12.0;
    let mu = 24.0 * 0.16 * 1000.0 / 9.81;
    let f1 = 1.8751f64.powi(2) / (2.0 * std::f64::consts::PI) * (ei / (mu * 4f64.powi(4))).sqrt();
    let got = s.modes[0].frequency;
    eprintln!("f1 {got:.3} Hz, theory {f1:.3}; weight {:.2} kN (expected {:.2})", s.weight, 24.0 * 0.16 * 4.0);
    assert!((got - f1).abs() / f1 < 0.01, "{got} vs {f1}");
    assert!((s.weight - 24.0 * 0.16 * 4.0).abs() < 1e-6);
    // the two bending modes are equal (square section)
    assert!((s.modes[1].frequency - got).abs() / got < 1e-3);
}

#[test]
fn response_spectrum_base_shear() {
    // a flat spectrum: each mode's base shear is Meff·Sa·Ie/R, and the CQC sum lies between
    // the first mode's and the whole mass's
    let mut m = column(4.0);
    m.seismic.spectrum = Spectrum::Table { points: vec![(0.0, 0.4), (10.0, 0.4)] };
    m.seismic.r = 4.0;
    m.seismic.elf_ratio = 0.0;
    let a = analyze(&m).unwrap();
    let s = a.seismic.as_ref().unwrap();
    let ex = &s.dirs[0];
    let w = s.weight;
    let v1 = s.modes.iter().map(|md| md.mass_ratio[0]).fold(0.0, f64::max) * w * 0.4 / 4.0;
    eprintln!("V_rsa {:.3} kN, first mode {v1:.3}, all mass {:.3}; mass reached {:?}", ex.v_rsa, w * 0.1, s.mass_reached);
    assert!(ex.v_rsa >= v1 * 0.999 && ex.v_rsa <= w * 0.1 * 1.001);
    // the base shear the column carries under Ex is the response's base shear
    let base = ex.case.members[0].stations[0].vy.max(ex.case.members[0].stations[0].vz);
    assert!((base - ex.v_rsa).abs() / ex.v_rsa < 0.05, "column base shear {base} vs {}", ex.v_rsa);
}
