// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Members of other materials, checked with the Stabileo engine: sawn timber to NDS (allowable
//! stress design, so under the service combinations), reinforced masonry to TMS 402 (strength
//! design, under the ULS combinations, with the bar area given per member) and cold-formed
//! lipped channels to AISI S100 (LRFD). For the cold-formed sections Pondus supplies what the
//! engine takes as given: J and Cw from the section finite elements (`secfe`) and the effective
//! area and section modulus at Fy by the effective width method of AISI S100 1.1 (web and
//! flanges as stiffened elements, k = 4, the lips unstiffened, k = 0.43: an adequate lip is
//! assumed, and distortional buckling is not evaluated — the engine says so too).

use dedaliano_engine::postprocess::cfs_check::*;
use dedaliano_engine::postprocess::masonry_check::*;
use dedaliano_engine::postprocess::timber_check::*;
use serde::Serialize;

use crate::model::*;
use crate::steel::SteelCheck;

#[derive(Clone, Debug, Serialize)]
pub struct OtherDesign {
    pub member: Id,
    pub bim_id: Option<String>,
    pub code: String,
    pub section: String,
    pub checks: Vec<SteelCheck>,
    pub ratio: f64,
    pub governs: String,
    pub ok: bool,
    pub notes: Vec<String>,
}

fn finish(member: &Member, code: &str, section: &str, checks: Vec<SteelCheck>, notes: Vec<String>, target: f64) -> OtherDesign {
    let (ratio, governs) = checks.iter().map(|c| (c.ratio, c.name.clone())).fold((0.0, String::new()), |a, b| if b.0 > a.0 { b } else { a });
    OtherDesign { member: member.id, bim_id: member.bim_id.clone(), code: code.into(), section: section.into(), ok: ratio <= target + 1e-9, checks, ratio, governs, notes }
}

fn worst(checks: &mut Vec<SteelCheck>, name: &str, r: f64, detail: String) {
    match checks.iter_mut().find(|c| c.name == name) {
        Some(c) if c.ratio >= r => {}
        Some(c) => {
            c.ratio = r;
            c.detail = detail;
        }
        None => checks.push(SteelCheck { name: name.into(), ratio: r, detail }),
    }
}

/// Stations (combination, x, N tension +, Vy, Vz, My, Mz) of a member under some combinations.
pub type Stations = Vec<(String, f64, f64, f64, f64, f64, f64)>;

pub fn timber(m: &Member, mat: &Material, b: f64, h: f64, len: f64, st: &Stations, set: &DesignSettings) -> OtherDesign {
    let data = |id: usize| TimberMemberData {
        element_id: id,
        fb: mat.fb * 1e6,
        ft: mat.ft * 1e6,
        fc: mat.fc * 1e6,
        fv: mat.fv * 1e6,
        e: mat.e * 1e6,
        e_min: Some(mat.e_min * 1e6),
        b,
        d: h,
        le: m.k.unwrap_or(1.0) * len,
        lu: Some(m.lb.unwrap_or(len)),
        cd: Some(1.0),
        cm: None,
        ct: None,
        cf_bending: None,
        cf_tension: None,
        cf_compression: None,
        cfu: None,
        ci: None,
        cr: None,
    };
    let members: Vec<TimberMemberData> = (0..st.len()).map(|k| data(k + 1)).collect();
    let forces: Vec<TimberDesignForces> = st.iter().enumerate().map(|(k, s)| TimberDesignForces { element_id: k + 1, m: s.5.abs() * 1e3, n: Some(s.2 * 1e3), v: Some(s.4.abs() * 1e3) }).collect();
    let mut checks = vec![];
    for (r, s) in check_timber_members(&TimberCheckInput { members, forces }).iter().zip(st) {
        let at = format!("{}, x = {:.2} m", s.0, s.1);
        worst(&mut checks, "bending (NDS 3.3)", r.bending_ratio, format!("{at}: M {:.2} kN·m, Fb' {:.2} MPa", s.5.abs(), r.fb_prime / 1e6));
        worst(&mut checks, "compression (3.6, 3.7)", r.compression_ratio, format!("{at}: N {:.1} kN, Fc' {:.2} MPa", s.2, r.fc_prime / 1e6));
        worst(&mut checks, "tension (3.8)", r.tension_ratio, format!("{at}: N {:.1} kN", s.2));
        worst(&mut checks, "shear (3.4)", r.shear_ratio, format!("{at}: V {:.1} kN", s.4.abs()));
        worst(&mut checks, "bending + axial (3.9)", r.interaction_ratio, at.clone());
    }
    finish(m, "NDS 2018 (ASD, service combinations)", &format!("{:.0} x {:.0} mm", b * 1e3, h * 1e3), checks, vec!["load duration factor CD = 1.0 and CM = Ct = CF = 1.0: adjust the grade values for other conditions".into()], set.target_ratio)
}

pub fn masonry(m: &Member, mat: &Material, b: f64, t: f64, len: f64, st: &Stations, set: &DesignSettings) -> OtherDesign {
    let as_ = m.rebar.unwrap_or(0.0) / 1e6;
    let data = |id: usize| MasonryMemberData { element_id: id, fm: mat.fc * 1e6, fy: set.fy * 1e6, em: None, b, t, d: t / 2.0, as_tension: as_, h: len, k: m.k, an: None, av: None, s_stirrup: None };
    let members: Vec<MasonryMemberData> = (0..st.len()).map(|k| data(k + 1)).collect();
    let forces: Vec<MasonryDesignForces> = st.iter().enumerate().map(|(k, s)| MasonryDesignForces { element_id: k + 1, pu: Some((-s.2).max(0.0) * 1e3), mu: Some(s.5.abs().max(s.6.abs()) * 1e3), vu: Some(s.4.abs().max(s.3.abs()) * 1e3) }).collect();
    let mut checks = vec![];
    let mut slender = 0.0;
    for (r, s) in check_masonry_members(&MasonryCheckInput { members, forces }).iter().zip(st) {
        let at = format!("{}, x = {:.2} m", s.0, s.1);
        worst(&mut checks, "axial (9.3.4.1.1)", r.axial_ratio, format!("{at}: Pu {:.1} kN, Pn {:.0} kN", -s.2, r.pn / 1e3));
        worst(&mut checks, "flexure (9.3.3)", r.flexure_ratio, format!("{at}: Mu {:.2} kN·m, Mn {:.2} kN·m", s.5.abs().max(s.6.abs()), r.mn / 1e3));
        worst(&mut checks, "shear (9.3.4.1.2)", r.shear_ratio, format!("{at}: Vu {:.1} kN, Vn {:.1} kN", s.4.abs().max(s.3.abs()), r.vn / 1e3));
        worst(&mut checks, "axial + flexure", r.interaction_ratio, at.clone());
        slender = r.slenderness;
    }
    let mut notes = vec![format!("bars As {:.0} mm² at mid-thickness (d = t/2); h/t = {slender:.1}", as_ * 1e6)];
    if as_ <= 0.0 {
        notes.push("no reinforcement given (member rebar, mm²): flexure is resisted by nothing".into());
    }
    finish(m, "TMS 402-16 (strength design)", &format!("{:.0} x {:.0} mm", b * 1e3, t * 1e3), checks, notes, set.target_ratio)
}

/// Effective width of a flat element (AISI S100 1.1): ρ w.
fn eff_width(w: f64, t: f64, k: f64, f: f64, e: f64) -> f64 {
    let lam = 1.052 / k.sqrt() * (w / t) * (f / e).sqrt();
    if lam <= 0.673 {
        w
    } else {
        ((1.0 - 0.22 / lam) / lam * w).max(0.0)
    }
}

/// Gross and effective properties of a lipped channel h x b x d x t (m) at stress fy (MPa).
#[derive(Clone, Debug)]
pub struct LippedProps {
    pub fe: crate::secfe::SectionFe,
    pub ae: f64,
    pub se_x: f64,
    pub sf_x: f64,
    pub se_y: f64,
}

pub fn lipped_props(h: f64, b: f64, d: f64, t: f64, fy: f64) -> Result<LippedProps, String> {
    // the section FE (3000 elements) is the cost of a cold-formed check: a roof of hundreds
    // of purlins tries the same few catalogue sections again and again, so they are kept
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<[i64; 5], LippedProps>> = std::cell::RefCell::new(std::collections::HashMap::new());
    }
    let key = [(h * 1e6).round() as i64, (b * 1e6).round() as i64, (d * 1e6).round() as i64, (t * 1e6).round() as i64, (fy * 1e3).round() as i64];
    if let Some(p) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Ok(p);
    }
    let p = lipped_props_uncached(h, b, d, t, fy)?;
    CACHE.with(|c| c.borrow_mut().insert(key, p.clone()));
    Ok(p)
}

fn lipped_props_uncached(h: f64, b: f64, d: f64, t: f64, fy: f64) -> Result<LippedProps, String> {
    let fe = crate::secfe::analyze(&crate::secfe::lipped_c(h, b, d, t), 3000)?;
    let e = 203_000.0;
    // flat widths between the corners (sharp corners)
    let (ww, wf, wl) = (h - 2.0 * t, b - 2.0 * t, d - t);
    let (bw, bf, bl) = (eff_width(ww, t, 4.0, fy, e), eff_width(wf, t, 4.0, fy, e), eff_width(wl, t, 0.43, fy, e));
    let ae = fe.a - t * ((ww - bw) + 2.0 * (wf - bf) + 2.0 * (wl - bl));
    // strong-axis bending: the compression flange and lip lose their ineffective parts, the
    // web in bending (k = 24) is taken as effective; the neutral axis shift is neglected
    let (bf_b, bl_b) = (bf, eff_width(wl, t, 0.43, fy, e));
    let lost = [((wf - bf_b) * t, h / 2.0 - t / 2.0), ((wl - bl_b) * t, h / 2.0 - t - wl / 2.0)];
    // the engine's iy here is about the horizontal axis: the channel's strong axis
    let ix_e = fe.iy - lost.iter().map(|(a, y)| a * y * y).sum::<f64>();
    let sf_x = fe.iy / (h / 2.0);
    let se_x = ix_e / (h / 2.0);
    let cx = fe.centroid[0];
    let se_y = fe.iz / cx.max(b - cx) * (ae / fe.a);
    Ok(LippedProps { fe, ae, se_x, sf_x, se_y })
}

pub fn cold_formed(m: &Member, mat: &Material, h: f64, b: f64, d: f64, t: f64, len: f64, st: &Stations, set: &DesignSettings) -> Result<OtherDesign, String> {
    let lp = lipped_props(h, b, d, t, mat.fy)?;
    let f = &lp.fe;
    let data = |id: usize| CfsMemberData {
        element_id: id,
        fy: mat.fy * 1e6,
        e: 203e9,
        ag: f.a,
        ae: lp.ae,
        ix: f.iy,
        se_x: lp.se_x,
        sf_x: lp.sf_x,
        iy: f.iz,
        se_y: lp.se_y,
        rx: (f.iy / f.a).sqrt(),
        ry: (f.iz / f.a).sqrt(),
        j: f.j,
        cw: f.cw,
        lb: m.lb.unwrap_or(len),
        lc: len,
        k: m.k,
        cb: None,
        fcrd: None,
        fcrd_flex: None,
        aw: Some((h - 2.0 * t) * t),
        h: Some(h - 2.0 * t),
        t: Some(t),
    };
    let members: Vec<CfsMemberData> = (0..st.len()).map(|k| data(k + 1)).collect();
    let forces: Vec<CfsDesignForces> = st.iter().enumerate().map(|(k, s)| CfsDesignForces { element_id: k + 1, axial: Some(s.2 * 1e3), mx: Some(s.5 * 1e3), my: Some(s.6 * 1e3), shear: Some(s.4.abs() * 1e3) }).collect();
    let mut checks = vec![];
    for (r, s) in check_cfs_members(&CfsCheckInput { members, forces }).iter().zip(st) {
        let at = format!("{}, x = {:.2} m", s.0, s.1);
        worst(&mut checks, "compression (E)", r.compression_ratio, format!("{at}: P {:.1} kN", -s.2));
        worst(&mut checks, "tension (D)", r.tension_ratio, format!("{at}: T {:.1} kN", s.2));
        worst(&mut checks, "flexure x (F)", r.flexure_ratio_x, format!("{at}: Mx {:.2} kN·m", s.5.abs()));
        worst(&mut checks, "flexure y (F)", r.flexure_ratio_y, format!("{at}: My {:.2} kN·m", s.6.abs()));
        worst(&mut checks, "shear (G)", r.shear_ratio, format!("{at}: V {:.1} kN", s.4.abs()));
        worst(&mut checks, "interaction (H)", r.interaction_ratio, at.clone());
    }
    let notes = vec![format!("A {:.0} mm², Ae {:.0} mm², Sx {:.0} / Se,x {:.0} mm³, J {:.1} mm⁴, Cw {:.3e} mm⁶ (section FE)", f.a * 1e6, lp.ae * 1e6, lp.sf_x * 1e9, lp.se_x * 1e9, f.j * 1e12, f.cw * 1e18), "adequate lips assumed (k = 4 for the flanges); distortional buckling not evaluated".into()];
    Ok(finish(m, "AISI S100-16 (LRFD)", &format!("C {:.0}x{:.0}x{:.0}x{:.1}", h * 1e3, b * 1e3, d * 1e3, t * 1e3), checks, notes, set.target_ratio))
}
