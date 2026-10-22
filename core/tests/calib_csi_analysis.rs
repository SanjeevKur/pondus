//! Calibration of Pondus's analysis against the analysis problems of the CSI verification
//! manuals: *SAP2000 Software Verification* (one PDF per problem,
//! docs.csiamerica.com/manuals/sap2000/Verification/Analysis/Frames/Problem 1-0xx.pdf) and
//! *Software Verification Examples for ETABS 2016* (Examples 1 – 15). Each problem prints an
//! independent value (a textbook formula, a hand calculation or a published program result),
//! which is the reference here, and SAP2000's or ETABS's own result, which is recorded beside
//! it (`calib::program`, calibration/programs.csv) without being asserted. Every case is built
//! as a Pondus model and solved through Pondus's analysis, modal, response spectrum, P-Delta,
//! buckling, staged and Winkler paths. The data extracted from the PDFs are in
//! calibration/sources/csi/specs/; the notes are in calibration/NOTES_csi.md.
//!
//! Plane frames lie in the x-z plane. A support that is "pinned" in the plane also holds the
//! rotations about x and z, so the frame cannot fall over out of its plane.

mod calib;
use calib::{check, program, Row};

use pondus_core::analysis::{analyze, analyze_with, Analysis, CaseResult};
use pondus_core::commands::run;
use pondus_core::model::*;
use serde_json::json;

const IN: f64 = 0.0254; // m
const FT: f64 = 0.3048; // m
const KIP: f64 = 4.448_222; // kN
const KSI: f64 = 6.894_757; // MPa
const KSF: f64 = KSI / 144.0; // MPa
/// kip·s²/in and kip·s²/ft in tonnes (kN·s²/m), kip·in·s² in t·m².
const KSI_MASS: f64 = KIP / IN;
const KSF_MASS: f64 = KIP / FT;
const KIS2: f64 = KIP * IN;
const SAP: &str = "CSI, SAP2000 Software Verification";
const ETABS: &str = "CSI, Software Verification Examples for ETABS 2016";

#[allow(clippy::too_many_arguments)]
fn row(aspect: &str, case: &str, source: &str, quantity: &str, unit: &str, reference: f64, pondus: f64, tol: f64, kind: &str) {
    check(Row { suite: "csi", aspect, case, source, quantity, unit, reference, pondus, tol, kind });
}

fn bare(name: &str) -> Model {
    let mut m = Model::new(name);
    m.cases[0].self_weight = false;
    m
}

fn general(m: &mut Model, a: f64, iy: f64, iz: f64, j: f64) -> Id {
    let id = m.next_section_id();
    m.sections.push(Section { id, name: format!("G{id}"), shape: Shape::General { a, iy, iz, j, shear: None } });
    id
}

fn mem(m: &mut Model, a: [f64; 3], b: [f64; 3], s: Id, mat: Id) -> Id {
    let (i, j) = (m.node_at(a), m.node_at(b));
    let id = m.next_member_id();
    m.members.push(Member { id, i, j, section: s, material: mat, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    id
}

fn sup(m: &mut Model, p: [f64; 3], fix: [bool; 6]) -> Id {
    let n = m.find_node(p).expect("support at a node");
    m.supports.retain(|s| s.node != n);
    m.supports.push(Support { node: n, fix, springs: [0.0; 6], uplift: false });
    n
}

const FIXED: [bool; 6] = [true; 6];
/// Pinned in the x-z plane: translations held, rotation about y free.
const PIN: [bool; 6] = [true, true, true, true, false, true];
/// A roller holding z only.
const ROLLER_Z: [bool; 6] = [false, false, true, false, false, false];

fn line(m: &mut Model, case: Id, member: Id, wi: [f64; 3], wj: [f64; 3]) {
    m.loads.push(Load { case, kind: LoadKind::Line { member, wi, wj, a: None, b: None } });
}
fn nodal(m: &mut Model, case: Id, node: Id, f: [f64; 6]) {
    m.loads.push(Load { case, kind: LoadKind::Nodal { node, f } });
}
fn node(m: &Model, p: [f64; 3]) -> Id {
    m.find_node(p).expect("node")
}
fn reaction(cr: &CaseResult, n: Id) -> [f64; 6] {
    cr.reactions.iter().find(|r| r.0 == n).map(|r| r.1).unwrap_or([0.0; 6])
}
/// The station nearest x along a member.
fn st_at(cr: &CaseResult, member: Id, x: f64) -> pondus_core::analysis::Station {
    let mr = cr.member(member).unwrap();
    mr.stations.iter().min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs())).unwrap().clone()
}
fn steel(m: &mut Model, e_mpa: f64) -> Id {
    let id = m.next_material_id();
    let mut mt = Material::steel(id, "steel", 250.0, 400.0);
    mt.e = e_mpa;
    mt.gamma = 0.0;
    m.materials.push(mt);
    id
}
/// Modal and response spectrum settings without code scaling: R = Ie = 1, no ELF scaling.
fn rsa(m: &mut Model, points: Vec<(f64, f64)>, damping: f64, combination: &str, max_modes: usize) {
    m.seismic.enabled = true;
    m.seismic.live_mass = 0.0;
    m.seismic.r = 1.0;
    m.seismic.ie = 1.0;
    m.seismic.elf_ratio = 0.0;
    m.seismic.damping = damping;
    m.seismic.combination = combination.into();
    m.seismic.max_modes = max_modes;
    m.seismic.mass_target = 0.999;
    m.seismic.spectrum = pondus_core::seismic::Spectrum::Table { points };
}
fn sap(p: &str) -> String {
    format!("{SAP}, Example {p}")
}
fn etabs(p: &str) -> String {
    format!("{ETABS}, Example {p}")
}

// =============================================================== SAP2000 1-001

/// A three-member frame under seven load cases (self-weight, span and joint loads, loads on a
/// member's projection and in its local axes, trapezoids, a point load); independent: unit
/// load method (Cook & Young 1985). Bending only (A x 1000).
#[test]
fn sap_1_001_general_loading() {
    let src = sap("1-001 (unit load method, Cook & Young 1985)");
    let case = "SAP2000 1-001 three-member frame";
    let (p1, p2, p3, p4) = ([0.0, 0.0, 0.0], [0.0, 0.0, 10.0 * FT], [8.0 * FT, 0.0, 16.0 * FT], [-4.0 * FT, 0.0, 10.0 * FT]);
    let w = |k: f64| k * KIP / FT; // kip/ft to kN/m
    let want = [(-0.02639, -0.02639), (0.06296, 0.06296), (0.06296, 0.06296), (-0.29630, -0.29630), (0.31250, 0.31250), (0.11556, 0.11556), (0.00651, 0.00651)];
    for (k, (indep, sap_v)) in want.iter().enumerate() {
        let mut m = bare("1-001");
        let st = steel(&mut m, 3600.0 * KSI);
        let i = 1728.0 * IN.powi(4);
        let s = general(&mut m, 144.0 * IN * IN * 1000.0, i, i, 2.0 * i);
        let m1 = mem(&mut m, p1, p2, s, st);
        let m2 = mem(&mut m, p2, p3, s, st);
        let m3 = mem(&mut m, p4, p2, s, st);
        sup(&mut m, p1, PIN);
        sup(&mut m, p3, [true, false, false, false, false, false]);
        let (n2, n4) = (node(&m, p2), node(&m, p4));
        match k {
            0 => {
                for x in [m1, m2, m3] {
                    line(&mut m, 1, x, [0.0, 0.0, -w(0.15)], [0.0, 0.0, -w(0.15)]);
                }
            }
            1 => {
                line(&mut m, 1, m3, [0.0, 0.0, -w(1.8)], [0.0, 0.0, -w(1.8)]);
                nodal(&mut m, 1, n4, [0.0, 0.0, -10.0 * KIP, 0.0, 0.0, 0.0]);
            }
            // the statically equivalent joint force and moment (counter-clockwise in the x-z
            // view, which is a negative moment about +y)
            2 => nodal(&mut m, 1, n2, [0.0, 0.0, -17.2 * KIP, 0.0, -54.4 * KIP * FT, 0.0]),
            // 2 k/ft on the horizontal projection: 1.6 k/ft along the 10 ft member
            3 => line(&mut m, 1, m2, [0.0, 0.0, -w(1.6)], [0.0, 0.0, -w(1.6)]),
            // 2 k/ft in local axes: +x on the column, normal (0.6, −0.8) on the inclined member
            4 => {
                line(&mut m, 1, m1, [w(2.0), 0.0, 0.0], [w(2.0), 0.0, 0.0]);
                line(&mut m, 1, m2, [w(1.2), 0.0, -w(1.6)], [w(1.2), 0.0, -w(1.6)]);
            }
            5 => {
                line(&mut m, 1, m1, [w(0.9984), 0.0, 0.0], [w(0.3744), 0.0, 0.0]);
                line(&mut m, 1, m2, [w(0.6 * 0.3744), 0.0, -w(0.8 * 0.3744)], [0.0; 3]);
            }
            _ => m.loads.push(Load { case: 1, kind: LoadKind::Point { member: m2, a: 5.0 * FT, p: [9.0 * KIP, 0.0, -12.0 * KIP] } }),
        }
        let a = analyze(&m).unwrap();
        let cr = a.case(1).unwrap();
        let (q, got) = if k < 4 { ("Uz joint 3", cr.displacements[&node(&m, p3)][2] / IN) } else { ("Ux joint 2", cr.displacements[&n2][0] / IN) };
        let c = format!("{case}, load case {}", k + 1);
        row("CSI frame statics", &c, &src, q, "in", *indep, got, 0.1, "published");
        program(&c, q, "in", *indep, "SAP2000", *sap_v, got);
    }
}

// =============================================================== SAP2000 1-002

/// Temperature: a 10 in bar, 2 × 3 in, E 29 000 ksi, α 6.5e-6 /°F, as a cantilever (A) and a
/// propped cantilever (B), under a uniform 20 °F, a rise varying from 0 to 20 °F along the
/// length (modelled as ten members, each at its mean) and a gradient of 60 °F across the 3 in
/// depth (top hotter). Independent: αΔT L, EAαΔT and Roark Table 3 items 6a and 6c.
#[test]
fn sap_1_002_temperature() {
    let src = sap("1-002 (thermal expansion; Roark & Young 1975, Table 3, 6a and 6c)");
    let l = 10.0 * IN;
    let f_to_c = 5.0 / 9.0;
    for model_b in [false, true] {
        for case_no in 1..=3 {
            let mut m = bare("1-002");
            let st = steel(&mut m, 29_000.0 * KSI);
            m.materials.iter_mut().find(|x| x.id == st).unwrap().alpha = 6.5e-6 * 1.8;
            // exact inches (rect_section rounds to whole millimetres: 76.2 mm would become 76)
            let s = general(&mut m, 6.0 * IN * IN, 4.5 * IN.powi(4), 2.0 * IN.powi(4), 4.0 * IN.powi(4));
            // the rise varying along the length as two halves at their mean temperatures, exact
            // for the free elongation (its integral) and for the restrained force (its mean)
            let n = if case_no == 2 { 2 } else { 1 };
            let mut ids = vec![];
            for k in 0..n {
                ids.push(mem(&mut m, [l * k as f64 / n as f64, 0.0, 0.0], [l * (k + 1) as f64 / n as f64, 0.0, 0.0], s, st));
            }
            sup(&mut m, [0.0, 0.0, 0.0], FIXED);
            if model_b {
                sup(&mut m, [l, 0.0, 0.0], [true, true, true, false, false, false]);
            }
            for (k, id) in ids.iter().enumerate() {
                let (dt, dt_z) = match case_no {
                    1 => (20.0 * f_to_c, 0.0),
                    2 => ((k as f64 + 0.5) * 10.0 * f_to_c, 0.0),
                    _ => (0.0, 60.0 * f_to_c),
                };
                m.loads.push(Load { case: 1, kind: LoadKind::Thermal { member: *id, dt, dt_y: 0.0, dt_z } });
            }
            let a = analyze(&m).unwrap_or_else(|e| panic!("1-002: {e}"));
            let cr = a.case(1).unwrap();
            let tip = node(&m, [l, 0.0, 0.0]);
            let c = format!("SAP2000 1-002 model {}, case {case_no}", if model_b { "B" } else { "A" });
            let (q, unit, indep, got) = match (model_b, case_no) {
                (false, 3) => ("Uz free end", "in", -0.00650, cr.displacements[&tip][2] / IN),
                (false, _) => ("Ux free end", "in", if case_no == 1 { 0.00130 } else { 0.00065 }, cr.displacements[&tip][0] / IN),
                (true, 3) => ("Fz at the prop", "kip", 2.545, reaction(cr, tip)[2] / KIP),
                (true, _) => ("Fx at the prop", "kip", if case_no == 1 { -22.62 } else { -11.31 }, reaction(cr, tip)[0] / KIP),
            };
            row("CSI frame statics", &c, &src, q, unit, indep, got, 0.2, "published");
            program(&c, q, unit, indep, "SAP2000", indep, got);
        }
    }
}

// =============================================================== SAP2000 1-005

/// A portal 144 × 144 in, E 29 000 ksi, A 144 in² (x 1e5), I 1728 in⁴, fixed at joint 1, under
/// imposed support displacements: (A) −0.5 in at the roller at joint 4, (B) −0.2 in at the
/// grounded end of a 10 k/in spring at joint 4 (the spring force k·δ = −2 k applied at the
/// joint), (C) a 0.01 rad rotation of the fixed joint 1, (D) a 0.01 rad rotation of the
/// grounded end of an 80 000 k·in/rad rotational spring at a pinned joint 4 (the moment k·θ
/// applied at the joint; the source's figure shows only the spring, and a joint with no
/// translational restraint reproduces both printed values). Independent: unit load method. The skewed supports
/// of models E and F are not a Pondus feature.
#[test]
fn sap_1_005_support_displacements() {
    let src = sap("1-005 (unit load method, Cook & Young 1985)");
    let h = 144.0 * IN;
    let build = |j4: Support| {
        let mut m = bare("1-005");
        let st = steel(&mut m, 29_000.0 * KSI);
        let i = 1728.0 * IN.powi(4);
        let s = general(&mut m, 144.0 * IN * IN * 1e5, i, i, 2.0 * i);
        mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, h], s, st);
        mem(&mut m, [0.0, 0.0, h], [h, 0.0, h], s, st);
        mem(&mut m, [h, 0.0, 0.0], [h, 0.0, h], s, st);
        sup(&mut m, [0.0, 0.0, 0.0], FIXED);
        let n4 = node(&m, [h, 0.0, 0.0]);
        m.supports.push(Support { node: n4, ..j4 });
        m
    };
    let n = |m: &Model, x: f64| node(m, [x, 0.0, 0.0]);
    let report = |m: &Model, label: &str, rows: &[(&str, &str, f64, f64)]| {
        for (q, unit, indep, got) in rows {
            let c = format!("SAP2000 1-005 model {label}");
            row("CSI frame statics", &c, &src, q, unit, *indep, *got, 0.1, "published");
            program(&c, q, unit, *indep, "SAP2000", *indep, *got);
        }
        let _ = m;
    };
    // A: settlement of the roller
    let mut m = build(Support { node: 0, fix: ROLLER_Z, springs: [0.0; 6], uplift: false });
    let n4 = n(&m, h);
    m.loads.push(Load { case: 1, kind: LoadKind::Settlement { node: n4, d: [0.0, 0.0, -0.5 * IN, 0.0, 0.0, 0.0] } });
    let a = analyze(&m).unwrap();
    let r1 = reaction(a.case(1).unwrap(), n(&m, 0.0));
    report(&m, "A (settlement 0.5 in)", &[("Fz joint 1", "kip", 6.293, r1[2] / KIP), ("My joint 1", "kip-in", -906.250, r1[4] / (KIP * IN))]);
    // B: spring with a grounded-end displacement = the spring force at the joint
    let mut m = build(Support { node: 0, fix: [false; 6], springs: [0.0, 0.0, 10.0 * KIP / IN, 0.0, 0.0, 0.0], uplift: false });
    let n4 = n(&m, h);
    nodal(&mut m, 1, n4, [0.0, 0.0, 10.0 * KIP / IN * (-0.2 * IN), 0.0, 0.0, 0.0]);
    let a = analyze(&m).unwrap();
    let r1 = reaction(a.case(1).unwrap(), n(&m, 0.0));
    report(&m, "B (spring end displaced 0.2 in)", &[("Fz joint 1", "kip", 1.115, r1[2] / KIP), ("My joint 1", "kip-in", -160.492, r1[4] / (KIP * IN))]);
    // C: rotation of the fixed support
    let mut m = build(Support { node: 0, fix: ROLLER_Z, springs: [0.0; 6], uplift: false });
    let n1 = n(&m, 0.0);
    m.loads.push(Load { case: 1, kind: LoadKind::Settlement { node: n1, d: [0.0, 0.0, 0.0, 0.0, 0.01, 0.0] } });
    let a = analyze(&m).unwrap();
    let r1 = reaction(a.case(1).unwrap(), n1);
    report(&m, "C (rotation 0.01 rad of joint 1)", &[("Fz joint 1", "kip", -18.125, r1[2] / KIP), ("My joint 1", "kip-in", 2610.0, r1[4] / (KIP * IN))]);
    // D: a rotational spring alone at joint 4 (no translational restraint), its grounded end
    // rotated 0.01 rad
    let kr = 80_000.0 * KIP * IN;
    let mut m = build(Support { node: 0, fix: [false; 6], springs: [0.0, 0.0, 0.0, 0.0, kr, 0.0], uplift: false });
    let n4 = n(&m, h);
    nodal(&mut m, 1, n4, [0.0, 0.0, 0.0, 0.0, kr * 0.01, 0.0]);
    let a = analyze(&m).unwrap();
    let cr = a.case(1).unwrap();
    let r1 = reaction(cr, n(&m, 0.0));
    report(&m, "D (rotational spring end rotated 0.01 rad)", &[("My joint 1", "kip-in", -473.469, r1[4] / (KIP * IN)), ("Ry joint 4", "rad", 0.00408, cr.displacements[&n4][4])]);
}

// =============================================================== SAP2000 1-007

/// End releases, model C: an L-frame (fixed column 10 ft, beam 10 ft on a roller) with a
/// moment release at the beam's end at the corner and 10 k at mid-span: Fz = 5 k and My = 0 at
/// the fixed support (statics). Shear and axial releases (models A, B) are not Pondus features.
#[test]
fn sap_1_007_moment_release() {
    let src = sap("1-007 (statics)");
    let mut m = bare("1-007");
    let st = steel(&mut m, 29_000.0 * KSI);
    let s = general(&mut m, 0.01, 1e-4, 1e-4, 1e-4);
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, 10.0 * FT], s, st);
    let b = mem(&mut m, [0.0, 0.0, 10.0 * FT], [10.0 * FT, 0.0, 10.0 * FT], s, st);
    m.members.iter_mut().find(|x| x.id == b).unwrap().hinge_i = true;
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [10.0 * FT, 0.0, 10.0 * FT], [false, true, true, true, false, false]);
    m.loads.push(Load { case: 1, kind: LoadKind::Point { member: b, a: 5.0 * FT, p: [0.0, 0.0, -10.0 * KIP] } });
    let a = analyze(&m).unwrap();
    let r = reaction(a.case(1).unwrap(), node(&m, [0.0, 0.0, 0.0]));
    let c = "SAP2000 1-007 model C (moment release)";
    row("CSI frame statics", c, &src, "Fz fixed support", "kip", 5.0, r[2] / KIP, 0.01, "published");
    row("CSI frame statics", c, &src, "My fixed support (absolute, kip-ft)", "kip-ft", 0.0, r[4] / (KIP * FT), 1e-6, "published");
}

// =============================================================== SAP2000 1-012

/// No-tension and no-compression members: a 120 × 120 in braced bent, A 8 in², E 30 000 ksi,
/// pinned bases, the beam and both braces pinned at their ends (the braces do not meet at
/// their crossing), 100 k in +x at joint 2; (A) linear, (B) no compression in brace 5 (3–2),
/// (C) no tension in brace 4 (1–4). Independent: unit load method and statics.
#[test]
fn sap_1_012_tension_compression_only() {
    let src = sap("1-012 (unit load method; statics)");
    let d = 120.0 * IN;
    let (p1, p2, p3, p4) = ([0.0, 0.0, 0.0], [0.0, 0.0, d], [d, 0.0, 0.0], [d, 0.0, d]);
    let want = [
        ("A (linear)", [0.10677, -44.224, -100.0, -55.776, 100.0]),
        ("B (brace 5 no compression)", [0.24142, -100.0, -100.0, 0.0, 100.0]),
        ("C (brace 4 no tension)", [0.19142, 0.0, -100.0, -100.0, 100.0]),
    ];
    for (k, (label, v)) in want.iter().enumerate() {
        let mut m = bare("1-012");
        let st = steel(&mut m, 30_000.0 * KSI);
        let s = general(&mut m, 8.0 * IN * IN, 100.0 * IN.powi(4), 100.0 * IN.powi(4), 10.0 * IN.powi(4));
        mem(&mut m, p1, p2, s, st);
        mem(&mut m, p3, p4, s, st);
        let beam = mem(&mut m, p2, p4, s, st);
        let b4 = mem(&mut m, p1, p4, s, st);
        let b5 = mem(&mut m, p3, p2, s, st);
        for x in m.members.iter_mut().filter(|x| [beam, b4, b5].contains(&x.id)) {
            x.truss = true;
        }
        match k {
            1 => m.members.iter_mut().find(|x| x.id == b5).unwrap().behaviour = Behaviour::TensionOnly,
            2 => m.members.iter_mut().find(|x| x.id == b4).unwrap().behaviour = Behaviour::CompressionOnly,
            _ => {}
        }
        sup(&mut m, p1, PIN);
        sup(&mut m, p3, PIN);
        let n2 = node(&m, p2);
        nodal(&mut m, 1, n2, [100.0 * KIP, 0.0, 0.0, 0.0, 0.0, 0.0]);
        // a nonlinear member is solved per combination: one combination of the case
        m.combos = vec![Combination { id: 1, name: "LOAD".into(), kind: ComboKind::Uls, factors: vec![(1, 1.0)] }];
        let a = analyze(&m).unwrap();
        let cr = a.result(&m, "LOAD").unwrap();
        let (r1, r3) = (reaction(&cr, node(&m, p1)), reaction(&cr, node(&m, p3)));
        let got = [cr.displacements[&n2][0] / IN, r1[0] / KIP, r1[2] / KIP, r3[0] / KIP, r3[2] / KIP];
        let names = ["Ux joint 2", "Fx joint 1", "Fz joint 1", "Fx joint 3", "Fz joint 3"];
        let c = format!("SAP2000 1-012 model {label}");
        for q in 0..5 {
            let (unit, tol) = if q == 0 { ("in", 0.1) } else if v[q] == 0.0 { ("kip", 1e-6) } else { ("kip", 0.1) };
            row("CSI nonlinear members", &c, &src, names[q], unit, v[q], got[q], tol, "published");
        }
    }
}

// =============================================================== SAP2000 1-013

/// A simply supported beam on an elastic foundation: 15 ft, 36 × 36 in, E 3600 ksi, k = 3 ft ×
/// 800 k/ft³ = 2400 k/ft², 500 k at mid-span. Independent: Timoshenko (1956), Problem 3, p. 23.
#[test]
fn sap_1_013_beam_on_elastic_foundation() {
    let src = sap("1-013 (Timoshenko 1956, Problem 3, p. 23)");
    let mut m = bare("1-013");
    let st = steel(&mut m, 3600.0 * KSI);
    let i = 139_968.0 * IN.powi(4);
    let s = general(&mut m, 1296.0 * IN * IN * 1000.0, i, i, i);
    let l = 15.0 * FT;
    let b1 = mem(&mut m, [0.0, 0.0, 0.0], [l / 2.0, 0.0, 0.0], s, st);
    let b2 = mem(&mut m, [l / 2.0, 0.0, 0.0], [l, 0.0, 0.0], s, st);
    let k = 2400.0 * KIP / (FT * FT);
    for x in m.members.iter_mut() {
        x.winkler = Some([k, 0.0]);
    }
    sup(&mut m, [0.0, 0.0, 0.0], PIN);
    sup(&mut m, [l, 0.0, 0.0], [false, true, true, true, false, false]);
    let mid = node(&m, [l / 2.0, 0.0, 0.0]);
    nodal(&mut m, 1, mid, [0.0, 0.0, -500.0 * KIP, 0.0, 0.0, 0.0]);
    let a = analyze_with(&m, 64, 2).unwrap();
    let cr = a.case(1).unwrap();
    let _ = b2;
    let c = "SAP2000 1-013 beam on elastic foundation";
    let uz = cr.displacements[&mid][2] / IN;
    let my = st_at(cr, b1, l / 2.0).my / (KIP * IN);
    row("CSI foundations", c, &src, "Uz at mid-span", "in", -0.08933, uz, 0.2, "published");
    row("CSI foundations", c, &src, "My at mid-span", "kip-in", 17_698.0, my, 0.2, "published");
    program(c, "Uz at mid-span (100 elements)", "in", -0.08933, "SAP2000", -0.08933, uz);
    program(c, "My at mid-span (100 elements)", "kip-in", 17_698.0, "SAP2000", 17_698.0, my);
}

// =============================================================== SAP2000 1-014

/// The first five bending periods of a 96 in cantilever, 12 × 18 in, E 3600 ksi, ρ 2.3e-7
/// k·s²/in⁴ (independent: Clough & Penzien 1975, uniform cantilever). Pondus's mass comes from
/// the D case: a unit weight γ = ρ g; 32 elements, consistent mass.
#[test]
fn sap_1_014_cantilever_eigenvalues() {
    let src = sap("1-014 (Clough & Penzien 1975, uniform cantilever)");
    let mut m = Model::new("1-014");
    let rho = 2.3e-7 * KSI_MASS / IN.powi(3); // t/m³
    m.materials[0].e = 3600.0 * KSI;
    m.materials[0].gamma = rho * pondus_core::seismic::G;
    let s = m.rect_section(12.0 * IN, 18.0 * IN);
    mem(&mut m, [0.0, 0.0, 0.0], [96.0 * IN, 0.0, 0.0], s, 1);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    rsa(&mut m, vec![(0.0, 0.1), (10.0, 0.1)], 0.05, "cqc", 30);
    let a = analyze_with(&m, 32, 2).unwrap();
    let sr = a.seismic.as_ref().unwrap_or_else(|| panic!("no seismic result: {:?}", a.warnings));
    let mut t: Vec<f64> = sr.modes.iter().filter(|md| md.mass_ratio[1] > 1e-4 || md.mass_ratio[2] > 1e-4).map(|md| md.period).collect();
    t.sort_by(|a, b| b.total_cmp(a));
    let want = [(0.038005, 0.038003), (0.025337, 0.025335), (0.006064, 0.006065), (0.004043, 0.004043), (0.002165, 0.002166)];
    let c = "SAP2000 1-014 cantilever";
    for (k, (indep, sap_g)) in want.iter().enumerate() {
        let q = format!("T{} (bending)", k + 1);
        row("CSI modal", c, &src, &q, "s", *indep, t[k], 0.2, "published");
        program(c, &q, "s", *indep, "SAP2000 (96 elements)", *sap_g, t[k]);
    }
}

// =============================================================== SAP2000 1-016

/// Tension stiffening: a 300 in simple span, 3 × 3 in, E 30 000 ksi, 0.002 k/in, without and
/// with 20.25 k of axial tension (P-Delta). Independent: Timoshenko (1956) eqs. 43 and 45.
#[test]
fn sap_1_016_tension_stiffening() {
    let src = sap("1-016 (Timoshenko 1956, eqs. 43, 45 and 23)");
    let l = 300.0 * IN;
    for stiffened in [false, true] {
        let mut m = bare("1-016");
        let st = steel(&mut m, 30_000.0 * KSI);
        let s = general(&mut m, 9.0 * IN * IN, 6.75 * IN.powi(4), 6.75 * IN.powi(4), 11.4 * IN.powi(4));
        let b1 = mem(&mut m, [0.0, 0.0, 0.0], [l / 2.0, 0.0, 0.0], s, st);
        let b2 = mem(&mut m, [l / 2.0, 0.0, 0.0], [l, 0.0, 0.0], s, st);
        sup(&mut m, [0.0, 0.0, 0.0], PIN);
        sup(&mut m, [l, 0.0, 0.0], [false, true, true, true, false, false]);
        let w = 0.002 * KIP / IN;
        for b in [b1, b2] {
            line(&mut m, 1, b, [0.0, 0.0, -w], [0.0, 0.0, -w]);
        }
        let end = node(&m, [l, 0.0, 0.0]);
        if stiffened {
            nodal(&mut m, 1, end, [20.25 * KIP, 0.0, 0.0, 0.0, 0.0, 0.0]);
            m.design.second_order = true;
        }
        m.combos = vec![Combination { id: 1, name: "LOAD".into(), kind: ComboKind::Uls, factors: vec![(1, 1.0)] }];
        let a = analyze(&m).unwrap();
        let cr = if stiffened { a.second.get(&1).expect("P-Delta result").0.clone() } else { a.case(1).unwrap().clone() };
        let mid = node(&m, [l / 2.0, 0.0, 0.0]);
        let uz = cr.displacements[&mid][2] / IN;
        let my = st_at(&cr, b1, l / 2.0).my / (KIP * IN);
        let (wu, wm) = if stiffened { (-0.54330, 11.498) } else { (-1.04167, 22.500) };
        let c = format!("SAP2000 1-016 {}", if stiffened { "with 20.25 k tension (P-Delta)" } else { "without tension" });
        row("CSI second order", &c, &src, "Uz at mid-span", "in", wu, uz, 0.2, "published");
        // P-Delta member moments are recovered from cubic elements with their geometric end
        // forces, which converges with the square of the element length; with the tension at
        // 91 % of the Euler load this shows: +1.8 % at the default 8 elements per member
        let tol = if stiffened { 2.0 } else { 0.2 };
        row("CSI second order", &c, &src, "My at mid-span (default mesh, 8 elements per member)", "kip-in", wm, my, tol, "published");
        if stiffened {
            let a = analyze_with(&m, 128, 2).unwrap();
            let my = st_at(&a.second.get(&1).unwrap().0, b1, l / 2.0).my / (KIP * IN);
            row("CSI second order", &c, &src, "My at mid-span (128 elements per member)", "kip-in", wm, my, 0.05, "published");
            program(&c, "My at mid-span (128 elements per member)", "kip-in", wm, "SAP2000 (16 elements)", 11.498, my);
        }
    }
}

// =============================================================== SAP2000 1-018

/// Bending, shear and axial deformation of a portal: columns 144 in, a 288 in beam under 0.1
/// k/in, W8X31 (A 9.12 in², I 110 in⁴, Av 2.28 in²), E 29 900 ksi, G 11 500 ksi; pinned at
/// joint 1 and on a roller at joint 3. Mid-span deflection with every deformation (A), bending
/// only (B), shear only (C) and axial only (D), by switching the properties as the example
/// does; A and C need Pondus's frame shear deformation (`design.shear_deformation`).
/// Independent: unit load method.
#[test]
fn sap_1_018_bending_shear_axial() {
    let src = sap("1-018 (unit load method, Cook & Young 1985)");
    let (h, l) = (144.0 * IN, 288.0 * IN);
    for (label, a_f, i_f, shear, want) in [("A bending + shear + axial", 1.0, 1.0, true, -2.77076), ("B bending only", 1e5, 1.0, false, -2.72361), ("C shear only", 1e5, 1e7, true, -0.03954), ("D axial only", 1.0, 1e7, false, -0.00760)] {
        let mut m = bare("1-018");
        let st = steel(&mut m, 29_900.0 * KSI);
        m.materials.iter_mut().find(|x| x.id == st).unwrap().nu = 29_900.0 / (2.0 * 11_500.0) - 1.0;
        let i = 110.0 * IN.powi(4) * i_f;
        let id = m.next_section_id();
        m.sections.push(Section { id, name: "W8X31".into(), shape: Shape::General { a: 9.12 * IN * IN * a_f, iy: i, iz: i, j: i, shear: Some([0.0, 2.28 * IN * IN]) } });
        m.design.shear_deformation = shear;
        mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, h], id, st);
        mem(&mut m, [l, 0.0, 0.0], [l, 0.0, h], id, st);
        let b1 = mem(&mut m, [0.0, 0.0, h], [l / 2.0, 0.0, h], id, st);
        let b2 = mem(&mut m, [l / 2.0, 0.0, h], [l, 0.0, h], id, st);
        for b in [b1, b2] {
            line(&mut m, 1, b, [0.0, 0.0, -0.1 * KIP / IN], [0.0, 0.0, -0.1 * KIP / IN]);
        }
        sup(&mut m, [0.0, 0.0, 0.0], PIN);
        sup(&mut m, [l, 0.0, 0.0], [false, true, true, true, false, true]);
        let a = analyze(&m).unwrap();
        let uz = a.case(1).unwrap().displacements[&node(&m, [l / 2.0, 0.0, h])][2] / IN;
        let c = format!("SAP2000 1-018 model {label}");
        row("CSI frame statics", &c, &src, "Uz joint 5 (mid-span)", "in", want, uz, 0.1, "published");
        program(&c, "Uz joint 5", "in", want, "SAP2000", want, uz);
    }
}

// =============================================================== SAP2000 1-019

/// Buckling of a portal 144 × 144 in with pinned bases, W8X31 (I 110 in⁴, A x 1e5), E 29 000
/// ksi (the property box prints 29 900; the hand calculation and SAP2000 use 29 000): kl tan kl
/// = 6, Pcr = (kl)² EI/h² = 280.19 k (Timoshenko & Gere 1961, Art. 2.4).
#[test]
fn sap_1_019_portal_buckling() {
    let src = sap("1-019 (Timoshenko & Gere 1961, Art. 2.4)");
    let h = 144.0 * IN;
    let mut m = bare("1-019");
    let st = steel(&mut m, 29_000.0 * KSI);
    let i = 110.0 * IN.powi(4);
    let s = general(&mut m, 9.12 * IN * IN * 1e5, i, 10.0 * i, 10.0 * i);
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, h], s, st);
    mem(&mut m, [h, 0.0, 0.0], [h, 0.0, h], s, st);
    mem(&mut m, [0.0, 0.0, h], [h, 0.0, h], s, st);
    sup(&mut m, [0.0, 0.0, 0.0], PIN);
    sup(&mut m, [h, 0.0, 0.0], PIN);
    for x in [0.0, h] {
        let n = node(&m, [x, 0.0, h]);
        nodal(&mut m, 1, n, [0.0, 0.0, -100.0 * KIP, 0.0, 0.0, 0.0]);
    }
    m.combos = vec![Combination { id: 1, name: "P".into(), kind: ComboKind::Uls, factors: vec![(1, 1.0)] }];
    let a = analyze_with(&m, 8, 8).unwrap();
    let modes = pondus_core::stability::buckling(&m, &a, &m.combos[0], 3).unwrap();
    let pcr = modes[0].factor * 100.0;
    let c = "SAP2000 1-019 pinned portal";
    row("CSI stability", c, &src, "Pcr per column (kl tan kl = 6)", "kip", 280.19, pcr, 0.1, "published");
    program(c, "Pcr (4 elements per member)", "kip", 280.19, "SAP2000", 280.19, pcr);
}

// =============================================================== SAP2000 1-020

/// Response spectrum of a two-storey plane frame with lumped masses at the beam mid-spans (2m
/// and m, m = 0.5182 k·s²/in, x only), columns and beams of EI and 2EI (E 3000 ksi, I 1000
/// in⁴), axially rigid, SRSS (independent: Chopra 1995, Example 13.11).
#[test]
fn sap_1_020_response_spectrum_2d() {
    let src = sap("1-020 (Chopra 1995, Example 13.11)");
    let (h, l) = (120.0 * IN, 240.0 * IN);
    let mut m = bare("1-020");
    let st = steel(&mut m, 3000.0 * KSI);
    let i1 = 1000.0 * IN.powi(4);
    // axially rigid by area (1 m² against I = 0.0004 m⁴); a weightless member still gets 1
    // kg/m³, so the area is kept moderate: 1 kg/m against the lumped 91 t
    let s1 = general(&mut m, 1.0, i1, 10.0 * i1, 10.0 * i1);
    let s2 = general(&mut m, 1.0, 2.0 * i1, 20.0 * i1, 20.0 * i1);
    let c1 = mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, h], s2, st);
    let c2 = mem(&mut m, [0.0, 0.0, h], [0.0, 0.0, 2.0 * h], s1, st);
    mem(&mut m, [l, 0.0, 0.0], [l, 0.0, h], s2, st);
    mem(&mut m, [l, 0.0, h], [l, 0.0, 2.0 * h], s1, st);
    let b5 = mem(&mut m, [0.0, 0.0, h], [l / 2.0, 0.0, h], s2, st);
    let b6 = mem(&mut m, [l / 2.0, 0.0, h], [l, 0.0, h], s2, st);
    let b7 = mem(&mut m, [0.0, 0.0, 2.0 * h], [l / 2.0, 0.0, 2.0 * h], s1, st);
    let b8 = mem(&mut m, [l / 2.0, 0.0, 2.0 * h], [l, 0.0, 2.0 * h], s1, st);
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [l, 0.0, 0.0], FIXED);
    let mm = 0.5182 * KSI_MASS;
    let (n7, n8) = (node(&m, [l / 2.0, 0.0, h]), node(&m, [l / 2.0, 0.0, 2.0 * h]));
    m.masses = vec![NodalMass { node: n7, m: [2.0 * mm, 0.0, 0.0], r: [0.0; 3] }, NodalMass { node: n8, m: [mm, 0.0, 0.0], r: [0.0; 3] }];
    let sa = vec![(0.0, 0.5), (0.03, 0.5), (0.125, 1.355), (0.5868, 1.355), (0.66, 1.355), (1.562, 0.576), (4.12, 0.218), (10.0, 0.037)];
    rsa(&mut m, sa, 0.05, "srss", 2);
    let a = analyze(&m).unwrap();
    let sr = a.seismic.as_ref().unwrap_or_else(|| panic!("no seismic result: {:?}", a.warnings));
    let mut t: Vec<f64> = sr.modes.iter().filter(|md| md.mass_ratio[0] > 1e-3).map(|md| md.period).collect();
    t.sort_by(|a, b| b.total_cmp(a));
    let ex = sr.case("Ex").expect("Ex");
    let c = "SAP2000 1-020 two-storey frame, SRSS";
    let n2 = node(&m, [0.0, 0.0, h]);
    let n3 = node(&m, [0.0, 0.0, 2.0 * h]);
    let kin = KIP * IN;
    let rows: Vec<(&str, &str, f64, f64, f64)> = vec![
        ("T1", "s", 1.562, 1.562, t[0]),
        ("T2", "s", 0.5868, 0.5868, t[1]),
        ("Ux joint 2", "in", 7.566, 7.576, ex.displacements[&n2][0] / IN),
        ("Ux joint 3", "in", 18.81, 18.84, ex.displacements[&n3][0] / IN),
        ("M first-storey column at joint 1", "kip-in", 12_624.0, 12_636.0, st_at(ex, c1, 0.0).my.abs() / kin),
        ("M first-storey column at joint 2", "kip-in", 6_792.0, 6_793.0, st_at(ex, c1, h).my.abs() / kin),
        ("M second-storey column at joint 2", "kip-in", 6_024.0, 6_023.0, st_at(ex, c2, 0.0).my.abs() / kin),
        ("M second-storey column at joint 3", "kip-in", 5_220.0, 5_222.0, st_at(ex, c2, h).my.abs() / kin),
        ("M first-floor beam at joint 2", "kip-in", 9_792.0, 9_810.0, st_at(ex, b5, 0.0).my.abs() / kin),
        ("M first-floor beam at joint 5", "kip-in", 9_792.0, 9_810.0, st_at(ex, b6, l / 2.0).my.abs() / kin),
        ("M roof beam at joint 3", "kip-in", 5_220.0, 5_222.0, st_at(ex, b7, 0.0).my.abs() / kin),
        ("M roof beam at joint 6", "kip-in", 5_220.0, 5_222.0, st_at(ex, b8, l / 2.0).my.abs() / kin),
    ];
    for (q, unit, indep, sap_v, got) in rows {
        row("CSI response spectrum", c, &src, q, unit, indep, got, 0.5, "published");
        program(c, q, unit, indep, "SAP2000", sap_v, got);
    }
}

// =============================================================== SAP2000 1-021 / ETABS 6

/// Bathe and Wilson's ten-bay, nine-storey plane frame (bays 20 ft, storeys 10 ft), E 432 000
/// k/ft², A 3 ft², I 1 ft⁴, mass 3 k·s²/ft per ft of every member: the first three eigenvalues
/// ω² of the in-plane modes (out of its plane the frame is made stiff). The mass is given as
/// a weight w = m g in the D case, which Pondus turns into a consistent mass.
#[test]
fn sap_1_021_bathe_wilson() {
    let src = "CSI, SAP2000 Software Verification, Example 1-021, and ETABS Example 6 (Bathe & Wilson 1972; Peterson 1981)";
    let mut m = Model::new("1-021");
    m.cases[0].self_weight = false;
    let st = steel(&mut m, 432_000.0 * KSF);
    let s = general(&mut m, 3.0 * FT * FT, 1.0 * FT.powi(4), 1e3 * FT.powi(4), 1e3 * FT.powi(4));
    let w = 3.0 * KSF_MASS / FT * pondus_core::seismic::G; // kN/m: (k·s²/ft per ft of member) x g
    let mut ids = vec![];
    for i in 0..=10 {
        for k in 0..9 {
            let x = 20.0 * FT * i as f64;
            ids.push(mem(&mut m, [x, 0.0, 10.0 * FT * k as f64], [x, 0.0, 10.0 * FT * (k + 1) as f64], s, st));
        }
    }
    for k in 1..=9 {
        for i in 0..10 {
            let z = 10.0 * FT * k as f64;
            ids.push(mem(&mut m, [20.0 * FT * i as f64, 0.0, z], [20.0 * FT * (i + 1) as f64, 0.0, z], s, st));
        }
    }
    for id in ids {
        line(&mut m, 1, id, [0.0, 0.0, -w], [0.0, 0.0, -w]);
    }
    for i in 0..=10 {
        sup(&mut m, [20.0 * FT * i as f64, 0.0, 0.0], FIXED);
    }
    rsa(&mut m, vec![(0.0, 0.1), (10.0, 0.1)], 0.05, "cqc", 12);
    m.seismic.mass_target = 0.0;
    let a = analyze_with(&m, 4, 2).unwrap();
    let sr = a.seismic.as_ref().unwrap_or_else(|| panic!("no seismic result: {:?}", a.warnings));
    // the in-plane modes: no out-of-plane displacement anywhere in the shape
    let in_plane = |md: &&pondus_core::seismic::ModeInfo| md.shape.values().all(|u| u[1].abs() < 1e-3);
    let mut om: Vec<f64> = sr.modes.iter().filter(in_plane).map(|md| (2.0 * std::f64::consts::PI * md.frequency).powi(2)).collect();
    om.sort_by(f64::total_cmp);
    let c = "SAP2000 1-021 / ETABS 6 Bathe-Wilson frame";
    for (k, (indep, sap_v, etabs_v)) in [(0.589541, 0.589541, 0.5895), (5.52695, 5.52696, 5.5270), (16.5878, 16.5879, 16.5879)].iter().enumerate() {
        let q = format!("eigenvalue ω²{}", k + 1);
        // the references lump the mass at the joints of one element per member; Pondus's
        // consistent mass is converged (one to eight elements per beam change ω²2 by 0.05 %),
        // and the two part with the mode number: +0.05, +0.4 and +1.1 %
        row("CSI modal", c, src, &q, "rad²/s²", *indep, om[k], 1.5, "published");
        program(c, &q, "rad²/s²", *indep, "SAP2000", *sap_v, om[k]);
        program(c, &q, "rad²/s²", *indep, "ETABS", *etabs_v, om[k]);
    }
}

// =============================================================== SAP2000 1-022 / ETABS 7

/// A seven-storey, two-bay plane moment frame with rigid floor diaphragms (bays 30 ft, storeys
/// 13.5, 13.5 and 13 ft x 5), W14 columns and W24 beams by their A and I, E 29 500 ksi: a static
/// lateral case, the seven periods with the mass 0.49 k·s²/in (x only) at the middle joints,
/// and the response spectrum of El Centro 1940 N-S (5 %, SRSS). Independent: published
/// program results (Engineering/Analysis Corporation; Computers/Structures International).
#[test]
fn sap_1_022_seven_storey_frame() {
    let src = "CSI, SAP2000 Software Verification, Example 1-022, and ETABS Example 7 (EAC / CSI reference results)";
    let z = [0.0, 13.5, 27.0, 40.0, 53.0, 66.0, 79.0, 92.0].map(|v| v * FT);
    let mut m = bare("1-022");
    let st = steel(&mut m, 29_500.0 * KSI);
    let sec = |m: &mut Model, a: f64, i: f64| general(m, a * IN * IN, i * IN.powi(4), i * IN.powi(4), 0.1 * i * IN.powi(4));
    let w176 = sec(&mut m, 51.7, 2150.0);
    let w211 = sec(&mut m, 62.1, 2670.0);
    let w246 = sec(&mut m, 72.3, 3230.0);
    let w287 = sec(&mut m, 84.4, 3910.0);
    let w110 = sec(&mut m, 32.5, 3330.0);
    let w130 = sec(&mut m, 38.3, 4020.0);
    let w160 = sec(&mut m, 47.1, 5120.0);
    let outer = [w246, w246, w246, w211, w211, w176, w176];
    let middle = [w287, w287, w287, w246, w246, w211, w211];
    let beams = [w160, w160, w130, w130, w110, w110, w110];
    let mut col1 = 0;
    for k in 0..7 {
        let c = mem(&mut m, [0.0, 0.0, z[k]], [0.0, 0.0, z[k + 1]], outer[k], st);
        if k == 0 {
            col1 = c;
        }
        mem(&mut m, [30.0 * FT, 0.0, z[k]], [30.0 * FT, 0.0, z[k + 1]], middle[k], st);
        mem(&mut m, [60.0 * FT, 0.0, z[k]], [60.0 * FT, 0.0, z[k + 1]], outer[k], st);
        mem(&mut m, [0.0, 0.0, z[k + 1]], [30.0 * FT, 0.0, z[k + 1]], beams[k], st);
        mem(&mut m, [30.0 * FT, 0.0, z[k + 1]], [60.0 * FT, 0.0, z[k + 1]], beams[k], st);
    }
    for x in [0.0, 30.0, 60.0] {
        sup(&mut m, [x * FT, 0.0, 0.0], FIXED);
    }
    m.diaphragms = z[1..].to_vec();
    let lat = [2.5, 5.0, 7.5, 10.0, 12.5, 15.0, 20.0];
    for k in 0..7 {
        let n = node(&m, [0.0, 0.0, z[k + 1]]);
        nodal(&mut m, 1, n, [lat[k] * KIP, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }
    let roof = node(&m, [0.0, 0.0, z[7]]);
    let base = node(&m, [0.0, 0.0, 0.0]);
    let a = analyze(&m).unwrap();
    let cr = a.case(1).unwrap();
    let c = "SAP2000 1-022 / ETABS 7 seven-storey frame, static LAT";
    let stat = [("Ux joint 22 (roof)", "in", 1.45076, cr.displacements[&roof][0] / IN), ("axial force, column 1", "kip", 69.99, st_at(cr, col1, 0.0).n.abs() / KIP), ("moment, column 1 at joint 1", "kip-in", 2324.68, reaction(cr, base)[4].abs() / (KIP * IN))];
    for (q, unit, indep, got) in stat {
        row("CSI frame statics", c, src, q, unit, indep, got, 0.1, "published");
        program(c, q, unit, indep, "SAP2000", indep, got);
    }
    // the dynamic model: lumped masses at the middle joints, x only
    m.loads.clear();
    m.masses = (1..=7).map(|k| NodalMass { node: node(&m, [30.0 * FT, 0.0, z[k]]), m: [0.49 * KSI_MASS, 0.0, 0.0], r: [0.0; 3] }).collect();
    let el_centro = vec![
        (0.0769, 0.505311), (0.0795, 0.519598), (0.0800, 0.520045), (0.0833, 0.518093), (0.0870, 0.493366), (0.0909, 0.477599), (0.0951, 0.527825), (0.0952, 0.530631), (0.1000, 0.581609), (0.1053, 0.564412), (0.1111, 0.523663), (0.1176, 0.572438),
        (0.1190, 0.588211), (0.1250, 0.627807), (0.1333, 0.665413), (0.1429, 0.636531), (0.1538, 0.905796), (0.1602, 0.804605), (0.1667, 0.787220), (0.1818, 0.943909), (0.2000, 1.005620), (0.2222, 0.746135), (0.2420, 0.704753), (0.2500, 0.798052),
        (0.2857, 0.718264), (0.3333, 0.880624), (0.4000, 0.882996), (0.4313, 0.921167), (0.5000, 1.046620), (0.6667, 0.641750), (1.0000, 0.482251), (1.2730, 0.258617), (2.0000, 0.160189),
    ];
    for comb in ["srss", "cqc"] {
        rsa(&mut m, el_centro.clone(), 0.05, comb, 7);
        let a = analyze(&m).unwrap();
        let sr = a.seismic.as_ref().unwrap_or_else(|| panic!("no seismic result: {:?}", a.warnings));
        let mut t: Vec<f64> = sr.modes.iter().filter(|md| md.mass_ratio[0] > 1e-4).map(|md| md.period).collect();
        t.sort_by(|a, b| b.total_cmp(a));
        if comb == "srss" {
            let c = "SAP2000 1-022 / ETABS 7 seven-storey frame, modes";
            for (k, want) in [1.2732, 0.4313, 0.2420, 0.1602, 0.1190, 0.0951, 0.0795].iter().enumerate() {
                let q = format!("T{}", k + 1);
                row("CSI modal", c, src, &q, "s", *want, t[k], 0.2, "published");
                program(c, &q, "s", *want, "SAP2000", *want, t[k]);
            }
        }
        let ex = sr.case("Ex").unwrap();
        let c = format!("SAP2000 1-022 / ETABS 7 seven-storey frame, response spectrum {}", comb.to_uppercase());
        let got = [ex.displacements[&roof][0] / IN, st_at(ex, col1, 0.0).n.abs() / KIP, st_at(ex, col1, 0.0).my.abs() / (KIP * IN)];
        let names = ["Ux joint 22 (roof)", "axial force, column 1", "moment, column 1 at joint 1"];
        let indep = [5.438, 261.8, 9868.0];
        let sap_v = if comb == "srss" { [5.436, 261.7, 9864.0] } else { [5.431, 261.5, 9916.0] };
        for q in 0..3 {
            if comb == "srss" {
                row("CSI response spectrum", &c, src, names[q], ["in", "kip", "kip-in"][q], indep[q], got[q], 0.5, "published");
            }
            program(&c, names[q], ["in", "kip", "kip-in"][q], indep[q], "SAP2000", sap_v[q], got[q]);
        }
    }
}

// =============================================================== SAP2000 1-024 / ETABS 8

/// A two-storey 3D frame, 2 x 2 bays (35 and 25 ft), storeys 13 ft, rigid diaphragms, the
/// storey mass 6.2112 k·s²/ft (x and y, no rotational inertia) at the centre of mass (38, 27)
/// ft, a constant 0.4 g spectrum in x, 4 % damping (independent: Peterson 1981). The torsion
/// constant is not printed; J = 2.5 ft⁴ (columns) and 2.0 ft⁴ (beams) is assumed, which
/// changes the periods by less than 0.05 %.
#[test]
fn sap_1_024_two_storey_3d() {
    let src = "CSI, SAP2000 Software Verification, Example 1-024, and ETABS Example 8 (Peterson 1981; Wilson & Habibullah 1992)";
    let mut m = bare("1-024");
    let ec = steel(&mut m, 350_000.0 * KSF);
    let eb = steel(&mut m, 500_000.0 * KSF);
    let col = general(&mut m, 4.0 * FT * FT, 1.25 * FT.powi(4), 1.25 * FT.powi(4), 2.5 * FT.powi(4));
    let beam = general(&mut m, 5.0 * FT * FT, 2.61 * FT.powi(4), 1.67 * FT.powi(4), 2.0 * FT.powi(4));
    let (xs, ys) = ([0.0, 35.0, 70.0].map(|v| v * FT), [0.0, 25.0, 50.0].map(|v| v * FT));
    let zs = [0.0, 13.0, 26.0].map(|v| v * FT);
    for k in 0..2 {
        for &x in &xs {
            for &y in &ys {
                mem(&mut m, [x, y, zs[k]], [x, y, zs[k + 1]], col, ec);
            }
        }
        let z = zs[k + 1];
        for &y in &ys {
            for w in xs.windows(2) {
                mem(&mut m, [w[0], y, z], [w[1], y, z], beam, eb);
            }
        }
        for &x in &xs {
            for w in ys.windows(2) {
                mem(&mut m, [x, w[0], z], [x, w[1], z], beam, eb);
            }
        }
    }
    for &x in &xs {
        for &y in &ys {
            sup(&mut m, [x, y, 0.0], FIXED);
        }
    }
    m.diaphragms = vec![zs[1], zs[2]];
    let mass = 6.2112 * KSF_MASS;
    let cm: Vec<Id> = (1..=2).map(|k| m.node_at([38.0 * FT, 27.0 * FT, zs[k]])).collect();
    m.masses = cm.iter().map(|n| NodalMass { node: *n, m: [mass, mass, 0.0], r: [0.0; 3] }).collect();
    let mut done_modes = false;
    for comb in ["cqc", "srss"] {
        rsa(&mut m, vec![(0.0, 0.4), (10.0, 0.4)], 0.04, comb, 12);
        let a = analyze(&m).unwrap();
        let sr = a.seismic.as_ref().unwrap_or_else(|| panic!("no seismic result: {:?}", a.warnings));
        if !done_modes {
            let mut t: Vec<f64> = sr.modes.iter().map(|md| md.period).collect();
            t.sort_by(|a, b| b.total_cmp(a));
            let c = "SAP2000 1-024 / ETABS 8 two-storey 3D frame, modes";
            for (k, (want, etabs_v)) in [(0.2271, 0.22708), (0.2156, 0.21565), (0.0733, 0.07335), (0.0720, 0.07201)].iter().enumerate() {
                let q = format!("T{}", k + 1);
                row("CSI modal", c, src, &q, "s", *want, t[k], 0.2, "published");
                program(c, &q, "s", *want, "ETABS", *etabs_v, t[k]);
            }
            done_modes = true;
        }
        let ex = sr.case("Ex").unwrap();
        let ux = ex.displacements[&cm[1]][0] / FT;
        let want = if comb == "cqc" { 0.02014 } else { 0.02012 };
        let c = format!("SAP2000 1-024 / ETABS 8 two-storey 3D frame, constant 0.4 g, {}", comb.to_uppercase());
        row("CSI response spectrum", &c, src, "Ux at the roof centre of mass", "ft", want, ux, 0.5, "published");
        program(&c, "Ux at the roof centre of mass", "ft", want, "SAP2000", want, ux);
    }
}

// =============================================================== SAP2000 1-025 / ETABS 5

/// A three-storey L-shaped building braced by four planar X-braced frames (two bays of 20 ft,
/// storeys 12 ft), all members axial only (A 6 in², E 29 500 ksi), rigid diaphragms, storey
/// mass 1.24224 k·s²/in in x and y and 174 907.4 k·in·s² about z at the centre of mass (33.33,
/// 33.33) ft, a response spectrum in x (5 %). The corner column at (0, 80) ft is in two frames
/// and is modelled twice, once in each frame with its own joints, as the reference does.
/// Independent: Peterson (1981), Wilson & Habibullah (1992).
#[test]
fn sap_1_025_braced_l_building() {
    let src = "CSI, SAP2000 Software Verification, Example 1-025, and ETABS Example 5 (Peterson 1981; Wilson & Habibullah 1992)";
    let mut m = bare("1-025");
    let st = steel(&mut m, 29_500.0 * KSI);
    let s = general(&mut m, 6.0 * IN * IN, 1e-9, 1e-9, 1e-9);
    // each frame: its start point and direction in plan (ft). The reference models every frame
    // with its own joints (frame 2's joints are frame 1's + 12, frame 3's + 24), so the corner
    // column at (0, 80) ft is two columns whose only link is the diaphragm; frame 3 is placed
    // 10 mm off the x = 0 line so that Pondus keeps its corner joints separate
    let eps = 0.010 / FT;
    let frames: [([f64; 2], [f64; 2]); 4] = [([0.0, 0.0], [1.0, 0.0]), ([0.0, 80.0], [1.0, 0.0]), ([eps, 40.0], [0.0, 1.0]), ([80.0, 0.0], [0.0, 1.0])];
    let zs = [0.0, 12.0, 24.0, 36.0].map(|v| v * FT);
    let mut f1_col = 0;
    let mut f1_braces = vec![];
    for (fi, (o, d)) in frames.iter().enumerate() {
        let pt = |k: usize, z: f64| [(o[0] + d[0] * 20.0 * k as f64) * FT, (o[1] + d[1] * 20.0 * k as f64) * FT, z];
        for st_k in 0..3 {
            let (z0, z1) = (zs[st_k], zs[st_k + 1]);
            for k in 0..3 {
                let c = mem(&mut m, pt(k, z0), pt(k, z1), s, st);
                if fi == 0 && st_k == 0 && k == 0 {
                    f1_col = c;
                }
            }
            for k in 0..2 {
                let b1 = mem(&mut m, pt(k, z0), pt(k + 1, z1), s, st);
                let b2 = mem(&mut m, pt(k, z1), pt(k + 1, z0), s, st);
                if fi == 0 && st_k == 0 {
                    f1_braces.push((k, b1, b2));
                }
            }
        }
        for k in 0..3 {
            let p = pt(k, 0.0);
            if m.supports.iter().all(|sp| sp.node != m.find_node(p).unwrap()) {
                sup(&mut m, p, [true, true, true, false, false, false]);
            }
        }
    }
    for x in m.members.iter_mut() {
        x.truss = true;
    }
    m.diaphragms = zs[1..].to_vec();
    let (mass, mmi) = (1.24224 * KSI_MASS, 174_907.4 * KIS2);
    let cm: Vec<Id> = (1..=3).map(|k| m.node_at([100.0 / 3.0 * FT, 100.0 / 3.0 * FT, zs[k]])).collect();
    m.masses = cm.iter().map(|n| NodalMass { node: *n, m: [mass, mass, 0.0], r: [0.0, 0.0, mmi] }).collect();
    let sa = vec![(0.01, 0.794273), (0.320640, 0.794273), (0.326887, 0.810009), (10.0, 0.810009)];
    for comb in ["cqc", "srss"] {
        // the reference combines the first two modes only
        rsa(&mut m, sa.clone(), 0.05, comb, 2);
        let a = analyze(&m).unwrap();
        let sr = a.seismic.as_ref().unwrap_or_else(|| panic!("no seismic result: {:?}", a.warnings));
        if comb == "cqc" {
            let mut f: Vec<f64> = sr.modes.iter().map(|md| md.frequency).collect();
            f.sort_by(f64::total_cmp);

            let c = "SAP2000 1-025 / ETABS 5 braced L building, modes";
            for (k, (want, etabs_t)) in [(3.0592, 0.32686), (3.1188, 0.32061)].iter().enumerate() {
                let q = format!("f{}", k + 1);
                row("CSI modal", c, src, &q, "Hz", *want, f[k], 0.2, "published");
                program(c, &q, "Hz", *want, "ETABS", 1.0 / etabs_t, f[k]);
            }
        }
        let ex = sr.case("Ex").unwrap();
        let roof = cm[2];
        // the brace force pair of each bay of frame 1, storey 1: the larger is the diagonal the
        // manual numbers 4 (bay 1) and 6 (ETABS D1 and D3, parallel)
        let ax = |id: Id| ex.member(id).unwrap().stations.iter().map(|s| s.n.abs()).fold(0.0, f64::max) / KIP;
        let bay = |k: usize| {
            let (_, b1, b2) = f1_braces[k];
            (ax(b1), ax(b2))
        };
        let (d1, d3) = (bay(0), bay(1));
        eprintln!("1-025 {comb}: bay 1 braces {:.2} / {:.2}, bay 2 braces {:.2} / {:.2} kip", d1.0, d1.1, d3.0, d3.1);
        let want: Vec<(&str, &str, f64, f64)> = if comb == "cqc" {
            vec![("Ux roof centre of mass", "in", 1.0329, ex.displacements[&roof][0] / IN), ("Uy roof centre of mass", "in", 0.1414, ex.displacements[&roof][1] / IN), ("Rz roof", "rad", 0.000252, ex.displacements[&roof][5]), ("axial column C1, storey 1 (frame 1)", "kip", 279.48, ax(f1_col)), ("axial brace D1, storey 1 (frame 4)", "kip", 194.50, d1.0), ("axial brace D3, storey 1 (frame 6)", "kip", 120.52, d3.0)]
        } else {
            vec![("Ux roof centre of mass", "in", 0.7372, ex.displacements[&roof][0] / IN), ("axial column C1, storey 1 (frame 1)", "kip", 200.55, ax(f1_col)), ("axial brace D1, storey 1 (frame 4)", "kip", 139.57, d1.0), ("axial brace D3, storey 1 (frame 6)", "kip", 86.48, d3.0)]
        };
        let c = format!("SAP2000 1-025 / ETABS 5 braced L building, {}", comb.to_uppercase());
        for (q, unit, indep, got) in want {
            row("CSI response spectrum", &c, src, q, unit, indep, got, 0.5, "published");
            program(&c, q, unit, indep, "SAP2000", indep, got);
        }
    }
}

// =============================================================== SAP2000 1-027

/// Construction sequence: a W36X150 cantilever 144 in (I 9040 in⁴, Av 22.4375 in², E 29 900
/// ksi, G 11 500 ksi) loaded at its tip by 100 k in stage 1; in stage 2 two W4X13 struts (A
/// 3.83 in²) are added below the tip down to a pin, and 25 k is applied at their joint.
/// Shear deformation is on (Pondus `design.shear_deformation`). Stage 3 removes a strut, which
/// Pondus's staged analysis cannot do. Independent: unit load method.
#[test]
fn sap_1_027_construction_sequence() {
    let src = sap("1-027 (unit load method, Cook & Young 1985)");
    let mut m = bare("1-027");
    let st = steel(&mut m, 29_900.0 * KSI);
    m.materials.iter_mut().find(|x| x.id == st).unwrap().nu = 29_900.0 / (2.0 * 11_500.0) - 1.0;
    let i = 9040.0 * IN.powi(4);
    let id = m.next_section_id();
    m.sections.push(Section { id, name: "W36X150".into(), shape: Shape::General { a: 44.2 * IN * IN, iy: i, iz: 270.0 * IN.powi(4), j: 10.1 * IN.powi(4), shear: Some([0.0, 22.4375 * IN * IN]) } });
    let strut = general(&mut m, 3.83 * IN * IN, 11.3 * IN.powi(4), 3.86 * IN.powi(4), 0.151 * IN.powi(4));
    m.design.shear_deformation = true;
    let l = 144.0 * IN;
    let b = mem(&mut m, [0.0, 0.0, 0.0], [l, 0.0, 0.0], id, st);
    let s2 = mem(&mut m, [l, 0.0, 0.0], [l, 0.0, -72.0 * IN], strut, st);
    let s3 = mem(&mut m, [l, 0.0, -72.0 * IN], [l, 0.0, -144.0 * IN], strut, st);
    m.members.iter_mut().find(|x| x.id == s2).unwrap().hinge_i = true;
    m.members.iter_mut().find(|x| x.id == s3).unwrap().hinge_j = true;
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [l, 0.0, -144.0 * IN], PIN);
    let (tip, mid) = (node(&m, [l, 0.0, 0.0]), node(&m, [l, 0.0, -72.0 * IN]));
    m.cases.push(LoadCase { id: 3, name: "S2".into(), kind: CaseKind::D, self_weight: false });
    nodal(&mut m, 1, tip, [0.0, 0.0, -100.0 * KIP, 0.0, 0.0, 0.0]);
    nodal(&mut m, 3, mid, [0.0, 0.0, -25.0 * KIP, 0.0, 0.0, 0.0]);
    let root = node(&m, [0.0, 0.0, 0.0]);
    let c = "SAP2000 1-027 construction sequence";
    for (stage, uz_want, fz_want) in [(1usize, -0.42404, 100.0), (2, -0.43617, 102.859)] {
        let mut stages = vec![pondus_core::advanced::Stage { name: "1".into(), members: vec![b], cases: vec![1] }];
        if stage == 2 {
            stages.push(pondus_core::advanced::Stage { name: "2".into(), members: vec![s2, s3], cases: vec![3] });
        }
        m.stages = stages;
        let a = analyze(&m).unwrap();
        let (_, fin) = pondus_core::advanced::staged(&m, &a).unwrap();
        let uz = fin.displacements[&tip][2] / IN;
        let fz = reaction(&fin, root)[2] / KIP;
        row("CSI staged construction", &format!("{c}, stage {stage}"), &src, "Uz joint 2 (tip)", "in", uz_want, uz, 0.2, "published");
        row("CSI staged construction", &format!("{c}, stage {stage}"), &src, "Fz joint 1", "kip", fz_want, fz, 0.2, "published");
    }
}

// =============================================================== ETABS 1

/// A plane frame of two 18 ft beams over three columns 10 ft high, the beams pinned at the
/// outer columns (propped cantilevers continuous over the middle column), E 3000 ksi, axial
/// deformation neglected; beam B1's moments and shears at its ends and quarter points under
/// point loads (case 1) and 10 k/ft (case 2). Independent: AISC Manual (1989) beam tables.
#[test]
fn etabs_1_beam_span_loads() {
    let src = etabs("1 (AISC Manual 1989, propped cantilever tables)");
    let (l, h) = (216.0 * IN, 120.0 * IN);
    for case_no in [1, 2] {
        let mut m = bare("ETABS 1");
        let st = steel(&mut m, 3000.0 * KSI);
        let cs = general(&mut m, 288.0 * IN * IN * 1e4, 13_824.0 * IN.powi(4), 3456.0 * IN.powi(4), 1e4 * IN.powi(4));
        let bs = general(&mut m, 360.0 * IN * IN * 1e4, 27_000.0 * IN.powi(4), 4320.0 * IN.powi(4), 1e4 * IN.powi(4));
        for x in [0.0, l, 2.0 * l] {
            mem(&mut m, [x, 0.0, 0.0], [x, 0.0, h], cs, st);
            sup(&mut m, [x, 0.0, 0.0], FIXED);
        }
        let b1 = mem(&mut m, [0.0, 0.0, h], [l, 0.0, h], bs, st);
        let b2 = mem(&mut m, [l, 0.0, h], [2.0 * l, 0.0, h], bs, st);
        m.members.iter_mut().find(|x| x.id == b1).unwrap().hinge_i = true;
        m.members.iter_mut().find(|x| x.id == b2).unwrap().hinge_j = true;
        if case_no == 1 {
            for (x, p) in [(0.0, 50.0), (l, 100.0), (2.0 * l, 50.0)] {
                let n = node(&m, [x, 0.0, h]);
                nodal(&mut m, 1, n, [0.0, 0.0, -p * KIP, 0.0, 0.0, 0.0]);
            }
            for b in [b1, b2] {
                m.loads.push(Load { case: 1, kind: LoadKind::Point { member: b, a: l / 2.0, p: [0.0, 0.0, -100.0 * KIP] } });
            }
        } else {
            for b in [b1, b2] {
                line(&mut m, 1, b, [0.0, 0.0, -10.0 * KIP / FT], [0.0, 0.0, -10.0 * KIP / FT]);
            }
        }
        let a = analyze(&m).unwrap();
        let cr = a.case(1).unwrap();
        let mr = cr.member(b1).unwrap();
        // the shear at a point: the station's own, or just right of a load at the mid-span
        let shear_at = |x: f64| -> f64 {
            let after: Vec<&pondus_core::analysis::Station> = mr.stations.iter().filter(|s| s.x >= x - 1e-6).collect();
            let s = if case_no == 1 && (x - l / 2.0).abs() < 1e-6 { after.iter().find(|s| s.x > x + 1e-6).copied().unwrap() } else { after[0] };
            s.vz
        };
        let (mw, vw): ([f64; 5], [f64; 5]) = if case_no == 1 { ([0.0, 1687.5, 3375.0, -337.5, -4050.0], [-31.25, -31.25, 68.75, 68.75, 68.75]) } else { ([0.0, 2430.0, 2430.0, 0.0, -4860.0], [-67.5, -22.5, 22.5, 67.5, 112.5]) };
        // Pondus's shear sign is fixed once from the first station (ETABS: −31.25 / −67.5 at I)
        let sign = (shear_at(0.0) / vw[0]).signum();
        let c = format!("ETABS 1 beam B1, case {case_no} ({})", if case_no == 1 { "point loads" } else { "10 k/ft" });
        for (k, name) in ["end I", "1/4 point", "1/2 point", "3/4 point", "end J"].iter().enumerate() {
            let x = l * k as f64 / 4.0;
            let mm = st_at(cr, b1, x).my / (KIP * IN);
            let tol_m = if mw[k] == 0.0 { 1e-6 } else { 0.1 };
            let tol_m = if mw[k] == 0.0 { 0.01 } else { tol_m };
            row("CSI frame statics", &c, &src, &format!("M at {name}"), "kip-in", mw[k], mm, tol_m, "published");
            row("CSI frame statics", &c, &src, &format!("V at {name}"), "kip", vw[k], sign * shear_at(x) / KIP, 0.1, "published");
        }
    }
}

// =============================================================== ETABS 2

/// A three-storey plane frame: two W14X90 columns (I 999 in⁴, E 29 500 ksi, axially rigid)
/// 36 ft apart, storeys 12 ft with a 24 in rigid zone at the top of each column under rigid
/// beams, storey mass 0.4 k·s²/in (x). Periods and mass-normalised mode shapes. Independent:
/// the exact 3-mass shear building (Przemieniecki 1968; Paz 1985). The rigid zone is a short
/// stiff member (I x 1e4).
#[test]
fn etabs_2_three_storey_modes() {
    let src = etabs("2 (Przemieniecki 1968; Paz 1985)");
    let mut m = bare("ETABS 2");
    let st = steel(&mut m, 29_500.0 * KSI);
    let i = 999.0 * IN.powi(4);
    // axially rigid and rigid zones by stiffness, with moderate areas: a weightless member
    // still gets 1 kg/m³, which on a 100 m² area would be a tonne of mass per storey
    let col = general(&mut m, 26.5 * IN * IN * 100.0, i, i, i);
    let rigid = general(&mut m, 26.5 * IN * IN * 100.0, i * 1e4, i * 1e4, i * 1e4);
    // a rigid beam: I 1e5 times the column's, a small area so it adds no mass
    let beam = general(&mut m, 0.1, 100.0, 100.0, 100.0);
    let (s, h, zr) = (432.0 * IN, 144.0 * IN, 24.0 * IN);
    for k in 0..3 {
        let z0 = h * k as f64;
        for x in [0.0, s] {
            mem(&mut m, [x, 0.0, z0], [x, 0.0, z0 + h - zr], col, st);
            mem(&mut m, [x, 0.0, z0 + h - zr], [x, 0.0, z0 + h], rigid, st);
        }
        mem(&mut m, [0.0, 0.0, z0 + h], [s, 0.0, z0 + h], beam, st);
    }
    sup(&mut m, [0.0, 0.0, 0.0], FIXED);
    sup(&mut m, [s, 0.0, 0.0], FIXED);
    let mass = 0.4 * KSI_MASS;
    let tops: Vec<Id> = (1..=3).map(|k| node(&m, [0.0, 0.0, h * k as f64])).collect();
    m.masses = (1..=3).flat_map(|k| [0.0, s].map(|x| (k, x))).map(|(k, x)| NodalMass { node: node(&m, [x, 0.0, h * k as f64]), m: [mass / 2.0, 0.0, 0.0], r: [0.0; 3] }).collect();
    rsa(&mut m, vec![(0.0, 0.1), (10.0, 0.1)], 0.05, "cqc", 3);
    let a = analyze(&m).unwrap();
    let sr = a.seismic.as_ref().unwrap_or_else(|| panic!("no seismic result: {:?}", a.warnings));
    let mut modes: Vec<&pondus_core::seismic::ModeInfo> = sr.modes.iter().filter(|md| md.mass_ratio[0] > 1e-3).collect();
    modes.sort_by(|a, b| b.period.total_cmp(&a.period));
    let c = "ETABS 2 three-storey frame";
    let want = [(0.4414, [1.165, 0.934, 0.519]), (0.1575, [0.934, -0.519, -1.165]), (0.1090, [0.519, -1.165, 0.934])];
    for (k, (t, shape)) in want.iter().enumerate() {
        let md = modes[k];
        row("CSI modal", c, &src, &format!("T{}", k + 1), "s", *t, md.period, 0.2, "published");
        // mass-normalised (Σ m φ² = 1, m in k·s²/in), with the roof's sign
        let phi: Vec<f64> = tops.iter().rev().map(|n| md.shape[n][0]).collect();
        let norm = (phi.iter().map(|p| 0.4 * p * p).sum::<f64>()).sqrt();
        let sgn = phi[0].signum();
        for (j, lvl) in ["roof", "2nd", "1st"].iter().enumerate() {
            row("CSI modal", c, &src, &format!("mode {} shape, {lvl}", k + 1), "-", shape[j], sgn * phi[j] / norm, 0.5, "published");
        }
    }
}

// =============================================================== ETABS 4

/// A one-storey 3D frame, 30 × 30 ft, 15 ft high with a 36 in rigid zone under rigid beams;
/// columns 24 × 24 in on the front line (y = 0) and 18 × 18 in on the back, E 3000 ksi, axially
/// rigid; the storey weight 150 psf, i.e. a mass of 0.3494 k·s²/in at the plan centre with the
/// polar inertia of a uniform square, m (a² + b²)/12. Independent: the 3-degree-of-freedom
/// system (Przemieniecki 1968; Paz 1985).
#[test]
fn etabs_4_one_storey_torsion() {
    let src = etabs("4 (Przemieniecki 1968; Paz 1985)");
    let mut m = bare("ETABS 4");
    let st = steel(&mut m, 3000.0 * KSI);
    let sq = |b: f64| b.powi(4) / 12.0 * IN.powi(4);
    // column torsion is left out of the reference's 3-degree-of-freedom system
    let c24 = general(&mut m, 576.0 * IN * IN * 10.0, sq(24.0), sq(24.0), 1e-3 * sq(24.0));
    let c18 = general(&mut m, 324.0 * IN * IN * 10.0, sq(18.0), sq(18.0), 1e-3 * sq(18.0));
    let r24 = general(&mut m, 576.0 * IN * IN * 10.0, 1e4 * sq(24.0), 1e4 * sq(24.0), 1e-3 * sq(24.0));
    let beam = general(&mut m, 0.1, 100.0, 100.0, 100.0);
    let (a, h, zr) = (360.0 * IN, 180.0 * IN, 36.0 * IN);
    for (x, y, s) in [(0.0, 0.0, c24), (a, 0.0, c24), (0.0, a, c18), (a, a, c18)] {
        mem(&mut m, [x, y, 0.0], [x, y, h - zr], s, st);
        mem(&mut m, [x, y, h - zr], [x, y, h], r24, st);
        sup(&mut m, [x, y, 0.0], FIXED);
    }
    for (p, q) in [([0.0, 0.0], [a, 0.0]), ([0.0, a], [a, a]), ([0.0, 0.0], [0.0, a]), ([a, 0.0], [a, a])] {
        mem(&mut m, [p[0], p[1], h], [q[0], q[1], h], beam, st);
    }
    m.diaphragms = vec![h];
    let mass = 0.3494 * KSI_MASS;
    let cm = m.node_at([a / 2.0, a / 2.0, h]);
    m.masses = vec![NodalMass { node: cm, m: [mass, mass, 0.0], r: [0.0, 0.0, mass * 2.0 * a * a / 12.0] }];
    rsa(&mut m, vec![(0.0, 0.1), (10.0, 0.1)], 0.05, "cqc", 20);
    let an = analyze(&m).unwrap();
    // the storey's modes, by their participation (near-massless columns twisting under their
    // tiny torsion constants give modes of no mass)
    let mut t: Vec<f64> = an.seismic.as_ref().unwrap().modes.iter().filter(|md| md.mass_ratio[0] + md.mass_ratio[1] > 0.01).map(|md| md.period).collect();
    t.sort_by(|a, b| b.total_cmp(a));
    let c = "ETABS 4 one-storey 3D frame";
    for (k, want) in [0.1389, 0.1254, 0.0702].iter().enumerate() {
        let tol = if k == 2 { 0.6 } else { 0.2 };
        row("CSI modal", c, &src, &format!("T{}", k + 1), "s", *want, t[k], tol, "published");
    }
}

// =============================================================== ETABS 15a

/// Planar shear walls 12 in thick, 120, 360 and 720 in long, 1, 3 and 6 storeys of 120 in,
/// E 3000 ksi, ν 0.2, fixed base, 100 k at the top. The reference is a refined SAP2000 plane
/// stress mesh; ETABS's own one-panel-per-storey shells are recorded beside it. Pondus's wall
/// is a Timoshenko beam (wide column with shear area 5A/6), so squat walls, where plane-stress
/// effects at the base matter, differ by several per cent.
#[test]
fn etabs_15a_planar_walls() {
    let src = etabs("15a (reference: SAP2000 refined plane-stress mesh)");
    let data = [
        (6, 120.0, 2.3921, 2.4287), (6, 360.0, 0.0986, 0.1031), (6, 720.0, 0.0172, 0.0186),
        (3, 120.0, 0.3071, 0.3205), (3, 360.0, 0.0170, 0.0187), (3, 720.0, 0.0046, 0.0052),
        (1, 120.0, 0.0145, 0.0185), (1, 360.0, 0.0025, 0.0029), (1, 720.0, 0.0011, 0.0013),
    ];
    for (storeys, lw, etabs_v, sap_v) in data {
        let mut m = bare("ETABS 15a");
        m.materials[0].e = 3000.0 * KSI;
        m.materials[0].nu = 0.2;
        m.materials[0].gamma = 0.0;
        let h = 120.0 * IN * storeys as f64;
        run(&mut m, "add_walls", &json!({"walls": [{"from": [0.0, 0.0], "to": [lw * IN, 0.0], "t": 12.0 * IN, "z0": 0.0, "z1": h}], "storey_h": 120.0 * IN})).unwrap();
        let top = m.nodes.iter().filter(|n| (n.z - h).abs() < 1e-6).min_by(|a, b| a.x.total_cmp(&b.x)).map(|n| n.id).unwrap();
        nodal(&mut m, 1, top, [100.0 * KIP, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let a = analyze(&m).unwrap();
        let ux = a.case(1).unwrap().displacements[&top][0] / IN;
        let c = format!("ETABS 15a wall L = {lw:.0} in, {storeys} storeys");
        let tol = if storeys == 1 { 25.0 } else if lw > 120.0 { 15.0 } else { 5.0 };
        row("CSI shear walls", &c, &src, "top displacement", "in", sap_v, ux, tol, "independent");
        program(&c, "top displacement", "in", sap_v, "ETABS", etabs_v, ux);
    }
}
