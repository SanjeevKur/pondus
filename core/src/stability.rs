// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Linear (eigenvalue) buckling of a load combination with the Stabileo engine: the load
//! factors λ at which the combination's axial forces make the structure unstable, and their
//! mode shapes. λ < 1 means the structure buckles under the combination itself.

use std::collections::HashMap;

use dedaliano_engine::solver::buckling::solve_buckling_3d;
use serde::Serialize;

use crate::analysis::Analysis;
use crate::model::*;

#[derive(Clone, Debug, Serialize)]
pub struct BucklingMode {
    pub factor: f64,
    /// Translations at the model's nodes, the largest 1.
    pub shape: HashMap<Id, [f64; 3]>,
}

pub fn buckling(model: &Model, a: &Analysis, combo: &Combination, n: usize) -> Result<Vec<BucklingMode>, String> {
    let em = a.engine.as_ref().ok_or("analyse first")?;
    let mut inp = em.input.clone();
    inp.loads = em.combo_loads(&combo.factors);
    if inp.loads.is_empty() {
        return Err(format!("'{}' has no loads", combo.name));
    }
    let r = solve_buckling_3d(&inp, n.clamp(1, 30)).map_err(|e| format!("buckling of '{}': {e}", combo.name))?;
    let back: HashMap<usize, Id> = em.node_of.iter().map(|(a, b)| (*b, *a)).collect();
    Ok(r.modes
        .iter()
        .map(|m| {
            let mut shape: HashMap<Id, [f64; 3]> = m.displacements.iter().filter_map(|d| back.get(&d.node_id).map(|id| (*id, [d.ux, d.uy, d.uz]))).collect();
            let big = shape.values().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0, f64::max).max(1e-30);
            for v in shape.values_mut() {
                *v = v.map(|x| x / big);
            }
            BucklingMode { factor: m.load_factor, shape }
        })
        .collect())
}
