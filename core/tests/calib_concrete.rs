//! Calibration of Pondus's concrete, timber and masonry design against published worked
//! examples (suite "concrete"). Every comparison is a row of the calibration recorder; the
//! sources, their editions and pages, and the reasons for any band wider than 0.5 % are in
//! calibration/NOTES_concrete.md.
//!
//! Units. Examples printed in US units are converted exactly (in = 25.4 mm, ksi = 6.894757 MPa,
//! kip = 4.448222 kN). Pondus works in the SI form of ACI 318 (ACI 318M), whose constants are
//! rounded: 0.17√f'c for 2√f'c psi (+2.4 %), 0.62√f'c for 7.5√f'c (−0.5 %), 1.4/fy for 200/fy
//! (+1.5 %), 0.35 bw/fyt for 50 bw/fyt (+1.5 %), 0.42√f'c for 5√f'c (+1.1 %), 15 mm for
//! 0.6 in (−1.6 %), and β1 = 0.85 − 0.05(f'c − 28)/7 in MPa, which differs from the psi form
//! by up to 0.6 %. Rows whose value depends on such a constant carry the band that constant
//! implies, and say so; quantities free of them (As, a, Icr, At/s, Al, coefficients) keep
//! 0.5 %.

mod calib;
use calib::{check, Row};

use pondus_core::model::*;
use pondus_core::rc::*;

const IN: f64 = 25.4;
const KSI: f64 = 6.894_757;
const KIP: f64 = 4448.222; // N
const KIPFT: f64 = KIP * 304.8; // N·mm
const IN2: f64 = IN * IN;
const IN4: f64 = IN2 * IN2;
const PSF: f64 = 0.047_880_26; // kPa

fn row(aspect: &str, case: &str, source: &str, quantity: &str, unit: &str, reference: f64, pondus: f64, tol: f64, kind: &str) {
    check(Row { suite: "concrete", aspect, case, source, quantity, unit, reference, pondus, tol, kind });
}

// ===================================================================== two-way slabs (Method 2)

/// ACI 318-63 Appendix A, Method 2, Table I (moment coefficients), as reproduced in the
/// lecture notes of H. K. Hussain, University of Basrah ("Analysis and Design of Two way
/// concrete slab", scan of the 1963 table; calibration/sources/aci318-63_method2_table1_basrah.png).
/// Per case: rows negative at continuous edges, negative at discontinuous edges, positive at
/// midspan; columns m = 1.0, 0.9, 0.8, 0.7, 0.6, 0.5 and less, then the long span (all m).
/// NaN = "—" in the table.
const N: f64 = f64::NAN;
const TABLE_I: [[[f64; 7]; 3]; 5] = [
    [[0.033, 0.040, 0.048, 0.055, 0.063, 0.083, 0.033], [N, N, N, N, N, N, N], [0.025, 0.030, 0.036, 0.041, 0.047, 0.062, 0.025]],
    [[0.041, 0.048, 0.055, 0.062, 0.069, 0.085, 0.041], [0.021, 0.024, 0.027, 0.031, 0.035, 0.042, 0.021], [0.031, 0.036, 0.041, 0.047, 0.052, 0.064, 0.031]],
    [[0.049, 0.057, 0.064, 0.071, 0.078, 0.090, 0.049], [0.025, 0.028, 0.032, 0.036, 0.039, 0.045, 0.025], [0.037, 0.043, 0.048, 0.054, 0.059, 0.068, 0.037]],
    [[0.058, 0.066, 0.074, 0.082, 0.090, 0.098, 0.058], [0.029, 0.033, 0.037, 0.041, 0.045, 0.049, 0.029], [0.044, 0.050, 0.056, 0.062, 0.068, 0.074, 0.044]],
    [[N, N, N, N, N, N, N], [0.033, 0.038, 0.043, 0.047, 0.053, 0.055, 0.033], [0.050, 0.057, 0.064, 0.072, 0.080, 0.083, 0.050]],
];

#[test]
fn method2_table_every_entry() {
    let src = "ACI 318-63 App. A Method 2 Table I (scan in Hussain, U. Basrah notes)";
    let cases = ["case 1 interior", "case 2 one edge disc.", "case 3 two edges disc.", "case 4 three edges disc.", "case 5 four edges disc."];
    let rows = ["neg. continuous edge", "neg. discontinuous edge", "pos. midspan"];
    let ms = [1.0, 0.9, 0.8, 0.7, 0.6, 0.5];
    let mut n = 0;
    for (c, case) in cases.iter().enumerate() {
        for (r, rname) in rows.iter().enumerate() {
            for (k, m) in ms.iter().enumerate() {
                let want = TABLE_I[c][r][k];
                if want.is_nan() {
                    continue;
                }
                let (s, _) = method2(*m, c);
                row("two-way slab coefficients", &format!("{case}, {rname}, short span, m = {m}"), src, "C", "-", want, s[r], 0.01, "published");
                n += 1;
            }
            let want = TABLE_I[c][r][6];
            if !want.is_nan() {
                let (_, l) = method2(0.7, c);
                row("two-way slab coefficients", &format!("{case}, {rname}, long span"), src, "C", "-", want, l[r], 0.01, "published");
                n += 1;
            }
            // "0.5 and less": m = 0.4 takes the m = 0.5 column
            let want = TABLE_I[c][r][5];
            if !want.is_nan() {
                let (s, _) = method2(0.4, c);
                row("two-way slab coefficients", &format!("{case}, {rname}, short span, m = 0.4 (0.5 and less)"), src, "C", "-", want, s[r], 0.01, "published");
                n += 1;
            }
        }
    }
    assert_eq!(n, 13 * 7 + 13);
}

fn slab(lx: f64, ly: f64, h: f64) -> Slab {
    Slab { id: 1, name: "S".into(), x0: 0.0, y0: 0.0, x1: lx, y1: ly, z: 3.0, h, dead: 0.0, live: 0.0, bim_id: None }
}

#[test]
fn method2_worked_examples() {
    let src = "Hussain, U. Basrah two-way slab notes, Method 2 Example 1 (interior panel 6.0 x 7.2 m)";
    // interpolation at m = 0.833 (the example's own rounding of 6/7.2)
    let (s, l) = method2(0.833, 0);
    row("two-way slab coefficients", "Ex. 1 interior, m = 0.833, short neg.", src, "C", "-", 0.04536, s[0], 0.1, "published");
    row("two-way slab coefficients", "Ex. 1 interior, m = 0.833, short pos.", src, "C", "-", 0.03402, s[2], 0.1, "published");
    row("two-way slab coefficients", "Ex. 1 interior, long neg.", src, "C", "-", 0.033, l[0], 0.01, "published");
    // the whole slab design: 200 mm slab (4.8 kPa) + 3 kPa dead, 10 kPa live, all edges
    // continuous: wu = 1.2 x 7.8 + 1.6 x 10 = 25.36 kPa, M = C wu S² with S = 6 m
    let mat = RcMat { fc: 21.0, ec: 4700.0 * 21f64.sqrt(), fy: 420.0, fyt: 420.0 };
    let set = DesignSettings::default();
    let d = design_slab(&slab(6.0, 7.2, 0.2), [true; 4], 7.8, 10.0, &mat, &set);
    row("two-way slab design", "Ex. 1 interior", src, "wu", "kPa", 25.36, d.wu, 0.5, "published");
    let mu = |dir: &str, loc: &str| d.strips.iter().find(|x| x.direction == dir && x.location.starts_with(loc)).map(|x| x.mu).unwrap_or(f64::NAN);
    // Pondus interpolates at the exact m = 0.8333; the example at 0.833
    row("two-way slab design", "Ex. 1 interior", src, "short-span negative Mu", "kN·m/m", 41.41, mu("short span", "continuous"), 0.5, "published");
    row("two-way slab design", "Ex. 1 interior", src, "short-span positive Mu", "kN·m/m", 31.06, mu("short span", "midspan"), 0.5, "published");
    row("two-way slab design", "Ex. 1 interior", src, "long-span negative Mu", "kN·m/m", 30.13, mu("long span", "continuous"), 0.5, "published");
    row("two-way slab design", "Ex. 1 interior", src, "long-span positive Mu", "kN·m/m", 22.82, mu("long span", "midspan"), 0.5, "published");

    // Example 2: a 6.1 m square corner panel (case 3, two adjacent edges discontinuous),
    // 150 mm, DL 7.1 kPa (self-weight included), LL 2 kPa: wu = 11.72 kPa
    let src = "Hussain, U. Basrah two-way slab notes, Method 2 Example 2 (corner panel 6.1 x 6.1 m)";
    let d = design_slab(&slab(6.1, 6.1, 0.15), [false, true, true, false], 7.1, 2.0, &mat, &set);
    row("two-way slab design", "Ex. 2 corner", src, "wu", "kPa", 11.72, d.wu, 0.5, "published");
    let mu = |dir: &str, loc: &str| d.strips.iter().find(|x| x.direction == dir && x.location.starts_with(loc)).map(|x| x.mu).unwrap_or(f64::NAN);
    // the notes label the 0.049 row "discontinuous" and the 0.025 row "continuous"; Table I
    // has them the other way round, and so does Pondus (the values agree)
    row("two-way slab design", "Ex. 2 corner", src, "negative Mu, continuous edge (C 0.049)", "kN·m/m", 21.37, mu("short span", "continuous"), 0.5, "published");
    row("two-way slab design", "Ex. 2 corner", src, "negative Mu, discontinuous edge (C 0.025)", "kN·m/m", 10.90, mu("short span", "discontinuous"), 0.5, "published");
    row("two-way slab design", "Ex. 2 corner", src, "positive Mu (C 0.037)", "kN·m/m", 16.14, mu("short span", "midspan"), 0.5, "published");
}

// ===================================================================== one-way slabs (6.5)

#[test]
fn one_way_coefficients() {
    // StructurePoint "One-Way Slab Analysis and Design (ACI 318-14)" (2025), after Wight,
    // Reinforced Concrete Mechanics and Design, 7th ed., Example 5-7: wu = 0.257 kip/ft per
    // foot strip, end clear span 13.08 ft, interior 13.83 ft; Table 6.5.2 / 6.5.3 (unchanged in
    // ACI 318-19). Pondus is given wu as live load alone so that 1.6 wl = 0.257 ksf.
    let src = "StructurePoint One-Way Slab (ACI 318-14) after Wight Ex. 5-7, Tables 1-2";
    let wu = 0.257 / 1.6 * 1000.0 * PSF; // kPa as live load
    let mat = RcMat { fc: 4.0 * KSI, ec: 4700.0 * (4.0 * KSI).sqrt(), fy: 60.0 * KSI, fyt: 60.0 * KSI };
    let set = DesignSettings::default();
    let kipft_ft = KIPFT / 304.8 / 1000.0; // kN·m/m per kip-ft/ft
    let kip_ft = KIP / 304.8; // kN/m per kip/ft
    let ft = 0.3048;
    // an end span (one long edge continuous)
    let end = design_slab(&slab(13.08 * ft, 40.0 * ft, 0.15), [true, true, true, false], 0.0, wu, &mat, &set);
    let get = |d: &SlabDesign, loc: &str| d.strips.iter().find(|x| x.direction == "short span" && x.location.starts_with(loc)).map(|x| x.mu).unwrap_or(f64::NAN);
    row("one-way slab", "end span", src, "Mu at the exterior support, wl²/24", "kN·m/m", 1.83 * kipft_ft, get(&end, "discontinuous"), 0.5, "published");
    row("one-way slab", "end span", src, "Vu at the first interior support, 1.15 wl/2", "kN/m", 1.93 * kip_ft, end.vu, 0.5, "published");
    // conservative choices, not equalities: the first interior support takes wl²/9 (two spans;
    // the panel does not know how many follow) against 1/10 here (more than two spans, ln the
    // mean of the adjacent clear spans), and the end span's midspan wl²/11 (unrestrained end)
    // against 1/14 (end integral with the support)
    let neg = get(&end, "continuous");
    let pos = get(&end, "midspan");
    assert!(neg >= 4.65 * kipft_ft && pos >= 3.14 * kipft_ft, "{neg} {pos}");
    eprintln!("one-way conservative: first interior support {neg:.3} vs published {:.3} kN·m/m; end midspan {pos:.3} vs {:.3}", 4.65 * kipft_ft, 3.14 * kipft_ft);
    // an interior span
    let int = design_slab(&slab(13.83 * ft, 40.0 * ft, 0.15), [true; 4], 0.0, wu, &mat, &set);
    row("one-way slab", "interior span", src, "Mu at midspan, wl²/16", "kN·m/m", 3.07 * kipft_ft, get(&int, "midspan"), 0.5, "published");
    row("one-way slab", "interior span", src, "Mu at the supports, wl²/11", "kN·m/m", 4.47 * kipft_ft, get(&int, "continuous"), 0.5, "published");
    row("one-way slab", "interior span", src, "Vu at the supports, wl/2", "kN/m", 1.78 * kip_ft, int.vu, 0.5, "published");
}

// ===================================================================== beams

#[test]
fn inti_2i6_singly_reinforced() {
    // INTI-CIRSOC (Hernández Balat, Bissio, Ortega 2005) Ejemplo 2.I.6, as recorded in
    // Stabileo's design_worked_examples.rs: H-25, ADN 420, bw 120, d 366, As 402 mm²:
    // a = 66.21 mm, kc = c/d = 0.213 (εt > 0.005, φ 0.90), Mn = 56.20 kN·m (the ACI 318 Whitney
    // block, identical in CIRSOC 201-2005 for this section)
    let src = "INTI-CIRSOC 201 Ej. 2.I.6 (2005), as recorded in Stabileo";
    let phimn = phi_mn(402.0, 120.0, 366.0, 25.0, 420.0) / 1e6;
    row("RC beam flexure", "2.I.6 singly reinforced", src, "φMn (φ = 0.90, Mn = 56.20)", "kN·m", 0.9 * 56.20, phimn, 0.5, "published");
    let a = 402.0 * 420.0 / (0.85 * 25.0 * 120.0);
    row("RC beam flexure", "2.I.6 singly reinforced", src, "kc = c/d with β1(25 MPa)", "-", 0.213, a / beta1(25.0) / 366.0, 0.5, "published");
}

#[test]
fn simply_supported_beam_flexure_shear_deflection() {
    // StructurePoint "Simply Supported Reinforced Concrete Beam Analysis and Design (ACI
    // 318-14)" (2025), after Chaallal, Reinforced Concrete Structures, 2nd ed. 2018:
    // b 12 in, h 20 in, d 17.56 in, f'c 4.35 ksi, fy 60 ksi, Mu 201.88 kip-ft, 3-#9.
    let src = "StructurePoint Simply Supported RC Beam (ACI 318-14) after Chaallal (2018)";
    let (b, d, fc, fy) = (12.0 * IN, 17.56 * IN, 4.35 * KSI, 60.0 * KSI);
    let (as_, comp) = flexure_steel(201.88 * KIPFT, b, d, 0.0, fc, fy);
    assert_eq!(comp, 0.0);
    row("RC beam flexure", "Chaallal SS beam", src, "As required", "mm²", 2.872 * IN2, as_, 0.5, "published");
    row("RC beam flexure", "Chaallal SS beam", src, "β1 at 4.35 ksi (SI form of Table 22.2.2.4.3; printed to 2 digits)", "-", 0.83, beta1(fc), 1.0, "published");
    row("RC beam flexure", "Chaallal SS beam", src, "As,min (9.6.1.2b governs; SI 1.4/fy for 200/fy)", "mm²", 0.702 * IN2, as_min_beam(b, d, fc, fy), 2.0, "published");
    // shear at d from the support: Vu = 28.52 kips, φVc = 20.85 kips, Vs = 10.23 kips,
    // (Av/s)min = 0.0100 in²/in, s,max = d/2 = 8.78 in
    let st = shear_terms(fc, b, d, 28.52 * KIP, 0.0, b * 20.0 * IN, fy);
    row("RC beam shear", "Chaallal SS beam", src, "φVc (SI 0.17 for 2√f'c psi: +2.4 %)", "kN", 20.85 * KIP / 1000.0, 0.75 * st.vc / 1000.0, 2.5, "published");
    row("RC beam shear", "Chaallal SS beam", src, "Vs required (inherits the φVc offset: −6.4 %)", "kN", 10.23 * KIP / 1000.0, st.vs / 1000.0, 7.0, "published");
    row("RC beam shear", "Chaallal SS beam", src, "(Av/s)min (SI 0.35 bw/fyt for 50 bw/fyt)", "mm²/mm", 0.0100 * IN2 / IN, st.av_s_min, 2.0, "published");
    row("RC beam shear", "Chaallal SS beam", src, "s,max = d/2", "mm", 8.78 * IN, st.s_max, 0.5, "published");
    // deflection: n = 7.25, kd = 6.37 in, Icr = 3 759 in⁴, Ig = 8 000 in⁴, Mcr = 32.98 kip-ft
    let (k, icr) = cracked_inertia(b, d, 3.0 * IN2, 7.25);
    row("RC beam deflection", "Chaallal SS beam", src, "kd", "mm", 6.37 * IN, k * d, 0.5, "published");
    row("RC beam deflection", "Chaallal SS beam", src, "Icr", "mm⁴", 3759.0 * IN4, icr, 0.5, "published");
    let ig = b * (20.0 * IN).powi(3) / 12.0;
    let mcr = 0.62 * fc.sqrt() * ig / (10.0 * IN);
    row("RC beam deflection", "Chaallal SS beam", src, "Mcr (SI 0.62√f'c for 7.5√f'c)", "kN·m", 32.98 * KIPFT / 1e6, mcr / 1e6, 1.0, "published");
    // Ie: Pondus implements ACI 318-19 Table 24.2.3.5; the example is ACI 318-14 (Branson,
    // 4 337 and 3 812 in⁴). The 318-19 expression is evaluated here independently with the
    // example's Icr, Ig, Mcr and Ma (64.06 and 142.19 kip-ft).
    for (lbl, ma) in [("D", 64.06), ("D + L", 142.19)] {
        let (icr, ig, mcr): (f64, f64, f64) = (3759.0, 8000.0, 32.98);
        let want = icr / (1.0 - (2.0 / 3.0 * mcr / ma).powi(2) * (1.0 - icr / ig));
        let got = effective_inertia(ma * KIPFT, mcr * KIPFT, icr * IN4, ig * IN4) / IN4;
        row("RC beam deflection", &format!("Chaallal SS beam, Ie under {lbl}"), "ACI 318-19 Table 24.2.3.5 on the example's Icr, Ig, Mcr, Ma", "Ie", "in⁴", want, got, 0.01, "closed-form");
        eprintln!("Ie under {lbl}: ACI 318-19 {got:.0} in⁴, ACI 318-14 (Branson, printed) {} in⁴", if lbl == "D" { 4337 } else { 3812 });
    }
}

#[test]
fn equilibrium_torsion() {
    // StructurePoint "Equilibrium Torsion in Beams (ACI 318-14)" (2026), after Wight, 7th
    // ed., Example 7-2: cantilever b 14 in, h 24 in, d 21.5 in, f'c 4 ksi, fy = fyt 60 ksi,
    // clear cover 1.5 in to #4 closed stirrups, Tu 28 kip-ft, Vu 57.14 kips at d. The torsion
    // clauses used are the same in ACI 318-19.
    let src = "StructurePoint Equilibrium Torsion (ACI 318-14) after Wight Ex. 7-2";
    let (b, h, d, fc, fy) = (14.0 * IN, 24.0 * IN, 21.5 * IN, 4.0 * KSI, 60.0 * KSI);
    let t = torsion_terms(b, h, 1.5 * IN, 0.5 * IN, fc, fy, fy, 28.0 * KIPFT / 1e6);
    let kft = KIPFT / 1e6;
    row("RC torsion", "Wight 7-2", src, "Aoh", "mm²", 215.25 * IN2, t.aoh, 0.5, "published");
    row("RC torsion", "Wight 7-2", src, "ph", "mm", 62.00 * IN, t.ph, 0.5, "published");
    row("RC torsion", "Wight 7-2", src, "Ao = 0.85 Aoh", "mm²", 182.96 * IN2, t.ao, 0.5, "published");
    row("RC torsion", "Wight 7-2", src, "φTth (SI 0.083√f'c)", "kN·m", 5.87 * kft, t.phi_tth, 0.5, "published");
    row("RC torsion", "Wight 7-2", src, "φTcr = 4 φTth", "kN·m", 23.49 * kft, t.phi_tcr, 0.5, "published");
    row("RC torsion", "Wight 7-2", src, "At/s per leg", "mm²/mm", 0.0204 * IN, t.at_s, 0.5, "published");
    row("RC torsion", "Wight 7-2", src, "Al (22.7.6.1b)", "mm²", 1.265 * IN2, t.al, 0.5, "published");
    row("RC torsion", "Wight 7-2", src, "Al,min (9.6.4.3; SI 0.42√f'c for 5√f'c: +1.1 % on a difference)", "mm²", 0.506 * IN2, t.al_min, 5.0, "published");
    let st = shear_terms(fc, b, d, 57.14 * KIP, 0.0, b * h, fy);
    row("RC torsion", "Wight 7-2", src, "φVc (SI 0.17: +2.4 %)", "kN", 28.56 * KIP / 1000.0, 0.75 * st.vc / 1000.0, 2.5, "published");
    row("RC torsion", "Wight 7-2", src, "(Av + 2At)/s min (SI 0.35 for 50 psi)", "mm²/mm", 0.0117 * IN, st.av_s_min, 2.0, "published");
    row("RC torsion", "Wight 7-2", src, "s,max = ph/8 (9.7.6.3.3)", "mm", 7.75 * IN, t.ph / 8.0, 0.5, "published");
    // 22.7.7.1: 325.55 psi against 474.34 psi
    let r = torsion_section_ratio(57.14 * KIP, 28.0 * KIPFT, b, d, st.vc, t.aoh, t.ph, fc);
    row("RC torsion", "Wight 7-2", src, "cross-section limit 325.55 / 474.34 psi", "-", 325.55 / 474.34, r, 1.0, "published");
}

// ===================================================================== columns

fn tied_rows() -> Vec<(f64, f64)> {
    vec![(2.5 * IN, 4.0 * IN2), (13.5 * IN, 4.0 * IN2)]
}

#[test]
fn tied_column_interaction_diagram() {
    // StructurePoint "Interaction Diagram - Tied Reinforced Concrete Column Design Strength
    // (ACI 318-19)" (2022): 16 x 16 in, f'c 5 ksi, fy 60 ksi, 4-#9 top and bottom at 2.5 in
    // from the faces. Control points at the printed neutral-axis depths; β1 = 0.80 in the
    // psi form, 0.804 in the SI form at 34.47 MPa (a +0.5 %), hence 1 %.
    let src = "StructurePoint Interaction Diagram Tied Column (ACI 318-19), points 1-6";
    let (w, dpt, fc, fy) = (16.0 * IN, 16.0 * IN, 5.0 * KSI, 60.0 * KSI);
    let rows = tied_rows();
    let ast = 8.0 * IN2;
    let (p0, cap) = column_p0(w * dpt, ast, fc, fy);
    row("RC column P-M", "tied 16x16", src, "P0", "kip", 1534.0, p0 / KIP, 0.5, "published");
    row("RC column P-M", "tied 16x16", src, "φPn,max = 0.80 φ P0", "kip", 797.7, cap / KIP, 0.5, "published");
    for (name, c, pn, mn) in [("fs = 0", 13.5, 957.4, 261.33), ("fs = 0.5 fy", 10.04, 649.1, 338.54), ("fs = fy (balanced)", 7.99, 416.8, 385.81), ("εt = εy + 0.003", 5.02, 190.7, 318.61)] {
        let (p, m, _) = section_at(c * IN, w, dpt, &rows, fc, fy);
        row("RC column P-M", &format!("tied 16x16, {name}"), src, "Pn", "kip", pn, p / KIP, 1.0, "published");
        row("RC column P-M", &format!("tied 16x16, {name}"), src, "Mn", "kip-ft", mn, m / KIPFT, 1.0, "published");
    }
    let (m0, et0) = column_mn_nominal(w, dpt, &rows, fc, fy, 0.0);
    row("RC column P-M", "tied 16x16, pure bending", src, "Mn at Pn = 0", "kip-ft", 237.73, m0 / KIPFT, 1.0, "published");
    row("RC column P-M", "tied 16x16, pure bending", src, "φ (εt 0.00946 > εy + 0.003)", "-", 0.90, phi_tied(et0, fy), 0.01, "published");
    // the design search itself: φMn at the printed φPn of each point
    for (name, phipn, phimn) in [("fs = 0", 622.3, 169.86), ("fs = 0.5 fy", 421.9, 220.05), ("fs = fy (balanced)", 270.9, 250.77), ("εt = εy + 0.003", 171.6, 286.75), ("pure bending", 0.0, 213.96)] {
        let got = column_phi_mn(w, dpt, &rows, fc, fy, phipn * KIP) / KIPFT;
        row("RC column P-M", &format!("tied 16x16, {name}"), src, "φMn at the printed φPn", "kip-ft", phimn, got, 1.0, "published");
    }
}

#[test]
fn biaxial_load_contour_against_exact() {
    // StructurePoint "Biaxial Bending Interaction Diagrams for Rectangular RC Column Design
    // (ACI 318-19)" (2022), after Pincheira et al., Reinforced Concrete Design 9th ed., Ex.
    // 10.20.1: 16 x 20 in, f'c 6 ksi, fy 60 ksi, 10-#8 (rows of 3 on the 16 in faces, 4 on the
    // 20 in faces), cover 2.5 in to the bar centres. The exact skewed-axis solution at Pn =
    // 426 kips with Mnx/Mny = 1.60 is Mnx = 321, Mny = 201 kip-ft (reference; spColumn 320.8 /
    // 200.9). Pondus designs biaxial bending by Bresler's load contour (α = 1.15) on its own
    // uniaxial capacities at the same Pn: this row measures that approximation.
    let src = "StructurePoint Biaxial Rectangular Column (ACI 318-19) after Pincheira Ex. 10.20.1";
    let ab = 0.79 * IN2;
    let (fc, fy) = (6.0 * KSI, 60.0 * KSI);
    let rows_x = bar_rows(20.0 * IN, 2.5 * IN, 3, 4, ab); // bending about x: depth 20 in
    let rows_y = bar_rows(16.0 * IN, 2.5 * IN, 4, 3, ab); // bending about y: depth 16 in
    let pn = 426.0 * KIP;
    let (mx0, _) = column_mn_nominal(16.0 * IN, 20.0 * IN, &rows_x, fc, fy, pn);
    let (my0, _) = column_mn_nominal(20.0 * IN, 16.0 * IN, &rows_y, fc, fy, pn);
    let a = 1.15;
    let mx = 1.0 / ((1.0 / mx0).powf(a) + (1.0 / (1.6 * my0)).powf(a)).powf(1.0 / a);
    eprintln!("biaxial: uniaxial Mnx0 {:.1}, Mny0 {:.1} kip-ft at Pn 426 kips; load contour Mnx {:.1}, Mny {:.1}", mx0 / KIPFT, my0 / KIPFT, mx / KIPFT, mx / 1.6 / KIPFT);
    row("RC column biaxial", "Pincheira 10.20.1", src, "Mnx by load contour α 1.15 vs exact", "kip-ft", 321.0, mx / KIPFT, 15.0, "independent");
    row("RC column biaxial", "Pincheira 10.20.1", src, "Mny by load contour α 1.15 vs exact", "kip-ft", 201.0, mx / 1.6 / KIPFT, 15.0, "independent");
    assert!(mx / KIPFT <= 321.0 * 1.0, "the load contour should not exceed the exact capacity here");
}

#[test]
fn slender_column_non_sway() {
    // StructurePoint "Slenderness Effects for Columns in Non-Sway Frame - Moment Magnification
    // Method (ACI 318-19)" (2020), after Wang et al., Reinforced Concrete Design 8th ed., Ex.
    // 13.17.3: 17 x 17 in, f'c 3 ksi, Pu 525 kips, M1 = 0, M2 = 105 kip-ft, k = 0.959,
    // lu = 12 ft − 24 in. Printed: Mmin = 48.56 kip-ft, Cm = 0.6, δns = 1.00, Mc = 105 kip-ft.
    // The example takes (EI)eff by 6.6.4.4.4(b) with βdns = 0.4 (Pc = 7 871 kips); Pondus
    // uses (a), 0.4 EcIg/(1 + βdns), with βdns = 0.6 assumed in design.
    let src = "StructurePoint Slender Column Non-Sway (ACI 318-19) after Wang Ex. 13.17.3";
    let (dim, klu) = (17.0 * IN, 0.959 * (144.0 - 24.0) * IN);
    let ec = 57.0 * 3000f64.sqrt() * KSI; // 3 122 ksi, as printed
    let pu = 525.0 * KIP;
    let pc = critical_load(ec, dim, dim, klu, 0.4);
    let want_pc = std::f64::consts::PI.powi(2) * (0.4 * ec * dim.powi(4) / 12.0 / 1.4) / klu.powi(2);
    row("RC column slenderness", "Wang 13.17.3", "ACI 318-19 6.6.4.4.2 with (EI)eff 6.6.4.4.4(a), βdns 0.4", "Pc (option a)", "kip", want_pc / KIP, pc / KIP, 0.01, "closed-form");
    let (mc, delta) = moment_magnifier(pu, 0.0, 105.0 * KIPFT / 1e6, klu, dim, pc, false);
    row("RC column slenderness", "Wang 13.17.3", src, "Mc = δns M2", "kip-ft", 105.0, mc / KIPFT, 0.5, "published");
    row("RC column slenderness", "Wang 13.17.3", src, "δns", "-", 1.00, delta, 0.5, "published");
    // the minimum moment alone (M2 = 0): Pu (0.6 + 0.03 h) = 48.56 kip-ft; SI 15 mm for 0.6 in
    let (mmin, _) = moment_magnifier(pu, 0.0, 0.0, klu, dim, pc, false);
    row("RC column slenderness", "Wang 13.17.3", src, "M2,min = Pu (15 + 0.03h) (SI 15 mm for 0.6 in)", "kip-ft", 48.56, mmin / KIPFT, 1.0, "published");
    // the magnifier where it applies: klu = 1.8 x the example's (at 3 x, Pu > 0.75 Pc and
    // Pondus reports the column unstable, as it should), Cm 0.6, (EI)eff as above
    let klu3 = 1.8 * klu;
    let pc3 = critical_load(ec, dim, dim, klu3, 0.4);
    let (_, d3) = moment_magnifier(pu, 0.0, 105.0 * KIPFT / 1e6, klu3, dim, pc3, false);
    let want = (0.6 / (1.0 - pu / (0.75 * pc3))).max(1.0);
    assert!(d3 > 1.0, "{d3}");
    let (inf, _) = moment_magnifier(pu, 0.0, 105.0 * KIPFT / 1e6, 3.0 * klu, dim, critical_load(ec, dim, dim, 3.0 * klu, 0.4), false);
    assert!(inf.is_infinite(), "Pu > 0.75 Pc must be unstable");
    row("RC column slenderness", "Wang 13.17.3 with 1.8 klu", "ACI 318-19 6.6.4.5.2-3, Cm = 0.6 + 0.4 M1/M2", "δns", "-", want, d3, 0.01, "closed-form");
}

// ===================================================================== footings

#[test]
fn spread_footing_flexure() {
    // StructurePoint "Reinforced Concrete Spread Footing (Isolated Footing) Analysis and Design"
    // (ACI 318-14, 2018), after Wight 7th ed. Example 15-2: 11.17 ft square, 32 in thick, d =
    // 28 in, 18 in column, f'c 3 ksi, fy 60 ksi, qnu = 7 310 psf: Mu at the column face = 954
    // kip-ft over the width, As = 7.76 in² (As,min 7.72 in²). Flexure is the same in ACI
    // 318-19; the 318-19 one-way and punching shear add the size factor λs, so the printed
    // shear capacities (318-14) are not compared (the shear checks are the engine's).
    let src = "StructurePoint Square Spread Footing (ACI 318-14) after Wight Ex. 15-2";
    let ft = 0.3048;
    let (bw, c, d) = (11.17 * ft, 58.0 * IN / 1000.0, 28.0 * IN);
    let qu = 7310.0 * PSF;
    let mu_m = pondus_core::footing::face_moment(qu, c);
    row("RC footing", "Wight 15-2", src, "Mu at the column face over the width", "kip-ft", 954.0, mu_m * bw * 1e6 / KIPFT, 0.5, "published");
    let (as_m, _) = flexure_steel(mu_m * 1e6, 1000.0, d, 0.0, 3.0 * KSI, 60.0 * KSI);
    row("RC footing", "Wight 15-2", src, "As over the width", "in²", 7.76, as_m * bw / IN2, 0.5, "published");
    row("RC footing", "Wight 15-2", src, "As,min = 0.0018 b h over the width", "in²", 7.72, 0.0018 * 1000.0 * 32.0 * IN * bw / IN2, 0.5, "published");
}

// ===================================================================== Wood–Armer

/// Wood (1968), Concrete 2(2), and Armer's discussion (1968): the normal rules, then the
/// special cases when a design moment would be of the wrong sign. Evaluated here
/// independently for each branch.
fn wood_armer_ref(mx: f64, my: f64, mxy: f64) -> (f64, f64, f64, f64) {
    let a = mxy.abs();
    let (mut bx, mut by) = (mx + a, my + a);
    if bx < 0.0 {
        bx = 0.0;
        by = my + mxy * mxy / mx.abs();
    } else if by < 0.0 {
        by = 0.0;
        bx = mx + mxy * mxy / my.abs();
    }
    let (mut tx, mut ty) = (mx - a, my - a);
    if tx > 0.0 {
        tx = 0.0;
        ty = my - mxy * mxy / mx.abs();
    } else if ty > 0.0 {
        ty = 0.0;
        tx = mx - mxy * mxy / my.abs();
    }
    (bx.max(0.0), by.max(0.0), (-tx).max(0.0), (-ty).max(0.0))
}

#[test]
fn wood_armer_branches() {
    let src = "Wood (1968) / Armer (1968), normal and special rules";
    for (case, mx, my, mxy) in [("normal, both faces", 10.0, 5.0, 3.0), ("bottom x special", -10.0, 6.0, 2.0), ("bottom y special", 6.0, -10.0, 2.0), ("top x special", 10.0, -2.0, 3.0), ("top y special", -2.0, 10.0, 3.0), ("pure twist", 0.0, 0.0, 4.0), ("hogging both", -12.0, -8.0, 2.5)] {
        let want = wood_armer_ref(mx, my, mxy);
        let got = pondus_core::slabfe::wood_armer(mx, my, mxy);
        for (q, w, g) in [("Mx* bottom", want.0, got.0), ("My* bottom", want.1, got.1), ("Mx* top", want.2, got.2), ("My* top", want.3, got.3)] {
            row("Wood-Armer", case, src, q, "kN·m/m", w, g, if w.abs() > 1e-12 { 0.01 } else { 1e-9 }, "closed-form");
        }
    }
}

// ===================================================================== timber and masonry

fn member(kind: MemberKind, k: Option<f64>, rebar: Option<f64>) -> Member {
    Member { id: 1, i: 1, j: 2, section: 1, material: 1, kind, hinge_i: true, hinge_j: true, lb: None, k, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar, bim_id: None, tension: None, role: Default::default(), group: None }
}

#[test]
fn timber_column_nds() {
    // Dlubal knowledge base 001674, "Timber Column Design as per the 2018 NDS Standard":
    // Alaska cedar 7.5 x 7.5 in, L = 10 ft pinned, P = 30 kips; adjusted Fc before CP =
    // 673.40 psi, E'min = 418 ksi, le/d = 16, FcE = 1 342.17 psi, CP = 0.866, F'c = 585.86
    // psi, fc = 533.33 psi. Pondus applies CD = CM = Ct = CF = 1, so the example's adjusted
    // values are given as the grade values.
    let src = "Dlubal KB 001674, NDS 2018 timber column";
    let psi = KSI / 1000.0;
    let mut mat = Material::timber(1);
    mat.fc = 673.40 * psi;
    mat.e_min = 418.0 * KSI;
    mat.e = 1140.0 * KSI;
    let (b, len) = (7.5 * IN / 1000.0, 10.0 * 0.3048);
    let st: pondus_core::othermat::Stations = vec![("D + L".into(), len / 2.0, -30.0 * KIP / 1000.0, 0.0, 0.0, 0.0, 0.0)];
    let d = pondus_core::othermat::timber(&member(MemberKind::Column, Some(1.0), None), &mat, b, b, len, &st, &DesignSettings::default());
    let r = d.checks.iter().find(|c| c.name.starts_with("compression")).unwrap().ratio;
    // CP from the ratio: fc / (Fc* r); the printed F'c = 585.86 psi is not 673.40 x 0.866
    // (= 583.2): a misprint, so CP and η are compared
    let cp = 533.33 / r / 673.40;
    row("timber NDS", "column CP", src, "CP (NDS 3.7-1, c = 0.8; FcE 1 342 psi)", "-", 0.866, cp, 0.1, "published");
    row("timber NDS", "column CP", src, "η = fc / F'c", "-", 0.913, r, 0.15, "published");
}

#[test]
fn masonry_flexure_tms402() {
    // TMS 402-16 9.3.2: a = As fy / (0.80 f'm b), Mn = As fy (d − a/2), φ = 0.90; Pondus puts
    // the bars at mid-thickness, d = t/2. A 1 m strip of 190 mm grouted CMU, f'm 13.8 MPa,
    // 2 x DB12 (226 mm²) at d = 95 mm, fy 390 MPa, Mu 5 kN·m. Evaluated here independently.
    let mat = Material::masonry(1, 13.8);
    let (b, t, as_, fy) = (1.0, 0.19, 226.0, 390.0);
    let st: pondus_core::othermat::Stations = vec![("1.2D + 1.6L".into(), 1.5, 0.0, 0.0, 0.0, 5.0, 0.0)];
    let d = pondus_core::othermat::masonry(&member(MemberKind::Column, Some(1.0), Some(as_)), &mat, b, t, 3.0, &st, &DesignSettings::default());
    let r = d.checks.iter().find(|c| c.name.starts_with("flexure")).unwrap().ratio;
    let a = as_ * fy / (0.80 * 13.8 * 1000.0);
    let mn = as_ * fy * (95.0 - a / 2.0) / 1e6;
    row("masonry TMS 402", "out-of-plane flexure, d = t/2", "TMS 402-16 9.3.2 (Whitney block 0.80 f'm)", "Mu / φMn", "-", 5.0 / (0.9 * mn), r, 0.5, "closed-form");
}
