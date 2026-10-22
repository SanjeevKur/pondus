//! Calibration of Pondus's design code against the design examples of the CSI verification
//! manual (Computers and Structures, Inc., *Software Verification Examples for ETABS 2016*,
//! docs.csiamerica.com/manuals/etabs/Software Verification.pdf). Each example prints an
//! independent hand calculation, which is the reference here, and ETABS's own result, which is
//! recorded beside it (`calib::program`, calibration/programs.csv) without being asserted.
//!
//! Every value passes through Pondus's own design functions: `steel::capacities` and
//! `steel::cb` (AISC 360-16), `rc::flexure_steel`, `rc::shear_terms`, `rc::as_min_beam`,
//! `rc::section_at`, `rc::phi_tied` and `rc::column_phi_mn` (ACI 318-19), `codes::ec3_member`,
//! `codes::check_ec3` and the engine's EN 1993-1-1 checker as Pondus calls it, and
//! `codes::check_ec2_beam` (EN 1992-1-1). The examples are written to AISC 360-10, ACI 318-14
//! and EN 199x:2004/2005; where the edition Pondus implements differs, the expected offset is
//! stated on the row. The extracted data, with every printed intermediate value, are in
//! calibration/sources/csi/specs/etabs_*.md.

mod calib;
use calib::{check, program, Row};

use pondus_core::codes;
use pondus_core::model::{DesignSettings, Props};
use pondus_core::rc::*;
use pondus_core::steel::{self, HDims, SteelDemand};

use dedaliano_engine::postprocess::ec3_check::*;

const IN: f64 = 25.4; // mm
const IN2: f64 = IN * IN;
const KSI: f64 = 6.894757; // MPa
const KIP: f64 = 4448.222; // N
const KIPIN: f64 = KIP * IN; // N·mm
const KIPFT: f64 = KIPIN * 12.0;
const SRC: &str = "CSI, Software Verification Examples for ETABS 2016";

#[allow(clippy::too_many_arguments)]
fn row(aspect: &str, case: &str, page: &str, quantity: &str, unit: &str, reference: f64, pondus: f64, tol: f64) {
    let source = format!("{SRC}, {page}");
    check(Row { suite: "csi", aspect, case, source: &source, quantity, unit, reference, pondus, tol, kind: "published" });
}

// ============================================================ AISC 360-10 Example 001

/// W18X50, A572 Gr. 50, simple span 35 ft, wu = 1.74 k/ft (Mu = 266.44 k-ft), unbraced
/// lengths 5, 11.667 and 35 ft (pp. 90 – 96; independent: AISC Design Examples F.1-2a). The
/// Cb of each length is the one the example prints; Pondus's own Cb is compared where the
/// segment is the whole span. Pondus takes E = 200 000 MPa (29 000 ksi = 199 948 MPa).
#[test]
fn aisc_360_ex001_w18x50_bending() {
    let case = "ETABS AISC 360-10 Ex. 001, W18X50";
    let page = "AISC 360-10 Example 001, pp. 90-96";
    // properties as the example uses them; plates d 18, bf 7.5, tw 0.355, tf 0.57 in and the
    // Manual's kdes 0.972 in (r = kdes − tf) set only the element slenderness
    let p = Props {
        a: 14.7 * IN2 * 1e-6,
        iy: 800.0 * IN.powi(4) * 1e-12,
        iz: 40.1 * IN.powi(4) * 1e-12,
        j: 1.24 * IN.powi(4) * 1e-12,
        cw: 3045.644 * IN.powi(6) * 1e-18,
        sy: 88.9 * IN.powi(3) * 1e-9,
        sz: 10.7 * IN.powi(3) * 1e-9,
        zy: 101.0 * IN.powi(3) * 1e-9,
        zz: 16.6 * IN.powi(3) * 1e-9,
        ry: 7.38 * IN * 1e-3,
        rz: 1.652 * IN * 1e-3,
    };
    let h = HDims { d: 18.0 * IN, bf: 7.5 * IN, tw: 0.355 * IN, tf: 0.57 * IN, r: (0.972 - 0.57) * IN };
    let l = 35.0 * 12.0 * IN / 1e3;
    // Cb of the whole span under a uniform load
    let st: Vec<(f64, f64)> = (0..=32).map(|k| l * k as f64 / 32.0).map(|x| (x, 4.0 * x * (l - x) / (l * l))).collect();
    let cb_span = steel::cb(&st, l);
    row("AISC 360 flexure", case, page, "Cb, Lb = 35 ft (whole span, F1-1)", "-", 1.136, cb_span, 0.2);
    program(case, "Cb, Lb = 35 ft", "-", 1.136, "ETABS", 1.138, cb_span);
    for (lb_ft, cb, want, etabs) in [(5.0, 1.002, 378.750, 378.750), (11.667, 1.014, 306.657, 307.124), (35.0, 1.136, 94.218, 94.377)] {
        let c = steel::capacities(&p, h, 50.0 * KSI, 65.0 * KSI, l, lb_ft * 12.0 * IN / 1e3, 1.0);
        // φMn = min(Cb·φMn(Cb = 1), φMp) (F2-2 / F2-3)
        let got = (cb * c.phi_mx1).min(c.phi_mpx) / KIPFT;
        let q = format!("φbMn, Lb = {lb_ft} ft, Cb {cb}");
        row("AISC 360 flexure", case, page, &q, "kip-ft", want, got, 0.5);
        program(case, &q, "kip-ft", want, "ETABS", etabs, got);
    }
    // the plastic plateau and the end of the inelastic range: Mn = Mp at the printed Lp
    // (5.835 ft) and Mn = 0.7 Fy Sx at the printed Lr (16.966 ft), with Cb = 1
    let at = |lb_ft: f64| steel::capacities(&p, h, 50.0 * KSI, 65.0 * KSI, l, lb_ft * 12.0 * IN / 1e3, 1.0).phi_mx1 / 0.9 / KIPIN;
    row("AISC 360 flexure", case, page, "Mn at the printed Lp = 5.835 ft (= Mp = 5050 kip-in)", "kip-in", 5050.0, at(5.835), 0.5);
    row("AISC 360 flexure", case, page, "Mn at the printed Lr = 16.966 ft (= 0.7 Fy Sx)", "kip-in", 0.7 * 50.0 * 88.9, at(16.966), 0.5);
}

// ============================================================ ACI 318-14 Example 001

/// Singly reinforced 10 × 16 in beam, d = 13.5 in, f'c 4 ksi, fy = fyt = 60 ksi, simple span
/// 10 ft under wu = 9.736 k/ft (pp. 288 – 292; independent: PCA Notes on ACI 318-14 Ex. 6.1).
/// Pondus implements the SI form of ACI 318-19, whose constants 0.17√f'c (2√f'c psi) and
/// 0.35 bw/fyt (50 bw/fyt psi) are 2.4 % and 1.6 % larger; the 318-19 Vc with ρw, 0.66
/// ρw^(1/3)√f'c, is 0.172√f'c here and does not govern over the 0.17√f'c Pondus takes.
#[test]
fn aci_318_ex001_beam_flexure_shear() {
    let case = "ETABS ACI 318-14 Ex. 001, 10x16 beam";
    let page = "ACI 318-14 Example 001, pp. 288-292";
    let (b, h, d, fc, fy) = (10.0 * IN, 16.0 * IN, 13.5 * IN, 4.0 * KSI, 60.0 * KSI);
    // the analysis: a 10 ft simple span through Pondus's analysis
    use pondus_core::analysis::analyze;
    use pondus_core::model::*;
    let mut m = Model::new("ACI 318-14 Ex. 001");
    m.cases[0].self_weight = false;
    let s = m.rect_section(b / 1e3, h / 1e3);
    let span = 120.0 * IN / 1e3;
    let (ni, nj) = (m.node_at([0.0, 0.0, 0.0]), m.node_at([span, 0.0, 0.0]));
    m.members.push(Member { id: 1, i: ni, j: nj, section: s, material: 1, kind: MemberKind::Beam, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    m.supports.push(Support { node: ni, fix: [true, true, true, true, false, false], springs: [0.0; 6], uplift: false });
    m.supports.push(Support { node: nj, fix: [false, true, true, true, false, false], springs: [0.0; 6], uplift: false });
    let wu = 9.736 * KIP / (12.0 * IN); // N/mm = kN/m
    m.loads.push(Load { case: 1, kind: LoadKind::Line { member: 1, wi: [0.0, 0.0, -wu], wj: [0.0, 0.0, -wu], a: None, b: None } });
    let a = analyze(&m).unwrap();
    let cr = a.case(1).unwrap();
    let mr = cr.member(1).unwrap();
    let mu = mr.stations.iter().map(|st| st.my.abs()).fold(0.0, f64::max) * 1e6; // N·mm
    // the shear at d from the support, interpolated between Pondus's stations
    let x = d / 1e3;
    let k = mr.stations.iter().position(|st| st.x >= x).unwrap();
    let (s0, s1) = (&mr.stations[k - 1], &mr.stations[k]);
    let vu = (s0.vz + (s1.vz - s0.vz) * (x - s0.x) / (s1.x - s0.x)).abs() * 1e3; // N
    row("RC beam (ETABS)", case, page, "Mu = wu l²/8 (Pondus analysis)", "kip-in", 1460.4, mu / KIPIN, 0.1);
    row("RC beam (ETABS)", case, page, "Vu at d from the support (Pondus analysis)", "kip", 37.727, vu / KIP, 0.1);
    let (as_, comp) = flexure_steel(1460.4 * KIPIN, b, d, 0.0, fc, fy);
    assert_eq!(comp, 0.0);
    row("RC beam (ETABS)", case, page, "As required (Whitney block, φ 0.9)", "in²", 2.37, as_ / IN2, 0.5);
    program(case, "As required", "in²", 2.37, "ETABS", 2.37, as_ / IN2);
    let a_blk = as_ * fy / (0.85 * fc * b);
    row("RC beam (ETABS)", case, page, "depth of the stress block a", "in", 4.183, a_blk / IN, 0.5);
    row("RC beam (ETABS)", case, page, "As,min = 200 bw d/fy (SI 1.4/fy: +1.6 %)", "in²", 0.450, as_min_beam(b, d, fc, fy) / IN2, 2.0);
    let st = shear_terms(fc, b, d, 37.727 * KIP, 0.0, b * h, fy);
    row("RC beam (ETABS)", case, page, "φVc = φ 2√f'c bw d (SI 0.17√f'c: +2.4 %)", "kip", 12.807, 0.75 * st.vc / KIP, 2.6);
    row("RC beam (ETABS)", case, page, "φVs,max = φ 8√f'c bw d (SI 0.66√f'c: −0.6 %)", "kip", 51.229, 0.75 * st.vs_max / KIP, 1.0);
    row("RC beam (ETABS)", case, page, "(Av/s)min = 50 bw/fyt (SI 0.35 bw/fyt: +1.6 %)", "in²/in", 0.0083, st.av_s_min / IN, 2.5);
    let av_s = st.vs / (fy * d);
    row("RC beam (ETABS)", case, page, "Av/s = (Vu − φVc)/(φ fyt d) (inherits the φVc offset)", "in²/in", 0.041, av_s / IN, 2.5);
    program(case, "Av/s", "in²/in", 0.041, "ETABS", 0.041, av_s / IN);
}

// ============================================================ ACI 318-14 Example 002

/// Tied 14 × 22 in column, 4 in² of bars on each 14 in face at 2.5 in, f'c 4 ksi, fy 60 ksi,
/// under Pu = 398.4 kips, Mu = 332 kip-ft (e = 10 in), pp. 293 – 296. The hand calculation
/// finds c = 13.45 in, Pn = 612.9 kips, Mn = 510.8 kip-ft and φ = 0.65, so φPn = Pu and the
/// demand/capacity ratio is 1.00.
#[test]
fn aci_318_ex002_column_pm() {
    let case = "ETABS ACI 318-14 Ex. 002, 14x22 column";
    let page = "ACI 318-14 Example 002, pp. 293-296";
    let (w, depth, fc, fy) = (14.0 * IN, 22.0 * IN, 4.0 * KSI, 60.0 * KSI);
    let rows = vec![(2.5 * IN, 4.0 * IN2), (19.5 * IN, 4.0 * IN2)];
    let (pn, mn, et) = section_at(13.45 * IN, w, depth, &rows, fc, fy);
    row("RC column (ETABS)", case, page, "Pn at c = 13.45 in", "kip", 612.9, pn / KIP, 0.5);
    row("RC column (ETABS)", case, page, "Mn at c = 13.45 in", "kip-ft", 510.8, mn / KIPFT, 0.5);
    row("RC column (ETABS)", case, page, "εt at c = 13.45 in", "-", 0.00135, et, 0.5);
    row("RC column (ETABS)", case, page, "φ (compression-controlled)", "-", 0.65, phi_tied(et, fy), 0.01);
    // the capacity on the radial line e = Mu/Pu = 10 in: the c where Mn/Pn = e
    let e = 10.0 * IN;
    let (mut lo, mut hi) = (0.2 * depth, 3.0 * depth);
    for _ in 0..100 {
        let mid = (lo + hi) / 2.0;
        let (p, m, _) = section_at(mid, w, depth, &rows, fc, fy);
        if m / p > e {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (p, _, et) = section_at((lo + hi) / 2.0, w, depth, &rows, fc, fy);
    let dc = 398.4 * KIP / (phi_tied(et, fy) * p);
    row("RC column (ETABS)", case, page, "demand/capacity on the line e = 10 in", "-", 1.00, dc, 0.5);
    program(case, "demand/capacity", "-", 1.00, "ETABS", 1.000, dc);
    // the way Pondus's column design reads it: φMn at the factored Pu
    let phimn = column_phi_mn(w, depth, &rows, fc, fy, 398.4 * KIP) / KIPFT;
    row("RC column (ETABS)", case, page, "φMn at Pu = 398.4 kips (column design search)", "kip-ft", 332.0, phimn, 0.5);
}

// ============================================================ ACI 318-14 Wall-001

/// Rectangular wall pier 12 × 60 in, two #9 bars at each end and #4 at 14 in on each face
/// (five layers: 2.00, 0.40, 0.40, 0.40, 2.00 in² at 2, 16, 30, 44, 58 in), f'c 4 ksi, fy 60
/// ksi, under Pu = 735 kips and Mu = 1504 kip-ft in plane (pp. 432 – 435). The hand
/// calculation finds c = 30.1 in, Pn = 1035 kips, Mn = 2116 kip-ft and φ = 0.711 (ACI 318-14:
/// εt = 0.00278 between εy and 0.005). Pondus's walls use the same strain-compatibility
/// routine as its columns; ACI 318-19 moves the tension-controlled limit to εty + 0.003, so φ
/// is 0.709 (−0.3 %).
#[test]
fn aci_318_wall001_pier_pm() {
    let case = "ETABS ACI 318-14 Wall-001, 12x60 pier";
    let page = "ACI 318-14 Wall-001, pp. 432-435";
    let (t, lw, fc, fy) = (12.0 * IN, 60.0 * IN, 4.0 * KSI, 60.0 * KSI);
    let rows: Vec<(f64, f64)> = [(2.0, 2.0), (16.0, 0.4), (30.0, 0.4), (44.0, 0.4), (58.0, 2.0)].iter().map(|r| (r.0 * IN, r.1 * IN2)).collect();
    // the hand calculation solves equilibrium and moments about As5 together, so its state
    // lies on the line e = Mu/Pu = 24.54 in: Pondus's section_at on that line
    let e = 24.54 * IN;
    let (mut lo, mut hi) = (0.2 * lw, 2.0 * lw);
    for _ in 0..100 {
        let mid = (lo + hi) / 2.0;
        let (p, m, _) = section_at(mid, t, lw, &rows, fc, fy);
        if m / p > e {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let c = (lo + hi) / 2.0;
    let (pn, mn, et) = section_at(c, t, lw, &rows, fc, fy);
    // The hand iteration is not converged: at its c = 30.1 in equilibrium gives Pn = 1035
    // kips but moments about As5 give 1026 kips, and the section's own moment there is 2077
    // kip-ft (e = 24.1 in), not Pn·e = 2116. Its c, Pn and Mn therefore carry 2-2.5 %; ETABS prints D/C 1.007.
    row("RC wall (ETABS)", case, page, "c on the line e = 24.54 in (hand iteration not converged)", "in", 30.1, c / IN, 2.5);
    row("RC wall (ETABS)", case, page, "Pn on the line e = 24.54 in", "kip", 1035.0, pn / KIP, 2.5);
    row("RC wall (ETABS)", case, page, "Mn on the line e = 24.54 in", "kip-ft", 2116.0, mn / KIPFT, 2.5);
    row("RC wall (ETABS)", case, page, "φ at Pondus's c (εt 0.0029; 318-19 limit εty + 0.003)", "-", 0.711, phi_tied(et, fy), 1.5);
    let dc = 735.0 * KIP / (phi_tied(et, fy) * pn);
    row("RC wall (ETABS)", case, page, "demand/capacity on the line e = 24.54 in", "-", 1.00, dc, 2.0);
    program(case, "demand/capacity", "-", 1.00, "ETABS", 1.007, dc);
    let phimn = column_phi_mn(t, lw, &rows, fc, fy, 735.0 * KIP) / KIPFT;
    row("RC wall (ETABS)", case, page, "φMn at Pu = 735 kips (inherits the hand calculation's 2 %)", "kip-ft", 1504.0, phimn, 2.5);
}

// ============================================================ EN 1993-1-1

fn ec3_dem(length: f64, lb: f64) -> SteelDemand {
    SteelDemand { tension: None, length, lb, k: 1.0, uls: vec![], defl_live: 0.0, defl_total: 0.0, cantilever: false, top_flange_load: false }
}

fn ec3_run(m: Ec3MemberData, n_kn: f64, my_knm: f64, v_kn: f64) -> Ec3CheckResult {
    check_ec3_members(&Ec3CheckInput { members: vec![m], forces: vec![Ec3DesignForces { element_id: 1, n_ed: Some(n_kn * 1e3), my_ed: Some(my_knm * 1e3), mz_ed: Some(0.0), v_ed: Some(v_kn * 1e3) }] }).remove(0)
}

/// The ratio of one named check of `codes::check_ec3` (the design path) for one station.
fn design_check(p: &Props, h: HDims, fy: f64, length: f64, n_kn: f64, v_kn: f64, my_knm: f64, name: &str) -> f64 {
    let st = vec![(0.0, n_kn, 0.0, v_kn, my_knm, 0.0), (length, n_kn, 0.0, v_kn, my_knm, 0.0)];
    let dem = SteelDemand { uls: vec![("ULS".into(), st)], ..ec3_dem(length, 0.0) };
    let d = codes::check_ec3(1, None, "test", 0.0, p, h, fy, 1.25 * fy, &dem, &DesignSettings::default());
    d.checks.iter().find(|c| c.name.starts_with(name)).map(|c| c.ratio).unwrap_or(f64::NAN)
}

/// IPE 200, S235, column of 3.5 m in plane, NEd = 210 kN with My from 0 to 43 kN·m (pp. 181 –
/// 187; ECCS, New design rules in EN 1993-1-1 for member stability, worked example 1). The
/// example's bending ratio uses the Annex A (Method 1) factors, whereas Pondus uses Annex B,
/// so only the section properties, the class, χy and the axial ratio are compared.
#[test]
fn en3_ex001_ipe200_beam_column() {
    let case = "ETABS EN 3-2005 Ex. 001, IPE 200";
    let page = "EN 3-2005 Example 001, pp. 181-187";
    let h = HDims { d: 200.0, bf: 100.0, tw: 5.6, tf: 8.5, r: 12.0 };
    let p = pondus_core::sections::h_props(0.2, 0.1, 0.0056, 0.0085, 0.012);
    row("EN 1993-1-1 (ETABS)", case, page, "A (plates and fillets)", "mm²", 2848.0, p.a * 1e6, 0.5);
    row("EN 1993-1-1 (ETABS)", case, page, "Iy", "mm⁴", 19.43e6, p.iy * 1e12, 0.5);
    row("EN 1993-1-1 (ETABS)", case, page, "Wpl,y", "mm³", 220.6e3, p.zy * 1e9, 0.5);
    let (cls, _) = codes::ec3_class_nm(h, &p, 235.0, 210e3, 43e6);
    row("EN 1993-1-1 (ETABS)", case, page, "section class under N + M", "-", 1.0, cls as f64, 0.0);
    let m = codes::ec3_member(1, &p, h, 235.0, &ec3_dem(3.5, 0.0), 1.0, true);
    let r = ec3_run(m, -210.0, 43.0, 0.0);
    row("EN 1993-1-1 (ETABS)", case, page, "χy (curve a, λ̄y 0.451)", "-", 0.939, r.chi_y, 0.5);
    row("EN 1993-1-1 (ETABS)", case, page, "D/C axial NEd/(χy A fy)", "-", 0.334, r.compression_ratio, 0.5);
    program(case, "D/C axial", "-", 0.334, "ETABS", 0.334, r.compression_ratio);
    // the interaction through the design path, Annex B with Cmy of Table B.3 (ψ = 0: 0.6):
    // NEd/(χy NRk) + kyy My/MRk with kyy = Cmy (1 + (λ̄y − 0.2) NEd/(χy NRk)) for Class 1
    let st: Vec<(f64, f64, f64, f64, f64, f64)> = (0..=8).map(|k| 3.5 * k as f64 / 8.0).map(|x| (x, -210.0, 0.0, 12.29, 43.0 * x / 3.5, 0.0)).collect();
    let dem = SteelDemand { uls: vec![("ULS".into(), st)], ..ec3_dem(3.5, 0.0) };
    let cm = codes::ec3_cm(&dem, &dem.uls[0].1);
    check(Row { suite: "csi", aspect: "EN 1993-1-1 (ETABS)", case, source: "EN 1993-1-1 Table B.3, linear diagram ψ = 0", quantity: "Cmy = 0.6 + 0.4ψ", unit: "-", reference: 0.6, pondus: cm.cmy.unwrap(), tol: 0.01, kind: "closed-form" });
    let d = codes::check_ec3(1, None, "IPE 200", 0.0, &p, h, 235.0, 360.0, &dem, &DesignSettings::default());
    let got = d.checks.iter().find(|c| c.name.starts_with("interaction")).map(|c| c.ratio).unwrap();
    let ny = 0.334187;
    let lam = ((2848.0 * 235.0) / (std::f64::consts::PI.powi(2) * 210_000.0 * 19.43e6 / 3500f64.powi(2))).sqrt();
    let annex_b = ny + 0.6 * (1.0 + (lam - 0.2) * ny) * 43.0 / (220.6e3 * 235.0 / 1e6);
    check(Row { suite: "csi", aspect: "EN 1993-1-1 (ETABS)", case, source: "EN 1993-1-1 6.3.3 (6.61), Annex B Table B.1 (Class 1, no LTB)", quantity: "interaction (6.61), Annex B, Cmy 0.6", unit: "-", reference: annex_b, pondus: got, tol: 0.5, kind: "closed-form" });
    program(case, "interaction (example: Annex A 0.980 total; Pondus: Annex B)", "-", 0.980, "ETABS", 0.334 + 0.649, got);
}

/// 406x178x74 UB, S275, a 1.4 m simple span under 1050 kN at mid-span: VEd = 525 kN, MEd =
/// 367.5 kN·m (pp. 188 – 192; Narayanan & Beeby, Designers' Guide to EN 1993-1-1, Ex. 6.5).
/// The example takes η = 1.2 for the shear area; Pondus takes η = 1.0, the conservative value
/// that 6.2.6(3) allows, so Vpl,Rd is 7.4 % lower by design. With VEd/Vpl,Rd > 0.5 the
/// moment resistance is reduced for shear (6.2.8).
#[test]
fn en3_ex002_ub406_shear_and_moment() {
    let case = "ETABS EN 3-2005 Ex. 002, 406x178x74 UB";
    let page = "EN 3-2005 Example 002, pp. 188-192";
    let h = HDims { d: 412.8, bf: 179.5, tw: 9.5, tf: 16.0, r: 10.2 };
    let p = pondus_core::sections::h_props(0.4128, 0.1795, 0.0095, 0.016, 0.0102);
    row("EN 1993-1-1 (ETABS)", case, page, "A (plates and fillets)", "mm²", 9450.0, p.a * 1e6, 0.5);
    row("EN 1993-1-1 (ETABS)", case, page, "Wpl,y", "mm³", 1501e3, p.zy * 1e9, 0.5);
    let (cls, _) = codes::ec3_class(h, 275.0, false);
    row("EN 1993-1-1 (ETABS)", case, page, "section class (bending)", "-", 1.0, cls as f64, 0.0);
    let m = codes::ec3_member(1, &p, h, 275.0, &ec3_dem(1.4, 0.0), 1.0, false);
    let r = ec3_run(m.clone(), 0.0, 0.0, 525.0);
    let vpl = 525.0 / r.shear_ratio;
    // 6.2.6(3)a: Av = A − 2btf + (tw + 2r)tf = 9450 − 5744 + 478.4 = 4184.4 mm² (the example prints
    // 4021.2, an arithmetic slip that does not matter there because η hw tw = 4341 mm² governs)
    let av = 9450.0 - 2.0 * 179.5 * 16.0 + (9.5 + 2.0 * 10.2) * 16.0;
    check(Row { suite: "csi", aspect: "EN 1993-1-1 (ETABS)", case, source: &format!("{SRC}, {page}; EN 1993-1-1 6.2.6(3)a with η = 1.0"), quantity: "Vpl,Rd = Av fy/√3, Av from 6.2.6(3)a, η = 1.0", unit: "kN", reference: av * 275.0 / 3f64.sqrt() / 1e3, pondus: vpl, tol: 0.5, kind: "closed-form" });
    program(case, "Vpl,Rd (example η = 1.2: Av = 4341 mm²)", "kN", 689.2, "ETABS", 689.2, vpl);
    let r = ec3_run(m, 0.0, 367.5, 0.0);
    let mc = 367.5 / r.flexure_ratio_y;
    row("EN 1993-1-1 (ETABS)", case, page, "Mc,y,Rd = Wpl fy", "kN·m", 412.8, mc, 0.5);
    program(case, "Mc,y,Rd", "kN·m", 412.8, "ETABS", 412.8, mc);
    // the design path with V and M together: the moment resistance reduced for shear, 6.2.8
    // (ρ from Pondus's own Vpl,Rd, η = 1.0: ρ = (2·525/664.5 − 1)² = 0.338 instead of 0.27)
    let rho = (2.0 * 525.0 / vpl - 1.0).powi(2);
    let aw = (412.8 - 2.0 * 16.0) * 9.5;
    let mv_eta1 = (p.zy * 1e9 - rho * aw * aw / (4.0 * 9.5)) * 275.0 / 1e6;
    let ratio = design_check(&p, h, 275.0, 1.4, 0.0, 525.0, 367.5, "flexure y");
    check(Row { suite: "csi", aspect: "EN 1993-1-1 (ETABS)", case, source: &format!("{SRC}, {page}; EN 1993-1-1 6.2.8(5) with Pondus's Vpl,Rd"), quantity: "My,V,Rd (6.2.8) with ρ from Vpl,Rd at η = 1.0", unit: "kN·m", reference: mv_eta1, pondus: 367.5 / ratio, tol: 0.5, kind: "closed-form" });
    program(case, "My,V,Rd (example η = 1.2, ρ = 0.27)", "kN·m", 386.8, "ETABS", 386.8, 367.5 / ratio);
}

/// 457x191x98 UB, fy 235 MPa, a 0.4 m member under NEd = 1400 kN and My = 200 kN·m (pp. 193
/// – 197; Narayanan & Beeby Ex. 6.6): class 2 under N + M, Npl,Rd 2937.5 kN, Mpl,y,Rd 524.5
/// kN·m and the plastic resistance reduced for the axial force, MN,y,Rd 342.2 kN·m (6.2.9.1).
#[test]
fn en3_ex003_ub457_axial_reduced_moment() {
    let case = "ETABS EN 3-2005 Ex. 003, 457x191x98 UB";
    let page = "EN 3-2005 Example 003, pp. 193-197";
    let h = HDims { d: 467.2, bf: 192.8, tw: 11.4, tf: 19.6, r: 10.2 };
    let p = pondus_core::sections::h_props(0.4672, 0.1928, 0.0114, 0.0196, 0.0102);
    row("EN 1993-1-1 (ETABS)", case, page, "A (plates and fillets)", "mm²", 12500.0, p.a * 1e6, 0.5);
    row("EN 1993-1-1 (ETABS)", case, page, "Wpl,y", "mm³", 2232e3, p.zy * 1e9, 0.5);
    let (cls, _) = codes::ec3_class_nm(h, &p, 235.0, 1400e3, 200e6);
    row("EN 1993-1-1 (ETABS)", case, page, "section class under N + M", "-", 2.0, cls as f64, 0.0);
    let m = codes::ec3_member(1, &p, h, 235.0, &ec3_dem(0.4, 0.0), 1.0, false);
    let r = ec3_run(m.clone(), 1400.0, 0.0, 0.0);
    row("EN 1993-1-1 (ETABS)", case, page, "Npl,Rd = A fy", "kN", 2937.5, 1400.0 / r.tension_ratio, 0.5);
    let r = ec3_run(m, 0.0, 200.0, 0.0);
    let mpl = 200.0 / r.flexure_ratio_y;
    row("EN 1993-1-1 (ETABS)", case, page, "Mpl,y,Rd = Wpl fy", "kN·m", 524.5, mpl, 0.5);
    program(case, "Mpl,y,Rd", "kN·m", 524.5, "ETABS", 524.1, mpl);
    let ratio = design_check(&p, h, 235.0, 0.4, -1400.0, 0.0, 200.0, "cross-section N + M");
    let mn = 200.0 / ratio;
    row("EN 1993-1-1 (ETABS)", case, page, "MN,y,Rd = Mpl,y,Rd (1 − n)/(1 − 0.5a) (6.2.9.1)", "kN·m", 342.2, mn, 0.5);
    program(case, "MN,y,Rd", "kN·m", 342.2, "ETABS", 341.9, mn);
}

// ============================================================ EN 1992-1-1 Example 001

fn ec2_beam(as_mm2: f64, stirrup_s: Option<f64>) -> BeamDesign {
    BeamDesign {
        member: 1,
        bim_id: None,
        b: 0.230,
        h: 0.550,
        span: 6.0,
        flexure: vec![FlexZone { face: "bottom".into(), zone: "middle".into(), mu: 0.0, as_req: as_mm2, as_prov: as_mm2, bars: String::new(), ratio: 0.0 }, FlexZone { face: "bottom".into(), zone: "left".into(), mu: 0.0, as_req: 0.0, as_prov: 0.0, bars: String::new(), ratio: 0.0 }],
        shear: stirrup_s.map(|s| vec![ShearZone { zone: "ends".into(), vu: 0.0, phi_vc: 0.0, stirrups: String::new(), s, ratio: 0.0 }]).unwrap_or_default(),
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

/// 230 × 550 mm beam, d = 490 mm, fck 30 MPa, fyk = fywk 460 MPa, 6 m simple span under 36.67
/// kN/m: MEd = 165 kN·m with As = 916 mm², VEd = 110.01 kN at the support with Asw/s = 249.5
/// mm²/m at cot θ = 2.5 (pp. 334 – 340, CEN default parameters). Pondus's EN 1992-1-1 path is a
/// cross-check of the bars it chose to ACI 318, so it is given the example's As and Asw/s and
/// must return MRd = MEd and VRd,s = VEd; VRd,c of a support where ρl = 0 is the minimum.
/// Pondus's d is h − cover − 9 − 10 mm, so the cover is set to 41 mm for d = 490 mm.
#[test]
fn en2_ex001_beam_resistances() {
    let case = "ETABS EN 2-2004 Ex. 001, 230x550 beam";
    let page = "EN 2-2004 Example 001, pp. 334-340";
    let dem = |x: f64, m: f64, v: f64| BeamDemand { span: 6.0, uls: vec![vec![(x, m, v, 0.0, 0.0)]], ..Default::default() };
    let cover = 0.041;
    let r = codes::check_ec2_beam(&ec2_beam(916.0, None), &dem(3.0, 165.0, 0.0), 30.0, 460.0, 460.0, cover);
    row("EN 1992-1-1 (ETABS)", case, page, "MRd(As = 916 mm²) = MEd", "kN·m", 165.0, 165.0 / r.flexure, 0.5);
    let r = codes::check_ec2_beam(&ec2_beam(916.0, None), &dem(0.0, 0.0, 40.0), 30.0, 460.0, 460.0, cover);
    row("EN 1992-1-1 (ETABS)", case, page, "VRd,c,min = vmin bw d (ρl = 0 at the support)", "kN", 45.3, 40.0 / r.shear, 0.5);
    // Pondus's cross-check takes the stirrups as 2 legs of RB9 (127.2 mm²): at s = 127.2/0.2495
    // mm they give Asw/s = 249.5 mm²/m
    let s = 2.0 * pondus_core::sections::bar_area(9.0) * 1e6 / 0.2495;
    let r = codes::check_ec2_beam(&ec2_beam(916.0, Some(s)), &dem(0.0, 0.0, 100.0), 30.0, 460.0, 460.0, cover);
    row("EN 1992-1-1 (ETABS)", case, page, "VRd,s(Asw/s = 249.5 mm²/m, cot θ 2.5) = VEd", "kN", 110.01, 100.0 / r.shear, 0.5);
    program(case, "Asw/s", "mm²/m", 249.5, "ETABS", 249.5, 249.5 * 110.01 / (100.0 / r.shear));
}
