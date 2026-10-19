// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! DXF drawings in and out (ASCII, AutoCAD R12 / AC1009, which every CAD program reads).
//! Export writes the frame as one LINE per member on a layer named by its kind and section
//! ("BEAM_300x600", "COLUMN_H300x150x6p5x9": spaces dropped, a decimal point written p),
//! the nodes as POINTs (NODES), a small triangle under each support (SUPPORTS), the slabs as
//! 3DFACEs (SLAB_<h mm>), member labels as TEXT (LABELS) and, with an analysis, the My diagram
//! of a result as polylines (DIAGRAM_MY); either the whole model in 3D, a plan of one level
//! (with the columns' outlines on COLUMNS_PLAN) or the elevation of one frame line.
//! Import reads LINE, LWPOLYLINE and POLYLINE/VERTEX (2D and 3D) as members, one per segment,
//! with the kind and section from the layer name where it parses (otherwise a default
//! section), and 3DFACEs or closed polylines on SLAB_<h> layers as slab panels; the reserved
//! layers above are skipped, blocks (INSERT) are not exploded, arcs and circles are ignored.
//! The shared import result and `apply`, which adds it to a model, serve the IFC import too.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::analysis::Analysis;
use crate::model::*;

// ---------------------------------------------------------------- the shared import result

/// A member read from a file: its axis in model coordinates (m) and what is known of it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImportedMember {
    pub from: [f64; 3],
    pub to: [f64; 3],
    pub kind: MemberKind,
    /// A section name `Model::section_named` reads ("300 x 600 mm", "H 300x150x6.5x9").
    pub section: String,
    /// Steel or concrete when the file says so (None: from the section).
    pub material: Option<MaterialKind>,
    pub roll: f64,
    pub bim_id: Option<String>,
    pub name: Option<String>,
}

/// A slab read from a file: its outline in plan (m), top level and thickness.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImportedSlab {
    pub outline: Vec<[f64; 2]>,
    pub z: f64,
    pub h: f64,
    pub dead: f64,
    pub live: f64,
    pub bim_id: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ImportResult {
    pub members: Vec<ImportedMember>,
    pub slabs: Vec<ImportedSlab>,
    /// Entities read and skipped, by type.
    pub counts: BTreeMap<String, usize>,
    pub warnings: Vec<String>,
    /// The section used where a member's own does not parse.
    pub fallback_section: String,
    /// Ends closer than this merge into one node, m.
    pub tol: f64,
}

/// What `apply` added.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Applied {
    pub members: Vec<Id>,
    pub slabs: Vec<Id>,
    pub nodes: usize,
    pub warnings: Vec<String>,
}

/// Adds an import to the model: nodes merged within `r.tol` (and with the model's own),
/// sections by name (the fallback where one does not parse), steel for H and C shapes and
/// for members the file calls steel, concrete otherwise. A slab panel is added as it is when
/// it is an axis-aligned rectangle with no beam inside it; otherwise it becomes one panel per
/// closed bay of the beams at its level whose centre lies inside its outline.
pub fn apply(model: &mut Model, r: &ImportResult) -> Result<Applied, String> {
    let mut out = Applied { warnings: r.warnings.clone(), ..Default::default() };
    let tol = if r.tol > 0.0 { r.tol } else { 0.001 };
    let nodes_before = model.nodes.len();
    let node = |model: &mut Model, p: [f64; 3]| -> Id {
        if let Some(n) = model.nodes.iter().find(|n| dist(n.p(), p) <= tol) {
            return n.id;
        }
        let id = model.next_node_id();
        model.nodes.push(Node { id, x: p[0], y: p[1], z: p[2] });
        id
    };
    for im in &r.members {
        if dist(im.from, im.to) < tol {
            out.warnings.push(format!("a member of no length at {:?} skipped", im.from));
            continue;
        }
        let sid = match model.section_named(&im.section) {
            Ok(s) => s,
            Err(_) => {
                let fb = if r.fallback_section.is_empty() { "300 x 300" } else { &r.fallback_section };
                out.warnings.push(format!("section '{}' not known: {fb} used", im.section));
                model.section_named(fb)?
            }
        };
        let sec = model.section(sid).cloned().ok_or("no section")?;
        let steel_shape = matches!(sec.shape, Shape::H { .. } | Shape::LippedC { .. });
        let material = match im.material {
            Some(MaterialKind::Steel) => model.steel_id(),
            Some(MaterialKind::Concrete) => model.concrete_id(),
            _ if steel_shape => model.steel_id(),
            _ => model.concrete_id(),
        };
        let (i, j) = (node(model, im.from), node(model, im.to));
        if i == j {
            out.warnings.push(format!("a member from {:?} to {:?} closes on one node: skipped", im.from, im.to));
            continue;
        }
        if model.members.iter().any(|m| (m.i == i && m.j == j) || (m.i == j && m.j == i)) {
            out.warnings.push(format!("a second member between nodes {i} and {j} skipped"));
            continue;
        }
        let id = model.next_member_id();
        model.members.push(Member { id, i, j, section: sid, material, kind: im.kind, hinge_i: false, hinge_j: false, lb: None, k: None, roll: im.roll, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: im.bim_id.clone(), tension: None, role: Default::default(), group: None });
        out.members.push(id);
    }
    out.nodes = model.nodes.len() - nodes_before;
    // slabs: as they are, or one panel per bay inside them
    let mut bays_at: BTreeMap<i64, Vec<(f64, f64, f64, f64)>> = BTreeMap::new();
    for s in &r.slabs {
        if s.outline.len() < 3 {
            continue;
        }
        let (x0, y0, x1, y1) = bbox(&s.outline);
        let rect = s.outline.len() == 4 && s.outline.iter().all(|p| ((p[0] - x0).abs() < tol || (p[0] - x1).abs() < tol) && ((p[1] - y0).abs() < tol || (p[1] - y1).abs() < tol));
        let beam_inside = model.members.iter().any(|m| {
            let (Some(a), Some(b)) = (model.node(m.i), model.node(m.j)) else { return false };
            (a.z - s.z).abs() < 0.05 && (b.z - s.z).abs() < 0.05 && {
                let c = [(a.x + b.x) / 2.0, (a.y + b.y) / 2.0];
                c[0] > x0 + 0.05 && c[0] < x1 - 0.05 && c[1] > y0 + 0.05 && c[1] < y1 - 0.05
            }
        });
        let mut panels = vec![];
        if !(rect && !beam_inside) {
            let key = (s.z * 1000.0).round() as i64;
            let bays = bays_at.entry(key).or_insert_with(|| {
                let mut tmp = model.clone();
                tmp.slabs.clear();
                match crate::commands::run(&mut tmp, "add_slabs", &serde_json::json!({"bays": {"z": s.z}})) {
                    Ok(_) => tmp.slabs.iter().map(|q| (q.x0, q.y0, q.x1, q.y1)).collect(),
                    Err(_) => vec![],
                }
            });
            panels = bays.iter().copied().filter(|b| inside(&s.outline, [(b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0])).collect();
            if panels.is_empty() {
                out.warnings.push(format!("slab {}: no closed bay of beams inside it at z {:.3}: added as one panel over its bounding box", s.name, s.z));
            }
        }
        if panels.is_empty() {
            panels.push((x0, y0, x1, y1));
        }
        let n = panels.len();
        for (k, p) in panels.into_iter().enumerate() {
            let id = model.next_slab_id();
            let name = if n == 1 { s.name.clone() } else { format!("{} ({})", s.name, k + 1) };
            model.slabs.push(Slab { id, name: if name.is_empty() { format!("S{id}") } else { name }, x0: p.0, y0: p.1, x1: p.2, y1: p.3, z: s.z, h: s.h, dead: s.dead, live: s.live, bim_id: s.bim_id.clone() });
            out.slabs.push(id);
        }
    }
    Ok(out)
}

pub(crate) fn bbox(o: &[[f64; 2]]) -> (f64, f64, f64, f64) {
    o.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| (b.0.min(p[0]), b.1.min(p[1]), b.2.max(p[0]), b.3.max(p[1])))
}

/// Point in polygon (even-odd).
pub(crate) fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    let n = poly.len();
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
    }
    c
}

// ---------------------------------------------------------------- export

/// What the drawing shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "view", rename_all = "lowercase")]
pub enum DxfView {
    /// The whole model in 3D (model coordinates).
    #[default]
    #[serde(rename = "3d")]
    Model3d,
    /// One level: drawing X, Y = model x, y.
    Plan { z: f64 },
    /// The frame line at y: drawing X, Y = model x, z.
    #[serde(rename = "elevation_xz")]
    ElevationXz { y: f64 },
    /// The frame line at x: drawing X, Y = model y, z.
    #[serde(rename = "elevation_yz")]
    ElevationYz { x: f64 },
}

impl DxfView {
    fn project(&self, p: [f64; 3]) -> [f64; 3] {
        match self {
            DxfView::Model3d => p,
            DxfView::Plan { .. } => [p[0], p[1], 0.0],
            DxfView::ElevationXz { .. } => [p[0], p[2], 0.0],
            DxfView::ElevationYz { .. } => [p[1], p[2], 0.0],
        }
    }
    fn unproject(&self, q: [f64; 3]) -> [f64; 3] {
        match *self {
            DxfView::Model3d => q,
            DxfView::Plan { z } => [q[0], q[1], z + q[2]],
            DxfView::ElevationXz { y } => [q[0], y, q[1]],
            DxfView::ElevationYz { x } => [x, q[0], q[1]],
        }
    }
    fn shows(&self, a: [f64; 3], b: [f64; 3]) -> bool {
        let on = |v: f64, w: f64| (v - w).abs() < 0.01;
        match *self {
            DxfView::Model3d => true,
            DxfView::Plan { z } => on(a[2], z) && on(b[2], z),
            DxfView::ElevationXz { y } => on(a[1], y) && on(b[1], y),
            DxfView::ElevationYz { x } => on(a[0], x) && on(b[0], x),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DxfOptions {
    #[serde(flatten)]
    pub view: DxfView,
    /// Text height, m.
    pub text_height: f64,
    pub labels: bool,
    /// With an analysis: the result whose My diagram is drawn ("" = the first ULS
    /// combination), and the diagram's largest offset, m (0 = a tenth of the model's size).
    pub diagram: bool,
    pub result: String,
    pub diagram_size: f64,
}

impl Default for DxfOptions {
    fn default() -> Self {
        DxfOptions { view: DxfView::Model3d, text_height: 0.15, labels: true, diagram: true, result: String::new(), diagram_size: 0.0 }
    }
}

/// A layer name R12 accepts (letters, digits, _ - $), from a section name: "300 x 600 mm" →
/// "300x600", "H 300x150x6.5x9" → "H300x150x6p5x9".
pub fn section_layer(name: &str) -> String {
    let t = name.trim().trim_end_matches("mm").trim_end_matches("MM").trim();
    t.chars().filter_map(|c| match c {
        ' ' => None,
        '.' => Some('p'),
        c if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '$' => Some(c),
        '×' => Some('x'),
        _ => Some('-'),
    }).collect()
}

/// The section name of a layer's section part ("6p5" → "6.5").
fn layer_section(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    (0..c.len()).map(|k| if c[k] == 'p' && k > 0 && k + 1 < c.len() && c[k - 1].is_ascii_digit() && c[k + 1].is_ascii_digit() { '.' } else { c[k] }).collect()
}

fn kind_word(k: MemberKind) -> &'static str {
    match k {
        MemberKind::Column => "COLUMN",
        MemberKind::Brace => "BRACE",
        _ => "BEAM",
    }
}

struct Dxf {
    ent: String,
    layers: BTreeMap<String, i32>,
}

impl Dxf {
    fn layer(&mut self, name: &str, color: i32) -> String {
        self.layers.entry(name.to_string()).or_insert(color);
        name.to_string()
    }
    fn pair(&mut self, code: i32, v: impl std::fmt::Display) {
        let _ = write!(self.ent, "{code}\n{v}\n");
    }
    fn xyz(&mut self, base: i32, p: [f64; 3]) {
        self.pair(base, n(p[0]));
        self.pair(base + 10, n(p[1]));
        self.pair(base + 20, n(p[2]));
    }
    fn line(&mut self, layer: &str, a: [f64; 3], b: [f64; 3]) {
        self.pair(0, "LINE");
        self.pair(8, layer);
        self.xyz(10, a);
        self.xyz(11, b);
    }
    fn polyline(&mut self, layer: &str, pts: &[[f64; 3]], closed: bool) {
        self.pair(0, "POLYLINE");
        self.pair(8, layer);
        self.pair(66, 1);
        self.xyz(10, [0.0; 3]);
        self.pair(70, 8 | closed as i32);
        for p in pts {
            self.pair(0, "VERTEX");
            self.pair(8, layer);
            self.xyz(10, *p);
            self.pair(70, 32);
        }
        self.pair(0, "SEQEND");
        self.pair(8, layer);
    }
    fn text(&mut self, layer: &str, p: [f64; 3], h: f64, t: &str) {
        self.pair(0, "TEXT");
        self.pair(8, layer);
        self.xyz(10, p);
        self.pair(40, n(h));
        self.pair(1, t);
    }
}

fn n(x: f64) -> String {
    let x = if x.abs() < 1e-12 { 0.0 } else { x };
    let t = format!("{x:.6}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-" { "0".into() } else { t.into() }
}

/// The model as a DXF drawing (m).
pub fn export(model: &Model, a: Option<&Analysis>, opts: &DxfOptions) -> String {
    let v = opts.view;
    let mut d = Dxf { ent: String::new(), layers: BTreeMap::new() };
    d.layer("0", 7);
    let size = crate::scene::extent(model).map(|(lo, hi)| dist(lo, hi)).unwrap_or(10.0).max(1.0);
    let th = if opts.text_height > 0.0 { opts.text_height } else { 0.15 };
    for m in model.members.iter().filter(|m| m.role.is_frame()) {
        let (Some(pi), Some(pj)) = (model.node(m.i), model.node(m.j)) else { continue };
        let (a3, b3) = (pi.p(), pj.p());
        let sec = model.section(m.section).map(|s| s.name.clone()).unwrap_or_default();
        let kind = model.kind(m);
        if let DxfView::Plan { z } = v {
            // a column through the level: its outline
            if kind == MemberKind::Column && ((a3[2] - z).abs() < 0.01 || (b3[2] - z).abs() < 0.01) {
                if let (Some(s), Some((_, ey, ez))) = (model.section(m.section), crate::analysis::axes(model, m)) {
                    let (b, h) = match s.shape {
                        Shape::Rect { b, h } => (b, h),
                        Shape::H { d, bf, .. } => (bf, d),
                        Shape::LippedC { h, b, .. } => (b, h),
                        Shape::General { .. } => (0.2, 0.2),
                    };
                    let c = if (a3[2] - z).abs() < 0.01 { a3 } else { b3 };
                    let corner = |u: f64, w: f64| v.project([c[0] + ey[0] * u * b / 2.0 + ez[0] * w * h / 2.0, c[1] + ey[1] * u * b / 2.0 + ez[1] * w * h / 2.0, z]);
                    let l = d.layer("COLUMNS_PLAN", 4);
                    d.polyline(&l, &[corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)], true);
                }
            }
        }
        if !v.shows(a3, b3) {
            continue;
        }
        let layer = d.layer(&format!("{}_{}", kind_word(kind), section_layer(&sec)), match kind {
            MemberKind::Column => 1,
            MemberKind::Brace => 3,
            _ => 5,
        });
        let (pa, pb) = (v.project(a3), v.project(b3));
        d.line(&layer, pa, pb);
        if opts.labels {
            let l = d.layer("LABELS", 8);
            let mid = [(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0, (pa[2] + pb[2]) / 2.0];
            d.text(&l, mid, th, &format!("M{} {}", m.id, sec));
        }
    }
    for nd in &model.nodes {
        let used: Vec<&Member> = model.members.iter().filter(|m| m.i == nd.id || m.j == nd.id).collect();
        let seen = used.is_empty() || used.iter().any(|m| match (model.node(m.i), model.node(m.j)) {
            (Some(a), Some(b)) => v.shows(a.p(), b.p()),
            _ => false,
        }) || matches!(v, DxfView::Plan { z } if (nd.z - z).abs() < 0.01);
        if !seen || !v.shows(nd.p(), nd.p()) {
            continue;
        }
        let l = d.layer("NODES", 2);
        d.pair(0, "POINT");
        d.pair(8, &l);
        d.xyz(10, v.project(nd.p()));
        if model.supports.iter().any(|s| s.node == nd.id) {
            let l = d.layer("SUPPORTS", 6);
            let s = size * 0.015;
            let c = nd.p();
            let tri = match v {
                DxfView::Plan { .. } => [[c[0] - s, c[1] - s, c[2]], [c[0] + s, c[1] - s, c[2]], [c[0], c[1] + s, c[2]]],
                DxfView::ElevationYz { .. } => [[c[0], c[1] - s, c[2] - s], [c[0], c[1] + s, c[2] - s], c],
                _ => [[c[0] - s, c[1], c[2] - s], [c[0] + s, c[1], c[2] - s], c],
            };
            for k in 0..3 {
                d.line(&l, v.project(tri[k]), v.project(tri[(k + 1) % 3]));
            }
        }
    }
    for s in &model.slabs {
        let shown = match v {
            DxfView::Model3d => true,
            DxfView::Plan { z } => (s.z - z).abs() < 0.01,
            _ => false,
        };
        if !shown {
            continue;
        }
        let l = d.layer(&format!("SLAB_{:.0}", s.h * 1000.0), 9);
        let (lo, hi) = (s.lo(), s.hi());
        let c = [[lo.0, lo.1, s.z], [hi.0, lo.1, s.z], [hi.0, hi.1, s.z], [lo.0, hi.1, s.z]].map(|p| v.project(p));
        d.pair(0, "3DFACE");
        d.pair(8, &l);
        for (k, p) in c.iter().enumerate() {
            d.xyz(10 + k as i32, *p);
        }
        if opts.labels {
            let lb = d.layer("LABELS", 8);
            d.text(&lb, v.project([(lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0, s.z]), th, &format!("{} h {:.0}", s.name, s.h * 1000.0));
        }
    }
    // walls: the outline of the wall's face (3D, elevation) or of its plan (a plan view)
    for wl in &model.walls {
        let l = d.layer(&format!("WALL_{:.0}", wl.t * 1000.0), 5);
        let lw = wl.length().max(1e-9);
        let n = [-(wl.b[1] - wl.a[1]) / lw * wl.t / 2.0, (wl.b[0] - wl.a[0]) / lw * wl.t / 2.0];
        let pts: Vec<[f64; 3]> = match opts.view {
            DxfView::Plan { z } if z > wl.z0 - 1e-6 && z < wl.z1 + 1e-6 => vec![[wl.a[0] + n[0], wl.a[1] + n[1], z], [wl.b[0] + n[0], wl.b[1] + n[1], z], [wl.b[0] - n[0], wl.b[1] - n[1], z], [wl.a[0] - n[0], wl.a[1] - n[1], z]],
            DxfView::Plan { .. } => continue,
            _ => vec![[wl.a[0], wl.a[1], wl.z0], [wl.b[0], wl.b[1], wl.z0], [wl.b[0], wl.b[1], wl.z1], [wl.a[0], wl.a[1], wl.z1]],
        };
        let pts: Vec<[f64; 3]> = pts.into_iter().map(|p| v.project(p)).collect();
        d.polyline(&l, &pts, true);
        if opts.labels {
            let lb = d.layer("LABELS", 8);
            d.text(&lb, v.project([(wl.a[0] + wl.b[0]) / 2.0, (wl.a[1] + wl.b[1]) / 2.0, wl.z0]), th, &format!("{} t {:.0}", wl.name, wl.t * 1000.0));
        }
    }
    // the moment diagram of a result, drawn on the members' tension side
    if let (Some(a), true) = (a, opts.diagram) {
        let spec = crate::scene::Spec { result: opts.result.clone(), ..Default::default() };
        if let Some(res) = crate::scene::pick_result(model, a, &spec) {
            let mmax = res.members.iter().flat_map(|r| r.stations.iter().map(|s| s.my.abs())).fold(0.0, f64::max);
            if mmax > 1e-9 {
                let k = if opts.diagram_size > 0.0 { opts.diagram_size } else { size * 0.1 } / mmax;
                let l = d.layer("DIAGRAM_MY", 30);
                for r in &res.members {
                    let Some(m) = model.member(r.member) else { continue };
                    let (Some(pi), Some(pj), Some((ex, _, ez))) = (model.node(m.i), model.node(m.j), crate::analysis::axes(model, m)) else { continue };
                    if !v.shows(pi.p(), pj.p()) {
                        continue;
                    }
                    let at = |x: f64, off: f64| v.project([0, 1, 2].map(|q| pi.p()[q] + ex[q] * x - ez[q] * off));
                    let mut pts = vec![at(0.0, 0.0)];
                    pts.extend(r.stations.iter().map(|s| at(s.x, s.my * k)));
                    pts.push(at(model.length(m), 0.0));
                    d.polyline(&l, &pts, false);
                }
                let lb = d.layer("LABELS", 8);
                d.text(&lb, v.project(crate::scene::extent(model).map(|e| e.1).unwrap_or([0.0; 3])), th, &format!("My of {} (max {:.1} kNm)", res.name, mmax));
            }
        }
    }
    // the file
    let mut o = String::new();
    let mut p = |c: i32, v: &str| {
        let _ = write!(o, "{c}\n{v}\n");
    };
    p(0, "SECTION");
    p(2, "HEADER");
    p(9, "$ACADVER");
    p(1, "AC1009");
    p(9, "$INSUNITS");
    p(70, "6");
    p(0, "ENDSEC");
    p(0, "SECTION");
    p(2, "TABLES");
    p(0, "TABLE");
    p(2, "LTYPE");
    p(70, "1");
    p(0, "LTYPE");
    p(2, "CONTINUOUS");
    p(70, "0");
    p(3, "Solid line");
    p(72, "65");
    p(73, "0");
    p(40, "0.0");
    p(0, "ENDTAB");
    p(0, "TABLE");
    p(2, "LAYER");
    p(70, &d.layers.len().to_string());
    for (name, color) in &d.layers {
        p(0, "LAYER");
        p(2, name);
        p(70, "0");
        p(62, &color.to_string());
        p(6, "CONTINUOUS");
    }
    p(0, "ENDTAB");
    p(0, "ENDSEC");
    p(0, "SECTION");
    p(2, "ENTITIES");
    o.push_str(&d.ent);
    let mut p = |c: i32, v: &str| {
        let _ = write!(o, "{c}\n{v}\n");
    };
    p(0, "ENDSEC");
    p(0, "EOF");
    o
}

// ---------------------------------------------------------------- import

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DxfImport {
    /// Drawing units to m (0.001 for a drawing in mm); None: from $INSUNITS, else m.
    pub scale: Option<f64>,
    /// Where a 2D drawing sits in the model (a plan at z, an elevation at y or x); 3D:
    /// drawing coordinates are model coordinates.
    #[serde(flatten)]
    pub view: DxfView,
    /// The section of members on layers that name none (e.g. layer "0").
    pub default_section: String,
    /// Ends closer than this merge, m.
    pub tol: f64,
    /// The superimposed dead and live loads of imported slabs, kPa.
    pub slab_dead: f64,
    pub slab_live: f64,
}

impl Default for DxfImport {
    fn default() -> Self {
        DxfImport { scale: None, view: DxfView::Model3d, default_section: "300 x 300".into(), tol: 0.001, slab_dead: 1.5, slab_live: 2.0 }
    }
}

const RESERVED: [&str; 6] = ["NODES", "SUPPORTS", "LABELS", "COLUMNS_PLAN", "DEFPOINTS", "DIAGRAM_MY"];

/// The DXF's entities as (type, its group pairs), the ENTITIES section only.
fn entities(text: &str) -> Result<(Vec<(String, Vec<(i32, String)>)>, Option<i32>), String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 2 {
        return Err("not a DXF file: too short".into());
    }
    let mut pairs = Vec::with_capacity(lines.len() / 2);
    let mut k = 0;
    while k + 1 < lines.len() {
        let code: i32 = lines[k].trim().parse().map_err(|_| format!("not an ASCII DXF: line {} is no group code ('{}')", k + 1, lines[k].trim()))?;
        pairs.push((code, lines[k + 1].trim().to_string()));
        k += 2;
    }
    let mut insunits = None;
    let mut section = String::new();
    let mut out: Vec<(String, Vec<(i32, String)>)> = vec![];
    let mut k = 0;
    while k < pairs.len() {
        let (c, v) = &pairs[k];
        if *c == 0 && v == "SECTION" {
            if let Some((2, name)) = pairs.get(k + 1).cloned() {
                section = name;
            }
            k += 2;
            continue;
        }
        if *c == 0 && v == "ENDSEC" {
            section.clear();
            k += 1;
            continue;
        }
        if section == "HEADER" && *c == 9 && v == "$INSUNITS" {
            insunits = pairs.get(k + 1).and_then(|p| p.1.parse().ok());
        }
        if section == "ENTITIES" && *c == 0 {
            out.push((v.clone(), vec![]));
        } else if section == "ENTITIES" {
            if let Some(e) = out.last_mut() {
                e.1.push((*c, v.clone()));
            }
        }
        k += 1;
    }
    if out.is_empty() && !pairs.iter().any(|p| p.0 == 0 && p.1 == "ENTITIES") {
        return Err("no ENTITIES section".into());
    }
    Ok((out, insunits))
}

fn get(g: &[(i32, String)], code: i32) -> f64 {
    g.iter().find(|p| p.0 == code).and_then(|p| p.1.parse().ok()).unwrap_or(0.0)
}
fn layer_of(g: &[(i32, String)]) -> String {
    g.iter().find(|p| p.0 == 8).map(|p| p.1.clone()).unwrap_or_else(|| "0".into())
}

/// Reads a DXF drawing (see the module notes).
pub fn import(text: &str, opts: &DxfImport) -> Result<ImportResult, String> {
    let (ents, insunits) = entities(text)?;
    let scale = opts.scale.unwrap_or(match insunits {
        Some(1) => 0.0254,
        Some(2) => 0.3048,
        Some(4) => 0.001,
        Some(5) => 0.01,
        _ => 1.0,
    });
    let mut r = ImportResult { fallback_section: opts.default_section.clone(), tol: opts.tol, ..Default::default() };
    let v = opts.view;
    let place = |q: [f64; 3]| v.unproject(q.map(|x| x * scale));
    let mut segs: Vec<(String, [f64; 3], [f64; 3])> = vec![];
    let mut faces: Vec<(String, Vec<[f64; 3]>)> = vec![];
    let mut k = 0;
    while k < ents.len() {
        let (t, g) = (&ents[k].0, &ents[k].1);
        *r.counts.entry(t.clone()).or_default() += 1;
        let layer = layer_of(g);
        match t.as_str() {
            "LINE" => segs.push((layer, [get(g, 10), get(g, 20), get(g, 30)], [get(g, 11), get(g, 21), get(g, 31)])),
            "LWPOLYLINE" => {
                let elev = get(g, 38);
                let closed = get(g, 70) as i32 & 1 == 1;
                let mut pts: Vec<[f64; 3]> = vec![];
                for (c, val) in g {
                    let x: f64 = val.parse().unwrap_or(0.0);
                    match c {
                        10 => pts.push([x, 0.0, elev]),
                        20 => {
                            if let Some(p) = pts.last_mut() {
                                p[1] = x;
                            }
                        }
                        _ => {}
                    }
                }
                poly(&mut segs, &mut faces, &layer, pts, closed);
            }
            "POLYLINE" => {
                let flags = get(g, 70) as i32;
                let elev = get(g, 30);
                let three_d = flags & 8 != 0;
                let mut pts = vec![];
                k += 1;
                while k < ents.len() && ents[k].0 == "VERTEX" {
                    let vg = &ents[k].1;
                    pts.push([get(vg, 10), get(vg, 20), if three_d { get(vg, 30) } else { elev }]);
                    k += 1;
                }
                if k < ents.len() && ents[k].0 == "SEQEND" {
                    k += 1;
                }
                if flags & (16 | 64) != 0 {
                    r.warnings.push("a polygon mesh or polyface skipped".into());
                } else {
                    poly(&mut segs, &mut faces, &layer, pts, flags & 1 == 1);
                }
                continue;
            }
            "3DFACE" => {
                let mut pts: Vec<[f64; 3]> = (0..4).map(|q| [get(g, 10 + q), get(g, 20 + q), get(g, 30 + q)]).collect();
                if dist(pts[2], pts[3]) < 1e-9 {
                    pts.pop();
                }
                faces.push((layer, pts));
            }
            "INSERT" => r.warnings.push("a block reference (INSERT) is not exploded: explode blocks in CAD first".into()),
            _ => {}
        }
        k += 1;
    }
    for (layer, a, b) in segs {
        let up = layer.to_uppercase();
        if RESERVED.contains(&up.as_str()) || up.starts_with("SLAB_") || up.starts_with("DIAGRAM") {
            continue;
        }
        let (kind, section) = match up.split_once('_') {
            Some(("BEAM", s)) => (MemberKind::Beam, layer_section(&layer[layer.len() - s.len()..])),
            Some(("COLUMN", s)) => (MemberKind::Column, layer_section(&layer[layer.len() - s.len()..])),
            Some(("BRACE", s)) => (MemberKind::Brace, layer_section(&layer[layer.len() - s.len()..])),
            _ => (MemberKind::Auto, String::new()),
        };
        let section = if section.is_empty() { opts.default_section.clone() } else { section };
        r.members.push(ImportedMember { from: place(a), to: place(b), kind, section, material: None, roll: 0.0, bim_id: None, name: None });
    }
    for (layer, pts) in faces {
        let up = layer.to_uppercase();
        let Some(h) = up.strip_prefix("SLAB_").and_then(|h| h.replace('P', ".").parse::<f64>().ok()) else { continue };
        let pts: Vec<[f64; 3]> = pts.into_iter().map(place).collect();
        let z = pts.iter().map(|p| p[2]).sum::<f64>() / pts.len() as f64;
        let name = format!("S{}", r.slabs.len() + 1);
        r.slabs.push(ImportedSlab { outline: pts.iter().map(|p| [p[0], p[1]]).collect(), z, h: h / 1000.0, dead: opts.slab_dead, live: opts.slab_live, bim_id: None, name });
    }
    if r.members.is_empty() && r.slabs.is_empty() {
        r.warnings.push("no lines or polylines on member layers".into());
    }
    Ok(r)
}

fn poly(segs: &mut Vec<(String, [f64; 3], [f64; 3])>, faces: &mut Vec<(String, Vec<[f64; 3]>)>, layer: &str, pts: Vec<[f64; 3]>, closed: bool) {
    if layer.to_uppercase().starts_with("SLAB_") {
        if pts.len() >= 3 {
            faces.push((layer.to_string(), pts));
        }
        return;
    }
    for w in pts.windows(2) {
        segs.push((layer.to_string(), w[0], w[1]));
    }
    if closed && pts.len() > 2 {
        segs.push((layer.to_string(), pts[pts.len() - 1], pts[0]));
    }
}
