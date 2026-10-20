// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Analyses a model and reports the time, where the factorisations ran (CPU or an NVIDIA GPU)
//! and the results to compare between the two: the first periods, the reactions and the largest
//! displacement. Builds without the GUI, for a headless GPU server:
//!   cargo run --release -p pondus-core --example bench -- model.pondus
//! PONDUS_GPU=0 forces the CPU, PONDUS_GPU_MIN sets the smallest system put on the card.

use pondus_core::model::Model;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: bench model.pondus");
        std::process::exit(2);
    };
    let m = Model::load(std::path::Path::new(&path)).unwrap_or_else(|e| {
        eprintln!("bench: {e}");
        std::process::exit(1)
    });
    let g0 = pondus_core::compute::factorisations();
    let t0 = std::time::Instant::now();
    let a = pondus_core::analysis::analyze(&m).unwrap_or_else(|e| {
        eprintln!("bench: {e}");
        std::process::exit(1)
    });
    let secs = t0.elapsed().as_secs_f64();
    let modes: Vec<serde_json::Value> = a.seismic.as_ref().map(|s| s.modes.iter().take(6).map(|md| serde_json::json!({"T_s": md.period, "mass_xyz": md.mass_ratio})).collect()).unwrap_or_default();
    let reactions: Vec<[f64; 3]> = a.cases.iter().map(|c| c.reaction_sum()).collect();
    // the seismic response: base shears, the largest drift ratio and envelope moments, to compare
    let seismic: Vec<serde_json::Value> = a.seismic.as_ref().map(|s| s.dirs.iter().map(|d| {
        let st = d.case.members.iter().flat_map(|m| m.stations.iter());
        let my = st.clone().map(|s| s.my.abs()).fold(0.0, f64::max);
        let n = st.map(|s| s.n.abs()).fold(0.0, f64::max);
        let drift = d.drifts.iter().map(|x| x.ratio).fold(0.0, f64::max);
        serde_json::json!({"dir": d.name, "v_rsa_kN": d.v_rsa, "v_elf_kN": d.v_elf, "scale": d.scale, "max_drift_ratio": drift, "max_abs_my_kNm": my, "max_abs_n_kN": n, "n_modes": s.modes.len()})
    }).collect()).unwrap_or_default();
    let umax = a.cases.first().map(|c| c.displacements.values().map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()).fold(0.0, f64::max)).unwrap_or(0.0);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "model": path, "nodes": m.nodes.len(), "members": m.members.len(), "elements": a.elements,
            "seconds": secs, "gpu_factorisations": pondus_core::compute::factorisations() - g0,
            "compute": pondus_core::compute::summary(), "modes": modes, "seismic": seismic,
            "reactions_by_case_kN": reactions, "max_displacement_case1_m": umax,
        }))
        .unwrap_or_default()
    );
}
