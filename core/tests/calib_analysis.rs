//! Calibration of Pondus's analysis against published problems and closed-form textbook
//! solutions. Every case is built as a Pondus model and solved through Pondus (member
//! splitting, load conversion, sign conventions, combinations, slab transfer, the seismic,
//! time-history, stability and advanced modules), so the wrapper is what is calibrated, not
//! only the engine. Each comparison is one row of calibration/analysis.csv (PONDUS_CALIB).
//! The sources, the problem data and the discussion are in calibration/NOTES_analysis.md.

mod calib;
use calib::{check, Row};

use pondus_core::analysis::{analyze, analyze_with, Analysis, CaseResult};
use pondus_core::model::*;
use std::f64::consts::PI;

const SUITE: &str = "analysis";

#[allow(clippy::too_many_arguments)]
fn row(aspect: &str, case: &str, source: &str, quantity: &str, unit: &str, reference: f64, pondus: f64, tol: f64, kind: &str) {
    check(Row { suite: SUITE, aspect, case, source, quantity, unit, reference, pondus, tol, kind });
}

fn bare(name: &str) -> Model {
    let mut m = Model::new(name);
    m.cases[0].self_weight = false;
    m
}

fn mem(m: &mut Model, a: [f64; 3], b: [f64; 3], s: Id, mat: Id) -> Id {
    let (i, j) = (m.node_at(a), m.node_at(b));
    let id = m.next_member_id();
    m.members.push(Member { id, i, j, section: s, material: mat, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    id
}

fn general(m: &mut Model, a: f64, iy: f64, iz: f64, j: f64) -> Id {
    let id = m.next_section_id();
    m.sections.push(Section { id, name: format!("G{id}"), shape: Shape::General { a, iy, iz, j, shear: None } });
    id
}

/// A support holding the translations in `t` and rotations in `r`.
fn sup(m: &mut Model, p: [f64; 3], fix: [bool; 6]) -> Id {
    let n = m.find_node(p).expect("support at a node");
    m.supports.retain(|s| s.node != n);
    m.supports.push(Support { node: n, fix, springs: [0.0; 6], uplift: false });
    n
}

const PIN: [bool; 6] = [true, true, true, true, false, false];
const ROLLER: [bool; 6] = [false, true, true, true, false, false];
const FIXED: [bool; 6] = [true; 6];

fn line(m: &mut Model, case: Id, member: Id, w: [f64; 3]) {
    m.loads.push(Load { case, kind: LoadKind::Line { member, wi: w, wj: w, a: None, b: None } });
}
fn nodal(m: &mut Model, case: Id, node: Id, f: [f64; 6]) {
    m.loads.push(Load { case, kind: LoadKind::Nodal { node, f } });
}

fn st_at(cr: &CaseResult, member: Id, x: f64) -> pondus_core::analysis::Station {
    let mr = cr.member(member).unwrap();
    mr.stations.iter().min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs())).unwrap().clone()
}

fn reaction(cr: &CaseResult, node: Id) -> [f64; 6] {
    cr.reactions.iter().find(|r| r.0 == node).map(|r| r.1).unwrap_or([0.0; 6])
}

fn steel_e(m: &Model) -> f64 {
    m.materials.iter().find(|x| x.kind == MaterialKind::Steel).unwrap().e
}

// ================================================================ 1. linear statics

const IN: f64 = 0.0254;
const LBF: f64 = 0.004_448_221_6;
const PSI_MPA: f64 = 0.006_894_757;

/// ANSYS Mechanical APDL Verification Manual, VM2 "Beam stresses and deflections": a 30 in
/// wide-flange beam, l = 240 in between supports, overhangs a = 120 in loaded by w =
/// 10 000/12 lb/in, E = 30e6 psi, I = 7892 in⁴, h = 30 in. Targets: σ = 11 400 psi, mid-span
/// deflection 0.182 in (up).
#[test]
fn vm2_overhang_beam() {
    let mut m = bare("VM2");
    let st = 2;
    m.materials[1].e = 30e6 * PSI_MPA;
    let i = 7892.0 * IN.powi(4);
    let s = general(&mut m, 50.65 * IN * IN, i, i, 2.0 * i);
    let xs = [0.0, 120.0, 240.0, 360.0, 480.0];
    let mut ids = vec![];
    for k in 0..4 {
        ids.push(mem(&mut m, [xs[k] * IN, 0.0, 0.0], [xs[k + 1] * IN, 0.0, 0.0], s, st));
    }
    sup(&mut m, [120.0 * IN, 0.0, 0.0], PIN);
    sup(&mut m, [360.0 * IN, 0.0, 0.0], ROLLER);
    let w = 10_000.0 / 12.0 * LBF / IN;
    line(&mut m, 2, ids[0], [0.0, 0.0, -w]);
    line(&mut m, 2, ids[3], [0.0, 0.0, -w]);
    let a = analyze(&m).unwrap();
    let l = a.case(2).unwrap();
    let mmid = st_at(l, ids[1], 120.0 * IN).my / (LBF * IN);
    let sigma = mmid.abs() * 15.0 / 7892.0;
    let mid = m.find_node([240.0 * IN, 0.0, 0.0]).unwrap();
    let d = l.displacements[&mid][2] / IN;
    let src = "ANSYS MAPDL Verification Manual VM2";
    row("linear statics", "VM2 overhanging beam", src, "bending stress", "psi", 11_400.0, sigma, 0.5, "published");
    row("linear statics", "VM2 overhanging beam", src, "mid-span deflection (up)", "in", 0.182, d, 0.5, "published");
}

/// ANSYS VM4 "Deflection of a hinged support": two bars l = 15 ft, A = 0.5 in², θ = 30°,
/// E = 30e6 psi, F = 5000 lb at the apex. Targets: σ = 10 000 psi, δy = −0.12 in.
#[test]
fn vm4_hinged_bars() {
    let mut m = bare("VM4");
    m.materials[1].e = 30e6 * PSI_MPA;
    let s = general(&mut m, 0.5 * IN * IN, 1e-9, 1e-9, 1e-9);
    let (l, th) = (180.0 * IN, 30f64.to_radians());
    let apex = [0.0, 0.0, -l * th.sin()];
    let mut bars = vec![];
    for sx in [-1.0, 1.0] {
        let id = mem(&mut m, [sx * l * th.cos(), 0.0, 0.0], apex, s, 2);
        m.members.iter_mut().find(|x| x.id == id).unwrap().truss = true;
        bars.push(id);
        sup(&mut m, [sx * l * th.cos(), 0.0, 0.0], [true, true, true, false, false, false]);
    }
    let n = sup(&mut m, apex, [false, true, false, false, false, false]);
    nodal(&mut m, 2, n, [0.0, 0.0, -5000.0 * LBF, 0.0, 0.0, 0.0]);
    let a = analyze(&m).unwrap();
    let r = a.case(2).unwrap();
    let nf = r.member(bars[0]).unwrap().stations[0].n / LBF;
    let src = "ANSYS MAPDL Verification Manual VM4";
    row("linear statics", "VM4 hinged bars", src, "bar stress", "psi", 10_000.0, nf / 0.5, 0.5, "published");
    row("linear statics", "VM4 hinged bars", src, "apex deflection", "in", -0.12, r.displacements[&n][2] / IN, 0.5, "published");
}

/// Beams of Roark Table 8.1 and Hibbeler Chs. 10–12 (closed form).
#[test]
fn indeterminate_beams() {
    let (l, w, p) = (6.0, 10.0, 50.0);
    let mk = |sa: [bool; 6], sb: [bool; 6]| {
        let mut m = bare("beam");
        let s = m.rect_section(0.3, 0.6);
        let id = mem(&mut m, [0.0, 0.0, 0.0], [l, 0.0, 0.0], s, 1);
        sup(&mut m, [0.0, 0.0, 0.0], sa);
        sup(&mut m, [l, 0.0, 0.0], sb);
        (m, id)
    };
    // propped cantilever, UDL: R_B = 3wL/8, M_A = −wL²/8
    let (mut m, id) = mk(FIXED, ROLLER);
    line(&mut m, 2, id, [0.0, 0.0, -w]);
    let a = analyze(&m).unwrap();
    let c = a.case(2).unwrap();
    let src = "Hibbeler, Structural Analysis 10e, Ch. 10 (force method); Roark 9e Table 8.1 case 1c";
    row("linear statics", "propped cantilever, UDL", src, "R_B", "kN", 3.0 * w * l / 8.0, reaction(c, 2)[2], 1e-4, "closed-form");
    row("linear statics", "propped cantilever, UDL", src, "M_A", "kN·m", -w * l * l / 8.0, st_at(c, id, 0.0).my, 1e-4, "closed-form");
    // fixed-fixed, central point load: M_end = −PL/8, M_mid = PL/8, δ = PL³/192EI
    let (mut m, id) = mk(FIXED, FIXED);
    m.loads.push(Load { case: 2, kind: LoadKind::Point { member: id, a: l / 2.0, p: [0.0, 0.0, -p] } });
    let a = analyze(&m).unwrap();
    let c = a.case(2).unwrap();
    let ei = m.materials[0].e * 1e3 * 0.3 * 0.6f64.powi(3) / 12.0;
    let src = "Roark 9e Table 8.1 case 3a (fixed ends, centre load)";
    row("linear statics", "fixed-fixed, centre load", src, "M_end", "kN·m", -p * l / 8.0, st_at(c, id, 0.0).my, 1e-4, "closed-form");
    row("linear statics", "fixed-fixed, centre load", src, "M_mid", "kN·m", p * l / 8.0, st_at(c, id, l / 2.0).my, 1e-4, "closed-form");
    let dmid = -c.member(id).unwrap().local_deflection([0.0, 0.0, 1.0]).into_iter().fold(0.0f64, f64::min);
    row("linear statics", "fixed-fixed, centre load", src, "δ_mid", "mm", p * l.powi(3) / (192.0 * ei) * 1e3, dmid * 1e3, 0.1, "closed-form");
    // two equal spans, UDL: M_B = −wL²/8, R_B = 5wL/4 (three-moment equation)
    let mut m = bare("2span");
    let s = m.rect_section(0.3, 0.6);
    let b1 = mem(&mut m, [0.0, 0.0, 0.0], [l, 0.0, 0.0], s, 1);
    let b2 = mem(&mut m, [l, 0.0, 0.0], [2.0 * l, 0.0, 0.0], s, 1);
    sup(&mut m, [0.0, 0.0, 0.0], PIN);
    let nb = sup(&mut m, [l, 0.0, 0.0], ROLLER);
    sup(&mut m, [2.0 * l, 0.0, 0.0], ROLLER);
    line(&mut m, 2, b1, [0.0, 0.0, -w]);
    line(&mut m, 2, b2, [0.0, 0.0, -w]);
    let a = analyze(&m).unwrap();
    let c = a.case(2).unwrap();
    let src = "Ghali & Neville, Structural Analysis 7e, Ch. 4 (three-moment equation)";
    row("linear statics", "two-span continuous beam, UDL", src, "M_B", "kN·m", -w * l * l / 8.0, st_at(c, b1, l).my, 1e-4, "closed-form");
    row("linear statics", "two-span continuous beam, UDL", src, "R_B", "kN", 1.25 * w * l, reaction(c, nb)[2], 1e-4, "closed-form");
    // Gerber beam: fixed A, hinge at B (a = 4 m), roller C (b = 6 m), UDL on both
    let (ga, gb) = (4.0, 6.0);
    let mut m = bare("gerber");
    let s = m.rect_section(0.3, 0.6);
    let m1 = mem(&mut m, [0.0, 0.0, 0.0], [ga, 0.0, 0.0], s, 1);
    let m2 = mem(&mut m, [ga, 0.0, 0.0], [ga + gb, 0.0, 0.0], s, 1);
    m.members.iter_mut().find(|x| x.id == m1).unwrap().hinge_j = true;
    let na = sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    let nc = sup(&mut m, [ga + gb, 0.0, 0.0], ROLLER);
    line(&mut m, 2, m1, [0.0, 0.0, -w]);
    line(&mut m, 2, m2, [0.0, 0.0, -w]);
    let a = analyze(&m).unwrap();
    let c = a.case(2).unwrap();
    let src = "STAAD.Pro Verification V7 type (Gerber beam), statics";
    row("linear statics", "Gerber beam with hinge", src, "R_C", "kN", w * gb / 2.0, reaction(c, nc)[2], 1e-4, "closed-form");
    row("linear statics", "Gerber beam with hinge", src, "M_A", "kN·m", -(w * ga * ga / 2.0 + w * gb / 2.0 * ga), st_at(c, m1, 0.0).my, 1e-4, "closed-form");
    row("linear statics", "Gerber beam with hinge", src, "M at hinge", "kN·m", 0.0, st_at(c, m1, ga).my, 1e-6, "closed-form");
    let _ = na;
}

/// Kassimali, Structural Analysis 6e, Ch. 4: a Pratt truss of four 4 m panels, 4 m deep,
/// 10 kN at the three inner bottom joints; member forces by the method of sections.
#[test]
fn pratt_truss() {
    let mut m = bare("pratt");
    let s = general(&mut m, 2e-3, 1e-9, 1e-9, 1e-9);
    let b = |x: f64| [x, 0.0, 0.0];
    let t = |x: f64| [x, 0.0, 4.0];
    let pairs = [(b(0.0), b(4.0)), (b(4.0), b(8.0)), (b(8.0), b(12.0)), (b(12.0), b(16.0)), (t(4.0), t(8.0)), (t(8.0), t(12.0)), (b(0.0), t(4.0)), (t(12.0), b(16.0)), (b(4.0), t(4.0)), (b(8.0), t(8.0)), (b(12.0), t(12.0)), (t(4.0), b(8.0)), (t(12.0), b(8.0))];
    let mut ids = vec![];
    for (p, q) in pairs {
        let id = mem(&mut m, p, q, s, 2);
        m.members.iter_mut().find(|x| x.id == id).unwrap().truss = true;
        ids.push(id);
    }
    for n in m.nodes.clone() {
        m.supports.push(Support { node: n.id, fix: [false, true, false, false, false, false], springs: [0.0; 6], uplift: false });
    }
    sup(&mut m, b(0.0), [true, true, true, false, false, false]);
    sup(&mut m, b(16.0), [false, true, true, false, false, false]);
    for x in [4.0, 8.0, 12.0] {
        let n = m.find_node(b(x)).unwrap();
        nodal(&mut m, 2, n, [0.0, 0.0, -10.0, 0.0, 0.0, 0.0]);
    }
    let a = analyze(&m).unwrap();
    let c = a.case(2).unwrap();
    let n = |k: usize| c.member(ids[k]).unwrap().stations[0].n;
    let src = "Kassimali, Structural Analysis 6e, Ch. 4 (method of sections)";
    row("linear statics", "Pratt truss, 4 panels", src, "bottom chord, panel 2", "kN", 15.0, n(1), 1e-4, "closed-form");
    row("linear statics", "Pratt truss, 4 panels", src, "top chord, panel 2", "kN", -20.0, n(4), 1e-4, "closed-form");
    row("linear statics", "Pratt truss, 4 panels", src, "diagonal T4–B8", "kN", 5.0 * 2f64.sqrt(), n(11), 1e-4, "closed-form");
}

/// A fixed-base portal under a lateral load at the beam (slope-deflection):
/// k = (24EIc/h³)(1 + 6ρ)/(4 + 6ρ), ρ = (Ib/L)/(Ic/h).
#[test]
fn portal_sway_and_space_frame() {
    let (h, l, hl) = (4.0, 6.0, 10.0);
    let mut m = bare("portal");
    let (ic, ib) = (2.0e-3, 4.0e-3);
    let cs = general(&mut m, 10.0, ic, ic, 1e-3);
    let bs = general(&mut m, 10.0, ib, ib, 1e-3);
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, h], cs, 2);
    mem(&mut m, [l, 0.0, 0.0], [l, 0.0, h], cs, 2);
    mem(&mut m, [0.0, 0.0, h], [l, 0.0, h], bs, 2);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [l, 0.0, 0.0], FIXED);
    let top = m.find_node([0.0, 0.0, h]).unwrap();
    nodal(&mut m, 2, top, [hl, 0.0, 0.0, 0.0, 0.0, 0.0]);
    let a = analyze(&m).unwrap();
    let e = steel_e(&m) * 1e3;
    let rho = (ib / l) / (ic / h);
    let k = 24.0 * e * ic / h.powi(3) * (1.0 + 6.0 * rho) / (4.0 + 6.0 * rho);
    let u = a.case(2).unwrap().displacements[&top][0];
    row("linear statics", "fixed-base portal, lateral load", "Hibbeler 10e Ch. 11 (slope-deflection); Ghali & Neville 7e §3", "sway", "mm", hl / k * 1e3, u * 1e3, 0.05, "closed-form");
    // a 3D L-frame (McGuire, Gallagher & Ziemian 2e, Ch. 13): column H fixed at its base,
    // cantilever beam L along x, tip load P in y: δy = PL³/3EI + PH³/3EI + P L² H / GJ
    let (hh, ll, p) = (3.0, 4.0, 5.0);
    let mut m = bare("L");
    let (ii, jj) = (1.0e-4, 1.5e-4);
    let s = general(&mut m, 1.0, ii, ii, jj);
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, hh], s, 2);
    mem(&mut m, [0.0, 0.0, hh], [ll, 0.0, hh], s, 2);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    let tip = m.find_node([ll, 0.0, hh]).unwrap();
    nodal(&mut m, 2, tip, [0.0, p, 0.0, 0.0, 0.0, 0.0]);
    let a = analyze(&m).unwrap();
    let e = steel_e(&m) * 1e3;
    let g = e / (2.0 * (1.0 + m.materials[1].nu));
    let want = p * ll.powi(3) / (3.0 * e * ii) + p * hh.powi(3) / (3.0 * e * ii) + p * ll * ll * hh / (g * jj);
    row("linear statics", "3D L-frame, out-of-plane tip load", "McGuire, Gallagher & Ziemian 2e, Ch. 13 (bending + torsion)", "tip δy", "mm", want * 1e3, a.case(2).unwrap().displacements[&tip][1] * 1e3, 0.05, "closed-form");
}

/// Springs, settlement, temperature and a rigid diaphragm (closed form).
#[test]
fn support_and_load_types() {
    let (l, w) = (4.0, 10.0);
    // a cantilever on a spring at its tip under a UDL: R = (wL⁴/8EI) / (L³/3EI + 1/k)
    let mut m = bare("spring");
    let s = m.rect_section(0.3, 0.5);
    let id = mem(&mut m, [0.0, 0.0, 0.0], [l, 0.0, 0.0], s, 1);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    let k = 5_000.0;
    let n = m.find_node([l, 0.0, 0.0]).unwrap();
    m.supports.push(Support { node: n, fix: [false, true, false, true, false, false], springs: [0.0, 0.0, k, 0.0, 0.0, 0.0], uplift: false });
    line(&mut m, 2, id, [0.0, 0.0, -w]);
    let a = analyze(&m).unwrap();
    let ei = m.materials[0].e * 1e3 * 0.3 * 0.5f64.powi(3) / 12.0;
    let r = (w * l.powi(4) / (8.0 * ei)) / (l.powi(3) / (3.0 * ei) + 1.0 / k);
    let got = -a.case(2).unwrap().displacements[&n][2] * k;
    row("supports and loads", "cantilever on an elastic spring", "STAAD.Pro Verification V8 type; compatibility", "spring force", "kN", r, got, 1e-3, "closed-form");
    // a fixed-fixed beam with a 10 mm settlement of one end: M = 6EIΔ/L²
    let mut m = bare("settle");
    let s = m.rect_section(0.3, 0.6);
    let id = mem(&mut m, [0.0, 0.0, 0.0], [6.0, 0.0, 0.0], s, 1);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    let nb = sup(&mut m, [6.0, 0.0, 0.0], FIXED);
    m.loads.push(Load { case: 2, kind: LoadKind::Settlement { node: nb, d: [0.0, 0.0, -0.010, 0.0, 0.0, 0.0] } });
    let a = analyze(&m).unwrap();
    let ei = m.materials[0].e * 1e3 * 0.3 * 0.6f64.powi(3) / 12.0;
    row("supports and loads", "fixed-fixed beam, support settlement", "Ghali & Neville 7e, Ch. 4; 6EIΔ/L²", "end moment |M|", "kN·m", 6.0 * ei * 0.010 / 36.0, st_at(a.case(2).unwrap(), id, 0.0).my.abs(), 1e-3, "closed-form");
    // the same beam with ΔT = 20 °C across its depth: M = EIαΔT/h (α of the material: concrete 10e-6 /°C)
    let mut m = bare("thermal");
    let s = m.rect_section(0.3, 0.6);
    let id = mem(&mut m, [0.0, 0.0, 0.0], [6.0, 0.0, 0.0], s, 1);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [6.0, 0.0, 0.0], FIXED);
    m.loads.push(Load { case: 2, kind: LoadKind::Thermal { member: id, dt: 0.0, dt_y: 0.0, dt_z: 20.0 } });
    let a = analyze(&m).unwrap();
    let want = ei * m.materials[0].alpha() * 20.0 / 0.6;
    let c = a.case(2).unwrap();
    row("supports and loads", "fixed-fixed beam, temperature gradient", "Ghali & Neville 7e, Ch. 6; EIαΔT/h", "|M| at the ends", "kN·m", want, st_at(c, id, 0.0).my.abs(), 1e-3, "closed-form");
    row("supports and loads", "fixed-fixed beam, temperature gradient", "Ghali & Neville 7e, Ch. 6; EIαΔT/h", "|M| at mid-span", "kN·m", want, st_at(c, id, 3.0).my.abs(), 1e-3, "closed-form");
    // a uniform temperature rise, both ends fixed: N = EAαΔT
    let mut m = bare("thermal-n");
    let s = m.rect_section(0.3, 0.6);
    let id = mem(&mut m, [0.0, 0.0, 0.0], [6.0, 0.0, 0.0], s, 1);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [6.0, 0.0, 0.0], FIXED);
    m.loads.push(Load { case: 2, kind: LoadKind::Thermal { member: id, dt: 30.0, dt_y: 0.0, dt_z: 0.0 } });
    let a = analyze(&m).unwrap();
    let ea = m.materials[0].e * 1e3 * 0.18;
    row("supports and loads", "restrained bar, uniform temperature", "Timoshenko, Strength of Materials; EAαΔT", "N", "kN", -ea * m.materials[0].alpha() * 30.0, a.case(2).unwrap().member(id).unwrap().stations[0].n, 1e-3, "closed-form");
    // two columns tied by a rigid diaphragm share a storey shear as their stiffnesses
    let mut m = bare("diaphragm");
    let (i1, i2) = (1.0e-3, 3.0e-3);
    let s1 = general(&mut m, 10.0, i1, i1, 1e-3);
    let s2 = general(&mut m, 10.0, i2, i2, 1e-3);
    let c1 = mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, 3.0], s1, 2);
    mem(&mut m, [5.0, 0.0, 0.0], [5.0, 0.0, 3.0], s2, 2);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [5.0, 0.0, 0.0], FIXED);
    let top = m.find_node([5.0, 0.0, 3.0]).unwrap();
    nodal(&mut m, 2, top, [100.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    m.diaphragms = vec![3.0];
    let a = analyze(&m).unwrap();
    let st = &a.case(2).unwrap().member(c1).unwrap().stations[0];
    row("supports and loads", "rigid diaphragm, two columns", "stiffness distribution of storey shear", "shear in column 1", "kN", 100.0 * i1 / (i1 + i2), st.vy.abs().max(st.vz.abs()), 0.1, "closed-form");
}

// ================================================================ 2. slab load transfer

/// A 4 x 6 m two-way panel (ly/lx = 1.5) on four simply supported edge beams, 10 kPa: by the
/// 45° lines each short beam takes a triangle, q lx²/4 = 40 kN with peak q lx/2, and each long
/// beam a trapezoid, q (lx ly/2 − lx²/4) = 80 kN (tributary areas; e.g. Hibbeler 10e §2.1).
#[test]
fn slab_tributary_loads() {
    let (lx, ly, q) = (4.0, 6.0, 10.0);
    let mut m = bare("slab");
    let s = m.rect_section(0.3, 0.6);
    let c = [[0.0, 0.0, 3.0], [ly, 0.0, 3.0], [ly, lx, 3.0], [0.0, lx, 3.0]];
    let mut ids = vec![];
    for k in 0..4 {
        let id = mem(&mut m, c[k], c[(k + 1) % 4], s, 1);
        let x = m.members.iter_mut().find(|x| x.id == id).unwrap();
        x.hinge_i = true;
        x.hinge_j = true;
        ids.push(id);
    }
    for p in c {
        sup(&mut m, p, FIXED);
    }
    m.slabs.push(Slab { id: 1, name: "S1".into(), x0: 0.0, y0: 0.0, x1: ly, y1: lx, z: 3.0, h: 0.15, dead: 0.0, live: q, bim_id: None, ..Default::default() });
    let a = analyze(&m).unwrap();
    let cr = a.case(2).unwrap();
    let load = |id: Id| {
        let mr = cr.member(id).unwrap();
        mr.stations.first().unwrap().vz.abs() + mr.stations.last().unwrap().vz.abs()
    };
    let mmax = |id: Id| cr.member(id).unwrap().stations.iter().map(|s| s.my).fold(f64::MIN, f64::max);
    let src = "tributary areas by 45° lines (two-way slab on beams), e.g. Hibbeler 10e §2.1";
    row("slab load transfer", "4 x 6 m panel, 10 kPa", src, "long beam load", "kN", q * (lx * ly / 2.0 - lx * lx / 4.0), load(ids[0]), 1e-4, "closed-form");
    row("slab load transfer", "4 x 6 m panel, 10 kPa", src, "short beam load", "kN", q * lx * lx / 4.0, load(ids[1]), 1e-4, "closed-form");
    let w0 = q * lx / 2.0;
    row("slab load transfer", "4 x 6 m panel, 10 kPa", src, "long beam M_max (trapezoid)", "kN·m", w0 * (3.0 * ly * ly - 4.0 * (lx / 2.0).powi(2)) / 24.0, mmax(ids[0]), 0.05, "closed-form");
    row("slab load transfer", "4 x 6 m panel, 10 kPa", src, "short beam M_max (triangle)", "kN·m", w0 * lx * lx / 12.0, mmax(ids[1]), 0.05, "closed-form");
    row("slab load transfer", "4 x 6 m panel, 10 kPa", "equilibrium", "Σ reactions", "kN", q * lx * ly, cr.reaction_sum()[2], 1e-6, "closed-form");
}

/// A 3 x 8 m panel (ly/lx > 2) spans one way: each long beam takes half, q lx ly/2, and the
/// short beams nothing (ACI 318-19 §8.3 / the one-way rule).
#[test]
fn slab_one_way() {
    let (lx, ly, q) = (3.0, 8.0, 10.0);
    let mut m = bare("slab1");
    let s = m.rect_section(0.3, 0.6);
    let c = [[0.0, 0.0, 3.0], [ly, 0.0, 3.0], [ly, lx, 3.0], [0.0, lx, 3.0]];
    let mut ids = vec![];
    for k in 0..4 {
        let id = mem(&mut m, c[k], c[(k + 1) % 4], s, 1);
        let x = m.members.iter_mut().find(|x| x.id == id).unwrap();
        x.hinge_i = true;
        x.hinge_j = true;
        ids.push(id);
    }
    for p in c {
        sup(&mut m, p, FIXED);
    }
    m.slabs.push(Slab { id: 1, name: "S1".into(), x0: 0.0, y0: 0.0, x1: ly, y1: lx, z: 3.0, h: 0.15, dead: 0.0, live: q, bim_id: None, ..Default::default() });
    let a = analyze(&m).unwrap();
    let cr = a.case(2).unwrap();
    let load = |id: Id| {
        let mr = cr.member(id).unwrap();
        mr.stations.first().unwrap().vz.abs() + mr.stations.last().unwrap().vz.abs()
    };
    let src = "one-way slab (ly/lx > 2): load to the long edges, e.g. Hibbeler 10e §2.1";
    row("slab load transfer", "3 x 8 m panel, one way", src, "long beam load", "kN", q * lx * ly / 2.0, load(ids[0]), 1e-4, "closed-form");
    row("slab load transfer", "3 x 8 m panel, one way", src, "short beam load", "kN", 0.0, load(ids[1]), 1e-6, "closed-form");
}

// ================================================================ 3. plates

/// A plate a x b, h = a/100 (thin, so the Kirchhoff tables apply: the transverse shear the
/// Mindlin elements carry adds about (h/a)² relative deflection), E 25 GPa, ν 0.3, 10 kPa, on
/// rigid edge beams (free in torsion: simply supported; rigid in torsion: clamped), n elements
/// across a.
fn plate(a: f64, b: f64, n: usize, clamped: bool) -> (Model, f64) {
    let mut m = bare("plate");
    m.design.slab_method = pondus_core::slabfe::SlabMethod::Plate;
    m.design.slab_mesh = a / n as f64;
    m.materials[0].e = 25_000.0;
    m.materials[0].nu = 0.3;
    let s = general(&mut m, 10.0, 1e3, 1e3, if clamped { 1e3 } else { 1e-9 });
    let c = [[0.0, 0.0, 0.0], [a, 0.0, 0.0], [a, b, 0.0], [0.0, b, 0.0]];
    for k in 0..4 {
        mem(&mut m, c[k], c[(k + 1) % 4], s, 1);
    }
    for p in c {
        sup(&mut m, p, if clamped { FIXED } else { [true, true, true, false, false, true] });
    }
    let h = a / 100.0;
    m.slabs.push(Slab { id: 1, name: "S1".into(), x0: 0.0, y0: 0.0, x1: a, y1: b, z: 0.0, h, dead: 0.0, live: 10.0, bim_id: None, ..Default::default() });
    let d = 25_000e3 * h.powi(3) / (12.0 * (1.0 - 0.09));
    (m, d)
}

fn centre(cr: &CaseResult, x: f64, y: f64) -> pondus_core::slabfe::PlateResult {
    cr.plates.iter().min_by(|p, q| (p.x - x).hypot(p.y - y).total_cmp(&(q.x - x).hypot(q.y - y))).unwrap().clone()
}

/// Timoshenko & Woinowsky-Krieger, Theory of Plates and Shells 2e: Table 8 (simply supported
/// rectangular plates, ν 0.3: square w = 0.00406 qa⁴/D, Mx = My = 0.0479 qa²; b/a = 2
/// w = 0.01013 qa⁴/D, Mx = 0.1017 qa², My = 0.0464 qa²) and Table 35 (clamped square plate:
/// w = 0.00126 qa⁴/D, M centre 0.0231 qa², M mid-edge −0.0513 qa²). Mesh convergence of the
/// plate slab at n = 5, 9, 17, 33 elements across a (odd n, so an element sits at the centre).
#[test]
fn plates_timoshenko() {
    let (a, q) = (4.0, 10.0);
    let src_ss = "Timoshenko & Woinowsky-Krieger 2e, Table 8 (ν = 0.3)";
    let src_c = "Timoshenko & Woinowsky-Krieger 2e, Table 35 (ν = 0.3)";
    for n in [5usize, 9, 17, 33] {
        let tol = match n { 5 => 12.0, 9 => 5.0, 17 => 2.0, _ => 1.0 };
        let case = |s: &str| format!("{s}, n = {n}");
        // simply supported square
        let (m, d) = plate(a, a, n, false);
        let an = analyze(&m).unwrap();
        let cr = an.case(2).unwrap();
        let c = centre(cr, a / 2.0, a / 2.0);
        let wmax = cr.plates.iter().map(|p| -p.uz).fold(0.0, f64::max);
        row("plates", &case("SS square plate"), src_ss, "w_max", "mm", 0.00406 * q * a.powi(4) / d * 1e3, wmax * 1e3, tol, "published");
        row("plates", &case("SS square plate"), src_ss, "Mx centre", "kN·m/m", 0.0479 * q * a * a, c.mx, tol, "published");
        // clamped square
        let (m, d) = plate(a, a, n, true);
        let an = analyze(&m).unwrap();
        let cr = an.case(2).unwrap();
        let c = centre(cr, a / 2.0, a / 2.0);
        let wmax = cr.plates.iter().map(|p| -p.uz).fold(0.0, f64::max);
        row("plates", &case("clamped square plate"), src_c, "w_max", "mm", 0.00126 * q * a.powi(4) / d * 1e3, wmax * 1e3, tol * 1.5, "published");
        row("plates", &case("clamped square plate"), src_c, "Mx centre", "kN·m/m", 0.0231 * q * a * a, c.mx, tol * 1.5, "published");
        if n >= 9 {
            // the moment at the middle of an edge, extrapolated linearly from the two elements
            // next to it (centres a/2n and 3a/2n inside the edge) to the edge
            let h = a / n as f64;
            let (e1, e2) = (centre(cr, h / 2.0, a / 2.0), centre(cr, 1.5 * h, a / 2.0));
            let edge = 1.5 * e1.mx - 0.5 * e2.mx;
            row("plates", &case("clamped square plate"), src_c, "Mx mid-edge (extrapolated)", "kN·m/m", -0.0513 * q * a * a, edge, match n { 9 => 12.0, 17 => 6.0, _ => 3.0 }, "published");
        }
    }
    // a rectangle b = 2a, simply supported, n = 17 across a
    let (m, d) = plate(a, 2.0 * a, 17, false);
    let an = analyze(&m).unwrap();
    let cr = an.case(2).unwrap();
    let c = centre(cr, a / 2.0, a);
    let wmax = cr.plates.iter().map(|p| -p.uz).fold(0.0, f64::max);
    row("plates", "SS plate b/a = 2, n = 17", src_ss, "w_max", "mm", 0.01013 * q * a.powi(4) / d * 1e3, wmax * 1e3, 2.0, "published");
    row("plates", "SS plate b/a = 2, n = 17", src_ss, "Mx centre (short span)", "kN·m/m", 0.1017 * q * a * a, c.mx, 2.0, "published");
    row("plates", "SS plate b/a = 2, n = 17", src_ss, "My centre (long span)", "kN·m/m", 0.0464 * q * a * a, c.my, 3.0, "published");
    // the load reaches the supports
    row("plates", "SS plate b/a = 2, n = 17", "equilibrium", "Σ reactions", "kN", q * 2.0 * a * a, cr.reaction_sum()[2], 1e-6, "closed-form");
}

// ================================================================ 4. modal and seismic

/// Clough & Penzien 3e Table 18-1: a uniform cantilever's βnL = 1.8751, 4.6941, 7.8548;
/// fn = (βnL)²/(2πL²) √(EI/m̄). A horizontal 6 m cantilever, 0.3 x 0.6, bending sideways (y),
/// mass from its self-weight; 32 elements.
#[test]
fn modal_beams_and_shear_building() {
    let l = 6.0;
    let mut m = Model::new("cantilever");
    let s = m.rect_section(0.6, 0.3);
    mem(&mut m, [0.0, 0.0, 0.0], [l, 0.0, 0.0], s, 1);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    m.seismic.enabled = true;
    m.seismic.mass_target = 0.99;
    m.seismic.max_modes = 40;
    let a = analyze_with(&m, 32, 2).unwrap();
    let sr = a.seismic.as_ref().unwrap();
    let mat = &m.materials[0];
    let ei = mat.e * 1e3 * 0.3 * 0.6f64.powi(3) / 12.0; // bending in y: b 0.6 across y
    let mbar = mat.gamma * 0.18 / 9.81;
    let mut fy: Vec<f64> = sr.modes.iter().filter(|md| md.mass_ratio[1] > 1e-3).map(|md| md.frequency).collect();
    fy.sort_by(f64::total_cmp);
    let src = "Clough & Penzien 3e, Table 18-1 (βnL of a cantilever)";
    for (k, bl) in [1.8751, 4.6941, 7.8548].iter().enumerate() {
        let f = bl * bl / (2.0 * PI * l * l) * (ei / mbar).sqrt();
        row("modal", "uniform cantilever, 32 elements", src, &format!("f{} (lateral)", k + 1), "Hz", f, fy[k], 0.3, "published");
    }
    // two-storey shear building (Chopra 5e Example 10.x form): equal storey mass m and
    // stiffness k = 2·12EIc/h³ (rigid beams): ω² = (k/m)(3 ∓ √5)/2, first-mode mass 94.72 %
    let h = 3.0;
    let mut m = bare("shear building");
    let ic = 2e-4;
    let cs = general(&mut m, 10.0, ic, ic, 1e-4);
    let bs = general(&mut m, 10.0, 10.0, 10.0, 10.0);
    let span = 6.0;
    let wd = 50.0;
    for z in [0.0, h] {
        mem(&mut m, [0.0, 0.0, z], [0.0, 0.0, z + h], cs, 2);
        mem(&mut m, [span, 0.0, z], [span, 0.0, z + h], cs, 2);
        let b = mem(&mut m, [0.0, 0.0, z + h], [span, 0.0, z + h], bs, 2);
        line(&mut m, 1, b, [0.0, 0.0, -wd]);
    }
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [span, 0.0, 0.0], FIXED);
    m.seismic.enabled = true;
    m.seismic.live_mass = 0.0;
    m.seismic.spectrum = pondus_core::seismic::Spectrum::Table { points: vec![(0.0, 0.5), (20.0, 0.5)] };
    m.seismic.r = 1.0;
    m.seismic.ie = 1.0;
    m.seismic.elf_ratio = 0.0;
    let a = analyze(&m).unwrap();
    let sr = a.seismic.as_ref().unwrap();
    let e = steel_e(&m) * 1e3;
    let k = 2.0 * 12.0 * e * ic / h.powi(3);
    let mass = wd * span / 9.81;
    let mut fx: Vec<(f64, f64)> = sr.modes.iter().filter(|md| md.mass_ratio[0] > 1e-3).map(|md| (md.frequency, md.mass_ratio[0])).collect();
    fx.sort_by(|p, q| p.0.total_cmp(&q.0));
    let src = "Chopra, Dynamics of Structures 5e, Ch. 10 (two-storey shear frame)";
    for (kk, sgn) in [-1.0, 1.0].iter().enumerate() {
        let w = (k / mass * (3.0 + sgn * 5f64.sqrt()) / 2.0).sqrt();
        row("modal", "two-storey shear building", src, &format!("f{}", kk + 1), "Hz", w / (2.0 * PI), fx[kk].0, 0.2, "closed-form");
    }
    let phi = (1.0 + 5f64.sqrt()) / 2.0;
    let r1 = (1.0 + phi).powi(2) / (2.0 * (1.0 + phi * phi));
    row("modal", "two-storey shear building", src, "first-mode effective mass ratio", "–", r1, fx[0].1, 0.1, "closed-form");
    // response spectrum, flat Sa = 0.5 g, R = 1: modal base shears r_i W Sa, combined by CQC
    // with Der Kiureghian's ρ (ξ = 5 %)
    let wt = 2.0 * wd * span;
    let (w1, w2) = ((k / mass * (3.0 - 5f64.sqrt()) / 2.0).sqrt(), (k / mass * (3.0 + 5f64.sqrt()) / 2.0).sqrt());
    let (v1, v2) = (r1 * wt * 0.5, (1.0 - r1) * wt * 0.5);
    let r = w2 / w1;
    let xi = 0.05;
    let rho = 8.0 * xi * xi * (1.0 + r) * r.powf(1.5) / ((1.0 - r * r).powi(2) + 4.0 * xi * xi * r * (1.0 + r).powi(2));
    let vcqc = (v1 * v1 + v2 * v2 + 2.0 * rho * v1 * v2).sqrt();
    row("seismic", "two-storey shear building, flat spectrum", "Chopra 5e Ch. 13 (RSA); Der Kiureghian (1981) CQC", "base shear V_RSA", "kN", vcqc, sr.dirs[0].v_rsa, 0.2, "closed-form");
    row("seismic", "two-storey shear building, flat spectrum", "seismic weight = D", "W", "kN", wt, sr.weight, 1e-6, "closed-form");
}

/// ASCE 7-22 §12.8 equivalent lateral force on two frames: a 3-storey frame in the velocity
/// branch and a stiff one-storey frame whose period lies below T0, where Cs = SDS/(R/Ie)
/// (Eq. 12.8-2; the spectrum's ascending branch does not apply to the ELF).
#[test]
fn elf_asce7() {
    let (sds, sd1) = (1.0, 0.6);
    let src = "ASCE 7-22 §12.8, Eqs. 12.8-1 to 12.8-7";
    for (storeys, col, label) in [(3usize, 0.4, "3-storey RC frame"), (1usize, 1.2, "stiff 1-storey frame (T < T0)")] {
        let mut m = Model::new(label);
        let c = m.rect_section(col, col);
        let b = m.rect_section(0.3, 0.6);
        let h = 3.5;
        for k in 0..storeys {
            let z = k as f64 * h;
            for x in [0.0, 6.0] {
                mem(&mut m, [x, 0.0, z], [x, 0.0, z + h], c, 1);
            }
            let bm = mem(&mut m, [0.0, 0.0, z + h], [6.0, 0.0, z + h], b, 1);
            line(&mut m, 1, bm, [0.0, 0.0, -20.0]);
        }
        sup(&mut m, [0.0, 0.0, 0.0], FIXED);
        sup(&mut m, [6.0, 0.0, 0.0], FIXED);
        m.seismic.enabled = true;
        m.seismic.spectrum = pondus_core::seismic::Spectrum::Code { sds, sd1, tl: 8.0 };
        m.seismic.r = 8.0;
        m.seismic.ie = 1.0;
        let a = analyze(&m).unwrap();
        let sr = a.seismic.as_ref().unwrap();
        let d = &sr.dirs[0];
        let hn = storeys as f64 * h;
        let ta = 0.0466 * hn.powf(0.9);
        let t = d.t1.min(1.4 * ta);
        let ts = sd1 / sds;
        let mut cs = if t <= ts { sds / 8.0 } else { sd1 / (t * 8.0) };
        cs = cs.max(0.044 * sds).max(0.01);
        row("seismic", label, src, "T used (min(T1, Cu·Ta))", "s", t, d.t_used, 1e-6, "closed-form");
        row("seismic", label, src, "Cs", "–", cs, d.cs, 1e-6, "closed-form");
        row("seismic", label, src, "V_ELF = Cs·W", "kN", cs * sr.weight, d.v_elf, 1e-6, "closed-form");
        if storeys == 1 {
            eprintln!("  T1 {:.4} s, T0 {:.4} s", d.t1, 0.2 * ts);
        }
    }
}

// ================================================================ 5. time history

/// Chopra 5e §4.3: an undamped SDOF under a suddenly applied, constant ground acceleration a0
/// peaks at u = 2 a0/ω² (dynamic amplification 2). A massless cantilever column with its mass
/// on a short stiff stub at the top.
#[test]
fn time_history_step() {
    let h = 4.0;
    let mut m = bare("sdof");
    let ic = 5e-5;
    let cs = general(&mut m, 1.0, ic, ic, 1e-4);
    // a short stub, stiff but slim, so its rotary inertia stays negligible
    let stub = general(&mut m, 1.0, 1e-3, 1e-3, 1e-3);
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, h], cs, 2);
    let s1 = mem(&mut m, [-0.05, 0.0, h], [0.0, 0.0, h], stub, 2);
    let s2 = mem(&mut m, [0.0, 0.0, h], [0.05, 0.0, h], stub, 2);
    for s in [s1, s2] {
        line(&mut m, 1, s, [0.0, 0.0, -1000.0]);
    }
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    m.seismic.enabled = true;
    m.seismic.live_mass = 0.0;
    let a = analyze(&m).unwrap();
    let e = steel_e(&m) * 1e3;
    let k = 3.0 * e * ic / h.powi(3);
    let mass = 100.0 / 9.81;
    let w = (k / mass).sqrt();
    let tn = 2.0 * PI / w;
    let a0 = 0.1;
    let rec = pondus_core::timehistory::Record { name: "step".into(), dt: tn / 200.0, accel: vec![a0; 4000] };
    let st = pondus_core::timehistory::Settings { direction: 0, scale: 1.0, dt: tn / 200.0, duration: Some(3.0 * tn), damping: 0.0, alpha: None };
    let r = pondus_core::timehistory::run(&m, &a, &rec, &st).unwrap();
    let want = 2.0 * a0 * 9.81 / (w * w);
    row("time history", "undamped SDOF, step ground acceleration", "Chopra, Dynamics of Structures 5e, §4.3 (DAF = 2)", "peak relative displacement", "mm", want * 1e3, r.roof_peak * 1e3, 0.5, "closed-form");
    for md in a.seismic.as_ref().unwrap().modes.iter().take(4) {
        eprintln!("  mode {:.4} Hz mass {:?}", md.frequency, md.mass_ratio);
    }
    let f1 = a.seismic.as_ref().unwrap().modes.iter().find(|md| md.mass_ratio[0] > 0.5).unwrap().frequency;
    row("time history", "undamped SDOF, step ground acceleration", "ω = √(k/m), k = 3EI/h³", "natural frequency", "Hz", w / (2.0 * PI), f1, 0.1, "closed-form");
}

// ================================================================ 6. stability

fn column(len: f64, base: [bool; 6], top: [bool; 6], p: f64) -> Model {
    let mut m = bare("column");
    m.combos = vec![Combination { id: 1, name: "ULS".into(), kind: ComboKind::Uls, factors: vec![(2, 1.0)] }];
    let s = general(&mut m, 0.01, 1e-4, 1e-4, 1e-4);
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, len], s, 2);
    sup(&mut m, [0.0, 0.0, 0.0], base);
    let t = m.find_node([0.0, 0.0, len]).unwrap();
    if top.iter().any(|x| *x) {
        sup(&mut m, [0.0, 0.0, len], top);
    }
    nodal(&mut m, 2, t, [0.0, 0.0, -p, 0.0, 0.0, 0.0]);
    m
}

/// Timoshenko & Gere, Theory of Elastic Stability 2e, Ch. 2: Pcr = c π²EI/L² with c = 1
/// (pinned), 1/4 (cantilever), 4 (fixed-fixed) and 20.19 EI/L² (fixed-pinned, printed §2.3);
/// a fixed-base portal (Ib = Ic, L = h) sways at x cot x = −6/G (G = 1), the alignment-chart
/// equation of AISC 360 Commentary C-A-7-2, which is exact here; P-Delta amplification of a
/// cantilever, 3(tan u − u)/u³ (Timoshenko & Gere §1.11).
#[test]
fn stability() {
    let (l, p) = (5.0, 100.0);
    let ei = 200_000e3 * 1e-4;
    let src = "Timoshenko & Gere, Theory of Elastic Stability 2e, Ch. 2";
    // a pinned end holds the translations (and the base the twist, rz); a fixed top slides
    // axially only
    let pin_base = [true, true, true, false, false, true];
    let pin_top = [true, true, false, false, false, false];
    let fixed_top = [true, true, false, true, true, true];
    for (label, base, top, c, kind) in [
        ("pinned-pinned", pin_base, pin_top, PI * PI, "closed-form"),
        ("fixed-free", FIXED, [false; 6], PI * PI / 4.0, "closed-form"),
        ("fixed-pinned", FIXED, pin_top, 20.19, "published"),
        ("fixed-fixed", FIXED, fixed_top, 4.0 * PI * PI, "closed-form"),
    ] {
        let m = column(l, base, top, p);
        let a = analyze_with(&m, 8, 8).unwrap();
        let modes = pondus_core::stability::buckling(&m, &a, &m.combos[0], 2).unwrap();
        row("stability", &format!("{label} column, 8 elements"), src, "Pcr", "kN", c * ei / (l * l), modes[0].factor * p, if kind == "published" { 0.1 } else { 0.1 }, kind);
    }
    // the fixed-base portal
    let h = 4.0;
    let mut m = bare("portal");
    m.combos = vec![Combination { id: 1, name: "ULS".into(), kind: ComboKind::Uls, factors: vec![(2, 1.0)] }];
    // axially rigid, as the closed form assumes (with A = 0.01 m² the axial shortening lowers
    // Pcr by 0.36 %)
    let s = general(&mut m, 100.0, 1e-4, 1e-4, 1e-4);
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, h], s, 2);
    mem(&mut m, [h, 0.0, 0.0], [h, 0.0, h], s, 2);
    mem(&mut m, [0.0, 0.0, h], [h, 0.0, h], s, 2);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [h, 0.0, 0.0], FIXED);
    for x in [0.0, h] {
        let n = m.find_node([x, 0.0, h]).unwrap();
        nodal(&mut m, 2, n, [0.0, 0.0, -p, 0.0, 0.0, 0.0]);
        // hold the frame in its plane
        m.supports.push(Support { node: n, fix: [false, true, false, true, false, true], springs: [0.0; 6], uplift: false });
    }
    let a = analyze_with(&m, 8, 8).unwrap();
    let modes = pondus_core::stability::buckling(&m, &a, &m.combos[0], 2).unwrap();
    // solve x cot x = −6 on (π/2, π)
    let (mut lo, mut hi) = (PI / 2.0 + 1e-9, PI - 1e-9);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if mid / mid.tan() + 6.0 > 0.0 { lo = mid } else { hi = mid }
    }
    let x = 0.5 * (lo + hi);
    let pcr = x * x * ei / (h * h);
    row("stability", "fixed-base portal, sway buckling", "Timoshenko & Gere 2e §2.5; AISC 360-16 Commentary Eq. C-A-7-2 (G_A = 0, G_B = 1)", "Pcr per column", "kN", pcr, modes[0].factor * p, 0.2, "closed-form");
    // P-Delta of a cantilever at 0.4 Pcr under a small lateral load
    let mut m = column(l, FIXED, [false; 6], 0.0);
    let pc = PI * PI * ei / (4.0 * l * l);
    let t = m.find_node([0.0, 0.0, l]).unwrap();
    m.loads[0] = Load { case: 2, kind: LoadKind::Nodal { node: t, f: [1.0, 0.0, -0.4 * pc, 0.0, 0.0, 0.0] } };
    let lin = analyze(&m).unwrap().case(2).unwrap().displacements[&t][0];
    m.design.second_order = true;
    let a = analyze_with(&m, 8, 8).unwrap();
    let u2 = a.second.get(&1).unwrap().0.displacements[&t][0];
    let u = PI / 2.0 * 0.4f64.sqrt();
    row("stability", "cantilever P-Delta at 0.4 Pcr", "Timoshenko & Gere 2e §1.11: 3(tan u − u)/u³", "sway amplification", "–", 3.0 * (u.tan() - u) / u.powi(3), u2 / lin, 0.2, "closed-form");
}

// ================================================================ 7. plastic collapse

fn steel_mp(m: &mut Model, name: &str) -> (Id, f64) {
    let s = m.section_named(name).unwrap();
    let zy = m.section(s).unwrap().props().zy;
    (s, 245.0e3 * zy)
}

fn collapse(m: &Model) -> f64 {
    let a = analyze(m).unwrap();
    let rep = pondus_core::design::design_pass(m, &a, &Default::default());
    let spec = pondus_core::advanced::PushoverSpec { combination: Some("ULS".into()), ..Default::default() };
    pondus_core::advanced::pushover(m, &a, Some(&rep), &spec).unwrap().collapse_factor
}

/// Neal, The Plastic Methods of Structural Analysis 3e, and Horne, Plastic Theory of
/// Structures: simple-beam 4Mp/L, propped cantilever 6Mp/L, fixed beam 16Mp/L², two equal
/// spans under UDL (6 + 4√2) Mp/L² = 11.66 Mp/L², and the fixed-base portal whose combined
/// mechanism governs (upper-bound theorem).
#[test]
fn plastic_collapse() {
    let l = 6.0;
    let src = "Neal, The Plastic Methods of Structural Analysis 3e, Chs. 2–3";
    let mk = |sa: [bool; 6], sb: [bool; 6], spans: usize| {
        let mut m = bare("plastic");
        m.combos = vec![Combination { id: 1, name: "ULS".into(), kind: ComboKind::Uls, factors: vec![(2, 1.0)] }];
        let (s, mp) = steel_mp(&mut m, "H 300x150x6.5x9");
        let mut ids = vec![];
        for k in 0..spans {
            ids.push(mem(&mut m, [k as f64 * l, 0.0, 0.0], [(k + 1) as f64 * l, 0.0, 0.0], s, 2));
        }
        sup(&mut m, [0.0, 0.0, 0.0], sa);
        for k in 1..=spans {
            sup(&mut m, [k as f64 * l, 0.0, 0.0], if k == spans { sb } else { ROLLER });
        }
        (m, ids, mp)
    };
    let pin = [true, true, true, true, false, true];
    let rol = [false, true, true, true, false, true];
    let (mut m, ids, mp) = mk(pin, rol, 1);
    m.loads.push(Load { case: 2, kind: LoadKind::Point { member: ids[0], a: l / 2.0, p: [0.0, 0.0, -1.0] } });
    row("plastic collapse", "simple beam, central load", src, "Pc", "kN", 4.0 * mp / l, collapse(&m), 0.1, "closed-form");
    let (mut m, ids, mp) = mk(FIXED, rol, 1);
    m.loads.push(Load { case: 2, kind: LoadKind::Point { member: ids[0], a: l / 2.0, p: [0.0, 0.0, -1.0] } });
    row("plastic collapse", "propped cantilever, central load", src, "Pc", "kN", 6.0 * mp / l, collapse(&m), 0.1, "closed-form");
    let (mut m, ids, mp) = mk(FIXED, FIXED, 1);
    line(&mut m, 2, ids[0], [0.0, 0.0, -1.0]);
    row("plastic collapse", "fixed-ended beam, UDL", src, "wc", "kN/m", 16.0 * mp / (l * l), collapse(&m), 0.1, "closed-form");
    let (mut m, ids, mp) = mk(pin, rol, 2);
    for id in ids {
        line(&mut m, 2, id, [0.0, 0.0, -1.0]);
    }
    row("plastic collapse", "two equal spans, UDL", src, "wc", "kN/m", (6.0 + 4.0 * 2f64.sqrt()) * mp / (l * l), collapse(&m), 0.5, "published");
    // the portal: h 4, L 8, V = 1.5 H at mid-span, H at the beam: beam 8Mp/L, sway 4Mp/h,
    // combined (H h + V L/2) θ = 6 Mp θ → H = 0.6 Mp/(… ) here
    let (h, ll) = (4.0, 8.0);
    let mut m = bare("portal");
    m.combos = vec![Combination { id: 1, name: "ULS".into(), kind: ComboKind::Uls, factors: vec![(2, 1.0)] }];
    let (s, mp) = steel_mp(&mut m, "H 300x150x6.5x9");
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, h], s, 2);
    mem(&mut m, [ll, 0.0, 0.0], [ll, 0.0, h], s, 2);
    let b = mem(&mut m, [0.0, 0.0, h], [ll, 0.0, h], s, 2);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [ll, 0.0, 0.0], FIXED);
    let tl = m.find_node([0.0, 0.0, h]).unwrap();
    nodal(&mut m, 2, tl, [1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    m.loads.push(Load { case: 2, kind: LoadKind::Point { member: b, a: ll / 2.0, p: [0.0, 0.0, -1.5] } });
    for n in m.nodes.clone().iter().filter(|n| n.z > 0.0) {
        m.supports.push(Support { node: n.id, fix: [false, true, false, true, false, true], springs: [0.0; 6], uplift: false });
    }
    let beam = 8.0 * mp / ll / 1.5;
    let sway = 4.0 * mp / h;
    let comb = 6.0 * mp / (h + 1.5 * ll / 2.0);
    let want = beam.min(sway).min(comb);
    row("plastic collapse", "fixed-base portal, V = 1.5H (combined mechanism)", "Horne, Plastic Theory of Structures, Ch. 3; Neal 3e Ch. 3", "λc", "–", want, collapse(&m), 0.5, "closed-form");
}

// ================================================================ 8. moving loads, staging, cables, foundations

/// Hibbeler 10e Ch. 6: the absolute maximum moment of two equal axles P spaced s on a simple
/// span is P(L − s/2)²/(2L); the influence line of the moment at a section a has its peak
/// a(L − a)/L there, and the left reaction's line is 1 − x/L.
#[test]
fn moving_and_influence() {
    let (l, p, s) = (10.0, 100.0, 2.0);
    let mut m = bare("moving");
    let sec = m.rect_section(0.4, 0.8);
    let id = mem(&mut m, [0.0, 0.0, 0.0], [l, 0.0, 0.0], sec, 1);
    let n0 = sup(&mut m, [0.0, 0.0, 0.0], PIN);
    sup(&mut m, [l, 0.0, 0.0], ROLLER);
    m.moving = Some(pondus_core::advanced::MovingSpec { name: "Axles".into(), path: vec![id], axles: vec![(0.0, p), (s, p)], step: 0.25, impact: 0.0, factor: 1.6 });
    let a = analyze(&m).unwrap();
    let (hi, _) = a.moving.clone().unwrap();
    let mmax = hi.member(id).unwrap().stations.iter().map(|s| s.my).fold(f64::MIN, f64::max);
    let src = "Hibbeler, Structural Analysis 10e, Ch. 6 (influence lines, absolute maximum moment)";
    row("moving loads", "two 100 kN axles 2 m apart, 10 m span", src, "absolute max moment", "kN·m", p * (l - s / 2.0).powi(2) / (2.0 * l), mmax, 0.1, "closed-form");
    let il = pondus_core::advanced::influence(&m, &a, "My", Some(id), 2.5, None).unwrap();
    let pk = il.iter().map(|q| q.value).fold(f64::MIN, f64::max);
    row("moving loads", "influence line of M at L/4", src, "peak ordinate", "m", 2.5 * 7.5 / l, pk, 0.1, "closed-form");
    let il = pondus_core::advanced::influence(&m, &a, "Fz", None, 0.0, Some(n0)).unwrap();
    let mid = il.iter().min_by(|a, b| (a.x - 5.0).abs().total_cmp(&(b.x - 5.0).abs())).unwrap();
    row("moving loads", "influence line of R_A", src, "ordinate at mid-span", "–", 0.5, mid.value.abs(), 0.1, "closed-form");
}

/// Staged construction (closed form): a 6 m cantilever carries its dead load alone; a stiff
/// prop is then built under its tip and the live load follows, so the root moment is
/// wD L²/2 + wL L²/8 and the prop takes 3wL L/8.
#[test]
fn staged_prop() {
    let (l, wd, wl) = (6.0, 10.0, 5.0);
    let mut m = bare("staged");
    let s = m.rect_section(0.3, 0.6);
    let strut = general(&mut m, 1.0, 1e-6, 1e-6, 1e-6);
    let b = mem(&mut m, [0.0, 0.0, 3.0], [l, 0.0, 3.0], s, 1);
    let p = mem(&mut m, [l, 0.0, 0.0], [l, 0.0, 3.0], strut, 2);
    m.members.iter_mut().find(|x| x.id == p).unwrap().truss = true;
    sup(&mut m, [0.0, 0.0, 3.0], FIXED);
    sup(&mut m, [l, 0.0, 0.0], [true, true, true, true, true, true]);
    let tip = m.find_node([l, 0.0, 3.0]).unwrap();
    m.supports.push(Support { node: tip, fix: [false, true, false, true, false, true], springs: [0.0; 6], uplift: false });
    line(&mut m, 1, b, [0.0, 0.0, -wd]);
    line(&mut m, 2, b, [0.0, 0.0, -wl]);
    m.stages = vec![pondus_core::advanced::Stage { name: "cantilever + D".into(), members: vec![b], cases: vec![1] }, pondus_core::advanced::Stage { name: "prop + L".into(), members: vec![p], cases: vec![2] }];
    let a = analyze(&m).unwrap();
    let (_, fin) = pondus_core::advanced::staged(&m, &a).unwrap();
    let src = "staged construction, superposition of the stages (closed form)";
    row("staged construction", "cantilever propped after its dead load", src, "root moment", "kN·m", -(wd * l * l / 2.0 + wl * l * l / 8.0), st_at(&fin, b, 0.0).my, 0.1, "closed-form");
    row("staged construction", "cantilever propped after its dead load", src, "prop force", "kN", -3.0 * wl * l / 8.0, fin.member(p).unwrap().stations[0].n, 0.2, "closed-form");
}

/// Hetényi, Beams on Elastic Foundation (1946): an infinite beam under a point load P deflects
/// w0 = Pβ/2k with M0 = P/4β, β = (k/4EI)^¼; a beam 10/β long, 64 elements.
#[test]
fn winkler_hetenyi() {
    let mut m = bare("winkler");
    let s = m.rect_section(0.5, 0.5);
    let ei = m.materials[0].e * 1e3 * 0.5f64.powi(4) / 12.0;
    let k = 20_000.0;
    let beta = (k / (4.0 * ei)).powf(0.25);
    let l = 10.0 / beta;
    let b1 = mem(&mut m, [0.0, 0.0, 0.0], [l / 2.0, 0.0, 0.0], s, 1);
    let b2 = mem(&mut m, [l / 2.0, 0.0, 0.0], [l, 0.0, 0.0], s, 1);
    for x in m.members.iter_mut() {
        x.winkler = Some([k, 0.0]);
    }
    sup(&mut m, [0.0, 0.0, 0.0], [true, true, false, true, false, false]);
    sup(&mut m, [l, 0.0, 0.0], [false, true, false, false, false, false]);
    let mid = m.find_node([l / 2.0, 0.0, 0.0]).unwrap();
    let p = 100.0;
    nodal(&mut m, 2, mid, [0.0, 0.0, -p, 0.0, 0.0, 0.0]);
    let a = analyze_with(&m, 64, 2).unwrap();
    let c = a.case(2).unwrap();

    let src = "Hetényi, Beams on Elastic Foundation (1946), infinite beam under a point load";
    row("foundations", "beam on Winkler foundation, βL = 10", src, "w under the load", "mm", p * beta / (2.0 * k) * 1e3, -c.displacements[&mid][2] * 1e3, 0.5, "closed-form");
    row("foundations", "beam on Winkler foundation, βL = 10", src, "M under the load", "kN·m", p / (4.0 * beta), st_at(c, b1, l / 2.0).my, 0.5, "closed-form");
    let _ = b2;
}

/// One-way members and lift-off (statics): a tension-only X brace takes the whole storey
/// shear in its tension diagonal, H / cos θ; a two-span beam loaded on one span lifts off its
/// far support, which then carries nothing.
#[test]
fn one_way_members() {
    let mut m = bare("x-brace");
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
    sup(&mut m, a, [true, true, true, true, false, true]);
    sup(&mut m, b, [true, true, true, true, false, true]);
    for p in [d, cc] {
        sup(&mut m, p, [false, true, false, true, false, true]);
    }
    let top = m.find_node(d).unwrap();
    nodal(&mut m, 2, top, [30.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    let r = analyze(&m).unwrap();
    let l = r.case(2).unwrap();
    let src = "statics of a tension-only brace (compression diagonal buckled out)";
    row("nonlinear members", "tension-only X brace", src, "tension diagonal", "kN", 30.0 / 0.8, l.member(t1).unwrap().stations[1].n, 0.1, "closed-form");
    row("nonlinear members", "tension-only X brace", src, "compression diagonal", "kN", 0.0, l.member(t2).unwrap().stations[1].n, 1e-3, "closed-form");
    // lift-off
    let mut m = bare("lift-off");
    let s = m.rect_section(0.3, 0.6);
    let b1 = mem(&mut m, [0.0, 0.0, 0.0], [5.0, 0.0, 0.0], s, 1);
    mem(&mut m, [5.0, 0.0, 0.0], [10.0, 0.0, 0.0], s, 1);
    sup(&mut m, [0.0, 0.0, 0.0], PIN);
    let n2 = sup(&mut m, [5.0, 0.0, 0.0], ROLLER);
    let n3 = sup(&mut m, [10.0, 0.0, 0.0], ROLLER);
    m.supports.iter_mut().find(|x| x.node == n3).unwrap().uplift = true;
    line(&mut m, 2, b1, [0.0, 0.0, -20.0]);
    let r = analyze(&m).unwrap();
    let c = r.case(2).unwrap();
    let src = "statics: the lifted support carries nothing, span 1 acts as a simple beam";
    row("nonlinear members", "two-span beam, lift-off support", src, "R at the lifted support", "kN", 0.0, reaction(c, n3)[2], 1e-3, "closed-form");
    row("nonlinear members", "two-span beam, lift-off support", src, "R at the middle support", "kN", 50.0, reaction(c, n2)[2], 0.1, "closed-form");
}

/// The engine's cables are tension-only trusses with Ernst's equivalent modulus for their own
/// sag (Ernst 1965; Irvine, Cable Structures, 1981): a horizontal stay 50 m long, A 1000 mm²,
/// w = γA, pulled by H = 100 kN, stretches HL/(E_eq A) with E_eq/E = 1/[1 + (wL)² EA/(12H³)].
#[test]
fn cable_ernst() {
    let (l, h) = (50.0, 100.0);
    let mut m = bare("stay");
    let area = 1e-3;
    let s = general(&mut m, area, 1e-8, 1e-8, 1e-8);
    let c = mem(&mut m, [0.0, 0.0, 0.0], [l, 0.0, 0.0], s, 2);
    m.members.iter_mut().find(|x| x.id == c).unwrap().behaviour = Behaviour::Cable;
    sup(&mut m, [0.0, 0.0, 0.0], [true, true, true, false, false, false]);
    let end = sup(&mut m, [l, 0.0, 0.0], [false, true, true, false, false, false]);
    nodal(&mut m, 2, end, [h, 0.0, 0.0, 0.0, 0.0, 0.0]);
    let a = analyze(&m).unwrap_or_else(|e| panic!("cable: {e}"));
    let cr = a.case(2).unwrap();
    let ea = steel_e(&m) * 1e3 * area;
    let w = m.materials[1].gamma * area;
    let factor = 1.0 / (1.0 + (w * l).powi(2) * ea / (12.0 * h.powi(3)));
    let src = "Ernst (1965), Der E-Modul von Seilen; Irvine, Cable Structures (1981)";
    row("cables", "horizontal stay 50 m, H = 100 kN", src, "elongation", "mm", h * l / (ea * factor) * 1e3, cr.displacements[&end][0] * 1e3, 0.1, "closed-form");
    row("cables", "horizontal stay 50 m, H = 100 kN", "statics", "tension", "kN", h, cr.member(c).unwrap().stations[0].n, 0.01, "closed-form");
}

// ================================================================ 9. section properties

/// Timoshenko & Goodier, Theory of Elasticity 3e, §109, table of k1 in J = k1 b t³ for a
/// rectangle b x t: b/t = 1 → 0.141, 1.5 → 0.196, 2 → 0.229, 3 → 0.263, 5 → 0.291, 10 → 0.312
/// (printed to three figures), against the section finite elements and Pondus's own
/// rectangle formula (Roark), which the frame analysis uses.
#[test]
fn section_properties() {
    let src = "Timoshenko & Goodier, Theory of Elasticity 3e, §109 (torsion of rectangles)";
    for (ratio, k1) in [(1.0, 0.141), (1.5, 0.196), (2.0, 0.229), (3.0, 0.263), (5.0, 0.291), (10.0, 0.312)] {
        let (t, b) = (0.1, 0.1 * ratio);
        let fe = pondus_core::secfe::analyze(&[[0.0, 0.0], [b, 0.0], [b, t], [0.0, t]], 4000).unwrap();
        row("section properties", &format!("rectangle b/t = {ratio}"), src, "k1 = J/(b t³), section FE", "–", k1, fe.j / (b * t.powi(3)), 0.5, "published");
        let p = Section { id: 0, name: String::new(), shape: Shape::Rect { b, h: t } }.props();
        row("section properties", &format!("rectangle b/t = {ratio}"), src, "k1, Pondus frame formula", "–", k1, p.j / (b * t.powi(3)), 1.0, "published");
    }
    // the warping constant of an I without fillets: tf bf³ h0²/24 (Timoshenko & Gere §5.10)
    let (d, bf, tw, tf) = (0.3, 0.15, 0.0065, 0.009);
    let fe = pondus_core::secfe::analyze(&pondus_core::secfe::h_outline(d, bf, tw, tf), 6000).unwrap();
    row("section properties", "I 300 x 150 x 6.5 x 9 (no fillets)", "Timoshenko & Gere 2e §5.10: Cw = tf bf³ h0²/24", "Cw", "m⁶ x 1e9", tf * bf.powi(3) * (d - tf).powi(2) / 24.0 * 1e9, fe.cw * 1e9, 2.0, "closed-form");
    // the shear centre of a thin channel: e = 3 b² tf / (6 b tf + h tw) from the web centreline
    let (h, b, t) = (0.2, 0.08, 0.002);
    let out = vec![[0.0, 0.0], [b + t / 2.0, 0.0], [b + t / 2.0, t], [t, t], [t, h], [b + t / 2.0, h], [b + t / 2.0, h + t], [0.0, h + t]];
    let fe = pondus_core::secfe::analyze(&out, 6000).unwrap();
    let e_fe = t / 2.0 - (fe.centroid[0] + fe.shear_centre[0]);
    let e = 3.0 * b * b * t / (6.0 * b * t + h * t);
    row("section properties", "thin channel 200 x 80 x 2", "Timoshenko & Gere 2e §5.4; Roark 9e Table 9.? (thin-walled shear centre)", "shear centre from web", "mm", e * 1e3, e_fe * 1e3, 2.0, "closed-form");
    // JIS G 3192 tables: A and Ix of rolled H shapes, from Pondus's plate-and-fillet geometry
    let mut m = bare("jis");
    for (n, a, ix) in [("H 300x150x6.5x9", 46.78, 7210.0), ("H 200x100x5.5x8", 26.67, 1810.0), ("H 400x200x8x13", 83.37, 23500.0), ("H 400x400x13x21", 218.7, 66600.0), ("H 700x300x13x24", 231.5, 197000.0)] {
        let s = m.section_named(n).unwrap();
        let p = m.section(s).unwrap().props();
        row("section properties", n, "JIS G 3192 (dimensions, mass and properties of H sections)", "A", "cm²", a, p.a * 1e4, 0.3, "published");
        row("section properties", n, "JIS G 3192", "Ix", "cm⁴", ix, p.iy * 1e8, 1.0, "published");
    }
}

#[allow(dead_code)]
fn _unused(_: &Analysis) {}
