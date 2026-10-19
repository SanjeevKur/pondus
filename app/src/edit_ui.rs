// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The edit window: move, copy (arrays), rotate, mirror, split and merge the selected members,
//! through the same commands the agent uses; and the file import / export of Excel, DXF and IFC.

use eframe::egui::{self, RichText};
use serde_json::json;

use crate::app::App;
use crate::theme::Tokens;

pub(crate) struct EditState {
    pub open: bool,
    pub d: [f64; 3],
    pub copy: bool,
    pub n: usize,
    pub centre: [f64; 3],
    pub angle: f64,
    pub point: [f64; 3],
    pub axis: usize,
    pub parts: usize,
}

impl Default for EditState {
    fn default() -> Self {
        EditState { open: false, d: [6.0, 0.0, 0.0], copy: true, n: 1, centre: [0.0; 3], angle: 90.0, point: [0.0; 3], axis: 0, parts: 2 }
    }
}

fn xyz(ui: &mut egui::Ui, v: &mut [f64; 3]) {
    for (k, l) in ["x", "y", "z"].iter().enumerate() {
        ui.label(*l);
        ui.add(egui::DragValue::new(&mut v[k]).speed(0.05).suffix(" m"));
    }
}

impl App {
    pub(crate) fn edit_window(&mut self, ctx: &egui::Context) {
        if !self.edit.open {
            return;
        }
        let mut open = true;
        let mut cmd: Option<(String, serde_json::Value)> = None;
        egui::Window::new("Edit members").open(&mut open).default_width(430.0).show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            let sel = self.selected.clone();
            ui.label(RichText::new(if sel.is_empty() { "Select members first (Select tool, click or drag a box).".to_string() } else { format!("{} members selected: {}", sel.len(), sel.iter().take(12).map(|x| x.to_string()).collect::<Vec<_>>().join(", ")) }).color(if sel.is_empty() { t.warn } else { t.text_dim }));
            let e = &mut self.edit;
            ui.horizontal(|ui| {
                ui.checkbox(&mut e.copy, "copy (keep the originals)");
                ui.add_enabled(e.copy, egui::DragValue::new(&mut e.n).range(1..=50).prefix("copies "));
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Move by").strong());
                xyz(ui, &mut e.d);
            });
            if ui.add_enabled(!sel.is_empty(), egui::Button::new(if e.copy { "Copy" } else { "Move" })).clicked() {
                cmd = Some(("transform".into(), json!({"members": sel, "action": "move", "d": e.d, "copy": e.copy, "n": e.n})));
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Rotate").strong());
                ui.add(egui::DragValue::new(&mut e.angle).speed(1.0).suffix("°"));
                ui.label("about z through");
            });
            ui.horizontal(|ui| xyz(ui, &mut e.centre));
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("Rotate")).clicked() {
                cmd = Some(("transform".into(), json!({"members": sel, "action": "rotate", "centre": e.centre, "angle": e.angle, "copy": e.copy})));
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mirror").strong());
                ui.selectable_value(&mut e.axis, 0, "plane x = const");
                ui.selectable_value(&mut e.axis, 1, "plane y = const");
                ui.selectable_value(&mut e.axis, 2, "plane z = const");
            });
            ui.horizontal(|ui| xyz(ui, &mut e.point));
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("Mirror")).clicked() {
                let mut n = [0.0; 3];
                n[e.axis] = 1.0;
                cmd = Some(("transform".into(), json!({"members": sel, "action": "mirror", "point": e.point, "normal": n, "copy": e.copy})));
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Split").strong());
                ui.add(egui::DragValue::new(&mut e.parts).range(2..=20).suffix(" parts"));
                if ui.add_enabled(sel.len() == 1, egui::Button::new("Split")).on_hover_text("one selected member into equal pieces").clicked() {
                    cmd = Some(("split_member".into(), json!({"member": sel[0], "parts": e.parts})));
                }
                if ui.add_enabled(sel.len() == 2, egui::Button::new("Merge")).on_hover_text("two collinear members sharing a free node").clicked() {
                    cmd = Some(("merge_members".into(), json!({"a": sel[0], "b": sel[1]})));
                }
            });
        });
        if let Some((name, args)) = cmd {
            let before = self.model.members.len();
            self.run_command(&name, args);
            if self.model.members.len() != before || name == "transform" {
                self.selected.retain(|id| self.model.member(*id).is_some());
            }
        }
        self.edit.open = open;
    }

    /// File › Import: Excel replaces the model, DXF and IFC add their members to it.
    pub(crate) fn import_file(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Excel, DXF or IFC", &["xlsx", "dxf", "ifc"]).pick_file() else { return };
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let res: Result<String, String> = (|| {
            if ext == "xlsx" {
                let m = pondus_core::excel::import(&p)?;
                self.set_model(m);
                return Ok(format!("Read {}", p.display()));
            }
            let txt = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
            let r = if ext == "dxf" { pondus_core::dxf::import(&txt, &Default::default())? } else { pondus_core::ifc::import(&txt, &Default::default())? };
            self.commit();
            let ap = pondus_core::dxf::apply(&mut self.model, &r)?;
            self.analysis = None;
            self.report = None;
            self.view.fit = true;
            let w: Vec<String> = r.warnings.iter().chain(&ap.warnings).cloned().collect();
            Ok(format!("Imported {}: {} members, {} slabs{}", p.display(), ap.members.len(), ap.slabs.len(), if w.is_empty() { String::new() } else { format!(" ({})", w.join("; ")) }))
        })();
        self.status = res.unwrap_or_else(|e| format!("Import: {e}"));
    }

    pub(crate) fn export_file(&mut self, ext: &str) {
        let name = format!("{}.{ext}", self.model.name.replace([' ', '/'], "_"));
        let Some(p) = rfd::FileDialog::new().add_filter(ext, &[ext]).set_file_name(&name).save_file() else { return };
        let r = match ext {
            "xlsx" => pondus_core::excel::export(&self.model, self.analysis.as_ref(), self.report.as_ref(), &p),
            "dxf" => std::fs::write(&p, pondus_core::dxf::export(&self.model, self.analysis.as_ref(), &Default::default())).map_err(|e| e.to_string()),
            _ => std::fs::write(&p, pondus_core::ifc::export(&self.model, self.report.as_ref())).map_err(|e| e.to_string()),
        };
        self.status = match r {
            Ok(()) => format!("Wrote {}", p.display()),
            Err(e) => format!("Export: {e}"),
        };
    }
}
