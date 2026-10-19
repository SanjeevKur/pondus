// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! What the views draw, in 3D: members, supports, slabs, loads, diagrams, the deflected shape
//! and utilisation colours, as lines, filled polygons and dots. The app paints them with egui
//! and the agent's pictures rasterise them (`picture.rs`), so both show the same thing.

use serde::{Deserialize, Serialize};

use crate::analysis::{axes, Analysis, CaseResult};
use crate::design::Report;
use crate::model::*;

pub type Rgba = [u8; 4];

#[derive(Clone, Debug)]
pub enum Prim {
    Line { a: [f64; 3], b: [f64; 3], color: Rgba, width: f32, member: Option<Id> },
    Poly { pts: Vec<[f64; 3]>, color: Rgba },
    Dot { p: [f64; 3], color: Rgba, r: f32 },
    /// A value written at a point (the app draws it; the agent's pictures leave it out).
    Text { p: [f64; 3], text: String, color: Rgba },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Diagram {
    #[default]
    None,
    /// Major-axis bending moment My (drawn on the tension side) and its shear Vz.
    Moment,
    Shear,
    Axial,
    /// Minor-axis moment Mz and its shear Vy (drawn in the member's local x-y plane).
    #[serde(alias = "mz")]
    MomentZ,
    #[serde(alias = "vy")]
    ShearY,
    Torsion,
    /// The slabs' plate moments mx and my (plate method), as colours.
    #[serde(alias = "slab_mx")]
    PlateMx,
    #[serde(alias = "slab_my")]
    PlateMy,
    /// The twisting moment mxy and the transverse shears qx, qy of the slabs.
    #[serde(alias = "slab_mxy")]
    PlateMxy,
    #[serde(alias = "slab_qx")]
    PlateQx,
    #[serde(alias = "slab_qy")]
    PlateQy,
    Deflection,
    /// A mode shape of the modal analysis (`Spec::mode`, scaled by `Spec::phase`).
    Mode,
    /// A buckling mode shape (`Spec::mode`).
    Buckling,
    /// Members coloured by their design ratio.
    Utilisation,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Spec {
    /// "3d" (default), "plan" or "elevation" (looking along +y).
    pub view: String,
    pub diagram: Diagram,
    /// Which results: a combination or case name (default: the first ULS combination; the
    /// first SLS one for deflections).
    pub result: String,
    /// Only what lies at this height (plan) or in this x-z plane at y (elevation), m.
    pub z: Option<f64>,
    pub y: Option<f64>,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub highlight: Vec<Id>,
    pub caption: String,
    pub show_loads: bool,
    /// Write the values of a slab diagram on its elements.
    pub values: bool,
    /// The mode shown (0 = the first) and its animation phase, −1..1 (1 when still).
    pub mode: usize,
    pub phase: Option<f64>,
}

impl Diagram {
    pub fn is_plate(self) -> bool {
        matches!(self, Diagram::PlateMx | Diagram::PlateMy | Diagram::PlateMxy | Diagram::PlateQx | Diagram::PlateQy)
    }
    /// The plate result a slab diagram shows, its name and its units.
    pub fn plate_value(self, p: &crate::slabfe::PlateResult) -> f64 {
        match self {
            Diagram::PlateMx => p.mx,
            Diagram::PlateMy => p.my,
            Diagram::PlateMxy => p.mxy,
            Diagram::PlateQx => p.qx,
            Diagram::PlateQy => p.qy,
            _ => 0.0,
        }
    }
    pub fn plate_label(self) -> (&'static str, &'static str) {
        match self {
            Diagram::PlateMx => ("mx: moment about y, bending in x (sagging +)", "kN·m/m"),
            Diagram::PlateMy => ("my: moment about x, bending in y (sagging +)", "kN·m/m"),
            Diagram::PlateMxy => ("mxy: twisting moment", "kN·m/m"),
            Diagram::PlateQx => ("qx: transverse shear on faces normal to x", "kN/m"),
            Diagram::PlateQy => ("qy: transverse shear on faces normal to y", "kN/m"),
            _ => ("", ""),
        }
    }
}

/// A diverging colour for t in −1..1: blue (negative) through near-white to red (positive).
pub fn diverging(t: f64) -> Rgba {
    let t = t.clamp(-1.0, 1.0);
    let (lo, mid, hi) = ([49.0, 104.0, 196.0], [236.0, 236.0, 236.0], [214.0, 72.0, 52.0]);
    let (a, b, u) = if t < 0.0 { (mid, lo, -t) } else { (mid, hi, t) };
    let c = [0, 1, 2].map(|k| (a[k] + (b[k] - a[k]) * u) as u8);
    [c[0], c[1], c[2], 235]
}

/// The largest |value| of a slab diagram over the slabs shown, and the slab level filter.
pub fn plate_range(model: &Model, r: &CaseResult, spec: &Spec) -> f64 {
    r.plates
        .iter()
        .filter(|p| !(spec.view == "plan" && spec.z.is_some_and(|z| model.slabs.iter().find(|s| s.id == p.slab).is_some_and(|s| (s.z - z).abs() > 0.05))))
        .map(|p| spec.diagram.plate_value(p).abs())
        .fold(0.0, f64::max)
}

pub const CONCRETE: Rgba = [176, 176, 182, 255];
pub const STEEL: Rgba = [90, 150, 230, 255];
pub const SUPPORT: Rgba = [80, 200, 120, 255];
pub const SLAB: Rgba = [150, 150, 160, 50];
pub const HIGHLIGHT: Rgba = [55, 142, 240, 255];
pub const DIAG_POS: Rgba = [230, 120, 60, 200];
pub const DIAG_NEG: Rgba = [70, 140, 230, 200];
pub const LOAD: Rgba = [230, 70, 70, 220];

/// Green below 0.7, yellow to 0.9, orange to 1.0, red over.
pub fn ratio_color(r: f64) -> Rgba {
    if !r.is_finite() || r > 1.0 {
        [230, 60, 60, 255]
    } else if r > 0.9 {
        [240, 150, 40, 255]
    } else if r > 0.7 {
        [230, 210, 60, 255]
    } else {
        [80, 200, 110, 255]
    }
}

/// The design ratio of a member or slab from a report.
pub fn member_ratio(rep: &Report, id: Id) -> Option<f64> {
    rep.beams.iter().find(|b| b.member == id).map(|b| b.ratio).or_else(|| rep.columns.iter().find(|b| b.member == id).map(|b| b.ratio)).or_else(|| rep.steel.iter().find(|b| b.member == id).map(|b| b.ratio)).or_else(|| rep.walls.iter().find(|b| b.member == id).map(|b| b.ratio))
}

fn visible(model: &Model, m: &Member, spec: &Spec) -> bool {
    // a wall's rigid arms are part of its analysis model, not something to look at
    if matches!(m.role, Role::WallLink(_)) {
        return false;
    }
    let (Some(a), Some(b)) = (model.node(m.i), model.node(m.j)) else { return false };
    if spec.view == "plan" {
        if let Some(z) = spec.z {
            return (a.z - z).abs() < 0.05 && (b.z - z).abs() < 0.05;
        }
    }
    if spec.view == "elevation" {
        if let Some(y) = spec.y {
            return (a.y - y).abs() < 0.05 && (b.y - y).abs() < 0.05;
        }
    }
    true
}

/// The result a picture shows: the named one, else the first ULS (SLS for deflection).
pub fn pick_result(model: &Model, a: &Analysis, spec: &Spec) -> Option<CaseResult> {
    if !spec.result.is_empty() {
        if let Some(r) = a.result(model, &spec.result) {
            return Some(r);
        }
    }
    let kind = if spec.diagram == Diagram::Deflection { ComboKind::Sls } else { ComboKind::Uls };
    model.combos.iter().find(|c| c.kind == kind).or(model.combos.first()).map(|c| a.combine(&c.name, &c.factors)).or_else(|| a.cases.first().cloned())
}

/// The scene. `scale` is the model size (for diagram heights); `analysis` and `report` are
/// optional.
pub fn build(model: &Model, analysis: Option<&Analysis>, report: Option<&Report>, spec: &Spec) -> Vec<Prim> {
    let mut out = vec![];
    let size = extent(model).map(|(lo, hi)| dist(lo, hi)).unwrap_or(10.0).max(1.0);
    for s in &model.slabs {
        if spec.view == "plan" && spec.z.is_some_and(|z| (s.z - z).abs() > 0.05) {
            continue;
        }
        if spec.view == "elevation" || (spec.diagram.is_plate() && analysis.is_some_and(|a| a.cases.iter().any(|c| !c.plates.is_empty()))) {
            continue;
        }
        let color = match report.and_then(|r| r.slabs.iter().find(|d| d.slab == s.id)) {
            Some(d) if spec.diagram == Diagram::Utilisation => {
                let c = ratio_color(d.ratio);
                [c[0], c[1], c[2], 70]
            }
            _ => SLAB,
        };
        out.push(Prim::Poly { pts: vec![[s.x0, s.y0, s.z], [s.x1, s.y0, s.z], [s.x1, s.y1, s.z], [s.x0, s.y1, s.z]], color });
    }
    // a mode shape: the members between their nodes' modal displacements
    if matches!(spec.diagram, Diagram::Mode | Diagram::Buckling) {
        let shape = if spec.diagram == Diagram::Mode {
            analysis.and_then(|a| a.seismic.as_ref()).and_then(|s| s.modes.get(spec.mode)).map(|m| &m.shape)
        } else {
            analysis.and_then(|a| a.buckling.as_ref()).and_then(|b| b.1.get(spec.mode)).map(|m| &m.shape)
        };
        if let Some(shape) = shape {
            let size = extent(model).map(|(lo, hi)| dist(lo, hi)).unwrap_or(10.0).max(1.0);
            let k = 0.08 * size * spec.phase.unwrap_or(1.0);
            let at = |id: Id| -> Option<[f64; 3]> {
                let n = model.node(id)?;
                let u = shape.get(&id).copied().unwrap_or([0.0; 3]);
                Some([n.x + k * u[0], n.y + k * u[1], n.z + k * u[2]])
            };
            for m in model.members.iter().filter(|m| !matches!(m.role, Role::WallLink(_))) {
                if let (Some(a), Some(b), Some(p), Some(q)) = (model.node(m.i), model.node(m.j), at(m.i), at(m.j)) {
                    out.push(Prim::Line { a: a.p(), b: b.p(), color: [120, 120, 126, 90], width: 1.0, member: Some(m.id) });
                    out.push(Prim::Line { a: p, b: q, color: DIAG_POS, width: 2.0, member: None });
                }
            }
            for s in &model.supports {
                if let Some(n) = model.node(s.node) {
                    out.push(Prim::Dot { p: n.p(), color: SUPPORT, r: 4.0 });
                }
            }
            return out;
        }
    }
    let result = analysis.and_then(|a| pick_result(model, a, spec));
    // slab plate results: each element coloured on the diverging scale, values if asked
    if let (Some(r), true) = (result.as_ref(), spec.diagram.is_plate()) {
        let vmax = plate_range(model, r, spec).max(1e-9);
        for p in &r.plates {
            let Some(s) = model.slabs.iter().find(|s| s.id == p.slab) else { continue };
            if spec.view == "plan" && spec.z.is_some_and(|z| (s.z - z).abs() > 0.05) {
                continue;
            }
            let v = spec.diagram.plate_value(p);
            let (hx, hy) = (p.dx / 2.0, p.dy / 2.0);
            out.push(Prim::Poly { pts: vec![[p.x - hx, p.y - hy, s.z], [p.x + hx, p.y - hy, s.z], [p.x + hx, p.y + hy, s.z], [p.x - hx, p.y + hy, s.z]], color: diverging(v / vmax) });
            if spec.values {
                out.push(Prim::Text { p: [p.x, p.y, s.z], text: format!("{v:.1}"), color: [20, 20, 22, 255] });
            }
        }
    }
    // diagram scale: the largest value over the visible members maps to 8 % of the model size
    let value = |s: &crate::analysis::Station| -> f64 {
        match spec.diagram {
            Diagram::Moment => s.my,
            Diagram::Shear => s.vz,
            Diagram::Axial => s.n,
            Diagram::MomentZ => s.mz,
            Diagram::ShearY => s.vy,
            Diagram::Torsion => s.t,
            _ => 0.0,
        }
    };
    let vmax = result.as_ref().map(|r| r.members.iter().filter(|mr| model.member(mr.member).is_some_and(|m| visible(model, m, spec))).flat_map(|mr| mr.stations.iter().map(|s| value(s).abs())).fold(0.0, f64::max)).unwrap_or(0.0);
    let dmax = result.as_ref().map(|r| r.displacements.values().map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()).fold(0.0, f64::max)).unwrap_or(0.0);
    // walls: a panel per storey (its footprint in a plan), coloured by its design ratio
    for w in &model.walls {
        let mut piers: Vec<&Member> = model.members.iter().filter(|m| m.role == Role::WallPier(w.id)).collect();
        piers.sort_by(|a, b| model.node(a.i).map(|n| n.z).unwrap_or(0.0).total_cmp(&model.node(b.i).map(|n| n.z).unwrap_or(0.0)));
        let colour = |pier: Option<&Member>| -> Rgba {
            let r = pier.and_then(|p| report.and_then(|rep| rep.walls.iter().find(|d| d.member == p.id))).map(|d| d.ratio);
            match (spec.diagram == Diagram::Utilisation, r) {
                (true, Some(r)) => {
                    let c = ratio_color(r);
                    [c[0], c[1], c[2], 150]
                }
                _ => [168, 160, 150, 120],
            }
        };
        let edge: Rgba = [205, 198, 188, 255];
        if spec.view == "plan" {
            let z = spec.z.unwrap_or(w.z1);
            if z < w.z0 - 0.05 || z > w.z1 + 0.05 {
                continue;
            }
            let lw = w.length().max(1e-9);
            let n = [-(w.b[1] - w.a[1]) / lw * w.t / 2.0, (w.b[0] - w.a[0]) / lw * w.t / 2.0];
            let pier = piers.iter().find(|p| model.node(p.i).is_some_and(|a| a.z <= z + 0.05) && model.node(p.j).is_some_and(|b| b.z >= z - 0.05)).copied();
            let pts = vec![[w.a[0] + n[0], w.a[1] + n[1], z], [w.b[0] + n[0], w.b[1] + n[1], z], [w.b[0] - n[0], w.b[1] - n[1], z], [w.a[0] - n[0], w.a[1] - n[1], z]];
            out.push(Prim::Poly { pts: pts.clone(), color: colour(pier) });
            for k in 0..4 {
                out.push(Prim::Line { a: pts[k], b: pts[(k + 1) % 4], color: edge, width: 1.2, member: pier.map(|p| p.id) });
            }
            continue;
        }
        if spec.view == "elevation" && spec.y.is_some_and(|y| (w.a[1] - y).abs() > 0.05 || (w.b[1] - y).abs() > 0.05) {
            continue;
        }
        for p in &piers {
            let (Some(i), Some(j)) = (model.node(p.i), model.node(p.j)) else { continue };
            let (z0, z1) = (i.z.min(j.z), i.z.max(j.z));
            out.push(Prim::Poly { pts: vec![[w.a[0], w.a[1], z0], [w.b[0], w.b[1], z0], [w.b[0], w.b[1], z1], [w.a[0], w.a[1], z1]], color: colour(Some(p)) });
            out.push(Prim::Line { a: [w.a[0], w.a[1], z1], b: [w.b[0], w.b[1], z1], color: edge, width: 1.0, member: None });
        }
        out.push(Prim::Line { a: [w.a[0], w.a[1], w.z0], b: [w.a[0], w.a[1], w.z1], color: edge, width: 1.2, member: None });
        out.push(Prim::Line { a: [w.b[0], w.b[1], w.z0], b: [w.b[0], w.b[1], w.z1], color: edge, width: 1.2, member: None });
    }
    for m in &model.members {
        if !visible(model, m, spec) {
            continue;
        }
        let (Some(a), Some(b)) = (model.node(m.i), model.node(m.j)) else { continue };
        let mut color = if model.is_steel(m) { STEEL } else { CONCRETE };
        if spec.diagram == Diagram::Utilisation {
            if let Some(r) = report.and_then(|rep| member_ratio(rep, m.id)) {
                color = ratio_color(r);
            }
        }
        if spec.highlight.contains(&m.id) {
            color = HIGHLIGHT;
        }
        let deflected = spec.diagram == Diagram::Deflection && dmax > 0.0;
        let width = if spec.diagram == Diagram::Utilisation { 3.0 } else { 2.0 };
        if deflected {
            out.push(Prim::Line { a: a.p(), b: b.p(), color: [color[0], color[1], color[2], 90], width: 1.0, member: Some(m.id) });
        } else {
            out.push(Prim::Line { a: a.p(), b: b.p(), color, width, member: Some(m.id) });
        }
        let (Some(r), Some((ex, ey, ez))) = (result.as_ref(), axes(model, m)) else { continue };
        // the plane a diagram is drawn in: local x-z for My, Vz, N, T; local x-y for Mz, Vy
        let ez = if matches!(spec.diagram, Diagram::MomentZ | Diagram::ShearY) { ey } else { ez };
        let Some(mr) = r.member(m.id) else { continue };
        let at = |x: f64| [a.x + ex[0] * x, a.y + ex[1] * x, a.z + ex[2] * x];
        if deflected {
            let k = 0.08 * size / dmax;
            let pts: Vec<[f64; 3]> = mr.stations.iter().map(|s| {
                let p = at(s.x);
                [p[0] + s.u[0] * k, p[1] + s.u[1] * k, p[2] + s.u[2] * k]
            }).collect();
            for w in pts.windows(2) {
                out.push(Prim::Line { a: w[0], b: w[1], color: DIAG_POS, width: 2.0, member: Some(m.id) });
            }
        } else if matches!(spec.diagram, Diagram::Moment | Diagram::Shear | Diagram::Axial | Diagram::MomentZ | Diagram::ShearY | Diagram::Torsion) && vmax > 0.0 {
            // drawn in the member's local x-z plane; a moment on the tension side (sagging below)
            let k = 0.08 * size / vmax;
            let dir = if matches!(spec.diagram, Diagram::Moment | Diagram::MomentZ) { -1.0 } else { 1.0 };
            // columns draw along their local z, which is horizontal
            for w in mr.stations.windows(2) {
                let (s0, s1) = (&w[0], &w[1]);
                let (v0, v1) = (value(s0), value(s1));
                let (p0, p1) = (at(s0.x), at(s1.x));
                let off = |p: [f64; 3], v: f64| [p[0] + ez[0] * v * k * dir, p[1] + ez[1] * v * k * dir, p[2] + ez[2] * v * k * dir];
                let c = if v0 + v1 >= 0.0 { DIAG_POS } else { DIAG_NEG };
                out.push(Prim::Poly { pts: vec![p0, p1, off(p1, v1), off(p0, v0)], color: [c[0], c[1], c[2], 90] });
                out.push(Prim::Line { a: off(p0, v0), b: off(p1, v1), color: c, width: 1.5, member: None });
            }
        }
    }
    let marker = 0.012 * size;
    for s in &model.supports {
        let Some(n) = model.node(s.node) else { continue };
        if spec.view == "plan" && spec.z.is_some_and(|z| (n.z - z).abs() > 0.05) {
            continue;
        }
        let p = n.p();
        if s.kind() == "fixed" {
            let d = marker;
            out.push(Prim::Poly { pts: vec![[p[0] - d, p[1] - d, p[2] - d * 0.6], [p[0] + d, p[1] - d, p[2] - d * 0.6], [p[0] + d, p[1] + d, p[2] - d * 0.6], [p[0] - d, p[1] + d, p[2] - d * 0.6]], color: SUPPORT });
        } else {
            out.push(Prim::Poly { pts: vec![p, [p[0] - marker, p[1], p[2] - 1.6 * marker], [p[0] + marker, p[1], p[2] - 1.6 * marker]], color: SUPPORT });
        }
    }
    if spec.show_loads {
        for l in &model.loads {
            if let LoadKind::Nodal { node, f } = l.kind {
                if let Some(n) = model.node(node) {
                    let len = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt().max(1e-9);
                    let a = 0.06 * size;
                    let p = n.p();
                    out.push(Prim::Line { a: [p[0] - f[0] / len * a, p[1] - f[1] / len * a, p[2] - f[2] / len * a], b: p, color: LOAD, width: 2.0, member: None });
                }
            }
        }
    }
    for n in &model.nodes {
        if spec.view == "plan" && spec.z.is_some_and(|z| (n.z - z).abs() > 0.05) {
            continue;
        }
        if spec.view == "elevation" && spec.y.is_some_and(|y| (n.y - y).abs() > 0.05) {
            continue;
        }
        out.push(Prim::Dot { p: n.p(), color: [210, 210, 215, 255], r: 2.0 });
    }
    out
}

pub fn extent(model: &Model) -> Option<([f64; 3], [f64; 3])> {
    let mut it = model.nodes.iter().map(|n| n.p()).chain(model.slabs.iter().flat_map(|s| [[s.x0, s.y0, s.z], [s.x1, s.y1, s.z]]));
    let f = it.next()?;
    Some(it.fold((f, f), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])))
}

/// An orthographic camera: yaw about z (degrees, 0 = looking along +y), pitch above the
/// horizon. `project` gives screen x right and y down, before fitting.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub yaw: f64,
    pub pitch: f64,
}

impl Camera {
    pub fn for_view(spec: &Spec) -> Camera {
        match spec.view.as_str() {
            "plan" => Camera { yaw: 0.0, pitch: 90.0 },
            "elevation" => Camera { yaw: 0.0, pitch: 0.0 },
            _ => Camera { yaw: spec.yaw.unwrap_or(-35.0), pitch: spec.pitch.unwrap_or(28.0) },
        }
    }
    /// The view's right, up and forward (into the screen) unit vectors.
    pub fn basis(&self) -> ([f64; 3], [f64; 3], [f64; 3]) {
        let (y, t) = (self.yaw.to_radians(), self.pitch.to_radians());
        let r = [y.cos(), y.sin(), 0.0];
        let fwd = [-y.sin() * t.cos(), y.cos() * t.cos(), -t.sin()];
        let up = [r[1] * fwd[2] - r[2] * fwd[1], r[2] * fwd[0] - r[0] * fwd[2], r[0] * fwd[1] - r[1] * fwd[0]];
        (r, up, fwd)
    }

    /// The point on the plane `axis` = `value` (axis 1: y, 2: z) seen at projected screen
    /// coordinates (sx, sy) as `project` gives them; None when the plane is seen edge-on.
    pub fn unproject_on(&self, sx: f64, sy: f64, axis: usize, value: f64) -> Option<[f64; 3]> {
        let (r, up, fwd) = self.basis();
        if fwd[axis].abs() < 1e-6 {
            return None;
        }
        let t = (value - sx * r[axis] + sy * up[axis]) / fwd[axis];
        Some([0, 1, 2].map(|k| sx * r[k] - sy * up[k] + t * fwd[k]))
    }

    pub fn project(&self, p: [f64; 3]) -> (f64, f64, f64) {
        let (r, up, fwd) = self.basis();
        let sx = p[0] * r[0] + p[1] * r[1] + p[2] * r[2];
        let sy = p[0] * up[0] + p[1] * up[1] + p[2] * up[2];
        let depth = p[0] * fwd[0] + p[1] * fwd[1] + p[2] * fwd[2];
        (sx, -sy, depth)
    }
}

/// The 2D bounds of the scene in a camera.
pub fn bounds(prims: &[Prim], cam: &Camera) -> Option<(f64, f64, f64, f64)> {
    let pts = prims.iter().flat_map(|p| match p {
        Prim::Line { a, b, .. } => vec![*a, *b],
        Prim::Poly { pts, .. } => pts.clone(),
        Prim::Dot { p, .. } | Prim::Text { p, .. } => vec![*p],
    });
    let mut b: Option<(f64, f64, f64, f64)> = None;
    for p in pts {
        let (x, y, _) = cam.project(p);
        b = Some(match b {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        });
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unproject_inverts_project() {
        for (yaw, pitch, axis) in [(-35.0, 28.0, 2), (0.0, 90.0, 2), (0.0, 0.0, 1), (20.0, 10.0, 1)] {
            let cam = Camera { yaw, pitch };
            let p = if axis == 2 { [3.0, -2.0, 4.0] } else { [3.0, 1.5, 4.0] };
            let (x, y, _) = cam.project(p);
            let q = cam.unproject_on(x, y, axis, p[axis]).unwrap();
            assert!(dist(p, q) < 1e-9, "{yaw} {pitch}: {q:?}");
        }
    }
}
