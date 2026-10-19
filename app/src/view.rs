// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The model view: the scene of `pondus_core::scene` painted with egui, orthographic, with
//! orbit (left drag), pan (right or middle drag, or Shift + left drag), zoom (scroll, about
//! the pointer) and picking of members (click).

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use pondus_core::model::{Id, Model};
use pondus_core::scene::{self, Camera, Prim};

use crate::theme::Tokens;

pub struct View {
    /// A primary drag in progress that a tool owns: where it started (on the plane, snapped),
    /// the node and member under its start.
    anchor: Option<Anchor>,
    pub yaw: f64,
    pub pitch: f64,
    /// Screen pixels per metre and the screen offset of the model's centre.
    zoom: f64,
    pan: Vec2,
    /// Fit the model on the next frame.
    pub fit: bool,
    pub labels: bool,
}

impl Default for View {
    fn default() -> Self {
        View { anchor: None, yaw: -35.0, pitch: 28.0, zoom: 20.0, pan: Vec2::ZERO, fit: true, labels: false }
    }
}

#[derive(Clone, Copy)]
struct Anchor {
    at: [f64; 3],
    node: Option<Id>,
    member: Option<Id>,
}

/// A tool's drag that ended this frame: from and to on the plane (snapped), and what lay
/// under its start.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub from: [f64; 3],
    pub to: [f64; 3],
    pub node: Option<Id>,
    pub member: Option<Id>,
}

/// What a primary drag does in the view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DragMode {
    /// Orbit the view (the default, and the select tool on empty space).
    #[default]
    Orbit,
    /// Draw a line from the press to the release (members).
    Line,
    /// Draw a rectangle on the plane (bays, slabs, columns at its corners).
    Rect,
    /// Move the node or the member under the press; orbit when the press hits nothing.
    Move,
}

#[derive(Clone)]
pub struct Picked {
    pub member: Option<Id>,
    /// The node under the pointer (within 9 px), which wins over a member.
    pub node: Option<Id>,
    /// The pointer on the work plane, snapped to the grid (or to the node under it).
    pub plane: Option<[f64; 3]>,
    pub clicked: bool,
    pub secondary: bool,
    /// The pointer in the camera's projected coordinates (for `Camera::unproject_on`).
    pub proj: Option<(f64, f64)>,
    /// The view's rectangle on screen.
    pub rect: egui::Rect,
    /// A tool's drag that ended this frame.
    pub drag: Option<Drag>,
    /// The pointer is over the view (also while something is dragged in from outside it).
    pub inside: bool,
}

/// What the drawing tools add to the view: the work plane and its grid, the rubber band
/// from the first point of a member, and whether left drag pans.
#[derive(Default)]
pub struct Overlay {
    /// The work plane: axis 2 (z = value, floors) or 1 (y = value, frames in elevation).
    pub plane: Option<(usize, f64)>,
    pub grid: f64,
    pub from: Option<[f64; 3]>,
    pub pan_tool: bool,
    /// Nodes are picked (the node, member and support tools).
    pub nodes: bool,
    pub marked_nodes: Vec<Id>,
    pub drag: DragMode,
    /// Draw the grid dots (the drawing tools).
    pub show_grid: bool,
    /// Something is being dragged in from the tool palette: pick under the pointer although
    /// the view itself is not hovered.
    pub dropping: bool,
}

fn c32(c: scene::Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

impl View {
    pub fn set_preset(&mut self, view: &str) {
        let c = Camera::for_view(&scene::Spec { view: view.into(), ..Default::default() });
        self.yaw = c.yaw;
        self.pitch = c.pitch;
        self.fit = true;
    }

    /// Paints the scene into the rest of `ui`; returns the member under a click.
    pub fn show(&mut self, ui: &mut egui::Ui, model: &Model, prims: &[Prim], selected: &[Id], ov: &Overlay, t: &Tokens) -> Picked {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, t.canvas);
        let cam = Camera { yaw: self.yaw, pitch: self.pitch };
        // everything is drawn about the model's centre, so orbiting turns about it
        let ctr = scene::extent(model).map(|(lo, hi)| [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0]).unwrap_or([0.0; 3]);
        let (c0x, c0y, _) = cam.project(ctr);
        if self.fit {
            if let Some((x0, y0, x1, y1)) = scene::bounds(prims, &cam) {
                let k = ((rect.width() as f64 - 80.0) / (x1 - x0).max(1e-6)).min((rect.height() as f64 - 80.0) / (y1 - y0).max(1e-6));
                self.zoom = k.clamp(0.5, 5000.0);
                let c = ((x0 + x1) / 2.0 - c0x, (y0 + y1) / 2.0 - c0y);
                self.pan = Vec2::new(-(c.0 * self.zoom) as f32, -(c.1 * self.zoom) as f32);
            }
            self.fit = false;
        }
        // input: a drawing tool owns the primary drag (Shift-drag still pans, right drag
        // orbits); the move tool owns it only when the press hits a node or a member
        let shift = ui.input(|i| i.modifiers.shift);
        let started = resp.drag_started_by(egui::PointerButton::Primary) && !shift && !ov.pan_tool && ov.drag != DragMode::Orbit;
        let tool_drag = self.anchor.is_some() || started;
        if resp.dragged_by(egui::PointerButton::Secondary) && ov.drag != DragMode::Orbit && ov.drag != DragMode::Move {
            let d = resp.drag_delta();
            self.yaw -= d.x as f64 * 0.4;
            self.pitch = (self.pitch + d.y as f64 * 0.4).clamp(-89.0, 90.0);
        } else if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) || (resp.dragged_by(egui::PointerButton::Primary) && (shift || ov.pan_tool)) {
            self.pan += resp.drag_delta();
        } else if resp.dragged_by(egui::PointerButton::Primary) && !tool_drag {
            let d = resp.drag_delta();
            self.yaw -= d.x as f64 * 0.4;
            self.pitch = (self.pitch + d.y as f64 * 0.4).clamp(-89.0, 90.0);
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let f = (scroll as f64 * 0.0025).exp();
                let p = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center()) - rect.center();
                self.pan = p + (self.pan - p) * f as f32;
                self.zoom *= f;
            }
        }
        let origin = rect.center() + self.pan;
        let cam2 = Camera { yaw: self.yaw, pitch: self.pitch };
        let (cx, cy, _) = cam2.project(ctr);
        let cam = cam2;
        let to = |p: [f64; 3]| -> (Pos2, f64) {
            let (x, y, d) = cam.project(p);
            let (x, y) = (x - cx, y - cy);
            (origin + Vec2::new((x * self.zoom) as f32, (y * self.zoom) as f32), d)
        };
        let zoom = self.zoom;
        // the work plane's grid: dots within the model's extent and a margin
        if let (Some((axis, value)), true) = (ov.plane, ov.grid > 0.0 && ov.show_grid) {
            let (lo, hi) = scene::extent(model).unwrap_or(([0.0; 3], [12.0, 12.0, 6.0]));
            let g = ov.grid;
            let m = 2.0 * g;
            let (a0, a1) = (((lo[0] - m) / g).floor() as i64, ((hi[0] + m) / g).ceil() as i64);
            let other = if axis == 2 { 1 } else { 2 };
            let (b0, b1) = (((lo[other] - m) / g).floor() as i64, ((hi[other] + m) / g).ceil() as i64);
            if (a1 - a0) * (b1 - b0) < 40_000 {
                for i in a0..=a1 {
                    for j in b0..=b1 {
                        let mut p = [i as f64 * g, 0.0, 0.0];
                        p[other] = j as f64 * g;
                        p[axis] = value;
                        painter.circle_filled(to(p).0, 1.2, t.canvas_dot);
                    }
                }
            }
        }
        // far to near
        let mut order: Vec<(f64, usize)> = prims
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let d = match p {
                    Prim::Line { a, b, .. } => (to(*a).1 + to(*b).1) / 2.0,
                    Prim::Poly { pts, .. } => pts.iter().map(|q| to(*q).1).sum::<f64>() / pts.len().max(1) as f64 + 1e-6,
                    Prim::Dot { p, .. } | Prim::Text { p, .. } => to(*p).1,
                };
                (-d, i)
            })
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        // value labels that would overlap one already written are left out
        let mut written: Vec<egui::Rect> = vec![];
        for (_, i) in order {
            match &prims[i] {
                Prim::Line { a, b, color, width, member } => {
                    let sel = member.is_some_and(|m| selected.contains(&m));
                    let (c, w) = if sel { (t.accent, width + 2.0) } else { (c32(*color), *width) };
                    painter.line_segment([to(*a).0, to(*b).0], Stroke::new(w, c));
                }
                Prim::Poly { pts, color } => {
                    let pts: Vec<Pos2> = pts.iter().map(|q| to(*q).0).collect();
                    painter.add(egui::Shape::convex_polygon(pts, c32(*color), Stroke::NONE));
                }
                Prim::Dot { p, color, r } => {
                    painter.circle_filled(to(*p).0, *r, c32(*color));
                }
                Prim::Text { p, text, color } => {
                    let font = egui::FontId::proportional(9.5);
                    let size = painter.layout_no_wrap(text.clone(), font.clone(), c32(*color)).size();
                    let r = egui::Rect::from_center_size(to(*p).0, size + Vec2::new(4.0, 2.0));
                    if !written.iter().any(|w| w.intersects(r)) {
                        painter.text(r.center(), egui::Align2::CENTER_CENTER, text, font, c32(*color));
                        written.push(r);
                    }
                }
            }
        }
        if self.labels {
            for m in &model.members {
                if let (Some(a), Some(b)) = (model.node(m.i), model.node(m.j)) {
                    let p = to([(a.x + b.x) / 2.0, (a.y + b.y) / 2.0, (a.z + b.z) / 2.0]).0;
                    painter.text(p + Vec2::new(3.0, -3.0), egui::Align2::LEFT_BOTTOM, m.id.to_string(), egui::FontId::proportional(10.5), t.ink);
                }
            }
        }
        // axes triad
        let base = rect.left_bottom() + Vec2::new(36.0, -36.0);
        for (v, name, c) in [([1.0, 0.0, 0.0], "x", Color32::from_rgb(230, 90, 80)), ([0.0, 1.0, 0.0], "y", Color32::from_rgb(110, 200, 110)), ([0.0, 0.0, 1.0], "z", Color32::from_rgb(100, 150, 240))] {
            let (x, y, _) = cam.project(v);
            let e = base + Vec2::new(x as f32 * 22.0, y as f32 * 22.0);
            painter.line_segment([base, e], Stroke::new(1.5, c));
            painter.text(e, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(10.5), c);
        }
        // marked nodes (the first end of a member being drawn)
        for n in &ov.marked_nodes {
            if let Some(n) = model.node(*n) {
                painter.circle_stroke(to(n.p()).0, 6.0, Stroke::new(2.0, t.accent));
            }
        }
        // picking: the node (within 9 px, which wins), the member (within 8 px) and the point on
        // the work plane (snapped to the grid, or to the node) under a screen position
        let pick = |pos: Pos2, plane: Option<(usize, f64)>| -> (Option<Id>, Option<Id>, Option<[f64; 3]>) {
            let mut node = None;
            if ov.nodes || ov.drag == DragMode::Move || ov.dropping {
                let mut best = 9.0f32;
                for n in &model.nodes {
                    let d = (to(n.p()).0 - pos).length();
                    if d < best {
                        best = d;
                        node = Some(n.id);
                    }
                }
            }
            let mut member = None;
            if node.is_none() {
                let mut best = 8.0f32;
                for p in prims {
                    if let Prim::Line { a, b, member: Some(m), .. } = p {
                        let (pa, pb) = (to(*a).0, to(*b).0);
                        let ab = pb - pa;
                        let tt = ((pos - pa).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
                        let d = (pa + ab * tt - pos).length();
                        if d < best {
                            best = d;
                            member = Some(*m);
                        }
                    }
                }
            }
            let snap = |p: [f64; 3]| if ov.grid > 0.0 { p.map(|x| (x / ov.grid).round() * ov.grid) } else { p };
            let pt = match node.and_then(|n| model.node(n)) {
                Some(n) => Some(n.p()),
                None => plane.and_then(|(axis, value)| {
                    let d = pos - origin;
                    cam.unproject_on(d.x as f64 / zoom + cx, d.y as f64 / zoom + cy, axis, value)
                }).map(|p| {
                    let mut q = snap(p);
                    if let Some((axis, value)) = plane {
                        q[axis] = value;
                    }
                    q
                }),
            };
            (node, member, pt)
        };
        let hover_any = ui.input(|i| i.pointer.latest_pos()).filter(|p| rect.contains(*p));
        let pointer = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.dragged() || resp.drag_stopped()).or_else(|| resp.hover_pos()).or(if ov.dropping { hover_any } else { None });
        let proj = pointer.map(|pos| {
            let d = pos - origin;
            (d.x as f64 / zoom + cx, d.y as f64 / zoom + cy)
        });
        let mut picked = Picked { member: None, node: None, plane: None, clicked: resp.clicked(), secondary: resp.secondary_clicked(), proj, rect, drag: None, inside: hover_any.is_some() };
        // a tool's drag: start, preview, end
        if started {
            let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            let (n, m, pt) = pick(press, ov.plane);
            let hit = n.is_some() || m.is_some();
            match (ov.drag, pt) {
                (DragMode::Move, _) if !hit => {}
                (DragMode::Move, _) => {
                    // the member moves on the plane through its first node
                    let at = match (n.and_then(|n| model.node(n)), m.and_then(|m| model.member(m)).and_then(|m| model.node(m.i))) {
                        (Some(nd), _) => Some(nd.p()),
                        (None, Some(nd)) => ov.plane.and_then(|(axis, _)| pick(press, Some((axis, nd.p()[axis]))).2),
                        _ => None,
                    };
                    if let Some(at) = at {
                        self.anchor = Some(Anchor { at, node: n, member: if n.is_none() { m } else { None } });
                    }
                }
                (_, Some(at)) => self.anchor = Some(Anchor { at, node: n, member: m }),
                _ => {}
            }
        }
        let anchor_plane = self.anchor.and_then(|an| ov.plane.map(|(axis, _)| (axis, an.at[axis])));
        let (node, member, plane_pt) = match pointer {
            Some(pos) => pick(pos, if self.anchor.is_some() { anchor_plane } else { ov.plane }),
            None => (None, None, None),
        };
        picked.node = node;
        picked.member = member;
        picked.plane = plane_pt;
        if let Some(an) = self.anchor {
            if let Some(cur) = plane_pt {
                let pen = Stroke::new(1.8, t.accent_text);
                match ov.drag {
                    DragMode::Rect => {
                        let (axis, _) = ov.plane.unwrap_or((2, 0.0));
                        let (u, v) = if axis == 2 { (0, 1) } else if axis == 1 { (0, 2) } else { (1, 2) };
                        let corner = |cu: f64, cv: f64| {
                            let mut q = an.at;
                            q[u] = cu;
                            q[v] = cv;
                            to(q).0
                        };
                        let ring = vec![corner(an.at[u], an.at[v]), corner(cur[u], an.at[v]), corner(cur[u], cur[v]), corner(an.at[u], cur[v]), corner(an.at[u], an.at[v])];
                        painter.add(egui::Shape::line(ring, pen));
                        painter.text(to(cur).0 + Vec2::new(10.0, 10.0), egui::Align2::LEFT_TOP, format!("{:.2} × {:.2} m", (cur[u] - an.at[u]).abs(), (cur[v] - an.at[v]).abs()), egui::FontId::monospace(10.5), t.ink);
                    }
                    _ => {
                        painter.line_segment([to(an.at).0, to(cur).0], pen);
                        painter.circle_filled(to(an.at).0, 3.5, t.accent_text);
                        let l = ((cur[0] - an.at[0]).powi(2) + (cur[1] - an.at[1]).powi(2) + (cur[2] - an.at[2]).powi(2)).sqrt();
                        painter.text(to(cur).0 + Vec2::new(10.0, 10.0), egui::Align2::LEFT_TOP, format!("{:.2}, {:.2}, {:.2} · {l:.2} m", cur[0], cur[1], cur[2]), egui::FontId::monospace(10.5), t.ink);
                    }
                }
                painter.circle_stroke(to(cur).0, 5.0, Stroke::new(1.5, t.accent_text));
            }
            if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
                if let Some(cur) = plane_pt {
                    picked.drag = Some(Drag { from: an.at, to: cur, node: an.node, member: an.member });
                }
                self.anchor = None;
            }
            // a drag is not a click
            picked.clicked = false;
        } else if let Some(p) = picked.plane {
            if ov.plane.is_some() && (ov.show_grid || ov.dropping) {
                let sp = to(p).0;
                painter.circle_stroke(sp, 4.0, Stroke::new(1.5, t.accent_text));
                if let Some(a) = ov.from {
                    painter.line_segment([to(a).0, sp], Stroke::new(1.5, t.accent_text));
                }
                painter.text(sp + Vec2::new(8.0, 8.0), egui::Align2::LEFT_TOP, format!("{:.2}, {:.2}, {:.2}", p[0], p[1], p[2]), egui::FontId::monospace(10.5), t.ink);
            }
        }
        if let Some(n) = picked.node.and_then(|n| model.node(n)) {
            painter.circle_stroke(to(n.p()).0, 7.0, Stroke::new(2.0, t.warn));
        }
        picked
    }
}
