//! Time history against an independent single-mode solution, and the record reader.

use pondus_core::analysis::analyze;
use pondus_core::model::*;
use pondus_core::timehistory::{parse_record, run, Settings};

#[test]
fn reads_peer_at2() {
    let text = "PEER NGA STRONG MOTION DATABASE RECORD\nTEST 1999, X\nACCELERATION TIME SERIES IN UNITS OF G\nNPTS=    6, DT=   .0100 SEC\n  .1 .2 -.3\n .4 0 0\n";
    let r = parse_record("t", text, None).unwrap();
    assert_eq!(r.accel, vec![0.1, 0.2, -0.3, 0.4, 0.0, 0.0]);
    assert!((r.dt - 0.01).abs() < 1e-12 && (r.pga() - 0.4).abs() < 1e-12);
    let r2 = parse_record("t", "0 0\n0.02 0.1\n0.04 -0.1\n", None).unwrap();
    assert!((r2.dt - 0.02).abs() < 1e-12 && r2.accel.len() == 3);
}

#[test]
fn cantilever_under_a_sine() {
    // a 4 m column (f1 = 12.39 Hz) shaken at 4 Hz, 0.1 g, 5 % damping: almost all first mode,
    // so the tip's peak = Γ1 φ1,tip x q_max with Γ1 φ1,tip = 1.566 (uniform cantilever) and q
    // the first mode's own response, integrated here by Newmark independently of the engine
    let mut m = Model::new("column");
    let a = m.node_at([0.0, 0.0, 0.0]);
    let b = m.node_at([0.0, 0.0, 4.0]);
    let s = m.rect_section(0.4, 0.4);
    m.members.push(Member { id: 1, i: a, j: b, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    m.supports.push(Support::of_kind(a, "fixed").unwrap());
    // a level at the top so the roof history is the tip's
    let an = analyze(&m).unwrap();
    let dt = 0.002;
    let n = 1500;
    let f = 4.0;
    let text: String = (0..=n).map(|k| format!("{} {}\n", k as f64 * dt, 0.1 * (2.0 * std::f64::consts::PI * f * k as f64 * dt).sin())).collect();
    let rec = parse_record("sine", &text, None).unwrap();
    let st = Settings { direction: 0, scale: 1.0, dt, duration: Some(n as f64 * dt), damping: 0.05, alpha: None };
    let r = run(&m, &an, &rec, &st).unwrap();
    // the first mode by Newmark (average acceleration), ω1 from beam theory
    let ei = m.materials[0].e * 1e6 * 0.4f64.powi(4) / 12.0;
    let mu = 24.0 * 0.16 * 1000.0 / 9.81;
    let w = 1.8751f64.powi(2) * (ei / (mu * 4f64.powi(4))).sqrt();
    let xi = 0.05;
    let (mut u, mut v, mut acc) = (0.0f64, 0.0f64, 0.0f64);
    let ag = |k: usize| -0.1 * 9.81 * (2.0 * std::f64::consts::PI * f * k as f64 * dt).sin();
    acc = ag(0) - 2.0 * xi * w * v - w * w * u + acc * 0.0;
    let mut qmax = 0.0f64;
    let kh = w * w + 2.0 * xi * w * 2.0 / dt + 4.0 / (dt * dt);
    for k in 1..=n {
        let p = ag(k) + 4.0 / (dt * dt) * u + 4.0 / dt * v + acc + 2.0 * xi * w * (2.0 / dt * u + v);
        let un = p / kh;
        let vn = 2.0 / dt * (un - u) - v;
        let an_ = 4.0 / (dt * dt) * (un - u) - 4.0 / dt * v - acc;
        u = un;
        v = vn;
        acc = an_;
        qmax = qmax.max(u.abs());
    }
    let want = 1.566 * qmax;
    eprintln!("tip peak {:.4} mm at {:.3} s, single mode {:.4} mm; storeys {:?}", r.roof_peak * 1e3, r.roof_peak_at, want * 1e3, r.storeys.iter().map(|s| s.drift).collect::<Vec<_>>());
    assert!((r.roof_peak - want).abs() / want < 0.05, "{} vs {want}", r.roof_peak);
}
