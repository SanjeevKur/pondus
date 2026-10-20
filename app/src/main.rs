// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Pondus: agentic structural analysis and design. The agent builds, analyses and designs
//! the structure through the chat; the engineer checks it in the views and the design table,
//! and edits it by hand where needed.
//!
//!   pondus [model.pondus | stabileo.json]      the app
//!   pondus --mcp [--open f] [--autosave f]      the MCP server (TigrimOS, Claude Code, Aedifex's agent)
//!   pondus design in.(pondus|json) out.json    analyse and design without the window, BIM JSON out
//!   pondus --about                              the licence notice and the source link

// a window, not a console, on Windows; the command-line modes attach to the parent console
#![cfg_attr(windows, windows_subsystem = "windows")]

mod advanced_ui;
mod app;
mod attach;
mod chat;
mod edit_ui;
mod icon_data;
mod icons;
mod macopen;
mod markdown;
mod seismic_ui;
mod section_ui;
mod theme;
mod tools;
#[cfg(test)]
mod ui_tests;
mod view;

use pondus_core::model::Model;

#[cfg(windows)]
fn attach_console() {
    extern "system" {
        fn AttachConsole(id: u32) -> i32;
    }
    // ATTACH_PARENT_PROCESS: print to the terminal that started us (pipes stay as they are)
    unsafe {
        AttachConsole(u32::MAX);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(windows)]
    if args.get(1).is_some_and(|a| matches!(a.as_str(), "design" | "bench" | "--gpu" | "--help" | "-h" | "--about" | "--license" | "--version")) {
        attach_console();
    }
    if args.iter().any(|a| a == "--mcp") {
        pondus_core::mcp::serve();
        return;
    }
    match args.get(1).map(String::as_str) {
        Some("design") if args.len() == 4 => {
            let r = Model::load(std::path::Path::new(&args[2])).and_then(|mut m| {
                let opts = pondus_core::design::Options { resize: true, select: true, ..Default::default() };
                let (rep, _) = pondus_core::design::run(&mut m, &opts)?;
                eprintln!("{}", serde_json::to_string_pretty(&serde_json::json!({"failing": rep.failing(), "changes": rep.changes, "skipped": rep.skipped})).unwrap_or_default());
                std::fs::write(&args[3], serde_json::to_string_pretty(&pondus_core::design::bim_export(&m, &rep)).unwrap_or_default()).map_err(|e| e.to_string())
            });
            if let Err(e) = r {
                eprintln!("pondus: {e}");
                std::process::exit(1);
            }
            return;
        }
        Some("--gpu") => {
            println!("{}", pondus_core::compute::summary());
            return;
        }
        Some("bench") if args.len() == 3 => {
            // analyse a model and report the time, where it ran and the results to compare
            let r = Model::load(std::path::Path::new(&args[2])).and_then(|m| {
                let g0 = pondus_core::compute::factorisations();
                let t0 = std::time::Instant::now();
                let a = pondus_core::analysis::analyze(&m)?;
                let secs = t0.elapsed().as_secs_f64();
                let modes: Vec<serde_json::Value> = a.seismic.as_ref().map(|s| s.modes.iter().take(6).map(|md| serde_json::json!({"T_s": md.period, "mass_xyz": md.mass_ratio})).collect()).unwrap_or_default();
                let reactions: Vec<[f64; 3]> = a.cases.iter().map(|c| { let r = c.reaction_sum(); [r[0], r[1], r[2]] }).collect();
                let umax = a.cases.first().map(|c| c.displacements.values().map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()).fold(0.0, f64::max)).unwrap_or(0.0);
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "model": args[2], "nodes": m.nodes.len(), "members": m.members.len(), "elements": a.elements,
                    "seconds": secs, "gpu_factorisations": pondus_core::compute::factorisations() - g0, "compute": pondus_core::compute::summary(),
                    "modes": modes, "reactions_by_case_kN": reactions, "max_displacement_case1_m": umax,
                })).unwrap_or_default());
                Ok(())
            });
            if let Err(e) = r {
                eprintln!("pondus: {e}");
                std::process::exit(1);
            }
            return;
        }
        Some("--help") | Some("-h") => {
            println!("pondus [file]                               the app (file: .pondus or a Stabileo/Aedifex .json)\npondus --mcp [--open f] [--autosave f]      MCP server on stdio\npondus design in.(pondus|json) out.json    design (resize + select) and write the BIM JSON\npondus bench model.pondus                   analyse and report the time, CPU or GPU, periods and reactions\npondus --gpu                                whether an NVIDIA GPU (CUDA, cuDSS) is used\npondus --about                              licence, copyright and source link\n\n{}", pondus_core::about::short());
            return;
        }
        Some("--about") | Some("--license") | Some("--version") => {
            println!("{}", pondus_core::about::notice());
            return;
        }
        _ => {}
    }
    let open = args.get(1).filter(|a| !a.starts_with('-')).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: {
            let v = eframe::egui::ViewportBuilder::default().with_title("Pondus").with_inner_size([1500.0, 940.0]).with_min_inner_size([960.0, 620.0]);
            // the window and Dock icon
            match eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
                Ok(icon) => v.with_icon(std::sync::Arc::new(icon)),
                Err(_) => v,
            }
        },
        // on the screen, wherever the last window was
        centered: true,
        ..Default::default()
    };
    macopen::install();
    if let Err(e) = eframe::run_native("Pondus", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, open))))) {
        eprintln!("pondus: {e}");
    }
}
