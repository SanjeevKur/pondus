// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Linear time-history analysis under a ground motion with the Stabileo engine (Newmark-β
//! average acceleration, or HHT-α), on the same mass as the modal analysis (`seismic::mass_model`).
//! A record is read as Stabileo reads one: a PEER NGA `.AT2` file (four header lines, the
//! fourth with NPTS and DT, values in g), two columns (time, acceleration in g) or one column
//! (acceleration in g at a stated dt); it is resampled onto the analysis step, because the
//! engine samples the ground by step index (Stabileo `accelerogram.ts`).

use serde::{Deserialize, Serialize};

use crate::analysis::Analysis;
use crate::model::*;
use crate::seismic::G;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    pub dt: f64,
    /// Ground acceleration, g.
    pub accel: Vec<f64>,
}

impl Record {
    pub fn duration(&self) -> f64 {
        self.dt * self.accel.len().saturating_sub(1) as f64
    }
    pub fn pga(&self) -> f64 {
        self.accel.iter().map(|a| a.abs()).fold(0.0, f64::max)
    }

    /// The record at step `dt`, by linear interpolation, `n + 1` values from t = 0.
    pub fn resample(&self, dt: f64, n: usize) -> Vec<f64> {
        (0..=n)
            .map(|k| {
                let t = k as f64 * dt;
                let x = t / self.dt;
                let i = x.floor() as usize;
                if i + 1 >= self.accel.len() {
                    return 0.0;
                }
                self.accel[i] + (self.accel[i + 1] - self.accel[i]) * (x - i as f64)
            })
            .collect()
    }
}

/// Reads a ground motion: PEER .AT2, two columns or one column (then `dt` is needed).
pub fn parse_record(name: &str, text: &str, dt: Option<f64>) -> Result<Record, String> {
    let lines: Vec<&str> = text.lines().collect();
    // PEER NGA: the fourth line "NPTS=  5000, DT=   .0050 SEC"
    if lines.len() > 4 && lines[3].to_uppercase().contains("NPTS") {
        let up = lines[3].to_uppercase();
        let grab = |k: &str| -> Option<f64> {
            let i = up.find(k)? + k.len();
            let rest: String = up[i..].chars().skip_while(|c| *c == '=' || *c == ' ').take_while(|c| c.is_ascii_digit() || *c == '.' || *c == 'E' || *c == '-' || *c == '+').collect();
            rest.parse().ok()
        };
        let npts = grab("NPTS").ok_or("AT2: no NPTS")? as usize;
        let dt = grab("DT").ok_or("AT2: no DT")?;
        let accel: Vec<f64> = lines[4..].iter().flat_map(|l| l.split_whitespace()).filter_map(|x| x.parse().ok()).take(npts).collect();
        return Ok(Record { name: name.into(), dt, accel });
    }
    let rows: Vec<Vec<f64>> = lines.iter().map(|l| l.split([' ', '\t', ',', ';']).filter_map(|x| x.trim().parse::<f64>().ok()).collect::<Vec<f64>>()).filter(|v| !v.is_empty()).collect();
    if rows.is_empty() {
        return Err("no numbers in the record".into());
    }
    if rows.iter().all(|r| r.len() >= 2) {
        let dt = (rows[rows.len() - 1][0] - rows[0][0]) / (rows.len() - 1).max(1) as f64;
        return Ok(Record { name: name.into(), dt, accel: rows.iter().map(|r| r[1]).collect() });
    }
    let dt = dt.ok_or("a single-column record needs its time step dt")?;
    Ok(Record { name: name.into(), dt, accel: rows.into_iter().flatten().collect() })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 0 = x, 1 = y.
    pub direction: usize,
    pub scale: f64,
    /// Analysis step, s, and length (default: the record's).
    pub dt: f64,
    pub duration: Option<f64>,
    pub damping: f64,
    /// HHT-α in [−1/3, 0] (None: Newmark average acceleration).
    pub alpha: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { direction: 0, scale: 1.0, dt: 0.01, duration: None, damping: 0.05, alpha: None }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct StoreyPeak {
    pub z0: f64,
    pub z1: f64,
    /// Largest storey drift and when, m and s; its ratio to the storey height.
    pub drift: f64,
    pub at: f64,
    pub ratio: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ThResult {
    pub record: String,
    pub pga_g: f64,
    pub time: Vec<f64>,
    /// The ground and the roof (the top level's mean displacement relative to the ground), m.
    pub ground: Vec<f64>,
    pub roof: Vec<f64>,
    pub roof_peak: f64,
    pub roof_peak_at: f64,
    pub storeys: Vec<StoreyPeak>,
    /// The peak reaction of each support summed: an upper bound of the peak base shear.
    pub base_shear_bound: f64,
    pub weight: f64,
    pub warnings: Vec<String>,
}

pub fn run(model: &Model, a: &Analysis, rec: &Record, st: &Settings) -> std::result::Result<ThResult, String> {
    let em = a.engine.as_ref().ok_or("analyse first")?;
    let (input, densities, weight, warnings) = crate::seismic::mass_model(model, &em.input, &em.pieces, &em.quads, &em.gravity, &em.point_cases)?;
    if rec.accel.len() < 2 || rec.dt <= 0.0 {
        return Err("the record is empty".into());
    }
    let dur = st.duration.unwrap_or(rec.duration()).min(rec.duration() + 5.0);
    let n = (dur / st.dt).ceil() as usize;
    if n > 40_000 {
        return Err(format!("{n} steps: use a longer dt or a shorter duration"));
    }
    let ag: Vec<f64> = rec.resample(st.dt, n).into_iter().map(|g| g * G * st.scale).collect();
    let mut inp = input.clone();
    inp.loads.clear();
    let th = dedaliano_engine::types::TimeHistoryInput3D {
        solver: inp,
        densities,
        time_step: st.dt,
        n_steps: n,
        method: if st.alpha.is_some() { "hht".into() } else { "newmark".into() },
        beta: 0.25,
        gamma: 0.5,
        alpha: st.alpha,
        damping_xi: Some(st.damping),
        ground_accel_x: (st.direction == 0).then(|| ag.clone()),
        ground_accel_y: (st.direction == 1).then(|| ag.clone()),
        ground_accel_z: None,
        force_history: None,
    };
    let r = dedaliano_engine::solver::time_integration::solve_time_history_3d(&th).map_err(|e| format!("time history: {e}"))?;
    let hist: std::collections::HashMap<usize, &dedaliano_engine::types::NodeTimeHistory3D> = r.node_histories.iter().map(|h| (h.node_id, h)).collect();
    let comp = |h: &dedaliano_engine::types::NodeTimeHistory3D| if st.direction == 0 { h.ux.clone() } else { h.uy.clone() };
    // the levels with columns, and each level's mean displacement history
    let mut levels: Vec<f64> = model.members.iter().filter(|m| model.kind(m) == MemberKind::Column).flat_map(|m| [m.i, m.j]).filter_map(|n| model.node(n).map(|n| (n.z * 1000.0).round() / 1000.0)).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let steps = r.time_steps.len();
    let level_hist = |z: f64| -> Vec<f64> {
        let ids: Vec<usize> = model.nodes.iter().filter(|n| (n.z - z).abs() < 0.005).filter_map(|n| em.node_of.get(&n.id).copied()).collect();
        let mut out = vec![0.0; steps];
        let mut cnt = 0.0f64;
        for i in ids {
            if let Some(h) = hist.get(&i) {
                for (k, v) in comp(h).iter().enumerate().take(steps) {
                    out[k] += v;
                }
                cnt += 1.0;
            }
        }
        out.iter().map(|v| v / cnt.max(1.0)).collect()
    };
    let lh: Vec<Vec<f64>> = levels.iter().map(|z| level_hist(*z)).collect();
    let mut storeys = vec![];
    for k in 1..levels.len() {
        let (mut peak, mut at) = (0.0f64, 0.0);
        for s in 0..steps {
            let d = (lh[k][s] - lh[k - 1][s]).abs();
            if d > peak {
                peak = d;
                at = r.time_steps[s];
            }
        }
        storeys.push(StoreyPeak { z0: levels[k - 1], z1: levels[k], drift: peak, at, ratio: peak / (levels[k] - levels[k - 1]).max(1e-9) });
    }
    let roof = lh.last().cloned().unwrap_or_default();
    let base = lh.first().cloned().unwrap_or_else(|| vec![0.0; steps]);
    let roof_rel: Vec<f64> = roof.iter().zip(&base).map(|(a, b)| a - b).collect();
    let (roof_peak, roof_peak_at) = roof_rel.iter().enumerate().fold((0.0f64, 0.0f64), |acc, (k, v)| if v.abs() > acc.0 { (v.abs(), r.time_steps[k]) } else { acc });
    let base_shear_bound: f64 = r.peak_reactions.iter().map(|x| if st.direction == 0 { x.fx.abs() } else { x.fy.abs() }).sum();
    Ok(ThResult { record: rec.name.clone(), pga_g: rec.pga() * st.scale, time: r.time_steps.clone(), ground: ag.iter().take(steps).map(|a| a / G).collect(), roof: roof_rel, roof_peak, roof_peak_at, storeys, base_shear_bound, weight, warnings })
}
