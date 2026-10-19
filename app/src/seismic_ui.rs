// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The earthquake settings window and the Seismic tab: modes, base shears and storey drifts.

use eframe::egui::{self, RichText};
use pondus_core::scene::Diagram;
use pondus_core::seismic::Spectrum;

use crate::app::App;
use crate::theme::{self, Tokens};

/// Lateral systems of ASCE 7-22 Table 12.2-1 (DPT 1301/1302-61 Table 2.4-1): R, Cd, Ω0.
const SYSTEMS: [(&str, f64, f64, f64); 6] = [
    ("RC special moment frame", 8.0, 5.5, 3.0),
    ("RC intermediate moment frame", 5.0, 4.5, 3.0),
    ("RC ordinary moment frame", 3.0, 2.5, 3.0),
    ("Steel special moment frame", 8.0, 5.5, 3.0),
    ("Steel ordinary moment frame", 3.5, 3.0, 3.0),
    ("RC special shear wall (bearing wall)", 5.0, 5.0, 2.5),
];

impl App {
    pub(crate) fn seismic_window(&mut self, ctx: &egui::Context) {
        if !self.seismic_open {
            return;
        }
        let mut open = true;
        let mut sc = self.model.seismic.clone();
        egui::Window::new("Earthquake").open(&mut open).default_width(470.0).show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.checkbox(&mut sc.enabled, RichText::new("Modal and response spectrum analysis (ASCE 7-22 / DPT 1301/1302-61)").strong());
            ui.add_enabled_ui(sc.enabled, |ui| {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Spectrum");
                    let code = matches!(sc.spectrum, Spectrum::Code { .. });
                    if ui.selectable_label(code, "code: SDS, SD1, TL").clicked() && !code {
                        sc.spectrum = Spectrum::Code { sds: 0.5, sd1: 0.3, tl: 6.0 };
                    }
                    if ui.selectable_label(!code, "table T – Sa").clicked() && code {
                        sc.spectrum = Spectrum::Table { points: vec![(0.0, 0.3), (0.5, 0.6), (1.0, 0.4), (3.0, 0.15), (6.0, 0.06)] };
                        self.table_text.clear();
                    }
                });
                match &mut sc.spectrum {
                    Spectrum::Code { sds, sd1, tl } => {
                        ui.horizontal(|ui| {
                            ui.label("SDS");
                            ui.add(egui::DragValue::new(sds).speed(0.01).range(0.01..=3.0).suffix(" g"));
                            ui.label("SD1");
                            ui.add(egui::DragValue::new(sd1).speed(0.01).range(0.01..=3.0).suffix(" g"));
                            ui.label("TL");
                            ui.add(egui::DragValue::new(tl).speed(0.1).range(1.0..=16.0).suffix(" s"));
                        });
                    }
                    Spectrum::Table { points } => {
                        if self.table_text.is_empty() {
                            self.table_text = points.iter().map(|p| format!("{} {}", p.0, p.1)).collect::<Vec<_>>().join("\n");
                        }
                        ui.label(RichText::new("One line per point: period (s) and Sa (g), e.g. a Bangkok basin zone of DPT 1302-61").small().color(t.text_dim));
                        if ui.add(egui::TextEdit::multiline(&mut self.table_text).desired_rows(5).font(egui::TextStyle::Monospace)).changed() {
                            let pts: Vec<(f64, f64)> = self.table_text.lines().filter_map(|l| {
                                let v: Vec<f64> = l.split([' ', ',', '\t', ';']).filter_map(|x| x.trim().parse().ok()).collect();
                                (v.len() >= 2).then(|| (v[0], v[1]))
                            }).collect();
                            if pts.len() >= 2 {
                                *points = pts;
                                points.sort_by(|a, b| a.0.total_cmp(&b.0));
                            }
                        }
                    }
                }
                spectrum_plot(ui, &sc.spectrum, &t, self.analysis.as_ref().and_then(|a| a.seismic.as_ref()).map(|s| s.modes.iter().take(6).map(|m| m.period).collect()).unwrap_or_default());
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("System");
                    egui::ComboBox::from_id_salt("system").selected_text(SYSTEMS.iter().find(|s| s.1 == sc.r && s.2 == sc.cd).map(|s| s.0).unwrap_or("custom")).width(240.0).show_ui(ui, |ui| {
                        for (n, r, cd, o) in SYSTEMS {
                            if ui.selectable_label(sc.r == r && sc.cd == cd, format!("{n}  (R {r}, Cd {cd}, Ω0 {o})")).clicked() {
                                sc.r = r;
                                sc.cd = cd;
                                sc.omega0 = o;
                            }
                        }
                    });
                });
                egui::Grid::new("seismic-grid").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("R");
                    ui.add(egui::DragValue::new(&mut sc.r).speed(0.1).range(1.0..=10.0));
                    ui.label("Ie");
                    ui.add(egui::DragValue::new(&mut sc.ie).speed(0.05).range(1.0..=1.5));
                    ui.end_row();
                    ui.label("Cd");
                    ui.add(egui::DragValue::new(&mut sc.cd).speed(0.1).range(1.0..=8.0));
                    ui.label("ρ");
                    ui.add(egui::DragValue::new(&mut sc.rho).speed(0.1).range(1.0..=1.3));
                    ui.end_row();
                    ui.label("Live in the mass");
                    ui.add(egui::DragValue::new(&mut sc.live_mass).speed(0.05).range(0.0..=1.0));
                    ui.label("Damping ξ");
                    ui.add(egui::DragValue::new(&mut sc.damping).speed(0.005).range(0.01..=0.2));
                    ui.end_row();
                    ui.label("Scale RSA to ELF ×");
                    ui.add(egui::DragValue::new(&mut sc.elf_ratio).speed(0.05).range(0.0..=1.0)).on_hover_text("ASCE 7-22 12.9.1.4: 1.0; older editions 0.85");
                    ui.label("Drift limit");
                    ui.add(egui::DragValue::new(&mut sc.drift_limit).speed(0.001).range(0.005..=0.03)).on_hover_text("Allowable storey drift / storey height (Table 12.12-1: 0.020)");
                    ui.end_row();
                    ui.label("Ta = Ct hn^x");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut sc.ct).speed(0.001).range(0.01..=0.1));
                        ui.add(egui::DragValue::new(&mut sc.x).speed(0.01).range(0.5..=1.0));
                    });
                    ui.label("Cu, live in E");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut sc.cu).speed(0.05).range(1.0..=2.0));
                        ui.add(egui::DragValue::new(&mut sc.live_in_e).speed(0.5).range(0.5..=1.0));
                    });
                    ui.end_row();
                });
                ui.label(RichText::new("Seismic weight W = D + share x L; combinations (1.2 + 0.2 SDS)D + L ± ρE and (0.9 − 0.2 SDS)D ± ρE for Ex and Ey are added to the design. Not included: accidental torsion, P-Δ, 100 %/30 % orthogonal combination.").small().color(t.text_dim));
            });
            if ui.button("Analyse").clicked() {
                self.pending_analyse = true;
            }
        });
        if sc != self.model.seismic {
            self.commit();
            self.model.seismic = sc;
        }
        if self.pending_analyse {
            self.pending_analyse = false;
            self.analyze();
            if self.model.seismic.enabled {
                self.bottom = crate::app::BottomTab::Seismic;
            }
        }
        self.seismic_open = open;
    }

    pub(crate) fn seismic_tab(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get(ui.ctx());
        let Some(s) = self.analysis.as_ref().and_then(|a| a.seismic.clone()) else {
            ui.label(RichText::new("Analyse to see the modes and the earthquake results.").color(t.text_dim));
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Seismic weight {:.0} kN ({:.1} t) · {} modes · mass taking part x {:.0} %, y {:.0} %", s.weight, s.mass, s.modes.len(), s.mass_reached[0] * 100.0, s.mass_reached[1] * 100.0)).font(theme::medium_font(12.5)));
            ui.checkbox(&mut self.animate, "animate");
        });
        for w in &s.warnings {
            ui.label(RichText::new(w).small().color(t.warn));
        }
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                egui::Grid::new("modes-table").striped(true).num_columns(5).spacing([12.0, 2.0]).show(ui, |ui| {
                    for h in ["Mode", "T (s)", "f (Hz)", "mass x / y / z", "Σ x / y"] {
                        ui.label(RichText::new(h).small().color(t.text_dim));
                    }
                    ui.end_row();
                    let mut run = [0.0; 3];
                    for (k, m) in s.modes.iter().enumerate() {
                        for q in 0..3 {
                            run[q] += m.mass_ratio[q];
                        }
                        if ui.selectable_label(self.spec.diagram == Diagram::Mode && self.spec.mode == k, format!("{}", k + 1)).clicked() {
                            self.spec.diagram = Diagram::Mode;
                            self.spec.mode = k;
                        }
                        ui.label(RichText::new(format!("{:.3}", m.period)).monospace());
                        ui.label(RichText::new(format!("{:.2}", m.frequency)).monospace());
                        ui.label(RichText::new(format!("{:.1} / {:.1} / {:.1} %", m.mass_ratio[0] * 100.0, m.mass_ratio[1] * 100.0, m.mass_ratio[2] * 100.0)).monospace());
                        ui.label(RichText::new(format!("{:.1} / {:.1} %", run[0] * 100.0, run[1] * 100.0)).monospace());
                        ui.end_row();
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                for d in &s.dirs {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(self.spec.result == d.name && self.spec.diagram == Diagram::Moment, RichText::new(&d.name).strong()).on_hover_text("Show the moments under it").clicked() {
                            self.spec.result = d.name.clone();
                            self.spec.diagram = Diagram::Moment;
                        }
                        ui.label(RichText::new(format!("T1 {:.3} s (ELF {:.3} s) · Cs {:.4} · V ELF {:.1} kN · V RSA {:.1} kN × {:.3}", d.t1, d.t_used, d.cs, d.v_elf, d.v_rsa, d.scale)).monospace());
                    });
                    egui::Grid::new(format!("drift-{}", d.name)).striped(true).num_columns(5).spacing([12.0, 2.0]).show(ui, |ui| {
                        for h in ["Storey z (m)", "δe (mm)", "Cd δe / Ie (mm)", "allowed (mm)", "ratio"] {
                            ui.label(RichText::new(h).small().color(t.text_dim));
                        }
                        ui.end_row();
                        for x in d.drifts.iter().rev() {
                            ui.label(format!("{:.2} – {:.2}", x.z0, x.z1));
                            ui.label(RichText::new(format!("{:.2}", x.delta_e * 1e3)).monospace());
                            ui.label(RichText::new(format!("{:.2}", x.delta * 1e3)).monospace());
                            ui.label(RichText::new(format!("{:.1}", x.allowed * 1e3)).monospace());
                            let c = if x.ratio > 1.0 { t.error } else if x.ratio > 0.9 { t.warn } else { t.ok };
                            ui.label(RichText::new(format!("{:.2}", x.ratio)).monospace().color(c));
                            ui.end_row();
                        }
                    });
                    ui.add_space(4.0);
                }
            });
        });
    }
}

/// The design spectrum Sa(T) with the first modes' periods marked.
fn spectrum_plot(ui: &mut egui::Ui, sp: &Spectrum, t: &Tokens, periods: Vec<f64>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 120.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, t.field);
    let tmax = 4.0f64;
    let samples: Vec<(f64, f64)> = (0..=200).map(|k| {
        let tt = tmax * k as f64 / 200.0;
        (tt, sp.sa(tt))
    }).collect();
    let smax = samples.iter().map(|s| s.1).fold(0.0, f64::max).max(1e-6) * 1.15;
    let to = |tt: f64, sa: f64| egui::pos2(rect.left() + 36.0 + (rect.width() - 46.0) * (tt / tmax) as f32, rect.bottom() - 18.0 - (rect.height() - 28.0) * (sa / smax) as f32);
    for w in samples.windows(2) {
        p.line_segment([to(w[0].0, w[0].1), to(w[1].0, w[1].1)], egui::Stroke::new(2.0, t.accent));
    }
    for (k, tt) in periods.iter().enumerate().filter(|(_, t)| **t <= tmax) {
        let a = to(*tt, 0.0);
        let b = to(*tt, sp.sa(*tt));
        p.line_segment([a, b], egui::Stroke::new(1.0, t.warn));
        p.text(b - egui::vec2(0.0, 4.0), egui::Align2::CENTER_BOTTOM, format!("T{}", k + 1), egui::FontId::proportional(9.5), t.warn);
    }
    p.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, format!("Sa (g), max {:.2}", smax / 1.15), egui::FontId::proportional(10.0), t.text_dim);
    p.text(rect.right_bottom() - egui::vec2(6.0, 4.0), egui::Align2::RIGHT_BOTTOM, format!("T (s) to {tmax:.0}"), egui::FontId::proportional(10.0), t.text_dim);
}

impl App {
    pub(crate) fn run_buckling(&mut self) {
        if self.analysis.is_none() {
            self.analyze();
        }
        let Some(a) = self.analysis.as_ref() else { return };
        let want = self.spec.result.clone();
        let Some(c) = self.model.combos.iter().find(|c| c.name == want && c.kind == pondus_core::model::ComboKind::Uls).or_else(|| self.model.combos.iter().find(|c| c.kind == pondus_core::model::ComboKind::Uls)).cloned() else {
            self.status = "Buckling needs a ULS combination.".into();
            return;
        };
        match pondus_core::stability::buckling(&self.model, a, &c, 6) {
            Ok(modes) => {
                self.status = format!("Buckling of '{}': λ1 = {:.3}", c.name, modes.first().map(|m| m.factor).unwrap_or(0.0));
                if let Some(a) = self.analysis.as_mut() {
                    a.buckling = Some((c.name.clone(), modes));
                }
                self.spec.diagram = Diagram::Buckling;
                self.spec.mode = 0;
                self.bottom = crate::app::BottomTab::Stability;
            }
            Err(e) => self.status = e,
        }
    }

    pub(crate) fn stability_tab(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get(ui.ctx());
        let Some((name, modes)) = self.analysis.as_ref().and_then(|a| a.buckling.clone()) else { return };
        ui.label(RichText::new(format!("Linear buckling of '{name}': λ times the combination makes the structure unstable")).font(theme::medium_font(12.5)));
        ui.label(RichText::new("λ < 1: unstable under it · λ < 10: second-order effects matter (Analyse with P-Delta) · λ ≥ 10: first order is adequate").small().color(t.text_dim));
        egui::Grid::new("buckling").striped(true).num_columns(3).show(ui, |ui| {
            for (k, m) in modes.iter().enumerate() {
                if ui.selectable_label(self.spec.diagram == Diagram::Buckling && self.spec.mode == k, format!("mode {}", k + 1)).clicked() {
                    self.spec.diagram = Diagram::Buckling;
                    self.spec.mode = k;
                }
                let c = if m.factor < 1.0 { t.error } else if m.factor < 10.0 { t.warn } else { t.ok };
                ui.label(RichText::new(format!("λ = {:.3}", m.factor)).monospace().color(c));
                ui.end_row();
            }
        });
    }

    pub(crate) fn time_history_window(&mut self, ctx: &egui::Context) {
        if !self.th_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Time history").open(&mut open).default_width(520.0).show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.horizontal(|ui| {
                if ui.button("Load record…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Ground motion (PEER .AT2, table)", &["at2", "AT2", "txt", "csv", "dat"]).pick_file() {
                        let dt = self.th_dt_text.trim().parse::<f64>().ok();
                        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|s| pondus_core::timehistory::parse_record(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), &s, dt)) {
                            Ok(r) => {
                                self.th_settings.dt = r.dt.max(0.005).min(0.02);
                                self.th_record = Some(r);
                                self.th_result = None;
                            }
                            Err(e) => self.status = format!("Record: {e}"),
                        }
                    }
                }
                ui.label(RichText::new("single-column files: dt").small().color(t.text_dim));
                ui.add(egui::TextEdit::singleline(&mut self.th_dt_text).desired_width(50.0));
            });
            if let Some(r) = &self.th_record {
                ui.label(RichText::new(format!("{} · {} points at {} s ({:.1} s) · PGA {:.3} g", r.name, r.accel.len(), r.dt, r.duration(), r.pga())).monospace());
            } else {
                ui.label(RichText::new("No record: load a PEER NGA .AT2 file or a two-column table (time s, acceleration g).").color(t.text_dim));
            }
            let st = &mut self.th_settings;
            ui.horizontal(|ui| {
                ui.label("Direction");
                ui.selectable_value(&mut st.direction, 0, "x");
                ui.selectable_value(&mut st.direction, 1, "y");
                ui.label("scale");
                ui.add(egui::DragValue::new(&mut st.scale).speed(0.05).range(0.01..=20.0));
                ui.label("dt");
                ui.add(egui::DragValue::new(&mut st.dt).speed(0.001).range(0.001..=0.05).suffix(" s"));
                ui.label("ξ");
                ui.add(egui::DragValue::new(&mut st.damping).speed(0.005).range(0.0..=0.2));
            });
            let can = self.th_record.is_some();
            if ui.add_enabled(can, egui::Button::new("Run")).clicked() {
                if self.analysis.is_none() {
                    self.pending_analyse = true;
                }
                self.pending_th = true;
            }
            if let Some(res) = &self.th_result {
                ui.separator();
                ui.label(RichText::new(format!("Roof {:.2} mm at {:.2} s · PGA {:.3} g · Σ peak reactions {:.0} kN (upper bound of the base shear) · W {:.0} kN", res.roof_peak * 1e3, res.roof_peak_at, res.pga_g, res.base_shear_bound, res.weight)).monospace());
                history_plot(ui, &res.time, &res.ground, &res.roof, &t);
                egui::Grid::new("th-storeys").striped(true).num_columns(4).show(ui, |ui| {
                    for h in ["Storey z (m)", "peak drift (mm)", "drift / h", "at (s)"] {
                        ui.label(RichText::new(h).small().color(t.text_dim));
                    }
                    ui.end_row();
                    for s in res.storeys.iter().rev() {
                        ui.label(format!("{:.2} – {:.2}", s.z0, s.z1));
                        ui.label(RichText::new(format!("{:.2}", s.drift * 1e3)).monospace());
                        ui.label(RichText::new(format!("{:.5}", s.ratio)).monospace());
                        ui.label(RichText::new(format!("{:.2}", s.at)).monospace());
                        ui.end_row();
                    }
                });
                ui.label(RichText::new("Linear elastic response: drifts are elastic, not divided by R.").small().color(t.text_dim));
            }
        });
        if self.pending_analyse {
            self.pending_analyse = false;
            self.analyze();
        }
        if std::mem::take(&mut self.pending_th) {
            if let (Some(a), Some(r)) = (self.analysis.as_ref(), self.th_record.as_ref()) {
                match pondus_core::timehistory::run(&self.model, a, r, &self.th_settings) {
                    Ok(res) => {
                        self.status = format!("Time history: roof {:.2} mm at {:.2} s", res.roof_peak * 1e3, res.roof_peak_at);
                        self.th_result = Some(res);
                    }
                    Err(e) => self.status = format!("Time history: {e}"),
                }
            }
        }
        self.th_open = open;
    }
}

/// The ground acceleration (g, scaled to fit) and the roof displacement (mm) against time.
fn history_plot(ui: &mut egui::Ui, time: &[f64], ground: &[f64], roof: &[f64], t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 150.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, t.field);
    let tmax = time.last().copied().unwrap_or(1.0).max(1e-6);
    let mid = rect.center().y;
    let h = rect.height() / 2.0 - 12.0;
    p.line_segment([egui::pos2(rect.left(), mid), egui::pos2(rect.right(), mid)], egui::Stroke::new(1.0, t.separator));
    for (series, color) in [(ground, t.text_faint), (roof, t.accent)] {
        let m = series.iter().map(|v| v.abs()).fold(0.0, f64::max).max(1e-12);
        let step = (series.len() / 800).max(1);
        let pts: Vec<egui::Pos2> = series.iter().zip(time).step_by(step).map(|(v, tt)| egui::pos2(rect.left() + rect.width() * (*tt / tmax) as f32, mid - h * (*v / m) as f32)).collect();
        p.add(egui::Shape::line(pts, egui::Stroke::new(if color == t.accent { 1.6 } else { 1.0 }, color)));
    }
    p.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, "roof displacement (blue) and ground acceleration (grey), each to its own peak", egui::FontId::proportional(10.0), t.text_dim);
    p.text(rect.right_bottom() - egui::vec2(6.0, 4.0), egui::Align2::RIGHT_BOTTOM, format!("t to {tmax:.1} s"), egui::FontId::proportional(10.0), t.text_dim);
}
