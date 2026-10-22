//! Validation against a real building: the full-scale 7-storey reinforced concrete wall building
//! slice tested on the UCSD-NEES shake table (October 2005 to January 2006), whose dynamic
//! properties were measured by system identification and computed by the designers and by
//! blind-prediction modellers. Unlike the calibration suites, these are comparisons with
//! measurements and with other models, so they are recorded, not asserted to a tolerance.
//!
//! Sources:
//! - Panagiotou, Restrepo, Conte & Englekirk (2010), NZSEE Conf. Paper 03: geometry (web wall
//!   3.66 m long, 0.20 m thick at levels 1 and 7, 0.15 m elsewhere, storeys 2.74 m), f'c 27.6 MPa
//!   specified, design C_s = 0.15 for V_b = 292 kN (so W = 1947 kN), measured T1 = 0.51 s from the
//!   first 0.03 g RMS white-noise test, T2 close to 0.10 s, and 0.50 s from the designers'
//!   uncracked three-dimensional model; T1 = 0.65, 0.82, 0.88 and 1.16 s after tests EQ1 to EQ4.
//! - Kelly (2007), Bull. NZSEE 40(3): 7 @ 2.743 m = 19.202 m, web wall 3.658 m, measured average
//!   f'c 41.4 MPa, slabs 203 mm at levels 1 and 7 and 152 mm between, and the periods of his
//!   ANSR-II model (0.557 s, 66.3 %; 0.094 s, 17.5 %).
//! - Moaveni, He, Conte, Restrepo & Panagiotou (2011), J. Struct. Eng. 137(6): ambient
//!   vibration 2.23 Hz before any testing (uncracked, peak picking), and at state S0 1.89 Hz,
//!   10.53 Hz and 24.58 Hz (SSI, Table 2).
//!
//! Rows go to <PONDUS_VALID>/ucsd7.csv.

use pondus_core::analysis::analyze;
use pondus_core::commands::run;
use pondus_core::model::*;
use serde_json::json;
use std::io::Write;

const G: f64 = 9.81;
const H: f64 = 2.743;
const LW: f64 = 3.658;
/// The seismic weight above the base, V_b / C_s = 292 / 0.15 kN (Panagiotou et al. 2010).
const W: f64 = 292.0 / 0.15;

fn record(case: &str, source: &str, quantity: &str, unit: &str, reference: f64, pondus: f64, kind: &str) {
    let err = (pondus - reference) / reference * 100.0;
    eprintln!("[ucsd7] {case} | {quantity}: Pondus {pondus:.4} vs {reference:.4} {unit} ({err:+.1} %) [{kind}; {source}]");
    if let Some(dir) = std::env::var_os("PONDUS_VALID") {
        let p = std::path::Path::new(&dir).join("ucsd7.csv");
        let new = !p.exists();
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p).unwrap();
        if new {
            writeln!(f, "case,source,quantity,unit,reference,pondus,error_pct,kind").unwrap();
        }
        let q = |s: &str| format!("\"{}\"", s.replace('"', "'"));
        writeln!(f, "{},{},{},{},{},{},{:.4},{}", q(case), q(source), q(quantity), q(unit), reference, pondus, err, kind).unwrap();
    }
}

/// The web wall on a fixed base with the seismic weight W: the wall's own weight from its
/// concrete (24 kN/m³) and the rest at the floors in proportion to their slab thickness, on a
/// pair of weightless rigid stubs at every level (symmetric about the wall, so the in-plane
/// modes stay planar). `fc` sets E = 4700 √f'c and `ie` scales the wall's stiffness (ACI
/// 318-19 6.6.3.1.1: 0.70 for uncracked walls).
fn slice(fc: f64, ie: f64) -> (Model, f64) {
    let mut m = Model::new("UCSD 7-storey building slice");
    m.materials[0] = Material::concrete(1, fc);
    m.materials[0].e *= ie;
    let levels: Vec<f64> = (0..=7).map(|k| k as f64 * H).collect();
    // the lowest segment is fixed to the base; the two above rest on it (base "none"), or each
    // would be clamped at its own base
    run(&mut m, "add_walls", &json!({"walls": [{"from": [0.0, 0.0], "to": [LW, 0.0], "t": 0.203, "z0": 0.0, "z1": H, "name": "web L1"}], "levels": levels})).unwrap();
    run(&mut m, "add_walls", &json!({"walls": [
        {"from": [0.0, 0.0], "to": [LW, 0.0], "t": 0.152, "z0": H, "z1": 6.0 * H, "name": "web L2-L6"},
        {"from": [0.0, 0.0], "to": [LW, 0.0], "t": 0.203, "z0": 6.0 * H, "z1": 7.0 * H, "name": "web L7"}
    ], "levels": levels, "base": "none"})).unwrap();
    assert_eq!(m.supports.len(), 1, "only the base of the wall is supported");
    let gamma = m.materials[0].gamma;
    let wall_w = LW * H * (2.0 * 0.203 + 5.0 * 0.152) * gamma;
    let slab = [0.203, 0.152, 0.152, 0.152, 0.152, 0.152, 0.203];
    let total_t: f64 = slab.iter().sum();
    // the stubs: weightless and rigid, 0.5 m either side of the wall's centre
    let id = m.next_material_id();
    m.materials.push(Material { id, name: "weightless rigid".into(), kind: MaterialKind::Other, e: 2.0e6, nu: 0.2, gamma: 0.0, fc: 0.0, fy: 0.0, fu: 0.0, fb: 0.0, ft: 0.0, fv: 0.0, e_min: 0.0, alpha: 0.0 });
    let sid = m.next_section_id();
    m.sections.push(Section { id: sid, name: "stub".into(), shape: Shape::General { a: 0.05, iy: 0.01, iz: 0.01, j: 0.01, shear: None } });
    let c = LW / 2.0;
    for (k, t) in slab.iter().enumerate() {
        let z = (k + 1) as f64 * H;
        let wi = (W - wall_w) * t / total_t;
        for y in [-0.5, 0.5] {
            let (i, j) = (m.node_at([c, 0.0, z]), m.node_at([c, y, z]));
            let mid = m.next_member_id();
            m.members.push(Member { id: mid, i, j, section: sid, material: id, kind: MemberKind::Beam, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
            m.loads.push(Load { case: 1, kind: LoadKind::Line { member: mid, wi: [0.0, 0.0, -wi], wj: [0.0, 0.0, -wi], a: None, b: None } });
        }
    }
    m.cases[0].self_weight = true;
    m.seismic.enabled = true;
    m.seismic.live_mass = 0.0;
    m.seismic.mass_target = 0.99;
    m.seismic.max_modes = 40;
    m.seismic.r = 1.0;
    m.seismic.ie = 1.0;
    m.seismic.elf_ratio = 0.0;
    (m, wall_w)
}

/// Pondus's own picture of the model (scene + software renderer), as a PNG.
fn save_png(path: &std::path::Path, m: &Model, a: &pondus_core::analysis::Analysis, spec: &pondus_core::scene::Spec, w: usize, h: usize) {
    let prims = pondus_core::scene::build(m, Some(a), None, spec);
    let img = pondus_core::picture::render(&prims, &pondus_core::scene::Camera::for_view(spec), w, h);
    let f = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&img.rgba).unwrap();
}

/// For the paper's figure: Pondus's picture of the model and the first two in-plane mode shapes
/// along the wall's centre line (z, ux of mode 1, ux of mode 2) in <PONDUS_VALID>.
fn figure_data(m: &Model) {
    let Some(dir) = std::env::var_os("PONDUS_VALID") else { return };
    let dir = std::path::Path::new(&dir);
    let a = analyze(m).unwrap();
    let spec = pondus_core::scene::Spec { view: "3d".into(), yaw: Some(-55.0), pitch: Some(18.0), ..Default::default() };
    save_png(&dir.join("ucsd7_model.png"), m, &a, &spec, 900, 1100);
    let s = a.seismic.as_ref().unwrap();
    let mut xs: Vec<usize> = (0..s.modes.len()).filter(|&k| s.modes[k].mass_ratio[0] > 0.01).collect();
    xs.sort_by(|&p, &q| s.modes[q].period.total_cmp(&s.modes[p].period));
    let c = LW / 2.0;
    let mut nodes: Vec<&Node> = m.nodes.iter().filter(|n| (n.x - c).abs() < 1e-6 && n.y.abs() < 1e-6).collect();
    nodes.sort_by(|p, q| p.z.total_cmp(&q.z));
    let mut f = std::fs::File::create(dir.join("ucsd7_modes.csv")).unwrap();
    writeln!(f, "z,mode1_ux,mode2_ux,T1,T2").unwrap();
    for n in nodes {
        let u = |k: usize| s.modes[xs[k]].shape.get(&n.id).map(|v| v[0]).unwrap_or(0.0);
        writeln!(f, "{},{},{},{},{}", n.z, u(0), u(1), s.modes[xs[0]].period, s.modes[xs[1]].period).unwrap();
    }
}

/// The in-plane (x) modes: (period s, frequency Hz, effective mass ratio).
fn x_modes(m: &Model) -> (Vec<(f64, f64, f64)>, f64) {
    let a = analyze(m).unwrap();
    let s = a.seismic.as_ref().unwrap();
    let mut v: Vec<(f64, f64, f64)> = s.modes.iter().filter(|md| md.mass_ratio[0] > 0.01).map(|md| (md.period, md.frequency, md.mass_ratio[0])).collect();
    v.sort_by(|p, q| q.0.total_cmp(&p.0));
    (v, s.weight)
}

#[test]
fn ucsd_seven_storey_slice() {
    let p2010 = "Panagiotou et al. (2010), NZSEE Paper 03, Sec. 4.2.1";
    let m2011 = "Moaveni et al. (2011), JSE 137(6), Table 2 and text";
    let k2007 = "Kelly (2007), Bull. NZSEE 40(3), Table 2";

    // the base model: gross section, E from the measured strength (Kelly: f'c 41.4 MPa)
    let (m, wall_w) = slice(41.4, 1.0);
    figure_data(&m);
    let (x, w) = x_modes(&m);
    eprintln!("seismic weight {w:.1} kN (target {W:.1}, of which web wall {wall_w:.1}); x modes {x:?}");
    let case = "web wall, gross section, E = 4700 sqrt(41.4) MPa";
    record(case, "Panagiotou et al. (2010): V_b / C_s = 292 / 0.15", "seismic weight W", "kN", W, w, "input");
    record(case, m2011, "f1, ambient vibration before testing (uncracked)", "Hz", 2.23, x[0].1, "measured");
    record(case, p2010, "T1, first 0.03 g RMS white-noise test", "s", 0.51, x[0].0, "measured");
    record(case, p2010, "T1, designers' uncracked 3-D model", "s", 0.50, x[0].0, "model");
    record(case, k2007, "T1, blind-prediction model (ANSR-II)", "s", 0.557, x[0].0, "model");
    record(case, m2011, "f2, ambient vibration at state S0", "Hz", 10.53, x[1].1, "measured");
    record(case, p2010, "T2, low-amplitude tests", "s", 0.10, x[1].0, "measured");
    record(case, k2007, "T2, blind-prediction model (ANSR-II)", "s", 0.094, x[1].0, "model");
    record(case, m2011, "f3, ambient vibration at state S0", "Hz", 24.58, x[2].1, "measured");
    record(case, k2007, "effective mass, mode 1", "-", 0.663, x[0].2, "model");
    record(case, k2007, "effective mass, mode 2", "-", 0.175, x[1].2, "model");

    // the stiffness assumptions an engineer would choose between
    for (fc, ie, label) in [(27.6, 1.0, "gross section, E = 4700 sqrt(27.6) MPa (specified f'c)"), (41.4, 0.70, "0.70 Ig (ACI 318-19 6.6.3.1.1, uncracked wall), E from f'c 41.4 MPa"), (41.4, 0.35, "0.35 Ig (ACI 318-19 6.6.3.1.1, cracked wall), E from f'c 41.4 MPa")] {
        let (m, _) = slice(fc, ie);
        let (x, _) = x_modes(&m);
        let case = format!("web wall, {label}");
        record(&case, p2010, "T1, first 0.03 g RMS white-noise test", "s", 0.51, x[0].0, "measured");
        record(&case, m2011, "f1, ambient vibration before testing (uncracked)", "Hz", 2.23, x[0].1, "measured");
        record(&case, m2011, "f2, ambient vibration at state S0", "Hz", 10.53, x[1].1, "measured");
        // the periods measured after the earthquake tests (0.03 g RMS white noise), against
        // which the code's reduced stiffnesses are judged
        if ie == 0.70 {
            record(&case, p2010, "T1 after test EQ1 (moderate, cracking)", "s", 0.65, x[0].0, "measured");
        }
        if ie == 0.35 {
            record(&case, p2010, "T1 after test EQ2", "s", 0.82, x[0].0, "measured");
            record(&case, p2010, "T1 after test EQ3", "s", 0.88, x[0].0, "measured");
            record(&case, p2010, "T1 after test EQ4 (Sylmar, near-fault)", "s", 1.16, x[0].0, "measured");
        }
    }
}
