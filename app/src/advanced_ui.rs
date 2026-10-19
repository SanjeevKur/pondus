// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The advanced analysis window: pushover (plastic hinges to collapse), a moving load train,
//! influence lines and construction stages, each on the engine of the last analysis.

use eframe::egui::{self, RichText};
use pondus_core::advanced::{InfluencePoint, MovingSpec, PushoverResult, PushoverSpec, Stage};
use pondus_core::model::Id;

use crate::app::App;
use crate::theme::Tokens;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AdvTab {
    #[default]
    Pushover,
    Moving,
    Influence,
    Stages,
}

/// The window's state between frames.
pub(crate) struct AdvState {
    pub open: bool,
    pub tab: AdvTab,
    pub push: PushoverSpec,
    pub push_combo: bool,
    pub push_result: Option<PushoverResult>,
    pub mov_path: String,
    pub mov_axles: String,
    pub il_quantity: String,
    pub il_member: Id,
    pub il_x: f64,
    pub il_node: Id,
    pub il_reaction: bool,
    pub il_result: Vec<InfluencePoint>,
    pub stage_text: Vec<String>,
    pub stage_summary: Vec<String>,
}

impl Default for AdvState {
    fn default() -> Self {
        AdvState {
            open: false,
            tab: AdvTab::Pushover,
            push: PushoverSpec { combination: None, direction: 0, pattern: "triangular".into() },
            push_combo: false,
            push_result: None,
            mov_path: String::new(),
            mov_axles: "0 50\n2 50".into(),
            il_quantity: "My".into(),
            il_member: 1,
            il_x: 0.0,
            il_node: 1,
            il_reaction: false,
            il_result: vec![],
            stage_text: vec![],
            stage_summary: vec![],
        }
    }
}

fn ids(s: &str) -> Vec<Id> {
    s.split([',', ' ', ';']).filter_map(|x| x.trim().parse().ok()).collect()
}

fn join(v: &[Id]) -> String {
    v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")
}

impl App {
    pub(crate) fn advanced_window(&mut self, ctx: &egui::Context) {
        if !self.adv.open {
            return;
        }
        let mut open = true;
        let mut run: Option<AdvTab> = None;
        egui::Window::new("Advanced analysis").open(&mut open).default_width(520.0).show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.horizontal(|ui| {
                for (k, l) in [(AdvTab::Pushover, "Pushover"), (AdvTab::Moving, "Moving load"), (AdvTab::Influence, "Influence line"), (AdvTab::Stages, "Stages")] {
                    ui.selectable_value(&mut self.adv.tab, k, l);
                }
            });
            ui.separator();
            match self.adv.tab {
                AdvTab::Pushover => {
                    ui.label(RichText::new("Plastic hinges at Mp = Fy·Z (steel) and Mn of the designed bars (RC), added event by event until the structure is a mechanism.").small().color(t.text_dim));
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.adv.push_combo, false, "lateral push");
                        ui.selectable_value(&mut self.adv.push_combo, true, "a combination to collapse");
                    });
                    if self.adv.push_combo {
                        let names: Vec<String> = self.model.combos.iter().map(|c| c.name.clone()).collect();
                        let cur = self.adv.push.combination.clone().or_else(|| names.first().cloned()).unwrap_or_default();
                        egui::ComboBox::from_id_salt("push-combo").selected_text(&cur).show_ui(ui, |ui| {
                            for n in &names {
                                if ui.selectable_label(*n == cur, n).clicked() {
                                    self.adv.push.combination = Some(n.clone());
                                }
                            }
                        });
                        if self.adv.push.combination.is_none() {
                            self.adv.push.combination = names.first().cloned();
                        }
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Direction");
                            ui.selectable_value(&mut self.adv.push.direction, 0, "x");
                            ui.selectable_value(&mut self.adv.push.direction, 1, "y");
                            ui.label("pattern");
                            ui.selectable_value(&mut self.adv.push.pattern, "triangular".into(), "triangular");
                            ui.selectable_value(&mut self.adv.push.pattern, "uniform".into(), "uniform");
                        });
                    }
                    if ui.button("Run pushover").clicked() {
                        run = Some(AdvTab::Pushover);
                    }
                    if let Some(r) = &self.adv.push_result {
                        ui.separator();
                        let v = if self.adv.push_combo { String::new() } else { format!(" · V {:.1} kN of W {:.1} kN", r.base_shear, r.weight) };
                        ui.label(RichText::new(format!("Collapse factor λ = {:.3}{v} · {} hinges{}", r.collapse_factor, r.hinges.len(), if r.is_mechanism { " · mechanism" } else { "" })).monospace().strong());
                        curve_plot(ui, &r.curve, &t);
                        egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                            egui::Grid::new("hinges").striped(true).num_columns(3).show(ui, |ui| {
                                for h in ["Member", "at (0–1)", "λ"] {
                                    ui.label(RichText::new(h).small().color(t.text_dim));
                                }
                                ui.end_row();
                                for h in &r.hinges {
                                    ui.label(h.member.to_string());
                                    ui.label(RichText::new(format!("{:.2}", h.position)).monospace());
                                    ui.label(RichText::new(format!("{:.3}", h.at)).monospace());
                                    ui.end_row();
                                }
                            });
                        });
                        for w in &r.warnings {
                            ui.label(RichText::new(w).small().color(t.text_dim));
                        }
                    }
                }
                AdvTab::Moving => {
                    ui.label(RichText::new("A train of axles run along members in order; the envelope becomes the results 'moving max' / 'moving min' and design adds 1.2 D + factor·M.").small().color(t.text_dim));
                    if self.adv.mov_path.is_empty() {
                        if let Some(m) = &self.model.moving {
                            self.adv.mov_path = join(&m.path);
                            self.adv.mov_axles = m.axles.iter().map(|a| format!("{} {}", a.0, a.1)).collect::<Vec<_>>().join("\n");
                        } else if !self.selected.is_empty() {
                            self.adv.mov_path = join(&self.selected);
                        }
                    }
                    let mut spec = self.model.moving.clone().unwrap_or(MovingSpec { name: "Train".into(), path: vec![], axles: vec![], step: 0.25, impact: 0.0, factor: 1.6 });
                    egui::Grid::new("moving").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        ui.label("Path members");
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.adv.mov_path).desired_width(200.0).hint_text("e.g. 3, 4, 5"));
                            if ui.small_button("selection").clicked() {
                                self.adv.mov_path = join(&self.selected);
                            }
                        });
                        ui.end_row();
                        ui.label("Axles: offset m, kN");
                        ui.add(egui::TextEdit::multiline(&mut self.adv.mov_axles).desired_rows(3).desired_width(200.0).font(egui::TextStyle::Monospace));
                        ui.end_row();
                        ui.label("Step / impact / factor");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut spec.step).speed(0.05).range(0.05..=2.0).suffix(" m"));
                            ui.add(egui::DragValue::new(&mut spec.impact).speed(0.01).range(0.0..=1.0));
                            ui.add(egui::DragValue::new(&mut spec.factor).speed(0.05).range(0.0..=3.0));
                        });
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Apply and analyse").clicked() {
                            spec.path = ids(&self.adv.mov_path);
                            spec.axles = self.adv.mov_axles.lines().filter_map(|l| {
                                let v: Vec<f64> = l.split([' ', ',', '\t']).filter_map(|x| x.trim().parse().ok()).collect();
                                (v.len() >= 2).then(|| (v[0], v[1]))
                            }).collect();
                            if spec.path.is_empty() || spec.axles.is_empty() {
                                self.status = "Moving load: give the path members and the axles.".into();
                            } else {
                                self.commit();
                                self.model.moving = Some(spec.clone());
                                run = Some(AdvTab::Moving);
                            }
                        }
                        if self.model.moving.is_some() && ui.button("Remove").clicked() {
                            self.commit();
                            self.model.moving = None;
                            self.analysis = None;
                        }
                    });
                    if let Some(m) = self.model.moving.as_mut() {
                        m.step = spec.step;
                        m.impact = spec.impact;
                        m.factor = spec.factor;
                    }
                }
                AdvTab::Influence => {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.adv.il_reaction, false, "member force");
                        ui.selectable_value(&mut self.adv.il_reaction, true, "support reaction");
                    });
                    ui.horizontal(|ui| {
                        let qs: &[&str] = if self.adv.il_reaction { &["Fz", "Fx", "Fy", "Mx", "My", "Mz"] } else { &["My", "Vz", "N", "T", "Mz", "Vy"] };
                        if !qs.contains(&self.adv.il_quantity.as_str()) {
                            self.adv.il_quantity = qs[0].into();
                        }
                        for q in qs {
                            ui.selectable_value(&mut self.adv.il_quantity, q.to_string(), *q);
                        }
                    });
                    ui.horizontal(|ui| {
                        if self.adv.il_reaction {
                            ui.label("node");
                            ui.add(egui::DragValue::new(&mut self.adv.il_node));
                        } else {
                            ui.label("member");
                            ui.add(egui::DragValue::new(&mut self.adv.il_member));
                            ui.label("x");
                            ui.add(egui::DragValue::new(&mut self.adv.il_x).speed(0.05).range(0.0..=500.0).suffix(" m"));
                        }
                        if ui.button("Compute").clicked() {
                            run = Some(AdvTab::Influence);
                        }
                    });
                    if !self.adv.il_result.is_empty() {
                        influence_plot(ui, &self.adv.il_result, &t);
                    }
                }
                AdvTab::Stages => {
                    ui.label(RichText::new("Stages in order: members built (ids) and load cases applied. A member built later starts free of stress.").small().color(t.text_dim));
                    let cases: Vec<(Id, String)> = self.model.cases.iter().map(|c| (c.id, c.name.clone())).collect();
                    self.adv.stage_text.resize(self.model.stages.len(), String::new());
                    let mut remove = None;
                    let mut changed = false;
                    for (k, st) in self.model.stages.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            changed |= ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(90.0)).changed();
                            if self.adv.stage_text[k].is_empty() && !st.members.is_empty() {
                                self.adv.stage_text[k] = join(&st.members);
                            }
                            if ui.add(egui::TextEdit::singleline(&mut self.adv.stage_text[k]).desired_width(140.0).hint_text("members")).changed() {
                                st.members = ids(&self.adv.stage_text[k]);
                                changed = true;
                            }
                            for (id, n) in &cases {
                                let mut on = st.cases.contains(id);
                                if ui.checkbox(&mut on, RichText::new(n).small()).changed() {
                                    st.cases.retain(|c| c != id);
                                    if on {
                                        st.cases.push(*id);
                                    }
                                    changed = true;
                                }
                            }
                            if ui.small_button("✕").clicked() {
                                remove = Some(k);
                            }
                        });
                    }
                    if let Some(k) = remove {
                        self.model.stages.remove(k);
                        self.adv.stage_text.remove(k);
                    }
                    let _ = changed;
                    ui.horizontal(|ui| {
                        if ui.button("Add stage").clicked() {
                            let members = if self.selected.is_empty() { vec![] } else { self.selected.clone() };
                            self.model.stages.push(Stage { name: format!("Stage {}", self.model.stages.len() + 1), members: members.clone(), cases: vec![] });
                            self.adv.stage_text.push(join(&members));
                        }
                        if !self.model.stages.is_empty() && ui.button("Solve stages").clicked() {
                            run = Some(AdvTab::Stages);
                        }
                    });
                    for s in &self.adv.stage_summary {
                        ui.label(RichText::new(s).monospace());
                    }
                }
            }
        });
        if let Some(what) = run {
            self.run_advanced(what);
        }
        self.adv.open = open;
    }

    pub(crate) fn run_advanced(&mut self, what: AdvTab) {
        if self.analysis.is_none() || what == AdvTab::Moving {
            self.analysis = None;
            self.analyze();
        }
        let Some(a) = self.analysis.clone() else { return };
        match what {
            AdvTab::Pushover => {
                let mut spec = self.adv.push.clone();
                if !self.adv.push_combo {
                    spec.combination = None;
                }
                match pondus_core::advanced::pushover(&self.model, &a, self.report.as_ref(), &spec) {
                    Ok(r) => {
                        self.status = format!("Pushover: λ = {:.3}, {} hinges", r.collapse_factor, r.hinges.len());
                        self.adv.push_result = Some(r);
                    }
                    Err(e) => self.status = format!("Pushover: {e}"),
                }
            }
            AdvTab::Moving => {
                if a.moving.is_some() {
                    self.spec.result = "moving max".into();
                    self.status = "Moving load: showing the envelope 'moving max' (pick 'moving min' in the result list).".into();
                    if self.spec.diagram == pondus_core::scene::Diagram::None {
                        self.spec.diagram = pondus_core::scene::Diagram::Moment;
                    }
                }
            }
            AdvTab::Influence => {
                let (member, node) = if self.adv.il_reaction { (None, Some(self.adv.il_node)) } else { (Some(self.adv.il_member), None) };
                match pondus_core::advanced::influence(&self.model, &a, &self.adv.il_quantity, member, self.adv.il_x, node) {
                    Ok(r) => {
                        self.status = format!("Influence line of {}: {} points", self.adv.il_quantity, r.len());
                        self.adv.il_result = r;
                    }
                    Err(e) => self.status = format!("Influence line: {e}"),
                }
            }
            AdvTab::Stages => match pondus_core::advanced::staged(&self.model, &a) {
                Ok((st, fin)) => {
                    self.adv.stage_summary = st.iter().map(|s| format!("{:<14} δmax {:>7.2} mm  |My|max {:>8.2} kN·m (member {})", s.name, s.max_displacement * 1e3, s.max_moment.1, s.max_moment.0)).collect();
                    if let Some(an) = self.analysis.as_mut() {
                        an.staged = Some(fin);
                    }
                    self.spec.result = "staged (final)".into();
                    self.status = "Stages solved: showing 'staged (final)'.".into();
                }
                Err(e) => self.status = format!("Stages: {e}"),
            },
        }
    }
}

/// The capacity curve: load factor against roof displacement.
fn curve_plot(ui: &mut egui::Ui, curve: &[(f64, f64)], t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 140.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, t.field);
    let dmax = curve.iter().map(|c| c.1.abs()).fold(0.0, f64::max).max(1e-9);
    let lmax = curve.iter().map(|c| c.0).fold(0.0, f64::max).max(1e-9);
    let inner = rect.shrink(14.0);
    let pts: Vec<egui::Pos2> = curve.iter().map(|c| egui::pos2(inner.left() + inner.width() * (c.1.abs() / dmax) as f32, inner.bottom() - inner.height() * (c.0 / lmax) as f32)).collect();
    p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.6, t.accent)));
    for q in pts {
        p.circle_filled(q, 2.5, t.accent);
    }
    p.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, format!("λ to {lmax:.3}"), egui::FontId::proportional(10.0), t.text_dim);
    p.text(rect.right_bottom() - egui::vec2(6.0, 4.0), egui::Align2::RIGHT_BOTTOM, format!("roof to {:.1} mm", dmax * 1e3), egui::FontId::proportional(10.0), t.text_dim);
}

/// The influence line, member by member: only the members that carry at least 2 % of the
/// largest ordinate, each labelled, so a building's dozens of far-away beams do not drown it.
fn influence_plot(ui: &mut egui::Ui, il: &[InfluencePoint], t: &Tokens) {
    let vmax = il.iter().map(|q| q.value.abs()).fold(0.0, f64::max).max(1e-12);
    let mut groups: Vec<(Id, Vec<&InfluencePoint>)> = vec![];
    for q in il {
        match groups.last_mut() {
            Some((m, g)) if *m == q.member => g.push(q),
            _ => groups.push((q.member, vec![q])),
        }
    }
    groups.retain(|(_, g)| g.iter().any(|q| q.value.abs() >= 0.02 * vmax));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 170.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, t.field);
    let mid = rect.center().y + 6.0;
    let h = rect.height() / 2.0 - 22.0;
    let n: usize = groups.iter().map(|g| g.1.len()).sum::<usize>().max(2);
    let w = rect.width() - 16.0;
    let mut k = 0usize;
    for (m, g) in &groups {
        let x0 = rect.left() + 8.0 + w * k as f32 / (n - 1) as f32;
        let pts: Vec<egui::Pos2> = g.iter().enumerate().map(|(i, q)| egui::pos2(rect.left() + 8.0 + w * (k + i) as f32 / (n - 1) as f32, mid - h * (q.value / vmax) as f32)).collect();
        let x1 = pts.last().map(|q| q.x).unwrap_or(x0);
        p.line_segment([egui::pos2(x0, mid), egui::pos2(x1, mid)], egui::Stroke::new(1.0, t.separator));
        let mut fillp = vec![egui::pos2(x0, mid)];
        fillp.extend(pts.iter().copied());
        fillp.push(egui::pos2(x1, mid));
        p.add(egui::Shape::line(fillp, egui::Stroke::new(1.6, t.accent)));
        p.text(egui::pos2((x0 + x1) / 2.0, rect.bottom() - 4.0), egui::Align2::CENTER_BOTTOM, format!("m{m}"), egui::FontId::proportional(9.0), t.text_dim);
        k += g.len();
    }
    let hi = il.iter().max_by(|a, b| a.value.total_cmp(&b.value));
    let lo = il.iter().min_by(|a, b| a.value.total_cmp(&b.value));
    if let (Some(hi), Some(lo)) = (hi, lo) {
        p.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, format!("max {:.4} (member {}, x {:.2} m) · min {:.4} (member {}, x {:.2} m) · {} of {} loaded members shown", hi.value, hi.member, hi.x, lo.value, lo.member, lo.x, groups.len(), il.iter().map(|q| q.member).collect::<std::collections::BTreeSet<_>>().len()), egui::FontId::proportional(10.0), t.text_dim);
    }
}
