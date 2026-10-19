//! Calibration of Pondus's steel and Eurocode design against published worked examples.
//!
//! Every check runs through Pondus's own functions: `steel::capacities`, `steel::cb` and
//! `steel::check` (AISC 360-16), `codes::ec3_member` with the engine's EN 1993-1-1 checker as
//! Pondus calls it, `codes::check_ec2_beam` (EN 1992-1-1) and `connections::shear_tab`.
//!
//! Sources (inputs and printed answers as recorded, with page numbers, in Stabileo's
//! `engine/tests/design_worked_examples.rs`):
//! [AISC] AISC, Companion to the AISC Steel Construction Manual, Vol. 1: Design Examples,
//!        v16.0 (2023), keyed to AISC 360-22 (the clauses used here are unchanged from 360-16).
//! [INTI301] Troglia et al., Ejemplos de Aplicación del Reglamento CIRSOC 301-2018 (2018).
//! [SBE] SCI/CTICM, Steel Buildings in Europe, Multi-Storey Steel Buildings, Part 4 (2010), App. A.
//! [SBE-MS8] SCI/CTICM, Steel Buildings in Europe, Multi-Storey, Part 8 (2010), App. A, Ex. 2.
//! [P364] Brettle & Smith, Steel Building Design: Worked Examples, Open Sections, SCI P364 (2009).
//! [JRC-EC2] Biasioli et al., Eurocode 2: Background & Applications, Worked examples, JRC EUR
//!        26566 EN (2014), ch. 3.
//!
//! Units: US examples are converted in (1 in = 25.4 mm, 1 ksi = 6.894757 MPa,
//! 1 kip = 4.448222 kN) and compared back in the example's units. Pondus takes E = 200 000 MPa
//! for AISC (29 000 ksi = 199 948 MPa, 0.03 % apart) and 210 000 MPa for EC3.

mod calib;
use calib::{check, Row};

use pondus_core::codes;
use pondus_core::model::{DesignSettings, Props};
use pondus_core::steel::{self, HDims, SteelDemand};

use dedaliano_engine::postprocess::ec3_check::*;

const IN: f64 = 0.0254; // m
const KSI: f64 = 6.894757; // MPa
const KIP: f64 = 4.448222; // kN

fn row(aspect: &str, case: &str, source: &str, quantity: &str, unit: &str, reference: f64, pondus: f64, tol: f64) {
    check(Row { suite: "steel", aspect, case, source, quantity, unit, reference, pondus, tol, kind: "published" });
}

fn closed(aspect: &str, case: &str, source: &str, quantity: &str, unit: &str, reference: f64, pondus: f64, tol: f64) {
    check(Row { suite: "steel", aspect, case, source, quantity, unit, reference, pondus, tol, kind: "closed-form" });
}

/// Props from properties in inch units (A in², I in⁴, S/Z in³, r in, J in⁴, Cw in⁶);
/// x is the major axis (Pondus's local y).
#[allow(clippy::too_many_arguments)]
fn props_in(a: f64, ix: f64, iy: f64, sx: f64, sy: f64, zx: f64, zy: f64, rx: f64, ry: f64, j: f64, cw: f64) -> Props {
    Props {
        a: a * IN.powi(2),
        iy: ix * IN.powi(4),
        iz: iy * IN.powi(4),
        j: j * IN.powi(4),
        cw: cw * IN.powi(6),
        sy: sx * IN.powi(3),
        sz: sy * IN.powi(3),
        zy: zx * IN.powi(3),
        zz: zy * IN.powi(3),
        ry: rx * IN,
        rz: ry * IN,
    }
}

/// Plate dimensions in inches to Pondus's HDims (mm).
fn dims_in(d: f64, bf: f64, tw: f64, tf: f64, r: f64) -> HDims {
    HDims { d: d * 25.4, bf: bf * 25.4, tw: tw * 25.4, tf: tf * 25.4, r: r * 25.4 }
}

/// φ-values of Pondus's capacities in kips and kip-in.
fn caps(p: &Props, h: HDims, fy_ksi: f64, fu_ksi: f64, l_in: f64, lb_in: f64) -> steel::Capacities {
    steel::capacities(p, h, fy_ksi * KSI, fu_ksi * KSI, l_in * IN, lb_in * IN, 1.0)
}
const KN_TO_KIP: f64 = 1.0 / KIP;
/// N·mm to kip-in.
fn nmm_to_kipin(m: f64) -> f64 {
    m / 1e3 / KIP / 25.4
}

// ------------------------------------------------------------------ AISC 360

#[test]
fn aisc_d1_w8x21_tension_yielding() {
    // [AISC] D.1, pp. D-2 – D-4: Ag = 6.16 in², Fy 50, Fu 65 ksi; D2-1 φtPn = 277 kips.
    let p = props_in(6.16, 75.3, 9.77, 18.2, 3.71, 20.4, 5.69, 3.49, 1.26, 0.282, 152.0);
    let h = dims_in(8.28, 5.27, 0.250, 0.400, 0.3);
    let c = caps(&p, h, 50.0, 65.0, 120.0, 120.0);
    row("AISC 360 tension", "AISC D.1 W8x21", "AISC Design Examples v16, D.1, p. D-3", "φtPn, D2-1 yielding", "kip", 277.0, c.phi_pt / 1e3 * KN_TO_KIP, 0.5);
    // D2-2 rupture through steel::check with the example's end connection: 3/4 in bolts, two
    // holes in each flange, l = 9 in (D-3: An = 4.76 in², x̄ = 0.831 in, U = 0.908, printed
    // φtPn = 211 kips). Pondus computes An and U (Table D3.1 case 2, x̄ of the split tee
    // without fillets; case 7 gives 0.85 as bf < 2d/3) from the shape.
    let l = 120.0 * IN;
    let pu = 100.0 * KIP;
    let st = vec![(0.0, pu, 0.0, 0.0, 0.0, 0.0), (l, pu, 0.0, 0.0, 0.0, 0.0)];
    let conn = pondus_core::model::TensionConnection { kind: "bolted".into(), connected: "flanges".into(), bolt_d: 0.75 * IN * 1000.0, holes_flange: 2, holes_web: 0, bolts_per_line: 4, length: 9.0 * IN };
    let (an, ae, how) = conn.net(p.a * 1e6, h, false);
    eprintln!("D.1 net section: {how}");
    row("AISC 360 tension", "AISC D.1 W8x21", "AISC Design Examples v16, D.1, p. D-3", "An (B4.3b, holes d + 1/8 in)", "in²", 4.76, an / (IN * IN * 1e6), 0.5);
    row("AISC 360 tension", "AISC D.1 W8x21", "AISC Design Examples v16, D.1, p. D-3", "U (Table D3.1 case 2, x̄ without fillets)", "-", 0.908, ae / an, 0.5);
    let dem = SteelDemand { length: l, lb: l, k: 1.0, uls: vec![("ULS".into(), st)], defl_live: 0.0, defl_total: 0.0, cantilever: false, top_flange_load: false, tension: Some(conn) };
    let d = steel::check(1, None, "W8x21", 0.0, &p, h, 50.0 * KSI, 65.0 * KSI, &dem, &DesignSettings::default());
    let r = d.checks.iter().find(|c| c.name.starts_with("axial")).map(|c| c.ratio).unwrap();
    row("AISC 360 tension", "AISC D.1 W8x21", "AISC Design Examples v16, D.1, p. D-3", "φtPn, D2-2 rupture (steel::check)", "kip", 211.0, 100.0 / r, 0.5);
}

#[test]
fn aisc_e1c_w14x132_inelastic_buckling() {
    // [AISC] E.1C, pp. E-8 – E-9: Ag 38.8 in², rx 6.28, ry 3.76 in, Lc = 30 ft both axes;
    // φcFn = 23.0 ksi, φcPn = 892 kips. Manual Table 1-1 plates (bf 14.7, tf 1.03, tw 0.645,
    // h/tw 17.7) set only the slenderness of the elements (none slender).
    let tw = 0.645;
    let (d, tf) = (14.7, 1.03);
    let r = (d - 2.0 * tf - 17.7 * tw) / 2.0;
    let p = props_in(38.8, 1530.0, 548.0, 209.0, 74.5, 234.0, 113.0, 6.28, 3.76, 12.3, 25500.0);
    let c = caps(&p, dims_in(d, 14.7, tw, tf, r), 50.0, 65.0, 360.0, 360.0);
    let pc = c.phi_pc / 1e3 * KN_TO_KIP;
    row("AISC 360 compression", "AISC E.1C W14x132", "AISC Design Examples v16, E.1C, p. E-9", "φcPn (E3-2)", "kip", 892.0, pc, 0.5);
    row("AISC 360 compression", "AISC E.1C W14x132", "AISC Design Examples v16, E.1C, p. E-9", "φcFn", "ksi", 23.0, pc / 38.8, 0.5);
}

#[test]
fn aisc_e1d_w14x90_major_axis_governs() {
    // [AISC] E.1D, pp. E-10 – E-12: Ag 26.5 in², rx 6.14, ry 3.70 in; Lcx = 30 ft, Lcy = 15 ft
    // (Pondus: member length 30 ft, Lb = 15 ft braces the weak axis); Fn = 38.9 ksi,
    // Pn = 1 030 kips, φcPn = 927 kips.
    let tw = 0.440;
    let (d, tf) = (14.0, 0.710);
    let r = (d - 2.0 * tf - 25.9 * tw) / 2.0;
    let p = props_in(26.5, 999.0, 362.0, 143.0, 49.9, 157.0, 75.6, 6.14, 3.70, 4.06, 16000.0);
    let c = caps(&p, dims_in(d, 14.5, tw, tf, r), 50.0, 65.0, 360.0, 180.0);
    let pc = c.phi_pc / 1e3 * KN_TO_KIP;
    row("AISC 360 compression", "AISC E.1D W14x90", "AISC Design Examples v16, E.1D, p. E-11", "φcPn (Lcx governs)", "kip", 927.0, pc, 0.5);
    row("AISC 360 compression", "AISC E.1D W14x90", "AISC Design Examples v16, E.1D, p. E-11", "Fn (E3-2)", "ksi", 38.9, pc / 0.9 / 26.5, 0.5);
}

#[test]
fn aisc_e3_3_elastic_branch_ipe300() {
    // [INTI301] Ej. 10(a), IPE 300, Fy 235 MPa, E 200 000 MPa, Ag 5380 mm², rmin 33.5 mm,
    // kL = 6500 mm: Fcr = 0.877 Fe = 46.0 MPa, Pn = 210.3/0.85 = 247.4 kN (same equation as
    // AISC E3-3; the comparison is on the nominal value).
    let p = Props { a: 5380e-6, iy: 8356e-8, iz: 5380e-6 * 0.0335f64.powi(2), j: 20e-8, cw: 126e-9, sy: 557e-6, sz: 80.5e-6, zy: 628e-6, zz: 125e-6, ry: 0.0335 * 3.0, rz: 0.0335 };
    let h = HDims { d: 300.0, bf: 150.0, tw: 7.1, tf: 10.7, r: 15.0 };
    let c = steel::capacities(&p, h, 235.0, 360.0, 6.5, 6.5, 1.0);
    let pn = c.phi_pc / 0.9;
    row("AISC 360 compression", "INTI301 Ej.10(a) IPE300", "Troglia et al. (2018), Ej. 10-2", "Fcr = 0.877 Fe (E3-3)", "MPa", 46.0, pn / 5380.0, 0.5);
    row("AISC 360 compression", "INTI301 Ej.10(a) IPE300", "Troglia et al. (2018), Ej. 10-2", "Pn", "kN", 247.4, pn / 1e3, 0.5);
}

/// W18×50 as printed in [AISC] F.1-1A/1B/2B: Zx 101, Sx 88.9 in³, ry 1.65, J 1.24, rts 1.98,
/// ho 17.4 in, Ix 800 in⁴. Pondus derives rts = (√(Iy·Cw)/Sx)^½ and ho = d − tf, so
/// Iy = 2·Sx·rts²/ho and Cw = Iy·ho²/4 reproduce the printed rts and ho exactly, and
/// d = ho + tf with tf = 0.570 in (Manual: bf 7.50, tf 0.570, tw 0.355 in, compact).
fn w18x50() -> (Props, HDims) {
    let (sx, rts, ho) = (88.9, 1.98, 17.4);
    let iy = 2.0 * sx * rts * rts / ho;
    let cw = iy * ho * ho / 4.0;
    (props_in(14.7, 800.0, iy, sx, 10.7, 101.0, 16.6, 7.38, 1.65, 1.24, cw), dims_in(ho + 0.570, 7.50, 0.355, 0.570, 0.4))
}

/// Pondus's Cb (F1-1) over a segment [x0, x1] of a simply supported beam of span L under a
/// uniform load: M(x) = x(L − x).
fn cb_udl(l: f64, x0: f64, x1: f64) -> f64 {
    let st: Vec<(f64, f64)> = (0..=40).map(|k| x0 + (x1 - x0) * k as f64 / 40.0).map(|x| (x - x0, x * (l - x))).collect();
    steel::cb(&st, x1 - x0)
}

#[test]
fn aisc_f1_w18x50_flexure_and_ltb() {
    let (p, h) = w18x50();
    // F.1-1B, p. F-8: continuously braced, Mp = 5 050 kip-in, φbMn = 379 kip-ft.
    let c = caps(&p, h, 50.0, 65.0, 420.0, 0.0);
    row("AISC 360 flexure", "AISC F.1-1B W18x50", "AISC Design Examples v16, F.1-1B, p. F-8", "Mp (F2-1)", "kip-in", 5050.0, nmm_to_kipin(c.phi_mpx) / 0.9, 0.5);
    row("AISC 360 flexure", "AISC F.1-1B W18x50", "AISC Design Examples v16, F.1-1B, p. F-8", "φbMn", "kip-ft", 379.0, nmm_to_kipin(c.phi_mx1) / 12.0, 0.5);
    // F.1-2B, pp. F-10 – F-11: third points, centre segment Cb = 1.01, Lb = 11.7 ft;
    // Lp = 69.9 in, Lr = 203 in; Mn = 4 060 kip-in, φbMn = 304 kip-ft.
    let l = 35.0;
    let cb2 = cb_udl(l, l / 3.0, 2.0 * l / 3.0);
    row("AISC 360 flexure", "AISC F.1-2B W18x50", "AISC Design Examples v16, F.1-2B, p. F-10", "Cb (F1-1), centre third", "-", 1.01, cb2, 0.5);
    // Mn with the example's own (rounded) Cb = 1.01, so that F2-2 is compared alone
    let c = caps(&p, h, 50.0, 65.0, 420.0, 11.7 * 12.0);
    let mn = (1.01 * c.phi_mx1).min(c.phi_mpx) / 0.9;
    row("AISC 360 flexure", "AISC F.1-2B W18x50", "AISC Design Examples v16, F.1-2B, p. F-11", "Mn (F2-2)", "kip-in", 4060.0, nmm_to_kipin(mn), 0.5);
    row("AISC 360 flexure", "AISC F.1-2B W18x50", "AISC Design Examples v16, F.1-2B, p. F-11", "φbMn", "kip-ft", 304.0, nmm_to_kipin(0.9 * mn) / 12.0, 0.5);
    let at = |lb: f64| nmm_to_kipin(caps(&p, h, 50.0, 65.0, 420.0, lb).phi_mx1) / 0.9;
    row("AISC 360 flexure", "AISC F.1-2B W18x50", "AISC Design Examples v16, F.1-2B, p. F-10", "Mn at the printed Lp = 69.9 in (= Mp)", "kip-in", 5050.0, at(69.9), 0.5);
    row("AISC 360 flexure", "AISC F.1-2B W18x50", "AISC Design Examples v16, F.1-2B, p. F-10", "Mn at the printed Lr = 203 in (= 0.7 Fy Sx)", "kip-in", 0.7 * 50.0 * 88.9, at(203.0), 0.5);
    // F.1-3B, pp. F-14 – F-15: braced at midspan, Cb = 1.30, Lb = 17.5 ft > Lr;
    // Fcr = 43.2 ksi, Mn = 3 840 kip-in, φbMn = 288 kip-ft.
    let cb3 = cb_udl(l, 0.0, l / 2.0);
    row("AISC 360 flexure", "AISC F.1-3B W18x50", "AISC Design Examples v16, F.1-3B, p. F-14", "Cb (F1-1), half span", "-", 1.30, cb3, 0.5);
    let c = caps(&p, h, 50.0, 65.0, 420.0, 17.5 * 12.0);
    let mn = (1.30 * c.phi_mx1).min(c.phi_mpx) / 0.9;
    row("AISC 360 flexure", "AISC F.1-3B W18x50", "AISC Design Examples v16, F.1-3B, p. F-15", "Fcr (F2-4)", "ksi", 43.2, nmm_to_kipin(mn) / 88.9, 0.5);
    row("AISC 360 flexure", "AISC F.1-3B W18x50", "AISC Design Examples v16, F.1-3B, p. F-15", "Mn (F2-3)", "kip-in", 3840.0, nmm_to_kipin(mn), 0.5);
    row("AISC 360 flexure", "AISC F.1-3B W18x50", "AISC Design Examples v16, F.1-3B, p. F-15", "φbMn", "kip-ft", 288.0, nmm_to_kipin(0.9 * mn) / 12.0, 0.5);
}

#[test]
fn aisc_f3b_w21x48_noncompact_flange() {
    // [AISC] F.3B, pp. F-24 – F-25: Sx 93.0, Zx 107 in³, bf/2tf = 9.47 (bf 8.14, tf 0.430 in);
    // Mn = 5 310 kip-in (F3-1), φbMn = 398 kip-ft.
    let p = props_in(14.1, 959.0, 38.7, 93.0, 9.52, 107.0, 14.9, 8.24, 1.66, 0.803, 3950.0);
    let c = caps(&p, dims_in(20.6, 8.14, 0.350, 0.430, 0.43), 50.0, 65.0, 480.0, 0.0);
    row("AISC 360 flexure", "AISC F.3B W21x48", "AISC Design Examples v16, F.3B, p. F-25", "Mn (F3-1, flange local buckling)", "kip-in", 5310.0, nmm_to_kipin(c.phi_mx1) / 0.9, 0.5);
    row("AISC 360 flexure", "AISC F.3B W21x48", "AISC Design Examples v16, F.3B, p. F-25", "φbMn", "kip-ft", 398.0, nmm_to_kipin(c.phi_mx1) / 12.0, 0.5);
}

#[test]
fn aisc_f5_w12x58_minor_axis() {
    // [AISC] F.5, pp. F-28 – F-29: Sy 21.4, Zy 32.5 in³; Mn = min(Fy Zy, 1.6 Fy Sy) =
    // 1 630 kip-in, φbMn = 122 kip-ft (bf 10.0, tf 0.640 in: compact).
    let p = props_in(17.0, 475.0, 107.0, 78.0, 21.4, 86.4, 32.5, 5.28, 2.51, 2.10, 3570.0);
    let c = caps(&p, dims_in(12.2, 10.0, 0.360, 0.640, 0.5), 50.0, 65.0, 240.0, 0.0);
    row("AISC 360 flexure", "AISC F.5 W12x58", "AISC Design Examples v16, F.5, p. F-29", "Mny (F6-1)", "kip-in", 1630.0, nmm_to_kipin(c.phi_mz) / 0.9, 0.5);
    row("AISC 360 flexure", "AISC F.5 W12x58", "AISC Design Examples v16, F.5, p. F-29", "φbMny", "kip-ft", 122.0, nmm_to_kipin(c.phi_mz) / 12.0, 0.5);
}

#[test]
fn aisc_g1b_w24x62_shear() {
    // [AISC] G.1B, p. G-4: d 23.7, tw 0.430 in, Aw = 10.2 in², Cv1 = 1, φv = 1.00 (G2.1(a),
    // rolled, h/tw = 50.1 ≤ 2.24√(E/Fy)): Vn = φvVn = 306 kips. h/tw is the Manual's.
    let (d, tw, tf) = (23.7, 0.430, 0.590);
    let r = (d - 2.0 * tf - 50.1 * tw) / 2.0;
    let p = props_in(18.2, 1550.0, 34.5, 131.0, 9.80, 153.0, 15.7, 9.23, 1.38, 1.71, 4620.0);
    let c = caps(&p, dims_in(d, 7.04, tw, tf, r), 50.0, 65.0, 240.0, 0.0);
    row("AISC 360 shear", "AISC G.1B W24x62", "AISC Design Examples v16, G.1B, p. G-4", "φvVn = Vn (G2-1, φv = 1.0)", "kip", 306.0, c.phi_vz / 1e3 * KN_TO_KIP, 0.5);
}

#[test]
fn aisc_h3_w14x82_tension_and_biaxial_flexure_through_check() {
    // [AISC] H.3, pp. H-8 – H-11: Ag 24.0 in², Sx 123, Zx 139, Sy 29.3, Zy 44.8 in³,
    // Iy 148 in⁴; Lp 8.76 ft, Lr 33.2 ft; Lb 30 ft; Cb = 1.41 (Table 3-1's 1.14 times the H1.2
    // tension factor 1.24, which Pondus does not compute: the moment diagram below is chosen so
    // that Pondus's F1-1 gives 1.41); Pu = 174 kips tension, Mux = 192, Muy = 67.6 kip-ft.
    // Printed: φtPn 1 080 kips, Mnx 6 560 kip-in, φbMnx 492, Mny 2 240 kip-in, φbMny 168
    // kip-ft, Pr/Pc 0.161, H1-1b 0.873. ry, J and ho are the Manual's (2.48 in, 5.07 in⁴,
    // 13.4 in), Cw = Iy·ho²/4; the printed Lp and Lr pin them.
    let (sx, ho, iy) = (123.0, 13.4, 148.0);
    let tf = 0.855;
    let p = props_in(24.0, 881.0, iy, sx, 29.3, 139.0, 44.8, 6.05, 2.48, 5.07, iy * ho * ho / 4.0);
    let h = dims_in(ho + tf, 10.1, 0.510, tf, 0.6);
    let at = |lb_ft: f64| nmm_to_kipin(caps(&p, h, 50.0, 65.0, 360.0, lb_ft * 12.0).phi_mx1) / 0.9;
    row("AISC 360 flexure", "AISC H.3 W14x82", "AISC Design Examples v16, H.3, p. H-9", "Mn at the printed Lp = 8.76 ft (= Mp)", "kip-in", 50.0 * 139.0, at(8.76), 0.5);
    row("AISC 360 flexure", "AISC H.3 W14x82", "AISC Design Examples v16, H.3, p. H-9", "Mn at the printed Lr = 33.2 ft (= 0.7 Fy Sx)", "kip-in", 0.7 * 50.0 * sx, at(33.2), 1.0);
    // The member through Pondus's own check, 30 ft long, Lb = 30 ft
    let l = 30.0 * 12.0 * IN;
    let mmax = 192.0 * KIP * 12.0 * IN; // kN·m
    let mq = mmax * (12.5 / 1.41 - 2.5) / 10.0;
    let st = vec![
        (0.0, 174.0 * KIP, 0.0, 0.0, mmax, 67.6 * KIP * 12.0 * IN),
        (0.25 * l, 174.0 * KIP, 0.0, 0.0, mq, 0.0),
        (0.5 * l, 174.0 * KIP, 0.0, 0.0, mq, 0.0),
        (0.75 * l, 174.0 * KIP, 0.0, 0.0, mq, 0.0),
        (l, 174.0 * KIP, 0.0, 0.0, mq, 0.0),
    ];
    let cbv = steel::cb(&st.iter().map(|s| (s.0, s.4)).collect::<Vec<_>>(), l);
    closed("AISC 360 flexure", "AISC H.3 W14x82", "F1-1 on the constructed diagram (target 1.41)", "Cb", "-", 1.41, cbv, 0.01);
    // H.3 takes the tension capacity from yielding on Ag (no end connection: rupture not checked)
    let dem = SteelDemand { length: l, lb: l, k: 1.0, uls: vec![("ULS".into(), st)], defl_live: 0.0, defl_total: 0.0, cantilever: false, top_flange_load: false, tension: None };
    let d = steel::check(1, None, "W14x82", 0.0, &p, h, 50.0 * KSI, 65.0 * KSI, &dem, &DesignSettings::default());
    let ratio = |n: &str| d.checks.iter().find(|c| c.name.starts_with(n)).map(|c| c.ratio).unwrap();
    let c = caps(&p, h, 50.0, 65.0, 360.0, 360.0);
    row("AISC 360 tension", "AISC H.3 W14x82", "AISC Design Examples v16, H.3, p. H-9", "φtPn (D2-1)", "kip", 1080.0, c.phi_pt / 1e3 * KN_TO_KIP, 0.5);
    let mnx = (1.41 * c.phi_mx1).min(c.phi_mpx) / 0.9;
    row("AISC 360 flexure", "AISC H.3 W14x82", "AISC Design Examples v16, H.3, p. H-10", "Mnx (F2-2, Cb 1.41)", "kip-in", 6560.0, nmm_to_kipin(mnx), 0.5);
    row("AISC 360 flexure", "AISC H.3 W14x82", "AISC Design Examples v16, H.3, p. H-10", "φbMnx", "kip-ft", 492.0, nmm_to_kipin(0.9 * mnx) / 12.0, 0.5);
    row("AISC 360 flexure", "AISC H.3 W14x82", "AISC Design Examples v16, H.3, p. H-10", "Mny (F6-1)", "kip-in", 2240.0, nmm_to_kipin(c.phi_mz) / 0.9, 0.5);
    row("AISC 360 flexure", "AISC H.3 W14x82", "AISC Design Examples v16, H.3, p. H-10", "φbMny", "kip-ft", 168.0, nmm_to_kipin(c.phi_mz) / 12.0, 0.5);
    row("AISC 360 interaction", "AISC H.3 W14x82", "AISC Design Examples v16, H.3, p. H-11", "Pr/Pc (steel::check)", "-", 0.161, ratio("axial"), 0.5);
    row("AISC 360 interaction", "AISC H.3 W14x82", "AISC Design Examples v16, H.3, p. H-11", "H1-1b (steel::check)", "-", 0.873, ratio("interaction"), 0.5);
}

// ------------------------------------------------------------------ EN 1993-1-1

fn ec3_dem(length: f64, lb: f64, top: bool) -> SteelDemand {
    SteelDemand { tension: None, length, lb, k: 1.0, uls: vec![], defl_live: 0.0, defl_total: 0.0, cantilever: false, top_flange_load: top }
}

fn ec3_run(m: Ec3MemberData, n_kn: f64, my_knm: f64, v_kn: f64) -> Ec3CheckResult {
    check_ec3_members(&Ec3CheckInput { members: vec![m], forces: vec![Ec3DesignForces { element_id: 1, n_ed: Some(n_kn * 1e3), my_ed: Some(my_knm * 1e3), mz_ed: Some(0.0), v_ed: Some(v_kn * 1e3) }] }).remove(0)
}

/// Props in m from cm-unit section tables (A cm², I cm⁴, W cm³, It cm⁴, Iw cm⁶).
#[allow(clippy::too_many_arguments)]
fn props_cm(a: f64, iy: f64, iz: f64, wel_y: f64, wel_z: f64, wpl_y: f64, wpl_z: f64, it: f64, iw: f64) -> Props {
    Props { a: a * 1e-4, iy: iy * 1e-8, iz: iz * 1e-8, j: it * 1e-8, cw: iw * 1e-12, sy: wel_y * 1e-6, sz: wel_z * 1e-6, zy: wpl_y * 1e-6, zz: wpl_z * 1e-6, ry: (iy / a).sqrt() * 1e-2, rz: (iz / a).sqrt() * 1e-2 }
}

/// A simply supported beam's moment stations under a uniform load (kN·m).
fn udl_stations(l: f64, mmax: f64) -> Vec<(f64, f64, f64, f64, f64, f64)> {
    (0..=16).map(|k| l * k as f64 / 16.0).map(|x| (x, 0.0, 0.0, 0.0, 4.0 * mmax * x * (l - x) / (l * l), 0.0)).collect()
}

#[test]
fn ec3_sbe_a1_ipe330() {
    // [SBE] A.1, pp. 4-57 – 4-62: IPE 330 S235 (h 330, b 160, tw 7.5, tf 11.5, r 18 mm),
    // A 62.6 cm², Wpl,y 804.3 cm³; Mc,Rd = 189.01 kNm, MEd/Mc,Rd = 0.479 (MEd 90.48);
    // Av = 3080 mm², Vpl,Rd = 417.9 kN (VEd 63.50, ratio 0.152).
    // LTB: Iz 788.1 cm⁴, It 28.15 cm⁴, Iw 199 100 cm⁶, L = 5.70 m, uniform load on the top
    // flange: C1 = 1.127, C2 = 0.454; Mcr = 113.9 kNm, λ̄LT = 1.288.
    let p = props_cm(62.6, 11770.0, 788.1, 713.1, 98.5, 804.3, 153.7, 28.15, 199_100.0);
    let h = HDims { d: 330.0, bf: 160.0, tw: 7.5, tf: 11.5, r: 18.0 };
    let m0 = codes::ec3_member(1, &p, h, 235.0, &ec3_dem(5.7, 0.0, true), 1.0, false);
    row("EN 1993-1-1", "SBE A.1 IPE330", "SBE Multi-Storey Part 4 (2010), A.1, p. 4-59", "Av (6.2.6(3))", "mm²", 3080.0, m0.av.unwrap() * 1e6, 0.5);
    let r = ec3_run(m0, 0.0, 90.48, 63.50);
    row("EN 1993-1-1", "SBE A.1 IPE330", "SBE Multi-Storey Part 4 (2010), A.1, p. 4-58", "Mc,Rd (6.2.5)", "kNm", 189.01, 90.48 / r.flexure_ratio_y, 0.5);
    row("EN 1993-1-1", "SBE A.1 IPE330", "SBE Multi-Storey Part 4 (2010), A.1, p. 4-58", "MEd/Mc,Rd", "-", 0.479, r.flexure_ratio_y, 0.5);
    row("EN 1993-1-1", "SBE A.1 IPE330", "SBE Multi-Storey Part 4 (2010), A.1, p. 4-59", "Vpl,Rd (6.2.6)", "kN", 417.9, 63.50 / r.shear_ratio, 0.5);
    // LTB as Pondus sets it up for a beam of the model: the stations of the uniformly loaded
    // span give C1 and C2, the load on the top flange
    let dem = SteelDemand { uls: vec![("ULS".into(), udl_stations(5.7, 90.48))], ..ec3_dem(5.7, 5.7, true) };
    let st = &dem.uls[0].1;
    let c1 = codes::ec3_c1(&dem, st);
    row("EN 1993-1-1", "SBE A.1 IPE330", "SBE Multi-Storey Part 4 (2010), A.1, p. 4-60 (SN003)", "C1 (Pondus: SN003 value for the diagram it recognises)", "-", 1.127, c1, 1.0);
    let mut m = codes::ec3_member(1, &p, h, 235.0, &dem, c1, false);
    m.ltb = codes::ec3_ltb(&dem, st);
    let r = ec3_run(m, 0.0, 90.48, 0.0);
    let mcr = r.mcr.unwrap() / 1e3;
    row("EN 1993-1-1", "SBE A.1 IPE330", "SBE Multi-Storey Part 4 (2010), A.1, p. 4-61", "Mcr (top flange, zg = +165 mm)", "kNm", 113.9, mcr, 1.0);
    row("EN 1993-1-1", "SBE A.1 IPE330", "SBE Multi-Storey Part 4 (2010), A.1, p. 4-61", "λ̄LT", "-", 1.288, (804.3e-6 * 235e6 / (mcr * 1e3)).sqrt(), 0.5);
}

#[test]
fn ec3_sbe_a2_ipe400() {
    // [SBE] A.2, pp. 4-64 – 4-69: IPE 400 S235 (h 400, b 180, tw 8.6, tf 13.5, r 21 mm),
    // A 84.46 cm², Wpl,y 1307 cm³; Mc,Rd 307.15 kNm (MEd 244.97, 0.798); Av 4269 mm²,
    // Vpl,Rd 579.21 kN (VEd 65.33, 0.113).
    let p = props_cm(84.46, 23130.0, 1318.0, 1156.0, 146.4, 1307.0, 229.0, 51.08, 490_000.0);
    let h = HDims { d: 400.0, bf: 180.0, tw: 8.6, tf: 13.5, r: 21.0 };
    let m = codes::ec3_member(1, &p, h, 235.0, &ec3_dem(7.3, 0.0, true), 1.0, false);
    row("EN 1993-1-1", "SBE A.2 IPE400", "SBE Multi-Storey Part 4 (2010), A.2, p. 4-66", "Av (6.2.6(3))", "mm²", 4269.0, m.av.unwrap() * 1e6, 0.5);
    let r = ec3_run(m, 0.0, 244.97, 65.33);
    row("EN 1993-1-1", "SBE A.2 IPE400", "SBE Multi-Storey Part 4 (2010), A.2, p. 4-66", "Mc,Rd (6.2.5)", "kNm", 307.15, 244.97 / r.flexure_ratio_y, 0.5);
    row("EN 1993-1-1", "SBE A.2 IPE400", "SBE Multi-Storey Part 4 (2010), A.2, p. 4-66", "Vpl,Rd (6.2.6)", "kN", 579.21, 65.33 / r.shear_ratio, 0.5);
}

#[test]
fn ec3_sbe_a5_heb300_column() {
    // [SBE] A.5, pp. 4-94 – 4-97: HE 300 B S235 (h 300, b 300, tw 11, tf 19, r 27 mm),
    // A 149 cm², Iy 25 170, Iz 8 560 cm⁴; Lcr,y = 8.00 m, Lcr,z = 5.60 m (Pondus: length 8 m,
    // Lb = 5.6 m braces the weak axis); curves b (y), c (z); χy 0.808, χz 0.671,
    // Nb,Rd 2 349.5 kN, NEd/Nb,Rd 0.85 (NEd 2000 kN).
    let p = props_cm(149.0, 25170.0, 8560.0, 1678.0, 570.9, 1869.0, 870.1, 185.0, 1_688_000.0);
    let h = HDims { d: 300.0, bf: 300.0, tw: 11.0, tf: 19.0, r: 27.0 };
    let (cy, cz, _) = codes::curves(h);
    // Table 6.2, h/b ≤ 1.2 and tf ≤ 100 mm: curve b about y, c about z
    assert_eq!(format!("{cy:?}/{cz:?}"), "B/C");
    let m = codes::ec3_member(1, &p, h, 235.0, &ec3_dem(8.0, 5.6, false), 1.0, true);
    let r = ec3_run(m, -2000.0, 0.0, 0.0);
    row("EN 1993-1-1", "SBE A.5 HEB300", "SBE Multi-Storey Part 4 (2010), A.5, p. 4-96", "χy", "-", 0.808, r.chi_y, 0.5);
    row("EN 1993-1-1", "SBE A.5 HEB300", "SBE Multi-Storey Part 4 (2010), A.5, p. 4-96", "χz", "-", 0.671, r.chi_z, 0.5);
    row("EN 1993-1-1", "SBE A.5 HEB300", "SBE Multi-Storey Part 4 (2010), A.5, p. 4-97", "Nb,Rd (6.3.1.1)", "kN", 2349.5, r.nb_rd / 1e3, 0.5);
    row("EN 1993-1-1", "SBE A.5 HEB300", "SBE Multi-Storey Part 4 (2010), A.5, p. 4-97", "NEd/Nb,Rd (printed to 2 figures)", "-", 0.85, r.compression_ratio, 1.0);
}

#[test]
fn ec3_p364_ex9_class3_ukc() {
    // [P364] Ex. 9, 356×368×129 UKC S355 (h 355.6, b 368.6, tw 10.4, tf 17.5, r 15.2 mm),
    // fy = 345 MPa (16 < t ≤ 40, given to Pondus), A 164 cm², iy 15.6, iz 9.43 cm, L = 6 m
    // pinned; Class 3; Nc,Rd 5 658 kN; χz 0.65 (2 dp), Nb,z,Rd 3 678 kN; Nb,y,Rd 5 010 kN.
    let a = 164.0;
    let p = props_cm(a, a * 15.6 * 15.6, a * 9.43 * 9.43, 2264.0, 755.0, 2479.0, 1150.0, 153.0, 4_180_000.0);
    let h = HDims { d: 355.6, bf: 368.6, tw: 10.4, tf: 17.5, r: 15.2 };
    let (cls, _) = codes::ec3_class(h, 345.0, true);
    row("EN 1993-1-1", "P364 Ex.9 UKC 356x368x129", "SCI P364 (2009), Ex. 9, sheet 2", "section class (Table 5.2)", "-", 3.0, cls as f64, 1e-9);
    let m = codes::ec3_member(1, &p, h, 345.0, &ec3_dem(6.0, 6.0, false), 1.0, true);
    let r = ec3_run(m.clone(), -3500.0, 0.0, 0.0);
    row("EN 1993-1-1", "P364 Ex.9 UKC 356x368x129", "SCI P364 (2009), Ex. 9, sheet 4 (χ to 2 dp)", "χz", "-", 0.65, r.chi_z, 0.8);
    row("EN 1993-1-1", "P364 Ex.9 UKC 356x368x129", "SCI P364 (2009), Ex. 9, sheet 4", "Nb,z,Rd", "kN", 3678.0, r.nb_rd / 1e3, 0.8);
    row("EN 1993-1-1", "P364 Ex.9 UKC 356x368x129", "SCI P364 (2009), Ex. 9 (Blue Book)", "Nb,y,Rd", "kN", 5010.0, r.chi_y * a * 1e-4 * 345e6 / 1e3, 0.5);
    let r = ec3_run(m, 3500.0, 0.0, 0.0);
    row("EN 1993-1-1", "P364 Ex.9 UKC 356x368x129", "SCI P364 (2009), Ex. 9, sheet 3", "Nc,Rd = A fy/γM0", "kN", 5658.0, 3500.0 / r.tension_ratio, 0.5);
}

#[test]
fn ec3_sbe_ms8_ex2_ipe450_beam_column() {
    // [SBE-MS8] Ex. 2, pp. 8-18 – 8-22: IPE 450 S355 (h 450, b 190, tw 9.4, tf 14.6, r 21),
    // A 98.8 cm², Wpl,y 1702 cm³, iy 18.5 cm, Iz 1676 cm⁴, It 66.9 cm⁴, Iw 791 000 cm⁶;
    // L = 1.7 m, NEd 127 kN, My,Ed 356 kNm uniform; χy 1.0, χz 0.865, Nb,y,Rd 3 507,
    // Nb,z,Rd 3 034 kN; Cmy = CmLT = 1.0; kyy = 0.997, kzy = 0.997 (Table B.2).
    let a = 98.8;
    let p = props_cm(a, a * 18.5 * 18.5, 1676.0, 1500.0, 176.0, 1702.0, 276.0, 66.9, 791_000.0);
    let h = HDims { d: 450.0, bf: 190.0, tw: 9.4, tf: 14.6, r: 21.0 };
    let st: Vec<(f64, f64, f64, f64, f64, f64)> = (0..=4).map(|k| (1.7 * k as f64 / 4.0, -127.0, 0.0, 0.0, 356.0, 0.0)).collect();
    let dem = SteelDemand { uls: vec![("ULS".into(), st.clone())], ..ec3_dem(1.7, 1.7, false) };
    let c1 = codes::ec3_c1(&dem, &st);
    closed("EN 1993-1-1", "SBE-MS8 Ex.2 IPE450", "uniform moment: C1 = 1 (SN003)", "C1", "-", 1.0, c1, 1e-9);
    let mut m = codes::ec3_member(1, &p, h, 355.0, &dem, c1, true);
    m.ltb = codes::ec3_ltb(&dem, &st);
    // the class as check_ec3 takes it, from the station's N and M (Table 5.2, α and ψ)
    let (cls, sc) = codes::ec3_class_nm(h, &p, 355.0, 127e3, 356e6);
    m.section_class = sc;
    row("EN 1993-1-1", "SBE-MS8 Ex.2 IPE450", "SBE Multi-Storey Part 8 (2010), App. A Ex. 2, p. 8-18", "section class under N + M (Table 5.2)", "-", 1.0, cls as f64, 1e-9);
    let r = ec3_run(m, -127.0, 356.0, 0.0);
    let src = "SBE Multi-Storey Part 8 (2010), App. A Ex. 2, pp. 8-19 – 8-21";
    row("EN 1993-1-1", "SBE-MS8 Ex.2 IPE450", src, "χy", "-", 1.0, r.chi_y, 0.5);
    row("EN 1993-1-1", "SBE-MS8 Ex.2 IPE450", src, "χz", "-", 0.865, r.chi_z, 0.5);
    row("EN 1993-1-1", "SBE-MS8 Ex.2 IPE450", src, "Nb,y,Rd", "kN", 3507.0, r.chi_y * a * 1e-4 * 355e6 / 1e3, 0.5);
    row("EN 1993-1-1", "SBE-MS8 Ex.2 IPE450", src, "Nb,z,Rd", "kN", 3034.0, r.nb_rd / 1e3, 0.5);
    row("EN 1993-1-1", "SBE-MS8 Ex.2 IPE450", src, "kyy (Annex B, Table B.2)", "-", 0.997, r.k_yy, 0.5);
    row("EN 1993-1-1", "SBE-MS8 Ex.2 IPE450", src, "kzy (Annex B, Table B.2)", "-", 0.997, r.k_zy, 0.5);
}

// ------------------------------------------------------------------ EN 1992-1-1 cross-check

fn jrc_beam(b: f64, as_mm2: f64, face: &str, zone: &str, stirrups: Option<f64>) -> pondus_core::rc::BeamDesign {
    use pondus_core::rc::*;
    BeamDesign {
        member: 1,
        bim_id: None,
        b,
        h: 0.400,
        span: 6.0,
        flexure: vec![FlexZone { face: face.into(), zone: zone.into(), mu: 0.0, as_req: as_mm2, as_prov: as_mm2, bars: String::new(), ratio: 0.0 }],
        shear: stirrups.map(|s| vec![ShearZone { zone: "ends".into(), vu: 0.0, phi_vc: 0.0, stirrups: String::new(), s, ratio: 0.0 }]).unwrap_or_default(),
        torsion: None,
        ec2: None,
        h_min: 0.0,
        support: String::new(),
        defl_live: 0.0,
        defl_total: 0.0,
        limit_live: 0.0,
        limit_total: 0.0,
        ratio: 0.0,
        governs: String::new(),
        ok: true,
        notes: vec![],
    }
}

fn jrc_demand(x: f64, m: f64, v: f64) -> pondus_core::rc::BeamDemand {
    pondus_core::rc::BeamDemand { span: 6.0, uls: vec![vec![(x, m, v, 0.0, 0.0)]], ..Default::default() }
}

#[test]
fn ec2_jrc_flexure_and_shear() {
    // [JRC-EC2] ch. 3: C25/30, B500, γc 1.5, γs 1.15, αcc 1.0, d = 372 mm (Pondus: h 400 mm,
    // cover 9 mm, its d = h − cover − 9 − 10 mm). §3.2.1.3 (pp. 57 – 58): mid-span beff
    // 2611.2 mm, MEd 89.3 kNm ↔ As 556 mm²; support bw 250 mm, MEd 132.9 kNm ↔ As 947 mm²
    // (the published (MEd, As) pairs: MRd(As) = MEd). §3.2.1.4 (pp. 58 – 60): bw 250, Asl 565
    // mm², VRd,c = 47.90 kN; asw = 317 mm²/m ↔ VRd,s = VEd,red = 115.52 kN (cot θ = 2.5).
    let cover = 0.009;
    let r = codes::check_ec2_beam(&jrc_beam(2.6112, 556.0, "bottom", "middle", None), &jrc_demand(3.0, 89.3, 0.0), 25.0, 500.0, 500.0, cover);
    row("EN 1992-1-1 (cross-check)", "JRC-EC2 3.2.1.3 T-beam", "Biasioli et al. (2014), §3.2.1.3, p. 57", "MRd(As = 556 mm²), mid-span", "kNm", 89.3, 89.3 / r.flexure, 0.5);
    let r = codes::check_ec2_beam(&jrc_beam(0.250, 947.0, "top", "left", None), &jrc_demand(0.5, -132.9, 0.0), 25.0, 500.0, 500.0, cover);
    row("EN 1992-1-1 (cross-check)", "JRC-EC2 3.2.1.3 T-beam", "Biasioli et al. (2014), §3.2.1.3, p. 58", "MRd(As = 947 mm²), support", "kNm", 132.9, 132.9 / r.flexure, 0.5);
    let r = codes::check_ec2_beam(&jrc_beam(0.250, 565.0, "bottom", "left", None), &jrc_demand(0.2, 0.0, 40.0), 25.0, 500.0, 500.0, cover);
    row("EN 1992-1-1 (cross-check)", "JRC-EC2 3.2.1.4 shear", "Biasioli et al. (2014), §3.2.1.4, p. 59", "VRd,c (6.2.a)", "kN", 47.90, 40.0 / r.shear, 0.5);
    // 2 legs of RB9 (127.2 mm²) at s = 127.2/0.317 = 401.3 mm give asw = 317 mm²/m
    let s = 2.0 * pondus_core::sections::bar_area(9.0) * 1e6 / 0.317;
    let r = codes::check_ec2_beam(&jrc_beam(0.250, 565.0, "bottom", "left", Some(s)), &jrc_demand(0.2, 0.0, 100.0), 25.0, 500.0, 500.0, cover);
    row("EN 1992-1-1 (cross-check)", "JRC-EC2 3.2.1.4 shear", "Biasioli et al. (2014), §3.2.1.4, p. 60", "VRd,s (6.8, asw 317 mm²/m, cot θ 2.5)", "kN", 115.52, 100.0 / r.shear, 0.5);
    // the stirrups' own grade: SR24 (fyt 235) under SD40 main bars (fy 390): VRd,s scales with
    // fywd, (6.8) — the engine takes one fyk, so Pondus scales Asw by fyt/fy
    let r1 = codes::check_ec2_beam(&jrc_beam(0.250, 565.0, "bottom", "left", Some(s)), &jrc_demand(0.2, 0.0, 100.0), 25.0, 390.0, 390.0, cover);
    let r2 = codes::check_ec2_beam(&jrc_beam(0.250, 565.0, "bottom", "left", Some(s)), &jrc_demand(0.2, 0.0, 100.0), 25.0, 390.0, 235.0, cover);
    closed("EN 1992-1-1 (cross-check)", "stirrup grade", "EN 1992-1-1 (6.8): VRd,s ∝ fywd", "VRd,s(fyt 235) / VRd,s(fyt 390)", "-", 235.0 / 390.0, r1.shear / r2.shear, 0.5);
}

// ------------------------------------------------------------------ connections

#[test]
fn shear_tab_by_hand() {
    // The single-plate shear tab of Pondus (connections::shear_tab) for 3-M20 at 75 mm, plate
    // 10 x 230, 2 x 6 mm E70 fillets, e = 75 mm, Vu = 150 kN on an H 400x200x8x13 (Fu 400),
    // against the clauses evaluated by hand:
    // bolt group, elastic method: Ip = Σy² = 2·75² = 11 250 mm², the end bolt carries
    // √((V/3)² + (V·e·75/Ip)²) = V·√(1/9 + 0.25) = 0.6009 V = 90.14 kN; φrn = 0.75·372·π·20²/4
    // = 87.65 kN → 1.028.
    // bearing on the 8 mm web (thinner than the 10 mm plate): 90.14 / (0.75·2.4·20·8·400) =
    // 0.7825. Plate shear: yielding 150/(0.6·245·230·10) = 0.4437, rupture
    // 150/(0.75·0.6·400·(230 − 3·22)·10) = 0.5081.
    let cs = pondus_core::connections::ConnSettings::default();
    let v = 150.0;
    let d = pondus_core::connections::shear_tab(1, "i", v, (0.400, 0.200, 0.008, 0.013, 0.016), 400.0, &cs);
    let end = v * (1.0f64 / 9.0 + 0.25).sqrt();
    let phi_rn = 0.75 * 372.0 * std::f64::consts::PI * 400.0 / 4.0 / 1e3;
    let _ = end;
    let n: f64 = d.bolts.split('-').next().unwrap().parse().unwrap();
    let depth = (n - 1.0) * 75.0 + 80.0;
    let ip: f64 = (0..n as usize).map(|k| (k as f64 - (n - 1.0) / 2.0) * 75.0).map(|y| y * y).sum();
    let ymax = (n - 1.0) / 2.0 * 75.0;
    let fb = ((v / n).powi(2) + (v * 75.0 * ymax / ip).powi(2)).sqrt();
    closed("Connections", &format!("shear tab {}", d.bolts), "AISC 360-16 J3.6, elastic bolt-group method", "bolt ratio", "-", fb / phi_rn, d.bolt_ratio, 0.5);
    closed("Connections", &format!("shear tab {}", d.bolts), "AISC 360-16 J3.10 (2.4 d t Fu, web 8 mm)", "bearing ratio", "-", fb / (0.75 * 2.4 * 20.0 * 8.0 * 400.0 / 1e3), d.bearing_ratio, 0.5);
    let py = v * 1e3 / (0.6 * 245.0 * depth * 10.0);
    let pr = v * 1e3 / (0.75 * 0.6 * 400.0 * (depth - n * 22.0) * 10.0);
    closed("Connections", &format!("shear tab {}", d.bolts), "AISC 360-16 J4.2 (yielding, rupture)", "plate shear ratio", "-", py.max(pr), d.plate_ratio, 0.5);
}

#[test]
fn ec3_net_section_closed_form() {
    // EN 1993-1-1 6.2.3(2)b: Nu,Rd = 0.9 Anet fu / γM2, γM2 = 1.25; H 300x150x6.5x9 (JIS), M20
    // bolts (d0 = 22 mm), two holes in each flange: Anet = A − 4·22·tf
    let (d, bf, tw, tf, r) = (300.0, 150.0, 6.5, 9.0, 13.0);
    let p = pondus_core::sections::h_props(d / 1e3, bf / 1e3, tw / 1e3, tf / 1e3, r / 1e3);
    let h = steel::HDims { d, bf, tw, tf, r };
    let anet = p.a * 1e6 - 4.0 * 22.0 * tf;
    let nu = 0.9 * anet * 400.0 / 1.25 / 1e3;
    let st = vec![(0.0, 300.0, 0.0, 0.0, 0.0, 0.0), (4.0, 300.0, 0.0, 0.0, 0.0, 0.0)];
    let dem = SteelDemand { tension: Some(Default::default()), uls: vec![("ULS".into(), st)], ..ec3_dem(4.0, 4.0, false) };
    let des = pondus_core::codes::check_ec3(1, None, "H 300x150x6.5x9", 0.0, &p, h, 245.0, 400.0, &dem, &DesignSettings::default());
    let r = des.checks.iter().find(|c| c.name.starts_with("tension net")).map(|c| c.ratio).unwrap();
    closed("EN 1993-1-1", "H 300x150x6.5x9, M20, 2 holes per flange", "EN 1993-1-1 6.2.3(2)b: 0.9 Anet fu/γM2", "Nu,Rd", "kN", nu, 300.0 / r, 0.01);
}
