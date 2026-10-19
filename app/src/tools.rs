// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The ribbon and the drawing tools, after Stabileo's editor: View (pan, select, fit,
//! views), Draw (node, member, slab, frame), Properties (materials, sections), Conditions
//! (support, load, hinge, delete), Analyse (analyse, design) and Results (deformed, N, My, Vz,
//! Mz, Vy, T, utilisation). The armed tool shows its options in the bar under the ribbon and
//! works by clicking in the view: points snap to the nodes and to the work plane's grid.
//! The model's data (nodes, members, supports, loads, sections, slabs) is edited in tables.

use eframe::egui::{self, RichText};
use pondus_core::model::*;
use pondus_core::scene::Diagram;
use serde_json::json;

use crate::app::{App, BottomTab, RightTab};
use crate::theme::{self, Tokens};
use crate::view::{Drag, DragMode, Overlay, Picked};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pan,
    Select,
    Node,
    Member,
    Beam,
    Column,
    Brace,
    Bay,
    Wall,
    Slab,
    Support,
    Load,
    Hinge,
    Delete,
}

impl Tool {
    pub fn key(self) -> &'static str {
        match self {
            Tool::Pan => "A",
            Tool::Select => "V",
            Tool::Node => "N",
            Tool::Member => "E",
            Tool::Beam => "G",
            Tool::Column => "C",
            Tool::Brace => "K",
            Tool::Bay => "Y",
            Tool::Wall => "W",
            Tool::Slab => "B",
            Tool::Support => "S",
            Tool::Load => "L",
            Tool::Hinge => "R",
            Tool::Delete => "X",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LoadType {
    Nodal,
    Line,
    Point,
    Settlement,
    Thermal,
}

/// The tools' options (Stabileo's tool options bar).
pub struct ToolOpts {
    pub node: [f64; 3],
    pub section: String,
    pub material: String,
    pub chain: bool,
    pub support: &'static str,
    pub load_type: LoadType,
    pub load_case: Id,
    /// Nodal force or moment (global), line load (kN/m), point load (kN) with its position as
    /// a fraction of the member.
    pub f: [f64; 6],
    pub w: f64,
    pub w_dir: usize,
    pub p: f64,
    pub p_at: f64,
    pub hinge_end: &'static str,
    pub springs: [f64; 6],
    pub settle: [f64; 6],
    pub dt: f64,
    pub slab_h: f64,
    pub slab_dead: f64,
    pub slab_live: f64,
    /// The work plane height (floors) when no level is chosen in the plan view; the grid.
    pub work_z: f64,
    pub grid: f64,
    /// The first point of the member being drawn.
    pub from: Option<[f64; 3]>,
    /// The beam, column and brace tools' sections; a column's height and whether it rises
    /// from the work plane (else hangs down to the level below); a bay's slab and columns.
    pub beam_section: String,
    pub col_section: String,
    pub brace_section: String,
    pub col_h: f64,
    pub col_up: bool,
    pub bay_slab: bool,
    pub bay_cols: bool,
    /// What a dropped beam or bay measures (a drop has no second point), m.
    pub drop_len: f64,
    pub bay_w: f64,
    pub bay_d: f64,
    /// The wall tool's thickness (m), and its base and top (None: the model's lowest and
    /// highest levels).
    pub wall_t: f64,
    pub wall_z0: Option<f64>,
    pub wall_z1: Option<f64>,
}

impl Default for ToolOpts {
    fn default() -> Self {
        ToolOpts { node: [0.0; 3], section: "300 x 600".into(), material: "auto".into(), chain: true, support: "fixed", load_type: LoadType::Line, load_case: 1, f: [0.0, 0.0, -10.0, 0.0, 0.0, 0.0], w: 10.0, w_dir: 2, p: 20.0, p_at: 0.5, hinge_end: "both", springs: [0.0, 0.0, 50_000.0, 0.0, 0.0, 0.0], settle: [0.0, 0.0, -0.01, 0.0, 0.0, 0.0], dt: 20.0, slab_h: 0.15, slab_dead: 1.5, slab_live: 2.0, work_z: 0.0, grid: 0.5, from: None, beam_section: "300 x 600".into(), col_section: "400 x 400".into(), brace_section: "H 150x150x7x10".into(), col_h: 3.5, col_up: true, bay_slab: true, bay_cols: false, drop_len: 6.0, bay_w: 6.0, bay_d: 5.0, wall_t: 0.25, wall_z0: None, wall_z1: None }
    }
}

/// A ribbon button: an icon above its label, lit when `on`.
fn big(ui: &mut egui::Ui, t: &Tokens, icon: &str, label: &str, on: bool, tip: &str) -> bool {
    let w = 46.0f32.max(ui.fonts_mut(|f| f.layout_no_wrap(label.into(), egui::FontId::proportional(10.5), t.text).size().x) + 8.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 50.0), egui::Sense::click());
    let fill = if on {
        t.accent_soft
    } else if resp.hovered() {
        t.hover
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, theme::RADIUS as f32, fill);
    if on {
        ui.painter().rect_stroke(rect, theme::RADIUS as f32, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    }
    let ir = egui::Rect::from_center_size(rect.center_top() + egui::vec2(0.0, 17.0), egui::vec2(20.0, 20.0));
    crate::icons::paint(ui, icon, ir, if on { t.accent_text } else { t.text });
    ui.painter().text(rect.center_bottom() - egui::vec2(0.0, 8.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(10.5), if on { t.accent_text } else { t.text_dim });
    let resp = resp.on_hover_text(tip);
    resp.clicked()
}

/// The delete button of a table row (a Lucide icon: Inter has no ✕ glyph).
fn xbtn(ui: &mut egui::Ui, t: &Tokens) -> egui::Response {
    ui.add(egui::Button::image(crate::icons::image("x", 11.0, t.text_dim)).small())
}

/// A text button for result diagrams (N, My, …).
fn small(ui: &mut egui::Ui, t: &Tokens, label: &str, on: bool, tip: &str) -> bool {
    let b = egui::Button::new(RichText::new(label).color(if on { t.accent_text } else { t.text })).fill(if on { t.accent_soft } else { egui::Color32::TRANSPARENT }).min_size(egui::vec2(27.0, 21.0));
    ui.add(b).on_hover_text(tip).clicked()
}

fn group(ui: &mut egui::Ui, t: &Tokens, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            add(ui);
        });
        ui.label(RichText::new(title).small().color(t.text_faint));
    });
    ui.add(egui::Separator::default().vertical().spacing(6.0));
}

impl App {
    pub(crate) fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.opts.from = None;
        if matches!(tool, Tool::Node | Tool::Member | Tool::Beam | Tool::Column | Tool::Brace | Tool::Bay | Tool::Wall | Tool::Support | Tool::Load | Tool::Hinge) {
            self.bottom = match tool {
                Tool::Node => BottomTab::Nodes,
                Tool::Member | Tool::Beam | Tool::Column | Tool::Brace | Tool::Bay | Tool::Wall | Tool::Hinge => BottomTab::Members,
                Tool::Support => BottomTab::Supports,
                _ => BottomTab::Loads,
            };
        }
        if tool == Tool::Slab {
            self.bottom = BottomTab::Slabs;
            // slabs are drawn on a floor: the plan of the work level
            if self.spec.view != "plan" {
                self.spec.view = "plan".into();
                self.view.set_preset("plan");
            }
            // on the lowest floor that has beams, unless one is chosen
            if self.spec.z.is_none() {
                let mut zs: Vec<f64> = self.model.members.iter().filter(|m| self.model.kind(m) == MemberKind::Beam).filter_map(|m| self.model.node(m.i).map(|n| (n.z * 1000.0).round() / 1000.0)).collect();
                zs.sort_by(f64::total_cmp);
                if let Some(z) = zs.first() {
                    self.spec.z = Some(*z);
                    self.opts.work_z = *z;
                    self.view.fit = true;
                }
            }
        }
        self.status = format!("{tool:?} tool ({}). {}", tool.key(), self.tool_hint());
    }

    fn tool_hint(&self) -> &'static str {
        match self.tool {
            Tool::Pan => "Drag to pan, scroll to zoom.",
            Tool::Select => "Click to select (Shift adds). Drag a node or member to move it, Ctrl-drag to copy; drag empty space to orbit.",
            Tool::Node => "Click on the work plane to add a node (it snaps to the grid), or type its coordinates.",
            Tool::Member => "Drag from the start to the end, or click both (nodes or grid points); Esc ends the chain.",
            Tool::Beam => "Drag a beam on the work plane (or click its two ends); it snaps to the grid and to nodes.",
            Tool::Column => "Click a point for a column, or drag a rectangle for one at each corner.",
            Tool::Brace => "Drag a brace between two nodes (an elevation view is easiest).",
            Tool::Bay => "Drag a rectangle on a floor: four beams, a slab and, if chosen, columns under the corners.",
            Tool::Wall => "Drag a wall along its plan line (a plan view is easiest); it runs from the base to the top level.",
            Tool::Slab => "In the plan of a floor, click inside a bay closed by beams to put a slab in it, or drag a rectangle for a panel.",
            Tool::Support => "Click a node to support it.",
            Tool::Load => "Click a node (nodal load) or a member (line or point load).",
            Tool::Hinge => "Click a member to pin or unpin its ends.",
            Tool::Delete => "Click a node or a member to delete it.",
        }
    }

    /// The work plane and the view overlay for the armed tool.
    pub(crate) fn overlay(&self) -> Overlay {
        self.overlay_for(self.tool, false)
    }

    /// The overlay of a tool, also for one dragged in from the palette (`dropping`).
    pub(crate) fn overlay_for(&self, tool: Tool, dropping: bool) -> Overlay {
        let draws = matches!(tool, Tool::Node | Tool::Member | Tool::Beam | Tool::Column | Tool::Brace | Tool::Bay | Tool::Wall | Tool::Slab);
        let drag = match tool {
            Tool::Member | Tool::Beam | Tool::Brace | Tool::Wall => DragMode::Line,
            Tool::Bay | Tool::Slab | Tool::Column => DragMode::Rect,
            Tool::Select => DragMode::Move,
            _ => DragMode::Orbit,
        };
        let plane = if !draws && tool != Tool::Select && !dropping {
            None
        } else if self.spec.view == "elevation" {
            Some((1, self.spec.y.unwrap_or(0.0)))
        } else if self.spec.view == "plan" {
            Some((2, self.spec.z.unwrap_or(self.opts.work_z)))
        } else {
            Some((2, self.opts.work_z))
        };
        let marked = self.opts.from.and_then(|p| self.model.find_node(p)).into_iter().collect();
        Overlay {
            plane,
            grid: self.opts.grid,
            from: if dropping { None } else { self.opts.from },
            pan_tool: tool == Tool::Pan,
            nodes: matches!(tool, Tool::Node | Tool::Member | Tool::Beam | Tool::Column | Tool::Brace | Tool::Wall | Tool::Support | Tool::Load | Tool::Delete),
            marked_nodes: marked,
            drag: if dropping { DragMode::Orbit } else { drag },
            show_grid: draws,
            dropping,
        }
    }

    pub(crate) fn tool_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let keys = [(egui::Key::A, Tool::Pan), (egui::Key::V, Tool::Select), (egui::Key::N, Tool::Node), (egui::Key::E, Tool::Member), (egui::Key::G, Tool::Beam), (egui::Key::C, Tool::Column), (egui::Key::K, Tool::Brace), (egui::Key::Y, Tool::Bay), (egui::Key::W, Tool::Wall), (egui::Key::B, Tool::Slab), (egui::Key::S, Tool::Support), (egui::Key::L, Tool::Load), (egui::Key::R, Tool::Hinge), (egui::Key::X, Tool::Delete)];
        let cmd = ctx.input(|i| i.modifiers.command);
        for (k, t) in keys {
            if !cmd && ctx.input(|i| i.key_pressed(k)) {
                self.set_tool(t);
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.opts.from.is_some() {
                self.opts.from = None;
            } else {
                self.set_tool(Tool::Select);
            }
        }
    }

    pub(crate) fn ribbon(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get(ui.ctx());
        // every group stays reachable on a narrow window: the ribbon scrolls sideways
        egui::ScrollArea::horizontal().id_salt("ribbon-scroll").show(ui, |ui| ui.horizontal(|ui| {
            group(ui, &t, "View", |ui| {
                if big(ui, &t, "hand", "Pan", self.tool == Tool::Pan, "Pan (A): drag to move the view") {
                    self.set_tool(Tool::Pan);
                }
                if big(ui, &t, "mouse-pointer-2", "Select", self.tool == Tool::Select, "Select (V)") {
                    self.set_tool(Tool::Select);
                }
                if big(ui, &t, "scan", "Fit", false, "Fit the model (F)") {
                    self.view.fit = true;
                }
                for (v, icon, label) in [("3d", "rotate-3d", "3D"), ("plan", "map", "Plan"), ("elevation", "panel-top", "Elevation")] {
                    if big(ui, &t, icon, label, self.spec.view == v, &format!("{label} view")) {
                        self.spec.view = v.into();
                        self.view.set_preset(v);
                    }
                }
            });
            group(ui, &t, "Draw", |ui| {
                if big(ui, &t, "circle-dot", "Node", self.tool == Tool::Node, "Node (N): click on the work plane or type coordinates") {
                    self.set_tool(Tool::Node);
                }
                if big(ui, &t, "minus", "Beam", self.tool == Tool::Beam, "Beam (G): drag along the work plane") {
                    self.set_tool(Tool::Beam);
                }
                if big(ui, &t, "rectangle-vertical", "Column", self.tool == Tool::Column, "Column (C): click a point, or drag a rectangle for its four corners") {
                    self.set_tool(Tool::Column);
                }
                if big(ui, &t, "move-diagonal", "Brace", self.tool == Tool::Brace, "Brace (K): drag between two nodes") {
                    self.set_tool(Tool::Brace);
                }
                if big(ui, &t, "frame", "Bay", self.tool == Tool::Bay, "Bay (Y): drag a rectangle on a floor for its four beams and slab") {
                    self.set_tool(Tool::Bay);
                }
                if big(ui, &t, "brick-wall", "Wall", self.tool == Tool::Wall, "Shear wall (W): drag its plan line; it runs from the base to the top level") {
                    self.set_tool(Tool::Wall);
                }
                if big(ui, &t, "slash", "Member", self.tool == Tool::Member, "Member (E): drag, or click its start and end") {
                    self.set_tool(Tool::Member);
                }
                if big(ui, &t, "layout-grid", "Slab", self.tool == Tool::Slab, "Slab (B): click inside a bay of beams, or drag a rectangle") {
                    self.set_tool(Tool::Slab);
                }
                if big(ui, &t, "building", "Frame", self.frame_form.is_some(), "A regular building frame: grid, storeys, sections, slabs") {
                    self.open_frame_form();
                }
            });
            group(ui, &t, "Modify", |ui| {
                if big(ui, &t, "copy", "Move / copy", self.edit.open, "Move, copy (arrays), rotate or mirror the selected members") {
                    self.edit.open = !self.edit.open;
                }
                if big(ui, &t, "scissors", "Split", false, "Split the selected member into equal pieces (Edit window)") {
                    self.edit.open = true;
                }
                if big(ui, &t, "combine", "Merge", false, "Merge two selected collinear members") {
                    if self.selected.len() == 2 {
                        let (a, b) = (self.selected[0], self.selected[1]);
                        self.run_command("merge_members", json!({"a": a, "b": b}));
                        self.selected.retain(|id| self.model.member(*id).is_some());
                    } else {
                        self.status = "Merge: select two collinear members that share a node.".into();
                    }
                }
            });
            group(ui, &t, "Properties", |ui| {
                if big(ui, &t, "layers", "Materials", self.settings_open, "Concrete and steel strengths, rebar grades, covers, limits") {
                    self.settings_open = !self.settings_open;
                }
                if big(ui, &t, "square", "Sections", self.bottom == BottomTab::Sections, "The sections and the steel catalogue") {
                    self.bottom = BottomTab::Sections;
                }
                if big(ui, &t, "shapes", "Section FE", self.sec.open, "Finite element properties of any cross-section: J, Cw, shear centre, plastic moduli") {
                    self.sec.open = !self.sec.open;
                }
            });
            group(ui, &t, "Conditions", |ui| {
                if big(ui, &t, "triangle", "Support", self.tool == Tool::Support, "Support (S): click a node") {
                    self.set_tool(Tool::Support);
                }
                if big(ui, &t, "arrow-down", "Load", self.tool == Tool::Load, "Load (L): click a node or a member") {
                    self.set_tool(Tool::Load);
                }
                if big(ui, &t, "circle", "Hinge", self.tool == Tool::Hinge, "Hinge (R): pin a member's ends") {
                    self.set_tool(Tool::Hinge);
                }
                if big(ui, &t, "trash", "Delete", self.tool == Tool::Delete, "Delete (X): click a node or a member") {
                    self.set_tool(Tool::Delete);
                }
                if big(ui, &t, "sheet", "Cases", self.loads_open, "Load cases and combinations") {
                    self.loads_open = !self.loads_open;
                }
            });
            group(ui, &t, "Analyse", |ui| {
                if big(ui, &t, "play", "Analyse", false, "Linear static analysis of every load case") {
                    self.analyze();
                }
                if big(ui, &t, "check", "Design", false, "ACI 318-19 beams and slabs, AISC 360-16 steel") {
                    self.design();
                }
                ui.vertical(|ui| {
                    use pondus_core::slabfe::SlabMethod;
                    let cur = self.model.design.slab_method;
                    let label = |m: SlabMethod| if m == SlabMethod::Plate { "slabs: plate FE" } else { "slabs: coefficients" };
                    let mut pick = cur;
                    egui::ComboBox::from_id_salt("slab-method").selected_text(RichText::new(label(cur)).small()).width(120.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut pick, SlabMethod::Coefficients, "coefficients (ACI 318-63 Method 2 / ACI 6.5)").on_hover_text("Slab load to the beams by the 45° lines; moments from the coefficient tables");
                        ui.selectable_value(&mut pick, SlabMethod::Plate, "plate FE (MITC4, Wood–Armer)").on_hover_text("Slabs meshed as plate elements and analysed with the frame; bars from the element moments and shears");
                    });
                    if pick != cur {
                        self.commit();
                        self.model.design.slab_method = pick;
                        self.status = format!("Slabs are now analysed and designed by {}.", label(pick));
                    }
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.resize, RichText::new("resize").small()).on_hover_text("Deepen failing concrete beams (50 mm steps) and thicken failing slabs");
                        ui.checkbox(&mut self.select, RichText::new("select steel").small()).on_hover_text("Choose the lightest passing H shape for every steel member (or group)");
                    });
                });
            });
            self.slab_group(ui, &t);
            group(ui, &t, "Dynamics & stability", |ui| {
                if big(ui, &t, "columns-3", "Buckling", self.spec.diagram == Diagram::Buckling, "Buckling load factors of the result's ULS combination and their mode shapes") {
                    self.run_buckling();
                }
                if big(ui, &t, "activity", "History", self.th_open, "Time history under a ground motion record (PEER .AT2 or a table)") {
                    self.th_open = !self.th_open;
                }
                if big(ui, &t, "waves", "Spectrum", self.seismic_open, "Spectrum, R, Ie, Cd, mass source: switches the modal and response spectrum analysis on") {
                    self.seismic_open = !self.seismic_open;
                }
                let on = self.model.seismic.enabled;
                if ui.add_enabled_ui(on, |ui| big(ui, &t, "waves", "Modes", self.spec.diagram == Diagram::Mode, "Mode shapes, periods, base shears and drifts")).inner {
                    if self.analysis.is_none() {
                        self.analyze();
                    }
                    self.spec.diagram = Diagram::Mode;
                    self.bottom = BottomTab::Seismic;
                }
            });
            group(ui, &t, "Advanced", |ui| {
                use crate::advanced_ui::AdvTab;
                for (icon, label, tab, tip) in [
                    ("trending-up", "Pushover", AdvTab::Pushover, "Plastic hinges to collapse: a lateral push or a combination scaled until it is a mechanism"),
                    ("truck", "Moving", AdvTab::Moving, "A moving load train along members: the envelope and its design combinations"),
                    ("chart-spline", "Influence", AdvTab::Influence, "Influence lines of member forces and support reactions"),
                    ("layers", "Stages", AdvTab::Stages, "Construction stages: members built and loads applied in order"),
                ] {
                    if big(ui, &t, icon, label, self.adv.open && self.adv.tab == tab, tip) {
                        if self.adv.open && self.adv.tab == tab {
                            self.adv.open = false;
                        } else {
                            self.adv.open = true;
                            self.adv.tab = tab;
                        }
                    }
                }
            });
            group(ui, &t, "Report", |ui| {
                if big(ui, &t, "file-chart-column", "PDF report", false, "The calculation report: basis of design, loads, analysis, design tables and the governing calculations (written next to the model file)") {
                    self.write_report(false);
                }
            });
            group(ui, &t, "Results", |ui| {
                ui.vertical(|ui| {
                    let rows: [&[(Diagram, &str, &str)]; 2] = [
                        &[(Diagram::None, "—", "No diagram"), (Diagram::Deflection, "δ", "Deformed shape"), (Diagram::Axial, "N", "Axial force"), (Diagram::Moment, "My", "Major-axis moment (tension side)"), (Diagram::Shear, "Vz", "Major-axis shear")],
                        &[(Diagram::MomentZ, "Mz", "Minor-axis moment"), (Diagram::ShearY, "Vy", "Minor-axis shear"), (Diagram::Torsion, "T", "Torsion"), (Diagram::Utilisation, "η", "Design ratio: green < 0.7, yellow < 0.9, orange < 1, red over")],
                    ];
                    for row in rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (d, l, tip) in row {
                                if small(ui, &t, l, self.spec.diagram == *d, tip) {
                                    self.spec.diagram = *d;
                                    if *d != Diagram::None && self.analysis.is_none() {
                                        self.analyze();
                                    }
                                }
                            }
                        });
                    }
                });
            });
        }));
    }

    /// The Slab group: the plate results of the slabs (plate method).
    fn slab_group(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let plate = self.model.design.slab_method == pondus_core::slabfe::SlabMethod::Plate;
        group(ui, t, "Slab (plate)", |ui| {
            ui.vertical(|ui| {
                let rows: [&[(Diagram, &str, &str)]; 2] = [
                    &[(Diagram::PlateMx, "mx", "Slab moment mx: bending in x, sagging +"), (Diagram::PlateMy, "my", "Slab moment my: bending in y, sagging +"), (Diagram::PlateMxy, "mxy", "Slab twisting moment mxy")],
                    &[(Diagram::PlateQx, "qx", "Slab transverse shear qx"), (Diagram::PlateQy, "qy", "Slab transverse shear qy")],
                ];
                for (k, row) in rows.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (d, l, tip) in row.iter() {
                            let tip = if plate { tip.to_string() } else { format!("{tip}. Choose 'slabs: plate FE' in Analyse first.") };
                            if ui.add_enabled_ui(plate, |ui| small(ui, t, l, self.spec.diagram == *d, &tip)).inner {
                                self.spec.diagram = *d;
                                if self.analysis.is_none() {
                                    self.analyze();
                                }
                            }
                        }
                        if k == 1 {
                            ui.checkbox(&mut self.spec.values, RichText::new("values").small()).on_hover_text("Write each element's value");
                        }
                    });
                }
            });
        });
    }

    /// The combination or case the diagrams show.
    pub(crate) fn result_picker(&mut self, ui: &mut egui::Ui) {
        let mut names: Vec<String> = self.model.combos.iter().map(|c| c.name.clone()).chain(self.model.cases.iter().map(|c| c.name.clone())).collect();
        if let Some(a) = self.analysis.as_ref().filter(|a| a.seismic.is_some()) {
            names.push("Ex".into());
            names.push("Ey".into());
            names.extend(pondus_core::seismic::combos(&self.model, a).into_iter().map(|c| c.0));
        }
        if let Some(a) = self.analysis.as_ref() {
            if a.moving.is_some() {
                names.push("moving max".into());
                names.push("moving min".into());
                names.extend(pondus_core::advanced::moving_combos(&self.model, a).into_iter().map(|c| c.0));
            }
            if a.staged.is_some() {
                names.push("staged (final)".into());
            }
        }
        let cur = if self.spec.result.is_empty() { "results: first ULS / SLS".to_string() } else { format!("results: {}", self.spec.result) };
        egui::ComboBox::from_id_salt("result").selected_text(cur).width(200.0).show_ui(ui, |ui| {
            if ui.selectable_label(self.spec.result.is_empty(), "first ULS / SLS").clicked() {
                self.spec.result.clear();
            }
            for n in names {
                if ui.selectable_label(self.spec.result == n, &n).clicked() {
                    self.spec.result = n;
                }
            }
        });
    }

    pub(crate) fn open_frame_form(&mut self) {
        if self.frame_form.is_none() {
            self.frame_form = Some(Default::default());
        }
    }

    /// The bar under the ribbon: the armed tool's options and the work plane.
    pub(crate) fn tool_options(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get(ui.ctx());
        let levels: Vec<f64> = {
            let mut z: Vec<f64> = self.model.nodes.iter().map(|n| (n.z * 1000.0).round() / 1000.0).collect();
            z.sort_by(f64::total_cmp);
            z.dedup();
            z
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:?}", self.tool)).font(theme::medium_font(13.0)).color(t.accent_text));
            ui.separator();
            match self.tool {
                Tool::Node => {
                    for (k, axis) in ["x", "y", "z"].iter().enumerate() {
                        ui.label(*axis);
                        ui.add(egui::DragValue::new(&mut self.opts.node[k]).speed(0.1).suffix(" m"));
                    }
                    if ui.button("Add node").clicked() {
                        let p = self.opts.node;
                        self.commit();
                        let id = self.model.node_at(p);
                        self.status = format!("Node {id} at ({:.2}, {:.2}, {:.2})", p[0], p[1], p[2]);
                    }
                }
                Tool::Member => {
                    ui.label("Section");
                    ui.add(egui::TextEdit::singleline(&mut self.opts.section).desired_width(120.0).hint_text("300 x 600"));
                    egui::ComboBox::from_id_salt("member-sec").selected_text("pick").width(60.0).show_ui(ui, |ui| {
                        for s in &self.model.sections {
                            if ui.selectable_label(false, &s.name).clicked() {
                                self.opts.section = s.name.clone();
                            }
                        }
                        ui.separator();
                        for s in pondus_core::sections::catalogue() {
                            if ui.selectable_label(false, &s.name).clicked() {
                                self.opts.section = s.name.clone();
                            }
                        }
                    });
                    ui.label("Material");
                    egui::ComboBox::from_id_salt("member-mat").selected_text(&self.opts.material).show_ui(ui, |ui| {
                        for m in ["auto", "concrete", "steel"] {
                            ui.selectable_value(&mut self.opts.material, m.to_string(), m);
                        }
                    });
                    ui.checkbox(&mut self.opts.chain, "chain");
                    if self.opts.from.is_some() && ui.button("End chain (Esc)").clicked() {
                        self.opts.from = None;
                    }
                }
                Tool::Beam | Tool::Column | Tool::Brace | Tool::Bay => {
                    let names: Vec<String> = self.model.sections.iter().map(|s| s.name.clone()).collect();
                    let tool = self.tool;
                    if matches!(tool, Tool::Beam | Tool::Bay) {
                        ui.label("Beam");
                        section_field(ui, "beam-sec", &mut self.opts.beam_section, &names);
                    }
                    if matches!(tool, Tool::Column) || (tool == Tool::Bay && self.opts.bay_cols) {
                        ui.label("Column");
                        section_field(ui, "col-sec", &mut self.opts.col_section, &names);
                        ui.label("h");
                        ui.add(egui::DragValue::new(&mut self.opts.col_h).speed(0.05).range(0.5..=20.0).suffix(" m"));
                    }
                    if tool == Tool::Column {
                        ui.selectable_value(&mut self.opts.col_up, true, "up from the plane");
                        ui.selectable_value(&mut self.opts.col_up, false, "down from the plane");
                    }
                    if tool == Tool::Brace {
                        ui.label("Brace");
                        section_field(ui, "brace-sec", &mut self.opts.brace_section, &names);
                    }
                    ui.label("Material");
                    egui::ComboBox::from_id_salt("draw-mat").selected_text(&self.opts.material).width(70.0).show_ui(ui, |ui| {
                        for m in ["auto", "concrete", "steel"] {
                            ui.selectable_value(&mut self.opts.material, m.to_string(), m);
                        }
                    });
                    if matches!(tool, Tool::Beam | Tool::Brace) {
                        ui.checkbox(&mut self.opts.chain, "chain clicks");
                    }
                    if tool == Tool::Bay {
                        ui.checkbox(&mut self.opts.bay_slab, "slab");
                        if self.opts.bay_slab {
                            let mut h = self.opts.slab_h * 1000.0;
                            if ui.add(egui::DragValue::new(&mut h).speed(5.0).range(80.0..=500.0).prefix("h ").suffix(" mm")).changed() {
                                self.opts.slab_h = h / 1000.0;
                            }
                        }
                        ui.checkbox(&mut self.opts.bay_cols, "columns below");
                        ui.label(RichText::new("dropped:").small().color(t.text_dim));
                        ui.add(egui::DragValue::new(&mut self.opts.bay_w).speed(0.1).range(0.5..=30.0).suffix(" m"));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut self.opts.bay_d).speed(0.1).range(0.5..=30.0).suffix(" m"));
                    }
                    if tool == Tool::Beam {
                        ui.label(RichText::new("dropped:").small().color(t.text_dim));
                        ui.add(egui::DragValue::new(&mut self.opts.drop_len).speed(0.1).range(0.5..=40.0).suffix(" m"));
                    }
                    work_plane(ui, &mut self.opts, &levels, self.spec.view.as_str());
                }
                Tool::Wall => {
                    ui.label("t");
                    let mut t_mm = self.opts.wall_t * 1000.0;
                    if ui.add(egui::DragValue::new(&mut t_mm).speed(5.0).range(100.0..=1000.0).suffix(" mm")).changed() {
                        self.opts.wall_t = t_mm / 1000.0;
                    }
                    for (label, v, default) in [("base", &mut self.opts.wall_z0, levels.first().copied().unwrap_or(0.0)), ("top", &mut self.opts.wall_z1, levels.last().copied().unwrap_or(3.0))] {
                        let mut on = v.is_some();
                        if ui.checkbox(&mut on, label).changed() {
                            *v = if on { Some(default) } else { None };
                        }
                        if let Some(z) = v.as_mut() {
                            ui.add(egui::DragValue::new(z).speed(0.1).suffix(" m"));
                        } else {
                            ui.label(RichText::new(if label == "base" { "lowest level" } else { "highest level" }).small().color(t.text_dim));
                        }
                    }
                    ui.label(RichText::new("analysed as piers with rigid arms; designed per storey to ACI 318-19 Ch. 11").small().color(t.text_dim));
                }
                Tool::Slab => {
                    ui.label("h");
                    let mut h = self.opts.slab_h * 1000.0;
                    if ui.add(egui::DragValue::new(&mut h).speed(5.0).range(80.0..=500.0).suffix(" mm")).changed() {
                        self.opts.slab_h = h / 1000.0;
                    }
                    ui.label("SDL");
                    ui.add(egui::DragValue::new(&mut self.opts.slab_dead).speed(0.1).suffix(" kPa"));
                    ui.label("Live");
                    ui.add(egui::DragValue::new(&mut self.opts.slab_live).speed(0.1).suffix(" kPa"));
                    let z = self.spec.z.unwrap_or(self.opts.work_z);
                    if ui.button(format!("Fill every bay at z = {z:.2}")).clicked() {
                        self.run_command("add_slabs", json!({"bays": {"z": z, "h": self.opts.slab_h, "dead": self.opts.slab_dead, "live": self.opts.slab_live}}));
                    }
                }
                Tool::Support => {
                    for k in ["fixed", "pinned", "roller", "spring"] {
                        if ui.selectable_label(self.opts.support == k, k).clicked() {
                            self.opts.support = k;
                        }
                    }
                    if self.opts.support == "spring" {
                        for (k, n) in ["kx", "ky", "kz", "krx", "kry", "krz"].iter().enumerate() {
                            ui.label(*n);
                            ui.add(egui::DragValue::new(&mut self.opts.springs[k]).speed(100.0).range(0.0..=1e9));
                        }
                    }
                    let base = levels.first().copied();
                    if let Some(z0) = base {
                        if ui.button(format!("Every node at z = {z0:.2}")).on_hover_text("The bases of all the columns").clicked() {
                            let pts: Vec<[f64; 3]> = self.model.nodes.iter().filter(|n| (n.z - z0).abs() < 1e-3).map(|n| n.p()).collect();
                            self.run_command("add_supports", json!({"points": pts, "type": self.opts.support}));
                        }
                    }
                }
                Tool::Load => {
                    ui.label("Case");
                    let cases = self.model.cases.clone();
                    egui::ComboBox::from_id_salt("tool-case").selected_text(self.model.case(self.opts.load_case).map(|c| c.name.clone()).unwrap_or("?".into())).show_ui(ui, |ui| {
                        for c in &cases {
                            ui.selectable_value(&mut self.opts.load_case, c.id, &c.name);
                        }
                    });
                    for (k, l) in [(LoadType::Nodal, "Nodal"), (LoadType::Line, "Line"), (LoadType::Point, "Point"), (LoadType::Settlement, "Settlement"), (LoadType::Thermal, "Temperature")] {
                        if ui.selectable_label(self.opts.load_type == k, l).clicked() {
                            self.opts.load_type = k;
                        }
                    }
                    ui.separator();
                    match self.opts.load_type {
                        LoadType::Nodal => {
                            for (k, n) in ["Fx", "Fy", "Fz", "Mx", "My", "Mz"].iter().enumerate() {
                                ui.label(*n);
                                ui.add(egui::DragValue::new(&mut self.opts.f[k]).speed(0.5));
                            }
                            ui.label(RichText::new("kN, kN·m, global").small().color(t.text_dim));
                        }
                        LoadType::Line => {
                            ui.label("w");
                            ui.add(egui::DragValue::new(&mut self.opts.w).speed(0.5).suffix(" kN/m"));
                            for (k, l) in [(2usize, "down (−z)"), (0, "+x"), (1, "+y")] {
                                ui.selectable_value(&mut self.opts.w_dir, k, l);
                            }
                            let z = self.spec.z;
                            if let Some(z) = z.filter(|_| self.spec.view == "plan") {
                                if ui.button(format!("All beams at z = {z:.2}")).clicked() {
                                    let w = self.load_vector(self.opts.w);
                                    self.run_command("add_loads", json!({"case": self.opts.load_case.to_string(), "loads": [{"type": "line", "z": z, "w": w}]}));
                                }
                            }
                        }
                        LoadType::Settlement => {
                            for (k, n) in ["dx", "dy", "dz"].iter().enumerate() {
                                ui.label(*n);
                                let mut mm = self.opts.settle[k] * 1000.0;
                                if ui.add(egui::DragValue::new(&mut mm).speed(1.0).suffix(" mm")).changed() {
                                    self.opts.settle[k] = mm / 1000.0;
                                }
                            }
                            ui.label(RichText::new("click a supported node").small().color(t.text_dim));
                        }
                        LoadType::Thermal => {
                            ui.label("ΔT");
                            ui.add(egui::DragValue::new(&mut self.opts.dt).speed(1.0).suffix(" °C"));
                            ui.label(RichText::new("uniform; click a member").small().color(t.text_dim));
                        }
                        LoadType::Point => {
                            ui.label("P");
                            ui.add(egui::DragValue::new(&mut self.opts.p).speed(0.5).suffix(" kN"));
                            for (k, l) in [(2usize, "down (−z)"), (0, "+x"), (1, "+y")] {
                                ui.selectable_value(&mut self.opts.w_dir, k, l);
                            }
                            ui.label("at");
                            ui.add(egui::DragValue::new(&mut self.opts.p_at).speed(0.01).range(0.0..=1.0).suffix(" L"));
                        }
                    }
                }
                Tool::Hinge => {
                    for k in ["start", "end", "both"] {
                        if ui.selectable_label(self.opts.hinge_end == k, k).clicked() {
                            self.opts.hinge_end = k;
                        }
                    }
                    ui.label(RichText::new("a click toggles").small().color(t.text_dim));
                }
                _ => {
                    ui.add(egui::Label::new(RichText::new(self.tool_hint()).color(t.text_dim)).truncate());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.result_picker(ui);
                ui.separator();
                ui.add(egui::DragValue::new(&mut self.opts.grid).speed(0.05).range(0.0..=10.0).suffix(" m"));
                ui.label("grid");
                if self.spec.view != "plan" {
                    egui::ComboBox::from_id_salt("work-z").selected_text(format!("z = {:.2}", self.opts.work_z)).width(90.0).show_ui(ui, |ui| {
                        for z in &levels {
                            ui.selectable_value(&mut self.opts.work_z, *z, format!("z = {z:.2}"));
                        }
                    });
                    ui.add(egui::DragValue::new(&mut self.opts.work_z).speed(0.1).suffix(" m"));
                    ui.label("work plane");
                }
            });
        });
    }

    fn load_vector(&self, v: f64) -> [f64; 3] {
        let mut w = [0.0; 3];
        w[self.opts.w_dir] = if self.opts.w_dir == 2 { -v } else { v };
        w
    }

    /// What a click in the view does with the armed tool. Returns true when the click was used.
    pub(crate) fn tool_click(&mut self, p: &Picked) -> bool {
        if !p.clicked {
            return false;
        }
        match self.tool {
            Tool::Node => {
                if let Some(pt) = p.plane {
                    self.commit();
                    let id = self.model.node_at(pt);
                    self.status = format!("Node {id} at ({:.2}, {:.2}, {:.2})", pt[0], pt[1], pt[2]);
                }
                true
            }
            Tool::Member => {
                let Some(pt) = p.plane else { return true };
                match self.opts.from {
                    None => self.opts.from = Some(pt),
                    Some(a) if dist(a, pt) > 1e-3 => {
                        let mut m = json!({"from": a, "to": pt, "section": self.opts.section});
                        if self.opts.material != "auto" {
                            m["material"] = json!(self.opts.material);
                        }
                        self.run_command("add_members", json!({"members": [m]}));
                        self.opts.from = if self.opts.chain { Some(pt) } else { None };
                    }
                    _ => {}
                }
                true
            }
            Tool::Beam | Tool::Brace => {
                let Some(pt) = p.plane else { return true };
                match self.opts.from {
                    None => self.opts.from = Some(pt),
                    Some(a) if dist(a, pt) > 1e-3 => {
                        self.add_member_of(self.tool, a, pt);
                        self.opts.from = if self.opts.chain { Some(pt) } else { None };
                    }
                    _ => {}
                }
                true
            }
            Tool::Column => {
                if let Some(pt) = p.plane {
                    self.add_columns(&[pt]);
                }
                true
            }
            Tool::Wall => {
                let Some(pt) = p.plane else { return true };
                match self.opts.from {
                    None => self.opts.from = Some(pt),
                    Some(a) if dist([a[0], a[1], 0.0], [pt[0], pt[1], 0.0]) > 0.1 => {
                        self.opts.from = None;
                        self.add_wall(a, pt);
                    }
                    _ => {}
                }
                true
            }
            Tool::Bay => {
                let Some(pt) = p.plane else { return true };
                match self.opts.from {
                    None => {
                        self.opts.from = Some(pt);
                        self.status = "Bay: now the opposite corner (or drag a rectangle).".into();
                    }
                    Some(a) => {
                        self.opts.from = None;
                        self.make_bay(a, pt);
                    }
                }
                true
            }
            Tool::Slab => {
                let Some(pt) = p.plane else { return true };
                let z = pt[2];
                let bay = pondus_core::commands::bays(&self.model, z).into_iter().find(|(x0, y0, x1, y1)| pt[0] > *x0 - 1e-6 && pt[0] < *x1 + 1e-6 && pt[1] > *y0 - 1e-6 && pt[1] < *y1 + 1e-6);
                match bay {
                    Some((x0, y0, x1, y1)) => {
                        if self.model.slabs.iter().any(|s| (s.z - z).abs() < 0.05 && (s.x0 - x0).abs() < 1e-3 && (s.y0 - y0).abs() < 1e-3 && (s.x1 - x1).abs() < 1e-3 && (s.y1 - y1).abs() < 1e-3) {
                            self.status = "That bay already has a slab.".into();
                        } else {
                            self.run_command("add_slabs", json!({"panels": [{"x0": x0, "y0": y0, "x1": x1, "y1": y1, "z": z, "h": self.opts.slab_h, "dead": self.opts.slab_dead, "live": self.opts.slab_live}]}));
                        }
                    }
                    None => self.status = format!("No bay closed by beams at ({:.2}, {:.2}) on z = {z:.2}: choose the floor in the plan view's level list.", pt[0], pt[1]),
                }
                true
            }
            Tool::Support => {
                if let Some(n) = p.node {
                    self.run_command("add_supports", json!({"nodes": [n], "type": self.opts.support, "springs": self.opts.springs}));
                }
                true
            }
            Tool::Load => {
                let case = self.opts.load_case.to_string();
                match (self.opts.load_type, p.node, p.member) {
                    (LoadType::Nodal, Some(n), _) => self.run_command("add_loads", json!({"case": case, "loads": [{"type": "nodal", "node": n, "f": self.opts.f}]})),
                    (LoadType::Line, _, Some(m)) => {
                        let mut ids = vec![m];
                        if self.selected.contains(&m) {
                            ids = self.selected.clone();
                        }
                        let w = self.load_vector(self.opts.w);
                        self.run_command("add_loads", json!({"case": case, "loads": [{"type": "line", "members": ids, "w": w}]}));
                    }
                    (LoadType::Point, _, Some(m)) => {
                        let len = self.model.member(m).map(|x| self.model.length(x)).unwrap_or(0.0);
                        let pv = self.load_vector(self.opts.p);
                        self.run_command("add_loads", json!({"case": case, "loads": [{"type": "point", "member": m, "a": self.opts.p_at * len, "p": pv}]}));
                    }
                    (LoadType::Settlement, Some(n), _) => self.run_command("add_loads", json!({"case": case, "loads": [{"type": "settlement", "node": n, "d": self.opts.settle}]})),
                    (LoadType::Thermal, _, Some(m)) => self.run_command("add_loads", json!({"case": case, "loads": [{"type": "thermal", "members": [m], "dt": self.opts.dt}]})),
                    (LoadType::Settlement, None, _) => self.status = "A settlement goes on a supported node: click one.".into(),
                    (LoadType::Nodal, None, _) => self.status = "A nodal load goes on a node: click one.".into(),
                    _ => self.status = "A line or point load goes on a member: click one.".into(),
                }
                true
            }
            Tool::Hinge => {
                if let Some(m) = p.member {
                    self.commit();
                    let end = self.opts.hinge_end;
                    if let Some(x) = self.model.members.iter_mut().find(|x| x.id == m) {
                        match end {
                            "start" => x.hinge_i = !x.hinge_i,
                            "end" => x.hinge_j = !x.hinge_j,
                            _ => {
                                let v = !(x.hinge_i && x.hinge_j);
                                x.hinge_i = v;
                                x.hinge_j = v;
                            }
                        }
                        self.status = format!("Member {m}: hinge at start {}, at end {}", x.hinge_i, x.hinge_j);
                    }
                }
                true
            }
            Tool::Delete => {
                if let Some(n) = p.node {
                    self.run_command("delete", json!({"nodes": [n]}));
                } else if let Some(m) = p.member {
                    self.run_command("delete", json!({"members": [m]}));
                    self.selected.retain(|x| *x != m);
                }
                true
            }
            Tool::Pan | Tool::Select => false,
        }
    }

    /// A member of the beam, column or brace tool (its section and kind), or of the member tool.
    pub(crate) fn add_member_of(&mut self, tool: Tool, a: [f64; 3], b: [f64; 3]) {
        let (section, kind) = match tool {
            Tool::Beam => (self.opts.beam_section.clone(), Some("beam")),
            Tool::Column => (self.opts.col_section.clone(), Some("column")),
            Tool::Brace => (self.opts.brace_section.clone(), Some("brace")),
            _ => (self.opts.section.clone(), None),
        };
        let mut m = json!({"from": a, "to": b, "section": section});
        if let Some(k) = kind {
            m["kind"] = json!(k);
        }
        if self.opts.material != "auto" {
            m["material"] = json!(self.opts.material);
        }
        self.run_command("add_members", json!({"members": [m]}));
    }

    /// Columns standing on (or hanging from) the given points of the work plane.
    pub(crate) fn add_columns(&mut self, pts: &[[f64; 3]]) {
        let h = if self.opts.col_up { self.opts.col_h } else { -self.opts.col_h };
        let members: Vec<serde_json::Value> = pts
            .iter()
            .map(|p| {
                let (lo, hi) = if h > 0.0 { (*p, [p[0], p[1], p[2] + h]) } else { ([p[0], p[1], p[2] + h], *p) };
                let mut m = json!({"from": lo, "to": hi, "section": self.opts.col_section, "kind": "column"});
                if self.opts.material != "auto" {
                    m["material"] = json!(self.opts.material);
                }
                m
            })
            .collect();
        if !members.is_empty() {
            self.run_command("add_members", json!({"members": members}));
        }
    }

    /// A wall in plan from `a` to `b`, from the base to the top (the tool's levels or the model's).
    pub(crate) fn add_wall(&mut self, a: [f64; 3], b: [f64; 3]) {
        let mut w = json!({"from": [a[0], a[1]], "to": [b[0], b[1]], "t": self.opts.wall_t});
        if let Some(z) = self.opts.wall_z0 {
            w["z0"] = json!(z);
        }
        if let Some(z) = self.opts.wall_z1 {
            w["z1"] = json!(z);
        }
        self.run_command("add_walls", json!({"walls": [w]}));
    }

    /// A bay on a floor from two opposite corners: four edge beams (where none is yet), a slab
    /// in it and, when chosen, columns under its corners down to the level below.
    pub(crate) fn make_bay(&mut self, a: [f64; 3], b: [f64; 3]) {
        let z = a[2];
        let (x0, x1, y0, y1) = (a[0].min(b[0]), a[0].max(b[0]), a[1].min(b[1]), a[1].max(b[1]));
        if x1 - x0 < 0.2 || y1 - y0 < 0.2 {
            self.status = "Bay: drag a rectangle on a floor (plan view or the 3D work plane).".into();
            return;
        }
        let corners = [[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]];
        let edge_exists = |m: &Model, p: [f64; 3], q: [f64; 3]| m.members.iter().any(|x| {
            let (Some(i), Some(j)) = (m.node(x.i), m.node(x.j)) else { return false };
            (dist(i.p(), p) < 1e-3 && dist(j.p(), q) < 1e-3) || (dist(i.p(), q) < 1e-3 && dist(j.p(), p) < 1e-3)
        });
        self.commit();
        let before = self.model.clone();
        let mut beams = vec![];
        for k in 0..4 {
            let (p, q) = (corners[k], corners[(k + 1) % 4]);
            if !edge_exists(&self.model, p, q) {
                let mut m = json!({"from": p, "to": q, "section": self.opts.beam_section, "kind": "beam"});
                if self.opts.material != "auto" {
                    m["material"] = json!(self.opts.material);
                }
                beams.push(m);
            }
        }
        let mut done = vec![];
        let mut err = None;
        let mut run = |m: &mut Model, name: &str, args: serde_json::Value| match pondus_core::commands::run(m, name, &args) {
            Ok(_) => done.push(name.to_string()),
            Err(e) => err = Some(format!("{name}: {e}")),
        };
        if !beams.is_empty() {
            run(&mut self.model, "add_members", json!({"members": beams}));
        }
        if self.opts.bay_cols && z - self.opts.col_h > -1e-6 {
            let cols: Vec<serde_json::Value> = corners.iter().map(|c| json!({"from": [c[0], c[1], z - self.opts.col_h], "to": c, "section": self.opts.col_section, "kind": "column"})).collect();
            run(&mut self.model, "add_members", json!({"members": cols}));
        }
        if self.opts.bay_slab {
            run(&mut self.model, "add_slabs", json!({"panels": [{"x0": x0, "y0": y0, "x1": x1, "y1": y1, "z": z, "h": self.opts.slab_h, "dead": self.opts.slab_dead, "live": self.opts.slab_live}]}));
        }
        match err {
            Some(e) => {
                self.model = before;
                self.status = format!("Bay: {e}");
            }
            None => {
                self.analysis = None;
                self.report = None;
                self.status = format!("Bay {:.2} × {:.2} m at z = {z:.2}: {} beams{}{}", x1 - x0, y1 - y0, beams.len(), if self.opts.bay_slab { ", a slab" } else { "" }, if self.opts.bay_cols { ", columns under the corners" } else { "" });
            }
        }
    }

    /// A tool's drag in the view: a member along it, a rectangle (bay, slab, corner columns), or
    /// moving the node or the member it started on (Ctrl/Cmd: copy the member).
    pub(crate) fn tool_drag(&mut self, d: &Drag, copy: bool) {
        let moved = dist(d.from, d.to) > 1e-3;
        match self.tool {
            Tool::Member | Tool::Beam | Tool::Brace if moved => {
                self.opts.from = None;
                self.add_member_of(self.tool, d.from, d.to);
            }
            Tool::Column if moved => {
                let (a, b) = (d.from, d.to);
                let mut pts = vec![a, [b[0], a[1], a[2]], [b[0], b[1], a[2]], [a[0], b[1], a[2]]];
                pts.dedup_by(|p, q| dist(*p, *q) < 1e-3);
                self.add_columns(&pts);
            }
            Tool::Wall if moved => {
                self.opts.from = None;
                self.add_wall(d.from, d.to);
            }
            Tool::Bay if moved => {
                self.opts.from = None;
                self.make_bay(d.from, d.to);
            }
            Tool::Slab if moved => {
                let (x0, x1, y0, y1) = (d.from[0].min(d.to[0]), d.from[0].max(d.to[0]), d.from[1].min(d.to[1]), d.from[1].max(d.to[1]));
                self.run_command("add_slabs", json!({"panels": [{"x0": x0, "y0": y0, "x1": x1, "y1": y1, "z": d.from[2], "h": self.opts.slab_h, "dead": self.opts.slab_dead, "live": self.opts.slab_live}]}));
            }
            Tool::Select if moved => match (d.node, d.member) {
                (Some(n), _) => {
                    self.run_command("move_node", json!({"node": n, "to": d.to}));
                }
                (None, Some(m)) => {
                    let dv = [d.to[0] - d.from[0], d.to[1] - d.from[1], d.to[2] - d.from[2]];
                    let ids = if self.selected.contains(&m) { self.selected.clone() } else { vec![m] };
                    self.run_command("transform", json!({"members": ids, "action": "move", "d": dv, "copy": copy}));
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Something dropped from the palette at the pointer: what the tool would do with a click
    /// there, and for a beam or a bay, one of the default size.
    pub(crate) fn tool_drop(&mut self, tool: Tool, support: Option<&'static str>, p: &Picked) {
        let prev = (self.tool, self.opts.support);
        self.tool = tool;
        if let Some(k) = support {
            self.opts.support = k;
        }
        match tool {
            Tool::Beam => {
                if let Some(a) = p.plane {
                    let b = [a[0] + self.opts.drop_len, a[1], a[2]];
                    self.add_member_of(Tool::Beam, a, b);
                }
            }
            Tool::Bay => {
                if let Some(a) = p.plane {
                    self.make_bay(a, [a[0] + self.opts.bay_w, a[1] + self.opts.bay_d, a[2]]);
                }
            }
            Tool::Column | Tool::Node | Tool::Slab | Tool::Support | Tool::Load | Tool::Hinge | Tool::Delete => {
                let click = Picked { clicked: true, ..p.clone() };
                self.tool_click(&click);
            }
            _ => {}
        }
        // a drop arms the tool, so more of the same follow by clicking (the drop's own message
        // stays in the status line)
        let said = std::mem::take(&mut self.status);
        self.set_tool(tool);
        self.status = said;
        if support.is_none() {
            self.opts.support = prev.1;
        }
    }

    /// The model's data as tables (Stabileo's Model data panel).
    pub(crate) fn data_tables(&mut self, ui: &mut egui::Ui, tab: BottomTab) {
        let t = Tokens::get(ui.ctx());
        let before = self.model.clone();
        let mut delete_nodes = vec![];
        let mut delete_members = vec![];
        let mut delete_supports = vec![];
        let mut delete_loads = vec![];
        let mut delete_slabs = vec![];
        let head = |ui: &mut egui::Ui, cols: &[&str]| {
            for c in cols {
                ui.label(RichText::new(*c).small().color(t.text_dim));
            }
            ui.end_row();
        };
        match tab {
            BottomTab::Nodes => {
                egui::Grid::new("nodes-table").striped(true).num_columns(6).spacing([12.0, 3.0]).show(ui, |ui| {
                    head(ui, &["Node", "x (m)", "y (m)", "z (m)", "Support", ""]);
                    for n in self.model.nodes.iter_mut() {
                        ui.label(n.id.to_string());
                        ui.add(egui::DragValue::new(&mut n.x).speed(0.05));
                        ui.add(egui::DragValue::new(&mut n.y).speed(0.05));
                        ui.add(egui::DragValue::new(&mut n.z).speed(0.05));
                        ui.label(self.model.supports.iter().find(|s| s.node == n.id).map(|s| s.kind()).unwrap_or(""));
                        if xbtn(ui, &t).on_hover_text("Delete the node and its members").clicked() {
                            delete_nodes.push(n.id);
                        }
                        ui.end_row();
                    }
                });
            }
            BottomTab::Members => {
                let sections: Vec<(Id, String)> = self.model.sections.iter().map(|s| (s.id, s.name.clone())).collect();
                let mats: Vec<(Id, String)> = self.model.materials.iter().map(|s| (s.id, s.name.clone())).collect();
                let lens: std::collections::HashMap<Id, f64> = self.model.members.iter().map(|m| (m.id, self.model.length(m))).collect();
                let kinds: std::collections::HashMap<Id, MemberKind> = self.model.members.iter().map(|m| (m.id, self.model.kind(m))).collect();
                egui::Grid::new("members-table").striped(true).num_columns(14).spacing([10.0, 3.0]).show(ui, |ui| {
                    head(ui, &["Member", "i", "j", "L (m)", "Kind", "Section", "Material", "Hinges i / j", "Truss", "Behaviour", "Roll °", "Lb (m)", "BIM id", ""]);
                    for m in self.model.members.iter_mut() {
                        if ui.selectable_label(self.selected.contains(&m.id), m.id.to_string()).clicked() {
                            self.selected = vec![m.id];
                            self.right = RightTab::Properties;
                        }
                        ui.add(egui::DragValue::new(&mut m.i));
                        ui.add(egui::DragValue::new(&mut m.j));
                        ui.label(format!("{:.3}", lens[&m.id]));
                        egui::ComboBox::from_id_salt(("kind", m.id)).selected_text(format!("{:?}", kinds[&m.id])).width(70.0).show_ui(ui, |ui| {
                            for k in [MemberKind::Auto, MemberKind::Beam, MemberKind::Column, MemberKind::Brace] {
                                ui.selectable_value(&mut m.kind, k, format!("{k:?}"));
                            }
                        });
                        egui::ComboBox::from_id_salt(("sec", m.id)).selected_text(sections.iter().find(|s| s.0 == m.section).map(|s| s.1.clone()).unwrap_or_default()).width(130.0).show_ui(ui, |ui| {
                            for (id, n) in &sections {
                                ui.selectable_value(&mut m.section, *id, n);
                            }
                        });
                        egui::ComboBox::from_id_salt(("mat", m.id)).selected_text(mats.iter().find(|s| s.0 == m.material).map(|s| s.1.clone()).unwrap_or_default()).width(130.0).show_ui(ui, |ui| {
                            for (id, n) in &mats {
                                ui.selectable_value(&mut m.material, *id, n);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut m.hinge_i, "");
                            ui.checkbox(&mut m.hinge_j, "");
                        });
                        ui.checkbox(&mut m.truss, "");
                        egui::ComboBox::from_id_salt(("beh", m.id)).selected_text(behaviour_name(m.behaviour)).width(92.0).show_ui(ui, |ui| {
                            for b in [Behaviour::Normal, Behaviour::TensionOnly, Behaviour::CompressionOnly, Behaviour::Cable] {
                                ui.selectable_value(&mut m.behaviour, b, behaviour_name(b));
                            }
                        });
                        ui.add(egui::DragValue::new(&mut m.roll).speed(1.0).range(-180.0..=180.0));
                        let mut lb = m.lb.unwrap_or(lens[&m.id]);
                        if ui.add(egui::DragValue::new(&mut lb).speed(0.05).range(0.0..=200.0)).changed() {
                            m.lb = Some(lb);
                        }
                        let mut bim = m.bim_id.clone().unwrap_or_default();
                        if ui.add(egui::TextEdit::singleline(&mut bim).desired_width(80.0)).changed() {
                            m.bim_id = (!bim.is_empty()).then_some(bim);
                        }
                        if xbtn(ui, &t).clicked() {
                            delete_members.push(m.id);
                        }
                        ui.end_row();
                    }
                });
            }
            BottomTab::Supports => {
                egui::Grid::new("supports-table").striped(true).num_columns(10).spacing([12.0, 3.0]).show(ui, |ui| {
                    head(ui, &["Node", "ux / kx", "uy / ky", "uz / kz", "rx / krx", "ry / kry", "rz / krz", "Type", "Lift-off", ""]);
                    for s in self.model.supports.iter_mut() {
                        ui.label(s.node.to_string());
                        for k in 0..6 {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut s.fix[k], "");
                                if !s.fix[k] {
                                    ui.add(egui::DragValue::new(&mut s.springs[k]).speed(100.0).range(0.0..=1e9)).on_hover_text("spring stiffness, kN/m or kN·m/rad (0 = free)");
                                }
                            });
                        }
                        ui.label(s.kind());
                        ui.checkbox(&mut s.uplift, "").on_hover_text("compression only: the support lifts off under tension (solved nonlinearly)");
                        if xbtn(ui, &t).clicked() {
                            delete_supports.push(s.node);
                        }
                        ui.end_row();
                    }
                });
            }
            BottomTab::Loads => {
                let cases: Vec<(Id, String)> = self.model.cases.iter().map(|c| (c.id, c.name.clone())).collect();
                egui::Grid::new("loads-table").striped(true).num_columns(5).spacing([12.0, 3.0]).show(ui, |ui| {
                    head(ui, &["#", "Case", "On", "Values", ""]);
                    for (k, l) in self.model.loads.iter_mut().enumerate() {
                        ui.label((k + 1).to_string());
                        egui::ComboBox::from_id_salt(("lcase", k)).selected_text(cases.iter().find(|c| c.0 == l.case).map(|c| c.1.clone()).unwrap_or_default()).width(80.0).show_ui(ui, |ui| {
                            for (id, n) in &cases {
                                ui.selectable_value(&mut l.case, *id, n);
                            }
                        });
                        match &mut l.kind {
                            LoadKind::Nodal { node, f } => {
                                ui.label(format!("node {node}"));
                                ui.horizontal(|ui| {
                                    for v in f.iter_mut() {
                                        ui.add(egui::DragValue::new(v).speed(0.5));
                                    }
                                    ui.label(RichText::new("F, M kN, kN·m").small().color(t.text_dim));
                                });
                            }
                            LoadKind::Line { member, wi, wj, a, b } => {
                                ui.label(format!("member {member} (line)"));
                                ui.horizontal(|ui| {
                                    for v in wi.iter_mut() {
                                        ui.add(egui::DragValue::new(v).speed(0.5));
                                    }
                                    ui.label("→");
                                    for v in wj.iter_mut() {
                                        ui.add(egui::DragValue::new(v).speed(0.5));
                                    }
                                    ui.label(RichText::new(format!("kN/m{}", match (a, b) { (Some(a), Some(b)) => format!(", {a:.2}–{b:.2} m"), _ => String::new() })).small().color(t.text_dim));
                                });
                            }
                            LoadKind::Settlement { node, d } => {
                                ui.label(format!("node {node} (settlement)"));
                                ui.horizontal(|ui| {
                                    for v in d.iter_mut().take(3) {
                                        ui.add(egui::DragValue::new(v).speed(0.001));
                                    }
                                    ui.label(RichText::new("m").small().color(t.text_dim));
                                });
                            }
                            LoadKind::Thermal { member, dt, dt_y, dt_z } => {
                                ui.label(format!("member {member} (temperature)"));
                                ui.horizontal(|ui| {
                                    ui.add(egui::DragValue::new(dt).speed(1.0).prefix("ΔT ").suffix(" °C"));
                                    ui.add(egui::DragValue::new(dt_y).speed(1.0).prefix("∇y "));
                                    ui.add(egui::DragValue::new(dt_z).speed(1.0).prefix("∇z "));
                                });
                            }
                            LoadKind::Point { member, a, p } => {
                                ui.label(format!("member {member} (point)"));
                                ui.horizontal(|ui| {
                                    for v in p.iter_mut() {
                                        ui.add(egui::DragValue::new(v).speed(0.5));
                                    }
                                    ui.label("kN at");
                                    ui.add(egui::DragValue::new(a).speed(0.05).suffix(" m"));
                                });
                            }
                        }
                        if xbtn(ui, &t).clicked() {
                            delete_loads.push(k);
                        }
                        ui.end_row();
                    }
                });
                ui.label(RichText::new("Slab loads come from the slabs and self-weight from the cases marked so; they are not listed here.").small().color(t.text_dim));
            }
            BottomTab::Sections => {
                let used: std::collections::HashMap<Id, usize> = self.model.sections.iter().map(|s| (s.id, self.model.members.iter().filter(|m| m.section == s.id).count())).collect();
                egui::Grid::new("sections-table").striped(true).num_columns(7).spacing([12.0, 3.0]).show(ui, |ui| {
                    head(ui, &["Section", "Shape", "A (cm²)", "Iy (cm⁴)", "Iz (cm⁴)", "Members", ""]);
                    for s in &self.model.sections {
                        let p = s.props();
                        ui.label(&s.name);
                        ui.label(match s.shape {
                            Shape::Rect { b, h } => format!("rectangle {:.0} x {:.0} mm", b * 1e3, h * 1e3),
                            Shape::H { .. } => "rolled H".into(),
                            Shape::LippedC { .. } => "cold-formed lipped C".into(),
                            Shape::General { .. } => "properties only".into(),
                        });
                        ui.label(format!("{:.1}", p.a * 1e4));
                        ui.label(format!("{:.0}", p.iy * 1e8));
                        ui.label(format!("{:.0}", p.iz * 1e8));
                        ui.label(used[&s.id].to_string());
                        if used[&s.id] > 0 && !self.selected.is_empty() && ui.small_button("Assign to selection").clicked() {
                            let name = s.name.clone();
                            let ids = self.selected.clone();
                            self.pending_command = Some(("set_members".into(), json!({"members": ids, "section": name})));
                        } else if used[&s.id] == 0 {
                            ui.label(RichText::new("unused").small().color(t.text_faint));
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Add");
                    ui.add(egui::TextEdit::singleline(&mut self.new_section).hint_text("300 x 600 or H 300x150x6.5x9").desired_width(180.0));
                    egui::ComboBox::from_id_salt("catalogue").selected_text("steel catalogue").show_ui(ui, |ui| {
                        for s in pondus_core::sections::catalogue() {
                            if ui.selectable_label(false, format!("{} · {:.1} kg/m", s.name, s.mass())).clicked() {
                                self.new_section = s.name.clone();
                            }
                        }
                    });
                    if ui.button("Add section").clicked() {
                        let n = self.new_section.clone();
                        let mut m = self.model.clone();
                        match m.section_named(&n) {
                            Ok(_) => {
                                self.commit();
                                self.model = m;
                                self.opts.section = n;
                            }
                            Err(e) => self.status = e,
                        }
                    }
                    if !self.selected.is_empty() && ui.button(format!("Assign to {} selected", self.selected.len())).clicked() {
                        let ids = self.selected.clone();
                        self.pending_command = Some(("set_members".into(), json!({"members": ids, "section": self.new_section.clone()})));
                    }
                });
            }
            BottomTab::Slabs => {
                egui::Grid::new("slabs-table").striped(true).num_columns(10).spacing([10.0, 3.0]).show(ui, |ui| {
                    head(ui, &["Slab", "x0", "y0", "x1", "y1", "z (top)", "h (mm)", "SDL (kPa)", "Live (kPa)", ""]);
                    for s in self.model.slabs.iter_mut() {
                        if ui.selectable_label(self.selected_slab == Some(s.id), &s.name).clicked() {
                            self.selected_slab = Some(s.id);
                            self.right = RightTab::Properties;
                        }
                        for v in [&mut s.x0, &mut s.y0, &mut s.x1, &mut s.y1, &mut s.z] {
                            ui.add(egui::DragValue::new(v).speed(0.05));
                        }
                        let mut h = s.h * 1e3;
                        if ui.add(egui::DragValue::new(&mut h).speed(5.0).range(60.0..=600.0)).changed() {
                            s.h = h / 1e3;
                        }
                        ui.add(egui::DragValue::new(&mut s.dead).speed(0.1).range(0.0..=100.0));
                        ui.add(egui::DragValue::new(&mut s.live).speed(0.1).range(0.0..=100.0));
                        if xbtn(ui, &t).clicked() {
                            delete_slabs.push(s.id);
                        }
                        ui.end_row();
                    }
                });
            }
            _ => {}
        }
        // one undo step per edit gesture: the state before the first change of the gesture
        let edited = self.model != before;
        if edited {
            if !self.editing {
                self.undo.push(before.clone());
                self.redo.clear();
                self.editing = true;
            }
            self.dirty = true;
            self.analysis = None;
            self.report = None;
        }
        if !ui.input(|i| i.pointer.any_down()) && !ui.ctx().egui_wants_keyboard_input() {
            self.editing = false;
        }
        if !delete_nodes.is_empty() {
            self.run_command("delete", json!({"nodes": delete_nodes}));
        }
        if !delete_members.is_empty() {
            self.run_command("delete", json!({"members": delete_members}));
        }
        if !delete_supports.is_empty() {
            self.run_command("delete", json!({"supports": delete_supports}));
        }
        if !delete_slabs.is_empty() {
            self.run_command("delete", json!({"slabs": delete_slabs}));
        }
        if !delete_loads.is_empty() {
            self.commit();
            for k in delete_loads.into_iter().rev() {
                self.model.loads.remove(k);
            }
        }
    }
}

fn behaviour_name(b: Behaviour) -> &'static str {
    match b {
        Behaviour::Normal => "normal",
        Behaviour::TensionOnly => "tension only",
        Behaviour::CompressionOnly => "compression only",
        Behaviour::Cable => "cable",
    }
}

/// A section name field with a list of the model's sections and the steel catalogue.
fn section_field(ui: &mut egui::Ui, id: &str, value: &mut String, names: &[String]) {
    ui.add(egui::TextEdit::singleline(value).desired_width(110.0).hint_text("300 x 600"));
    egui::ComboBox::from_id_salt(id).selected_text("▾").width(28.0).show_ui(ui, |ui| {
        for n in names {
            if ui.selectable_label(false, n).clicked() {
                *value = n.clone();
            }
        }
        ui.separator();
        for s in pondus_core::sections::catalogue() {
            if ui.selectable_label(false, &s.name).clicked() {
                *value = s.name.clone();
            }
        }
    });
}

/// The work plane level for drawing in 3D (plan and elevation views take their own level).
fn work_plane(ui: &mut egui::Ui, opts: &mut ToolOpts, levels: &[f64], view: &str) {
    if view != "3d" {
        return;
    }
    ui.separator();
    ui.label("plane z");
    ui.add(egui::DragValue::new(&mut opts.work_z).speed(0.1).suffix(" m"));
    egui::ComboBox::from_id_salt("plane-levels").selected_text("▾").width(28.0).show_ui(ui, |ui| {
        for z in levels {
            if ui.selectable_label((opts.work_z - z).abs() < 1e-6, format!("{z:.2}")).clicked() {
                opts.work_z = *z;
            }
        }
    });
}

/// The creation palette down the left of the view: (icon, name, tool, support kind).
pub(crate) const PALETTE: &[(&str, &str, Tool, Option<&'static str>)] = &[
    ("mouse-pointer-2", "Select and move (V): drag a node or member to move it", Tool::Select, None),
    ("hand", "Pan (A)", Tool::Pan, None),
    ("circle-dot", "Node (N)", Tool::Node, None),
    ("minus", "Beam (G): drag on the plane, or drop for a beam of the set length", Tool::Beam, None),
    ("rectangle-vertical", "Column (C): click or drop on a point", Tool::Column, None),
    ("move-diagonal", "Brace (K): drag between nodes", Tool::Brace, None),
    ("slash", "Member (E): any direction", Tool::Member, None),
    ("frame", "Bay (Y): drag a rectangle, or drop for one of the set size", Tool::Bay, None),
    ("brick-wall", "Shear wall (W): drag its plan line", Tool::Wall, None),
    ("layout-grid", "Slab (B): click or drop in a bay of beams", Tool::Slab, None),
    ("square-stop", "Fixed support: drop on a node", Tool::Support, Some("fixed")),
    ("triangle", "Pinned support: drop on a node", Tool::Support, Some("pinned")),
    ("disc", "Roller support: drop on a node", Tool::Support, Some("roller")),
    ("arrow-down", "Load (L): drop on a member or node (the Load options)", Tool::Load, None),
    ("circle", "Hinge (R): drop on a member", Tool::Hinge, None),
    ("trash", "Delete (X): click or drop on a node or member", Tool::Delete, None),
];

impl App {
    /// The palette: click arms a tool, drag carries it onto the model.
    pub(crate) fn palette(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("palette-scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| self.palette_items(ui));
    }

    fn palette_items(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::get(ui.ctx());
        ui.add_space(2.0);
        for (k, (icon, tip, tool, sup)) in PALETTE.iter().enumerate() {
            if matches!(k, 2 | 10 | 13) {
                ui.separator();
            }
            let on = self.tool == *tool && (sup.is_none() || self.opts.support == sup.unwrap_or_default());
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(34.0, 27.0), egui::Sense::click_and_drag());
            let fill = if on {
                t.accent_soft
            } else if resp.hovered() {
                t.hover
            } else {
                egui::Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect, 6.0, fill);
            let img = crate::icons::image(icon, 18.0, if on { t.accent_text } else { t.text });
            img.paint_at(ui, egui::Rect::from_center_size(rect.center(), egui::vec2(18.0, 18.0)));
            let resp = resp.on_hover_text(*tip);
            if resp.clicked() {
                self.set_tool(*tool);
                if let Some(s) = sup {
                    self.opts.support = s;
                }
            }
            if resp.drag_started() {
                self.palette_drag = Some(k);
            }
        }
    }
}
