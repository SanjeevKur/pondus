// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Geometry editing of members, as the agent and the editor tools do it: move or array-copy
//! by a vector, rotate about a vertical axis, mirror about a plane, split a member and merge
//! two collinear members back into one.
//!
//! A move (no copy) moves the nodes of the chosen members, so members attached to them but not
//! chosen stretch to follow; a moved node landing on another node is merged with it. A copy
//! makes new members (new ids, no BIM id) whose ends are merged with any node already at their
//! place, so copied bays join the frame. Member loads go with a moved member and are copied
//! with a copied one; the supports and nodal loads of the chosen members' nodes are copied to
//! nodes the copy makes (not to existing nodes, which keep theirs). Load vectors and the
//! sections' roll turn with a rotation or mirror (a mirror flips handedness, so a lipped
//! channel would open the other way: its roll is kept as the closest one, and noted). A slab
//! follows (move) or is copied when every edge lies on chosen members and the transformed
//! panel is still a level rectangle with sides along x and y (a translation, a rotation by a
//! multiple of 90° or a mirror about a plane x = c or y = c); other slabs are left where they
//! are and listed in the notes. Diaphragm levels, stages and the moving-load path refer to
//! ids and are kept; split and merge update them.

use serde::Serialize;

use crate::model::*;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Edit {
    /// The members made (copies, split pieces) or changed (moved, merged).
    pub members: Vec<Id>,
    pub new_nodes: usize,
    pub slabs: Vec<Id>,
    pub notes: Vec<String>,
}

/// An affine map x → L x + t; `det` is −1 for a mirror.
#[derive(Clone, Copy, Debug)]
pub struct Xform {
    pub l: [[f64; 3]; 3],
    pub t: [f64; 3],
}

impl Xform {
    pub fn translate(d: [f64; 3]) -> Xform {
        Xform { l: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: d }
    }
    /// A rotation by `deg` (counter-clockwise seen from above) about the vertical through `c`.
    pub fn rotate_z(c: [f64; 3], deg: f64) -> Xform {
        let (s, co) = deg.to_radians().sin_cos();
        let (s, co) = (snap(s), snap(co));
        let l = [[co, -s, 0.0], [s, co, 0.0], [0.0, 0.0, 1.0]];
        let lc = mul(&l, c);
        Xform { l, t: [c[0] - lc[0], c[1] - lc[1], c[2] - lc[2]] }
    }
    /// A mirror about the plane through `p` with normal `n`.
    pub fn mirror(p: [f64; 3], n: [f64; 3]) -> Result<Xform, String> {
        let m = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if m < 1e-12 {
            return Err("the mirror plane needs a normal".into());
        }
        let n = n.map(|x| x / m);
        let mut l = [[0.0; 3]; 3];
        for (i, row) in l.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = snap(if i == j { 1.0 } else { 0.0 } - 2.0 * n[i] * n[j]);
            }
        }
        let lp = mul(&l, p);
        Ok(Xform { l, t: [p[0] - lp[0], p[1] - lp[1], p[2] - lp[2]] })
    }
    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let q = mul(&self.l, p);
        [q[0] + self.t[0], q[1] + self.t[1], q[2] + self.t[2]]
    }
    pub fn det(&self) -> f64 {
        let l = &self.l;
        l[0][0] * (l[1][1] * l[2][2] - l[1][2] * l[2][1]) - l[0][1] * (l[1][0] * l[2][2] - l[1][2] * l[2][0]) + l[0][2] * (l[1][0] * l[2][1] - l[1][1] * l[2][0])
    }
    fn is_identity_l(&self) -> bool {
        (0..3).all(|i| (0..3).all(|j| (self.l[i][j] - if i == j { 1.0 } else { 0.0 }).abs() < 1e-12))
    }
    /// A force and a moment (a pseudo-vector: times det under a mirror).
    fn force(&self, f: [f64; 3]) -> [f64; 3] {
        mul(&self.l, f)
    }
    fn moment(&self, m: [f64; 3]) -> [f64; 3] {
        let d = self.det().signum();
        mul(&self.l, m).map(|x| x * d)
    }
    fn six(&self, f: [f64; 6]) -> [f64; 6] {
        let a = self.force([f[0], f[1], f[2]]);
        let b = self.moment([f[3], f[4], f[5]]);
        [a[0], a[1], a[2], b[0], b[1], b[2]]
    }
}

fn snap(x: f64) -> f64 {
    for v in [0.0, 1.0, -1.0, 0.5, -0.5] {
        if (x - v).abs() < 1e-12 {
            return v;
        }
    }
    x
}

fn mul(l: &[[f64; 3]; 3], p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| l[i][0] * p[0] + l[i][1] * p[1] + l[i][2] * p[2])
}

fn round_p(p: [f64; 3]) -> [f64; 3] {
    p.map(|x| (x * 1e9).round() / 1e9)
}

/// Options of a transform: copy (else move), how many copies (an array: copy k at the k-th
/// power of the map), and what goes with a copy.
#[derive(Clone, Debug, Serialize)]
pub struct Opts {
    pub copy: bool,
    pub n: usize,
    pub loads: bool,
    pub supports: bool,
    pub slabs: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { copy: false, n: 1, loads: true, supports: true, slabs: true }
    }
}

/// Moves the members by `d`, or copies them `n` times at d, 2d, ... (an array).
pub fn move_members(model: &mut Model, ids: &[Id], d: [f64; 3], copy: bool, n: usize) -> Result<Edit, String> {
    transform_members(model, ids, &Xform::translate(d), &Opts { copy, n: n.max(1), ..Default::default() })
}

/// Rotates (or copies rotated) the members by `deg` about the vertical through `centre`.
pub fn rotate_members(model: &mut Model, ids: &[Id], centre: [f64; 3], deg: f64, copy: bool) -> Result<Edit, String> {
    transform_members(model, ids, &Xform::rotate_z(centre, deg), &Opts { copy, ..Default::default() })
}

/// Mirrors (or copies mirrored) the members about the plane through `point` with `normal`.
pub fn mirror_members(model: &mut Model, ids: &[Id], point: [f64; 3], normal: [f64; 3], copy: bool) -> Result<Edit, String> {
    transform_members(model, ids, &Xform::mirror(point, normal)?, &Opts { copy, ..Default::default() })
}

/// The roll that gives the transformed member the transformed section axes (the section is
/// taken as symmetric, so its local y may come out reversed).
fn new_roll(old: (&[f64; 3], &[f64; 3], f64), new: (&[f64; 3], &[f64; 3]), x: &Xform) -> f64 {
    use dedaliano_engine::element::compute_local_axes_3d;
    let ax = |a: &[f64; 3], b: &[f64; 3], r: f64| compute_local_axes_3d(a[0], a[1], a[2], b[0], b[1], b[2], None, None, None, (r != 0.0).then_some(r), false);
    let (_, ey, _) = ax(old.0, old.1, old.2);
    let v = x.force(ey);
    let (_, ey0, ez0) = ax(new.0, new.1, 0.0);
    let ang = (v[0] * ez0[0] + v[1] * ez0[1] + v[2] * ez0[2]).atan2(v[0] * ey0[0] + v[1] * ey0[1] + v[2] * ey0[2]).to_degrees();
    // the engine's sense of roll: whichever of ±ang lines local y up with v
    let fit = |r: f64| {
        let (_, e, _) = ax(new.0, new.1, r);
        (e[0] * v[0] + e[1] * v[1] + e[2] * v[2]).abs()
    };
    let mut r = if fit(ang) >= fit(-ang) { ang } else { -ang };
    // a symmetric section: ±180° is the same, keep within (−90, 90]
    while r > 90.0 + 1e-9 {
        r -= 180.0;
    }
    while r <= -90.0 + 1e-9 {
        r += 180.0;
    }
    let r = (r * 1e6).round() / 1e6;
    if r.abs() < 1e-6 { 0.0 } else { r }
}

fn on_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> bool {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    if l2 < 1e-18 {
        return false;
    }
    let t = ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1] + (p[2] - a[2]) * ab[2]) / l2;
    if !(-1e-6..=1.0 + 1e-6).contains(&t) {
        return false;
    }
    dist(p, [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]]) < TOL
}

/// The slabs whose four edges lie on the given members.
fn slabs_on(model: &Model, ids: &[Id]) -> Vec<Id> {
    let segs: Vec<([f64; 3], [f64; 3])> = model.members.iter().filter(|m| ids.contains(&m.id)).filter_map(|m| Some((model.node(m.i)?.p(), model.node(m.j)?.p()))).collect();
    let covered = |p: [f64; 3]| segs.iter().any(|(a, b)| on_segment(p, *a, *b));
    model
        .slabs
        .iter()
        .filter(|s| {
            crate::slabload::edges(s).iter().all(|(a, b)| {
                let (a, b) = ([a[0], a[1], s.z], [b[0], b[1], s.z]);
                // the ends, the quarter points and the middle of each edge
                [0.0, 0.25, 0.5, 0.75, 1.0].iter().all(|t| covered([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), s.z]))
            })
        })
        .map(|s| s.id)
        .collect()
}

/// A slab's panel after the map, when it is still a level rectangle along x and y.
fn moved_slab(s: &Slab, x: &Xform) -> Option<Slab> {
    let c: Vec<[f64; 3]> = [(s.x0, s.y0), (s.x1, s.y0), (s.x1, s.y1), (s.x0, s.y1)].iter().map(|(a, b)| round_p(x.apply([*a, *b, s.z]))).collect();
    if c.iter().any(|p| (p[2] - c[0][2]).abs() > TOL) {
        return None;
    }
    let (x0, x1) = (c.iter().map(|p| p[0]).fold(f64::MAX, f64::min), c.iter().map(|p| p[0]).fold(f64::MIN, f64::max));
    let (y0, y1) = (c.iter().map(|p| p[1]).fold(f64::MAX, f64::min), c.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
    let axis = c.iter().all(|p| ((p[0] - x0).abs() < TOL || (p[0] - x1).abs() < TOL) && ((p[1] - y0).abs() < TOL || (p[1] - y1).abs() < TOL));
    axis.then(|| Slab { x0, y0, x1, y1, z: c[0][2], ..s.clone() })
}

/// Applies a map to members: moves them, or makes `n` copies (see the module notes).
pub fn transform_members(model: &mut Model, ids: &[Id], x: &Xform, o: &Opts) -> Result<Edit, String> {
    let mut sel: Vec<Id> = ids.to_vec();
    sel.sort_unstable();
    sel.dedup();
    if sel.is_empty() {
        return Err("no members chosen".into());
    }
    for id in &sel {
        model.member(*id).ok_or(format!("no member {id}"))?;
    }
    let mut edit = Edit::default();
    let slab_ids = if o.slabs { slabs_on(model, &sel) } else { vec![] };
    let nodes: Vec<Id> = {
        let mut v: Vec<Id> = model.members.iter().filter(|m| sel.contains(&m.id)).flat_map(|m| [m.i, m.j]).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    if x.det() < 0.0 && model.members.iter().any(|m| sel.contains(&m.id) && matches!(model.section(m.section).map(|s| &s.shape), Some(Shape::LippedC { .. }))) {
        edit.notes.push("a mirrored lipped channel keeps its roll: it opens the same way, not mirrored; turn it with roll if it must".into());
    }
    if !o.copy {
        // the new places first, then merges with the nodes already there
        let place: Vec<(Id, [f64; 3])> = nodes.iter().map(|n| (*n, round_p(x.apply(model.node(*n).unwrap().p())))).collect();
        let old: Vec<(Id, [f64; 3])> = nodes.iter().map(|n| (*n, model.node(*n).unwrap().p())).collect();
        let old_of = |id: Id| old.iter().find(|o| o.0 == id).unwrap().1;
        // rolls from the old geometry
        let rolls: Vec<(Id, f64)> = model
            .members
            .iter()
            .filter(|m| sel.contains(&m.id))
            .map(|m| {
                let (a, b) = (old_of(m.i), old_of(m.j));
                let (na, nb) = (place.iter().find(|p| p.0 == m.i).unwrap().1, place.iter().find(|p| p.0 == m.j).unwrap().1);
                (m.id, if x.is_identity_l() { m.roll } else { new_roll((&a, &b, m.roll), (&na, &nb), x) })
            })
            .collect();
        for (id, p) in &place {
            if let Some(nd) = model.nodes.iter_mut().find(|n| n.id == *id) {
                (nd.x, nd.y, nd.z) = (p[0], p[1], p[2]);
            }
        }
        for (id, r) in rolls {
            model.members.iter_mut().find(|m| m.id == id).unwrap().roll = r;
        }
        // merge each moved node with another node now at its place
        for (id, p) in &place {
            if let Some(other) = model.nodes.iter().find(|n| n.id != *id && dist(n.p(), *p) < TOL).map(|n| n.id) {
                merge_node(model, *id, other, &mut edit);
            }
        }
        // loads on the moved members and nodes turn with the map
        if !x.is_identity_l() {
            for l in model.loads.iter_mut() {
                match &mut l.kind {
                    LoadKind::Line { member, wi, wj, .. } if sel.contains(member) => {
                        *wi = x.force(*wi);
                        *wj = x.force(*wj);
                    }
                    LoadKind::Point { member, p, .. } if sel.contains(member) => *p = x.force(*p),
                    LoadKind::Nodal { node, f } if nodes.contains(node) => *f = x.six(*f),
                    LoadKind::Settlement { node, d } if nodes.contains(node) => *d = x.six(*d),
                    _ => {}
                }
            }
        }
        for sid in slab_ids {
            let s = model.slabs.iter().find(|s| s.id == sid).unwrap().clone();
            match moved_slab(&s, x) {
                Some(ns) => {
                    *model.slabs.iter_mut().find(|q| q.id == sid).unwrap() = ns;
                    edit.slabs.push(sid);
                }
                None => edit.notes.push(format!("slab {sid} stays: the moved panel is not a level rectangle along x and y")),
            }
        }
        model.members.retain(|m| {
            let ok = m.i != m.j;
            if !ok {
                edit.notes.push(format!("member {} vanished: its ends merged", m.id));
            }
            ok
        });
        edit.members = sel.iter().copied().filter(|id| model.member(*id).is_some()).collect();
        return Ok(edit);
    }
    // copies: copy k is the map applied k times
    let mut xk = *x;
    for k in 1..=o.n.max(1) {
        if k > 1 {
            xk = compose(x, &xk);
        }
        let mut made: std::collections::HashMap<Id, Id> = std::collections::HashMap::new();
        let mut fresh: Vec<Id> = vec![];
        for n in &nodes {
            let p = round_p(xk.apply(model.node(*n).unwrap().p()));
            let existed = model.find_node(p);
            let id = model.node_at(p);
            if existed.is_none() {
                fresh.push(*n);
                edit.new_nodes += 1;
            }
            made.insert(*n, id);
        }
        let mut idmap: Vec<(Id, Id)> = vec![];
        for mid in &sel {
            let m = model.member(*mid).unwrap().clone();
            let (ni, nj) = (made[&m.i], made[&m.j]);
            if ni == nj {
                edit.notes.push(format!("member {mid}: the copy has no length"));
                continue;
            }
            if model.members.iter().any(|q| (q.i == ni && q.j == nj) || (q.i == nj && q.j == ni)) {
                edit.notes.push(format!("member {mid}: a member is already where the copy goes"));
                continue;
            }
            let (a, b) = (model.node(m.i).unwrap().p(), model.node(m.j).unwrap().p());
            let (na, nb) = (model.node(ni).unwrap().p(), model.node(nj).unwrap().p());
            let roll = if xk.is_identity_l() { m.roll } else { new_roll((&a, &b, m.roll), (&na, &nb), &xk) };
            let id = model.next_member_id();
            model.members.push(Member { id, i: ni, j: nj, roll, bim_id: None, ..m });
            idmap.push((*mid, id));
            edit.members.push(id);
        }
        if o.loads {
            let mut add = vec![];
            for l in &model.loads {
                let kind = match &l.kind {
                    LoadKind::Line { member, wi, wj, a, b } => idmap.iter().find(|p| p.0 == *member).map(|p| LoadKind::Line { member: p.1, wi: xk.force(*wi), wj: xk.force(*wj), a: *a, b: *b }),
                    LoadKind::Point { member, a, p } => idmap.iter().find(|q| q.0 == *member).map(|q| LoadKind::Point { member: q.1, a: *a, p: xk.force(*p) }),
                    LoadKind::Thermal { member, dt, dt_y, dt_z } => idmap.iter().find(|q| q.0 == *member).map(|q| LoadKind::Thermal { member: q.1, dt: *dt, dt_y: *dt_y, dt_z: *dt_z }),
                    LoadKind::Nodal { node, f } if fresh.contains(node) => Some(LoadKind::Nodal { node: made[node], f: xk.six(*f) }),
                    _ => None,
                };
                if let Some(kind) = kind {
                    add.push(Load { case: l.case, kind });
                }
            }
            model.loads.extend(add);
        }
        if o.supports {
            let add: Vec<Support> = model.supports.iter().filter(|s| fresh.contains(&s.node)).map(|s| Support { node: made[&s.node], ..s.clone() }).collect();
            model.supports.extend(add);
        }
        for sid in &slab_ids {
            let s = model.slabs.iter().find(|s| s.id == *sid).unwrap().clone();
            match moved_slab(&s, &xk) {
                Some(ns) if !model.slabs.iter().any(|q| (q.x0.min(q.x1) - ns.x0).abs() < TOL && (q.y0.min(q.y1) - ns.y0).abs() < TOL && (q.x0.max(q.x1) - ns.x1).abs() < TOL && (q.y0.max(q.y1) - ns.y1).abs() < TOL && (q.z - ns.z).abs() < TOL) => {
                    let id = model.next_slab_id();
                    model.slabs.push(Slab { id, bim_id: None, name: if s.name.is_empty() { String::new() } else { format!("{} copy {k}", s.name) }, ..ns });
                    edit.slabs.push(id);
                }
                Some(_) => edit.notes.push(format!("slab {sid}: a slab is already where its copy {k} goes")),
                None => edit.notes.push(format!("slab {sid}: not copied, its copy is not a level rectangle along x and y")),
            }
        }
    }
    Ok(edit)
}

fn compose(a: &Xform, b: &Xform) -> Xform {
    // a after b: L = La Lb, t = La tb + ta
    let mut l = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            l[i][j] = (0..3).map(|k| a.l[i][k] * b.l[k][j]).sum();
        }
    }
    let t = mul(&a.l, b.t);
    Xform { l, t: [t[0] + a.t[0], t[1] + a.t[1], t[2] + a.t[2]] }
}

/// Replaces node `from` by `to` everywhere and removes it; a support at `from` goes over only
/// when `to` has none.
fn merge_node(model: &mut Model, from: Id, to: Id, edit: &mut Edit) {
    for m in model.members.iter_mut() {
        if m.i == from {
            m.i = to;
        }
        if m.j == from {
            m.j = to;
        }
    }
    let has = model.supports.iter().any(|s| s.node == to);
    if has {
        if model.supports.iter().any(|s| s.node == from) {
            edit.notes.push(format!("node {from} merged into {to}: {to}'s support kept"));
        }
        model.supports.retain(|s| s.node != from);
    } else {
        for s in model.supports.iter_mut().filter(|s| s.node == from) {
            s.node = to;
        }
    }
    for l in model.loads.iter_mut() {
        match &mut l.kind {
            LoadKind::Nodal { node, .. } | LoadKind::Settlement { node, .. } if *node == from => *node = to,
            _ => {}
        }
    }
    model.nodes.retain(|n| n.id != from);
}

/// Splits a member at fractions of its length (0 < t < 1, any order), or into `parts` equal
/// pieces when `at` is empty. The first piece keeps the member's id; the pieces keep its
/// section, material, kind, behaviour and BIM id, the end hinges stay at the outer ends, and
/// an unset unbraced length becomes the whole member's (so design is not made less safe).
/// Loads go to the piece they lie on: a line load over several pieces is cut at the nodes
/// with its intensity interpolated, a point load goes to the piece holding it, a thermal load
/// to every piece. Stages and a moving-load path that name the member name all its pieces.
pub fn split_member(model: &mut Model, id: Id, at: &[f64], parts: usize) -> Result<Edit, String> {
    let m = model.member(id).ok_or(format!("no member {id}"))?.clone();
    if m.axial_only() {
        // one element end to end in the analysis; a split adds a joint where the original had none
        if m.behaviour == Behaviour::Cable {
            return Err("a cable is one element from end to end: it is not split".into());
        }
    }
    let len = model.length(&m);
    let mut ts: Vec<f64> = if at.is_empty() {
        if parts < 2 {
            return Err("give the split points (fractions 0–1) or parts ≥ 2".into());
        }
        (1..parts).map(|k| k as f64 / parts as f64).collect()
    } else {
        at.to_vec()
    };
    ts.retain(|t| *t * len > TOL && (1.0 - *t) * len > TOL);
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|a, b| (*a - *b).abs() * len < TOL);
    if ts.is_empty() {
        return Err("no split point inside the member".into());
    }
    let (a, b) = (model.node(m.i).unwrap().p(), model.node(m.j).unwrap().p());
    let mut edit = Edit::default();
    let mut ends = vec![m.i];
    for t in &ts {
        let p = round_p([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])]);
        if model.find_node(p).is_none() {
            edit.new_nodes += 1;
        }
        ends.push(model.node_at(p));
    }
    ends.push(m.j);
    let s: Vec<f64> = std::iter::once(0.0).chain(ts.iter().map(|t| t * len)).chain(std::iter::once(len)).collect();
    let lb = Some(m.lb.unwrap_or(len));
    let mut pieces = vec![];
    for k in 0..ends.len() - 1 {
        let pid = if k == 0 { id } else { model.next_member_id() };
        let p = Member { id: pid, i: ends[k], j: ends[k + 1], hinge_i: k == 0 && m.hinge_i, hinge_j: k == ends.len() - 2 && m.hinge_j, lb, ..m.clone() };
        if k == 0 {
            *model.members.iter_mut().find(|q| q.id == id).unwrap() = p;
        } else {
            model.members.push(p);
        }
        pieces.push(pid);
    }
    // the loads
    let mut out = vec![];
    for l in std::mem::take(&mut model.loads) {
        match &l.kind {
            LoadKind::Line { member, wi, wj, a: la, b: lb } if *member == id => {
                let (x0, x1) = (la.unwrap_or(0.0), lb.unwrap_or(len));
                let w = |x: f64| -> [f64; 3] {
                    let f = if x1 - x0 > 1e-12 { (x - x0) / (x1 - x0) } else { 0.0 };
                    [0, 1, 2].map(|c| wi[c] + (wj[c] - wi[c]) * f)
                };
                for k in 0..pieces.len() {
                    let (lo, hi) = (x0.max(s[k]), x1.min(s[k + 1]));
                    if hi - lo <= 1e-9 {
                        continue;
                    }
                    let pl = s[k + 1] - s[k];
                    let (a2, b2) = (lo - s[k], hi - s[k]);
                    let whole = a2 < 1e-9 && (pl - b2) < 1e-9;
                    out.push(Load { case: l.case, kind: LoadKind::Line { member: pieces[k], wi: w(lo), wj: w(hi), a: (!whole).then_some(a2), b: (!whole).then_some(b2) } });
                }
            }
            LoadKind::Point { member, a: pa, p } if *member == id => {
                let k = (0..pieces.len()).find(|k| *pa < s[k + 1] - 1e-9).unwrap_or(pieces.len() - 1);
                out.push(Load { case: l.case, kind: LoadKind::Point { member: pieces[k], a: pa - s[k], p: *p } });
            }
            LoadKind::Thermal { member, dt, dt_y, dt_z } if *member == id => {
                for p in &pieces {
                    out.push(Load { case: l.case, kind: LoadKind::Thermal { member: *p, dt: *dt, dt_y: *dt_y, dt_z: *dt_z } });
                }
            }
            _ => out.push(l),
        }
    }
    model.loads = out;
    for st in model.stages.iter_mut().filter(|st| st.members.contains(&id)) {
        st.members.extend(pieces[1..].iter().copied());
    }
    if let Some(mv) = model.moving.as_mut() {
        if let Some(k) = mv.path.iter().position(|p| *p == id) {
            // in the direction the path runs over the member
            let rev = k > 0 && model.members.iter().find(|q| q.id == mv.path[k - 1]).is_some_and(|q| q.i == m.j || q.j == m.j);
            let mut ps = pieces.clone();
            if rev {
                ps.reverse();
            }
            mv.path.splice(k..=k, ps);
        }
    }
    if m.truss || m.behaviour != Behaviour::Normal {
        edit.notes.push("an axial-only member split adds pinned joints between its pieces: brace the new nodes or they are mechanisms".into());
    }
    edit.members = pieces;
    Ok(edit)
}

/// Merges two collinear members that share a node with nothing else at it (no other member,
/// support or nodal load, and no hinge there) and have the same section, material and
/// behaviour, into the first (its id and BIM id stay). The loads of both are remapped onto the
/// merged member; thermal loads must be the same on both.
pub fn merge_members(model: &mut Model, a: Id, b: Id) -> Result<Edit, String> {
    let ma = model.member(a).ok_or(format!("no member {a}"))?.clone();
    let mb = model.member(b).ok_or(format!("no member {b}"))?.clone();
    let shared = [ma.i, ma.j].into_iter().find(|n| *n == mb.i || *n == mb.j).ok_or(format!("members {a} and {b} share no node"))?;
    if model.members.iter().any(|m| m.id != a && m.id != b && (m.i == shared || m.j == shared)) {
        return Err(format!("another member frames into node {shared}"));
    }
    if model.supports.iter().any(|s| s.node == shared) {
        return Err(format!("node {shared} has a support"));
    }
    if model.loads.iter().any(|l| matches!(l.kind, LoadKind::Nodal { node, .. } | LoadKind::Settlement { node, .. } if node == shared)) {
        return Err(format!("node {shared} carries a nodal load"));
    }
    if ma.section != mb.section || ma.material != mb.material || ma.behaviour != mb.behaviour || ma.truss != mb.truss || ma.winkler != mb.winkler {
        return Err("the members differ in section, material or behaviour".into());
    }
    if (ma.i == shared && ma.hinge_i) || (ma.j == shared && ma.hinge_j) || (mb.i == shared && mb.hinge_i) || (mb.j == shared && mb.hinge_j) {
        return Err(format!("a hinge at node {shared} would be lost"));
    }
    let start = if ma.i == shared { ma.j } else { ma.i };
    let end = if mb.i == shared { mb.j } else { mb.i };
    let p = |n: Id| model.node(n).unwrap().p();
    let (ps, pm, pe) = (p(start), p(shared), p(end));
    let (la, lb_) = (dist(ps, pm), dist(pm, pe));
    let l = dist(ps, pe);
    if (la + lb_ - l).abs() > TOL {
        return Err(format!("members {a} and {b} are not collinear"));
    }
    let th = |id: Id| -> Vec<(Id, [u64; 3])> {
        let mut v: Vec<(Id, [u64; 3])> = model.loads.iter().filter_map(|x| if let LoadKind::Thermal { member, dt, dt_y, dt_z } = x.kind { (member == id).then_some((x.case, [dt.to_bits(), dt_y.to_bits(), dt_z.to_bits()])) } else { None }).collect();
        v.sort();
        v
    };
    if th(a) != th(b) {
        return Err("the members carry different thermal loads".into());
    }
    let a_rev = ma.i == shared; // a runs from the shared node: its x is measured from the far end
    let b_rev = mb.j == shared;
    let mut out = vec![];
    let mut kept_thermal = false;
    for ld in std::mem::take(&mut model.loads) {
        let map = |x: f64, from_a: bool| -> f64 {
            if from_a {
                if a_rev { la - x } else { x }
            } else if b_rev {
                la + (lb_ - x)
            } else {
                la + x
            }
        };
        match &ld.kind {
            LoadKind::Line { member, wi, wj, a: x0, b: x1 } if *member == a || *member == b => {
                let from_a = *member == a;
                let ml = if from_a { la } else { lb_ };
                let (u0, u1) = (map(x0.unwrap_or(0.0), from_a), map(x1.unwrap_or(ml), from_a));
                let (lo, hi, w0, w1) = if u0 <= u1 { (u0, u1, *wi, *wj) } else { (u1, u0, *wj, *wi) };
                let whole = lo < 1e-9 && (l - hi) < 1e-9;
                out.push(Load { case: ld.case, kind: LoadKind::Line { member: a, wi: w0, wj: w1, a: (!whole).then_some(lo), b: (!whole).then_some(hi) } });
            }
            LoadKind::Point { member, a: x, p } if *member == a || *member == b => {
                out.push(Load { case: ld.case, kind: LoadKind::Point { member: a, a: map(*x, *member == a), p: *p } });
            }
            LoadKind::Thermal { member, .. } if *member == a || *member == b => {
                if *member == a {
                    kept_thermal = true;
                    out.push(ld.clone());
                }
            }
            _ => out.push(ld),
        }
    }
    let _ = kept_thermal;
    model.loads = out;
    coalesce(model, a, l);
    let hinge_i = if ma.i == start { ma.hinge_i } else { ma.hinge_j };
    let hinge_j = if mb.j == end { mb.hinge_j } else { mb.hinge_i };
    let lbm = match (ma.lb, mb.lb) {
        (None, None) => None,
        (x, y) => {
            let v = x.unwrap_or(la).max(y.unwrap_or(lb_));
            ((v - l).abs() > TOL).then_some(v)
        }
    };
    let mut edit = Edit::default();
    if ma.bim_id != mb.bim_id && mb.bim_id.is_some() {
        edit.notes.push(format!("member {b}'s BIM id {:?} is dropped", mb.bim_id));
    }
    // the roll keeps a's section orientation when a is turned round
    let roll = if a_rev { new_roll((&pm, &ps, ma.roll), (&ps, &pe), &Xform::translate([0.0; 3])) } else { ma.roll };
    {
        let m = model.members.iter_mut().find(|m| m.id == a).unwrap();
        (m.i, m.j, m.hinge_i, m.hinge_j, m.lb, m.roll) = (start, end, hinge_i, hinge_j, lbm, roll);
    }
    model.members.retain(|m| m.id != b);
    model.nodes.retain(|n| n.id != shared);
    for st in model.stages.iter_mut() {
        if st.members.contains(&b) {
            st.members.retain(|x| *x != b);
            if !st.members.contains(&a) {
                st.members.push(a);
            }
        }
    }
    if let Some(mv) = model.moving.as_mut() {
        let has_a = mv.path.contains(&a);
        mv.path.retain(|x| *x != b || !has_a);
        for x in mv.path.iter_mut() {
            if *x == b {
                *x = a;
            }
        }
    }
    edit.members = vec![a];
    Ok(edit)
}

/// Joins the line loads of a member that meet end to end in one case with one linear
/// intensity (what a split cut apart) into one load again.
fn coalesce(model: &mut Model, id: Id, len: f64) {
    loop {
        let lines: Vec<(usize, Id, f64, f64, [f64; 3], [f64; 3])> = model
            .loads
            .iter()
            .enumerate()
            .filter_map(|(k, l)| match &l.kind {
                LoadKind::Line { member, wi, wj, a, b } if *member == id => Some((k, l.case, a.unwrap_or(0.0), b.unwrap_or(len), *wi, *wj)),
                _ => None,
            })
            .collect();
        let slope = |x0: f64, x1: f64, w0: [f64; 3], w1: [f64; 3]| [0, 1, 2].map(|c| (w1[c] - w0[c]) / (x1 - x0).max(1e-12));
        let found = lines.iter().find_map(|p| {
            lines.iter().find(|q| {
                q.0 != p.0 && q.1 == p.1 && (q.2 - p.3).abs() < 1e-9 && (0..3).all(|c| (q.4[c] - p.5[c]).abs() < 1e-9) && {
                    let (s1, s2) = (slope(p.2, p.3, p.4, p.5), slope(q.2, q.3, q.4, q.5));
                    (0..3).all(|c| (s1[c] - s2[c]).abs() < 1e-9)
                }
            }).map(|q| (*p, *q))
        });
        let Some((p, q)) = found else { return };
        let whole = p.2 < 1e-9 && (len - q.3) < 1e-9;
        model.loads[p.0].kind = LoadKind::Line { member: id, wi: p.4, wj: q.5, a: (!whole).then_some(p.2), b: (!whole).then_some(q.3) };
        model.loads.remove(q.0);
    }
}
