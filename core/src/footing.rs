// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Spread footings under the supported columns: sized for the allowable bearing pressure under
//! service loads, made thick enough for one-way and punching shear under factored loads, and
//! reinforced for the cantilever moment at the column face. The checks are the Stabileo
//! engine's (`postprocess::foundation_check`: Meyerhof effective area for eccentric loads,
//! overturning, sliding, ACI 318-19 shear with the size factor λs); the sizing and the bars are
//! Pondus's. Loads are the support reactions of the gravity combinations (the earthquake's
//! envelope has no signed reactions: footings under E are not checked).

use dedaliano_engine::postprocess::foundation_check::*;
use serde::{Deserialize, Serialize};

use crate::analysis::Analysis;
use crate::model::*;
use crate::rc;
use crate::sections::bar_name;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FootingSettings {
    /// Allowable (service) bearing pressure, kPa; depth of the footing base below ground, m;
    /// soil unit weight, kN/m³; base friction coefficient; least thickness, m.
    pub q_allow: f64,
    pub depth: f64,
    pub gamma_soil: f64,
    pub mu: f64,
    pub min_h: f64,
    /// Bars to choose from, mm, and the clear cover to the bottom bars, m.
    pub bars: Vec<f64>,
    pub cover: f64,
}

impl Default for FootingSettings {
    fn default() -> Self {
        FootingSettings { q_allow: 150.0, depth: 1.5, gamma_soil: 18.0, mu: 0.5, min_h: 0.3, bars: vec![12.0, 16.0, 20.0, 25.0], cover: 0.075 }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FootingDesign {
    pub node: Id,
    pub at: [f64; 3],
    /// Plan length along x, width along y, thickness, m.
    pub l: f64,
    pub b: f64,
    pub h: f64,
    pub bars_x: String,
    pub bars_y: String,
    /// Service pressure and its ratio; safety factors; shear ratios (factored).
    pub q_max: f64,
    pub bearing_ratio: f64,
    pub overturning_sf: f64,
    pub sliding_sf: f64,
    pub oneway_ratio: f64,
    pub punching_ratio: f64,
    pub ratio: f64,
    pub governs: String,
    pub ok: bool,
    pub notes: Vec<String>,
}

fn run_check(l: f64, b: f64, h: f64, cl: f64, cw: f64, fs: &FootingSettings, fc: f64, p: f64, mx: f64, my: f64, hz: f64) -> SpreadFootingResult {
    let data = SpreadFootingData { footing_id: 1, length: l, width: b, thickness: h, depth: fs.depth, q_allowable: fs.q_allow * 1e3, gamma_soil: fs.gamma_soil * 1e3, fc: fc * 1e6, col_length: cl, col_width: cw, d: Some(h - fs.cover - 0.016), mu_sliding: Some(fs.mu), rho_w: None };
    let f = SpreadFootingForces { footing_id: 1, p: p * 1e3, mx: Some(mx * 1e3), my: Some(my * 1e3), h: Some(hz * 1e3) };
    check_spread_footings(&SpreadFootingInput { footings: vec![data], forces: vec![f] }).remove(0)
}

/// The factored moment per metre of width at the column face (kN·m/m) under the net factored
/// pressure qu (kPa) on a cantilever c (m) from the face to the edge: qu c²/2.
pub fn face_moment(qu: f64, c: f64) -> f64 {
    qu * c * c / 2.0
}

/// Designs a footing under every supported node that carries a concrete column.
pub fn design(model: &Model, a: &Analysis) -> Vec<FootingDesign> {
    let fs = &model.design.footing;
    let conc = model.materials.iter().find(|m| m.kind == MaterialKind::Concrete);
    let (fc, gamma_c) = (conc.map(|m| m.fc).filter(|f| *f > 0.0).unwrap_or(24.0), conc.map(|m| m.gamma).unwrap_or(24.0));
    let uls: Vec<crate::analysis::CaseResult> = model.combos.iter().filter(|c| c.kind == ComboKind::Uls).map(|c| a.second.get(&c.id).map(|s| s.0.clone()).unwrap_or_else(|| a.combine(&c.name, &c.factors))).collect();
    let sls: Vec<crate::analysis::CaseResult> = model.combos.iter().filter(|c| c.kind == ComboKind::Sls).map(|c| a.combine(&c.name, &c.factors)).collect();
    let mut out = vec![];
    for s in &model.supports {
        let Some(n) = model.node(s.node) else { continue };
        let Some(col) = model.members.iter().find(|m| (m.i == s.node || m.j == s.node) && model.kind(m) == MemberKind::Column && model.is_concrete(m)) else { continue };
        let (cl, cw) = match model.section(col.section).map(|x| &x.shape) {
            // a column with roll 0 has its depth h along x
            Some(Shape::Rect { b, h }) => {
                if (col.roll.abs() - 90.0).abs() < 1.0 {
                    (*b, *h)
                } else {
                    (*h, *b)
                }
            }
            _ => continue,
        };
        let react = |r: &crate::analysis::CaseResult| r.reactions.iter().find(|x| x.0 == s.node).map(|x| x.1).unwrap_or([0.0; 6]);
        // the governing service and factored reactions: the largest vertical load
        let pick = |rs: &[crate::analysis::CaseResult]| rs.iter().map(react).max_by(|x, y| x[2].total_cmp(&y[2])).unwrap_or([0.0; 6]);
        let (rs, ru) = (pick(&sls), pick(&uls));
        if rs[2] <= 0.0 {
            continue;
        }
        let mut notes = vec![];
        // size: service load plus the footing and the soil over it
        let over = |h: f64| gamma_c * h + fs.gamma_soil * (fs.depth - h).max(0.0);
        let mut h = fs.min_h;
        let mut side = ((rs[2] / (fs.q_allow - over(h)).max(10.0)).sqrt() * 10.0).ceil() / 10.0;
        side = side.max(cl.max(cw) + 0.3);
        let mut chk = None;
        for _ in 0..200 {
            let ps = rs[2] + over(h) * side * side;
            let svc = run_check(side, side, h, cl, cw, fs, fc, ps, rs[3], rs[4], (rs[0] * rs[0] + rs[1] * rs[1]).sqrt());
            if svc.bearing_ratio > 1.0 {
                side += 0.1;
                continue;
            }
            // shear under the factored net load (the footing's own weight bears on the soil directly)
            let fac = run_check(side, side, h, cl, cw, fs, fc, ru[2], ru[3], ru[4], (ru[0] * ru[0] + ru[1] * ru[1]).sqrt());
            if fac.oneway_shear_ratio > 1.0 || fac.punching_shear_ratio > 1.0 {
                h += 0.05;
                continue;
            }
            chk = Some((svc, fac));
            break;
        }
        let Some((svc, fac)) = chk else {
            notes.push("no size up to the search limit passes".into());
            continue;
        };
        h = (h * 1000.0).round() / 1000.0;
        // flexure at the column face, both ways: q_u x B x c²/2 with the cantilever c
        let qu = ru[2] / (side * side) + 6.0 * ru[3].abs().max(ru[4].abs()) / side.powi(3);
        let d = (h - fs.cover - 0.016) * 1000.0;
        let as_min = 0.0018 * 1000.0 * h * 1000.0;
        let bars_for = |c: f64| -> (String, f64) {
            let mu = face_moment(qu, c);
            let (as_req, _as_prov, s, txt, fb) = rc::slab_bars(mu, d, as_min, (3.0 * h * 1000.0).min(450.0), &fs.bars, fc, model.design.fy, model.design.fyt);
            let _ = (as_req, fb);
            let n = ((side * 1000.0 - 2.0 * fs.cover * 1000.0) / s).floor() as usize + 1;
            let db: f64 = txt.split(['B', ' ']).nth(1).and_then(|x| x.parse().ok()).unwrap_or(16.0);
            (format!("{n}-{} @ {s:.0} mm", bar_name(db)), mu)
        };
        let (bx, mux) = bars_for((side - cl) / 2.0);
        let (by, _) = bars_for((side - cw) / 2.0);
        let ot = svc.overturning_sf_x.min(svc.overturning_sf_y);
        let mut checks = [("bearing", svc.bearing_ratio), ("one-way shear", fac.oneway_shear_ratio), ("punching shear", fac.punching_shear_ratio), ("overturning (SF ≥ 1.5)", if ot.is_finite() { 1.5 / ot } else { 0.0 }), ("sliding (SF ≥ 1.5)", if svc.sliding_sf.is_finite() { 1.5 / svc.sliding_sf } else { 0.0 })];
        checks.sort_by(|a, b| b.1.total_cmp(&a.1));
        let _ = mux;
        out.push(FootingDesign {
            node: s.node,
            at: n.p(),
            l: side,
            b: side,
            h,
            bars_x: bx,
            bars_y: by,
            q_max: svc.max_bearing_pressure / 1e3,
            bearing_ratio: svc.bearing_ratio,
            overturning_sf: ot,
            sliding_sf: svc.sliding_sf,
            oneway_ratio: fac.oneway_shear_ratio,
            punching_ratio: fac.punching_shear_ratio,
            ratio: checks[0].1,
            governs: checks[0].0.into(),
            ok: checks[0].1 <= 1.0,
            notes,
        });
    }
    out
}
