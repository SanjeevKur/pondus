// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The section analysis window: finite element properties of any solid cross-section (a model
//! section or a typed outline), with the outline, the centroid and the shear centre drawn.

use eframe::egui::{self, RichText};
use pondus_core::secfe::SectionFe;

use crate::app::App;
use crate::theme::Tokens;

#[derive(Default)]
pub(crate) struct SecState {
    pub open: bool,
    pub section: Option<pondus_core::model::Id>,
    pub outline_text: String,
    pub mesh: usize,
    pub result: Option<(Vec<[f64; 2]>, SectionFe)>,
    pub error: String,
}

fn parse(text: &str) -> Vec<[f64; 2]> {
    text.lines()
        .filter_map(|l| {
            let v: Vec<f64> = l.split([' ', ',', '\t', ';']).filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() >= 2).then(|| [v[0], v[1]])
        })
        .collect()
}

impl App {
    pub(crate) fn section_window(&mut self, ctx: &egui::Context) {
        if !self.sec.open {
            return;
        }
        if self.sec.mesh == 0 {
            self.sec.mesh = 800;
        }
        let mut open = true;
        let mut run = false;
        egui::Window::new("Section analysis").open(&mut open).default_width(560.0).show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.label(RichText::new("Finite element properties of a solid section: area, inertias, plastic moduli, St Venant J (Prandtl stress function), warping Cw and the shear centre.").small().color(t.text_dim));
            ui.horizontal(|ui| {
                let cur = self.sec.section.and_then(|id| self.model.section(id)).map(|s| s.name.clone()).unwrap_or_else(|| "outline below".into());
                egui::ComboBox::from_id_salt("sec-pick").selected_text(cur).width(200.0).show_ui(ui, |ui| {
                    if ui.selectable_label(self.sec.section.is_none(), "outline below").clicked() {
                        self.sec.section = None;
                    }
                    for s in &self.model.sections {
                        if ui.selectable_label(self.sec.section == Some(s.id), &s.name).clicked() {
                            self.sec.section = Some(s.id);
                            if let Some(o) = pondus_core::secfe::outline_of(&s.shape) {
                                self.sec.outline_text = o.iter().map(|p| format!("{:.4} {:.4}", p[0], p[1])).collect::<Vec<_>>().join("\n");
                            }
                        }
                    }
                });
                ui.label("triangles");
                ui.add(egui::DragValue::new(&mut self.sec.mesh).speed(20).range(100..=20_000));
                if ui.button("Analyse").clicked() {
                    run = true;
                }
            });
            ui.columns(2, |c| {
                c[0].label(RichText::new("Outline y z (m), one corner per line").small().color(t.text_dim));
                egui::ScrollArea::vertical().id_salt("outline").max_height(220.0).show(&mut c[0], |ui| {
                    if ui.add(egui::TextEdit::multiline(&mut self.sec.outline_text).desired_rows(12).font(egui::TextStyle::Monospace)).changed() {
                        self.sec.section = None;
                    }
                });
                let pts = parse(&self.sec.outline_text);
                draw(&mut c[1], &pts, self.sec.result.as_ref().map(|r| &r.1), &t);
            });
            if !self.sec.error.is_empty() {
                ui.label(RichText::new(&self.sec.error).color(t.error));
            }
            if let Some((_, r)) = &self.sec.result {
                egui::Grid::new("sec-props").striped(true).num_columns(4).show(ui, |ui| {
                    let row = |ui: &mut egui::Ui, a: &str, x: String, b: &str, y: String| {
                        ui.label(a);
                        ui.label(RichText::new(x).monospace());
                        ui.label(b);
                        ui.label(RichText::new(y).monospace());
                        ui.end_row();
                    };
                    row(ui, "A", format!("{:.5e} m²", r.a), "triangles", r.elements.to_string());
                    row(ui, "Iy (horizontal axis)", format!("{:.5e} m⁴", r.iy), "Iz", format!("{:.5e} m⁴", r.iz));
                    row(ui, "Zy plastic", format!("{:.5e} m³", r.zy), "Zz plastic", format!("{:.5e} m³", r.zz));
                    row(ui, "J", format!("{:.5e} m⁴", r.j), "Cw", format!("{:.5e} m⁶", r.cw));
                    row(ui, "centroid y, z", format!("{:.4}, {:.4} m", r.centroid[0], r.centroid[1]), "shear centre (from centroid)", format!("{:.4}, {:.4} m", r.shear_centre[0], r.shear_centre[1]));
                });
            }
        });
        if run {
            let pts = parse(&self.sec.outline_text);
            if pts.len() < 3 {
                self.sec.error = "The outline needs three corners or more.".into();
            } else {
                match pondus_core::secfe::analyze(&pts, self.sec.mesh) {
                    Ok(r) => {
                        self.sec.error.clear();
                        self.status = format!("Section: A {:.4e} m², J {:.4e} m⁴, Cw {:.4e} m⁶", r.a, r.j, r.cw);
                        self.sec.result = Some((pts, r));
                    }
                    Err(e) => self.sec.error = e,
                }
            }
        }
        self.sec.open = open;
    }
}

fn draw(ui: &mut egui::Ui, pts: &[[f64; 2]], r: Option<&SectionFe>, t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 240.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, t.field);
    if pts.len() < 2 {
        return;
    }
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for q in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let s = ((rect.width() - 30.0) as f64 / (hi[0] - lo[0]).max(1e-9)).min((rect.height() - 30.0) as f64 / (hi[1] - lo[1]).max(1e-9));
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let to = |q: [f64; 2]| egui::pos2(rect.center().x + ((q[0] - c[0]) * s) as f32, rect.center().y - ((q[1] - c[1]) * s) as f32);
    let mut poly: Vec<egui::Pos2> = pts.iter().map(|q| to(*q)).collect();
    poly.push(poly[0]);
    p.add(egui::Shape::line(poly, egui::Stroke::new(1.6, t.accent)));
    if let Some(r) = r {
        let g = to(r.centroid);
        p.circle_filled(g, 3.5, t.text);
        p.text(g + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, "G", egui::FontId::proportional(11.0), t.text);
        let sc = to([r.centroid[0] + r.shear_centre[0], r.centroid[1] + r.shear_centre[1]]);
        p.circle_stroke(sc, 4.5, egui::Stroke::new(1.4, t.error));
        p.text(sc + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, "S", egui::FontId::proportional(11.0), t.error);
    }
}
