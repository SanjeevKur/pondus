// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Stabileo's model file (the web editor's fixture JSON, also what Aedifex's
//! `export_stabileo` writes): read into a Pondus model and written back. On reading, chains
//! of collinear elements of one section joined end to end at nodes nothing else uses are
//! merged into one member (Aedifex splits each beam in four; Pondus designs the beam), and an
//! element's `bimId` (or `name`) is kept so the design goes back to the BIM element.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::model::*;

pub fn is_stabileo(v: &Value) -> bool {
    let m = v.get("model").unwrap_or(v);
    m.get("elements").is_some_and(Value::is_array) && m.get("nodes").is_some_and(Value::is_array)
}

#[derive(Debug, Default, serde::Serialize)]
pub struct ImportReport {
    pub elements: usize,
    pub members: usize,
    pub merged: usize,
    pub skipped: Vec<String>,
}

fn num(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}
fn id(v: &Value, k: &str) -> Id {
    v.get(k).and_then(Value::as_u64).unwrap_or(0) as Id
}

/// Reads a Stabileo model (`{"model": ...}` or the model itself).
pub fn import(v: &Value) -> Result<(Model, ImportReport), String> {
    let v = v.get("model").unwrap_or(v);
    let mut rep = ImportReport::default();
    let mut m = Model::new(v.get("name").and_then(Value::as_str).unwrap_or("Stabileo model"));
    m.materials.clear();
    m.cases.clear();
    m.combos.clear();
    let nodes = v["nodes"].as_array().ok_or("no nodes")?;
    let three_d = nodes.iter().any(|n| n.get("z").is_some());
    for n in nodes {
        let (x, y, z) = if three_d { (num(n, "x"), num(n, "y"), num(n, "z")) } else { (num(n, "x"), 0.0, num(n, "y")) };
        m.nodes.push(Node { id: id(n, "id"), x, y, z });
    }
    for mt in v["materials"].as_array().into_iter().flatten() {
        let e = num(mt, "e");
        let fy = num(mt, "fy");
        let name = mt.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        let steel = e > 100_000.0 || name.to_lowercase().contains("steel") || name.to_lowercase().contains("acero");
        let gamma = mt.get("rho").and_then(Value::as_f64).unwrap_or(if steel { 77.0 } else { 24.0 });
        let mut x = if steel { Material::steel(id(mt, "id"), &name, if fy > 0.0 { fy } else { 245.0 }, num(mt, "fu").max(if fy > 0.0 { fy * 1.25 } else { 400.0 })) } else { Material::concrete(id(mt, "id"), num(mt, "fc").max(0.0)) };
        if !steel && x.fc == 0.0 {
            // f'c from E = 4700 √f'c
            x.fc = ((e / 4700.0).powi(2) * 10.0).round() / 10.0;
            x.name = format!("{name} (f'c {:.0} MPa from E)", x.fc);
        } else if !name.is_empty() {
            x.name = name;
        }
        x.e = e;
        x.nu = mt.get("nu").and_then(Value::as_f64).unwrap_or(x.nu);
        x.gamma = gamma;
        m.materials.push(x);
    }
    if m.materials.is_empty() {
        m.materials = Model::new("").materials;
    }
    let mut steel_sections = vec![];
    for s in v["sections"].as_array().into_iter().flatten() {
        let name = s.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        let sid = id(s, "id");
        let shape = if let Some(h) = crate::sections::find(name.rsplit(':').next().unwrap_or(&name).trim()) {
            h.shape()
        } else if s.get("b").is_some() && s.get("h").is_some() && (s.get("shape").and_then(Value::as_str) == Some("rect") || (num(s, "a") - num(s, "b") * num(s, "h")).abs() < 0.02 * num(s, "a").max(1e-9)) {
            Shape::Rect { b: num(s, "b"), h: num(s, "h") }
        } else {
            steel_sections.push(sid);
            Shape::General { a: num(s, "a"), iy: num(s, "iy"), iz: num(s, "iz"), j: num(s, "j"), shear: None }
        };
        m.sections.push(Section { id: sid, name, shape });
    }
    for c in v["loadCases"].as_array().into_iter().flatten() {
        let kind = match c.get("type").and_then(Value::as_str).unwrap_or("D") {
            "D" | "SD" => CaseKind::D,
            "L" => CaseKind::L,
            "Lr" | "LR" => CaseKind::Lr,
            "W" => CaseKind::W,
            "E" => CaseKind::E,
            _ => CaseKind::Other,
        };
        m.cases.push(LoadCase { id: id(c, "id"), name: c.get("name").and_then(Value::as_str).unwrap_or("case").into(), kind, self_weight: false });
    }
    for sw in v.pointer("/analysis/selfWeight").and_then(Value::as_array).into_iter().flatten() {
        let cid = id(sw, "caseId");
        if let Some(c) = m.cases.iter_mut().find(|c| c.id == cid) {
            c.self_weight = true;
        }
    }
    if m.cases.is_empty() {
        m.cases.push(LoadCase { id: 1, name: "D".into(), kind: CaseKind::D, self_weight: false });
    }
    for c in v["combinations"].as_array().into_iter().flatten() {
        let name = c.get("name").and_then(Value::as_str).unwrap_or("combination").to_string();
        let sls = name.to_uppercase().contains("SLS") || name.to_lowercase().contains("service");
        let factors = c["factors"].as_array().into_iter().flatten().map(|f| (id(f, "caseId"), num(f, "factor"))).collect();
        m.combos.push(Combination { id: id(c, "id"), name, kind: if sls { ComboKind::Sls } else { ComboKind::Uls }, factors });
    }
    // elements as members
    let mut elem_member: HashMap<Id, Id> = HashMap::new();
    for e in v["elements"].as_array().into_iter().flatten() {
        rep.elements += 1;
        if e.get("type").and_then(Value::as_str) == Some("truss") {
            rep.skipped.push(format!("element {} is a truss: read as a frame member with pinned ends", id(e, "id")));
        }
        let rel = |k: &str| e.get(k).is_some_and(|r| r.get("my").and_then(Value::as_bool).unwrap_or(false) || r.get("mz").and_then(Value::as_bool).unwrap_or(false));
        let truss = e.get("type").and_then(Value::as_str) == Some("truss");
        let bim = e.get("bimId").or_else(|| e.get("name")).map(|b| b.as_str().map(String::from).unwrap_or_else(|| b.to_string()));
        let mid = id(e, "id");
        elem_member.insert(mid, mid);
        m.members.push(Member {
            id: mid,
            i: id(e, "nodeI"),
            j: id(e, "nodeJ"),
            section: id(e, "sectionId"),
            material: id(e, "materialId"),
            kind: MemberKind::Auto,
            hinge_i: truss || e.get("hingeStart").and_then(Value::as_bool).unwrap_or(false) || rel("releaseI"),
            hinge_j: truss || e.get("hingeEnd").and_then(Value::as_bool).unwrap_or(false) || rel("releaseJ"),
            lb: None,
            k: None,
            roll: e.get("rollAngle").and_then(Value::as_f64).unwrap_or(0.0),
            truss: false,
            behaviour: Default::default(),
            winkler: None,
            rebar: None,
            bim_id: bim,
            tension: None, role: Default::default(),
            group: None,
        });
    }
    for s in v["supports"].as_array().into_iter().flatten() {
        let t = s.get("type").and_then(Value::as_str).unwrap_or("fixed");
        let n = id(s, "nodeId");
        let sup = if three_d {
            Support::of_kind(n, t)
        } else {
            // a plane frame in x-z: the out-of-plane freedoms are held at the supports
            match t {
                "fixed" => Some(Support { node: n, fix: [true; 6], springs: [0.0; 6], uplift: false }),
                "pinned" => Some(Support { node: n, fix: [true, true, true, true, false, true], springs: [0.0; 6], uplift: false }),
                "rollerX" => Some(Support { node: n, fix: [false, true, true, true, false, true], springs: [0.0; 6], uplift: false }),
                "rollerZ" | "rollerY" => Some(Support { node: n, fix: [true, true, false, true, false, true], springs: [0.0; 6], uplift: false }),
                _ => None,
            }
        };
        match sup {
            Some(x) => m.supports.push(x),
            None => rep.skipped.push(format!("support at node {n} of type '{t}' is not read")),
        }
    }
    let default_case = m.cases[0].id;
    for l in v["loads"].as_array().into_iter().flatten() {
        let t = l.get("type").and_then(Value::as_str).unwrap_or("");
        let d = &l["data"];
        let case = d.get("caseId").and_then(Value::as_u64).map(|c| c as Id).unwrap_or(default_case);
        match t {
            "nodal3d" => m.loads.push(Load { case, kind: LoadKind::Nodal { node: id(d, "nodeId"), f: [num(d, "fx"), num(d, "fy"), num(d, "fz"), num(d, "mx"), num(d, "my"), num(d, "mz")] } }),
            "nodal" => m.loads.push(Load { case, kind: LoadKind::Nodal { node: id(d, "nodeId"), f: [num(d, "fx"), 0.0, num(d, "fz") + num(d, "fy"), 0.0, -num(d, "mz") - num(d, "my"), 0.0] } }),
            "distributed3d" | "distributed" => {
                let eid = id(d, "elementId");
                let Some(mem) = m.member(eid).cloned() else { continue };
                let global = d.get("frame").and_then(Value::as_str) == Some("global");
                let (wi, wj) = if t == "distributed" {
                    // a plane frame's load acts normal to the member in its plane (local z here)
                    let (_, _, ez) = crate::analysis::axes(&m, &mem).ok_or("bad element")?;
                    let (qi, qj) = (num(d, "qI"), num(d, "qJ"));
                    (ez.map(|x| x * qi), ez.map(|x| x * qj))
                } else if global {
                    ([num(d, "qXI"), num(d, "qYI"), num(d, "qZI")], [num(d, "qXJ"), num(d, "qYJ"), num(d, "qZJ")])
                } else {
                    let (ex, ey, ez) = crate::analysis::axes(&m, &mem).ok_or("bad element")?;
                    let g = |qx: f64, qy: f64, qz: f64| [0, 1, 2].map(|k| qx * ex[k] + qy * ey[k] + qz * ez[k]);
                    (g(num(d, "qXI"), num(d, "qYI"), num(d, "qZI")), g(num(d, "qXJ"), num(d, "qYJ"), num(d, "qZJ")))
                };
                m.loads.push(Load { case, kind: LoadKind::Line { member: eid, wi, wj, a: d.get("a").and_then(Value::as_f64), b: d.get("b").and_then(Value::as_f64) } });
            }
            "pointOnElement3d" | "pointOnElement" => {
                let eid = id(d, "elementId");
                let Some(mem) = m.member(eid).cloned() else { continue };
                let (_, ey, ez) = crate::analysis::axes(&m, &mem).ok_or("bad element")?;
                let (py, pz) = if t == "pointOnElement" { (0.0, num(d, "p")) } else { (num(d, "py"), num(d, "pz")) };
                m.loads.push(Load { case, kind: LoadKind::Point { member: eid, a: num(d, "a"), p: [0, 1, 2].map(|k| py * ey[k] + pz * ez[k]) } });
            }
            other => rep.skipped.push(format!("load of type '{other}' is not read")),
        }
    }
    if v["plates"].as_array().is_some_and(|a| !a.is_empty()) || v["quads"].as_array().is_some_and(|a| !a.is_empty()) {
        rep.skipped.push("plates and shells are not read (Pondus models slabs as panels on beams)".into());
    }
    rep.merged = merge_chains(&mut m);
    m.purge_nodes();
    rep.members = m.members.len();
    // a steel member with a section of stiffness only: Pondus chooses a catalogue shape when it designs
    let _ = steel_sections;
    Ok((m, rep))
}

/// Merges collinear members joined end to end, of one section, material and BIM element, at
/// nodes with no other member, support or nodal load and no hinge. Line loads move onto the
/// merged member. Returns how many members were merged away.
pub fn merge_chains(m: &mut Model) -> usize {
    let mut merged = 0;
    loop {
        let mut uses: HashMap<Id, Vec<usize>> = HashMap::new();
        for (k, mm) in m.members.iter().enumerate() {
            uses.entry(mm.i).or_default().push(k);
            uses.entry(mm.j).or_default().push(k);
        }
        let held: std::collections::HashSet<Id> = m.supports.iter().map(|s| s.node).chain(m.loads.iter().filter_map(|l| if let LoadKind::Nodal { node, .. } = l.kind { Some(node) } else { None })).collect();
        let mut done = false;
        for (node, ks) in &uses {
            if ks.len() != 2 || held.contains(node) {
                continue;
            }
            let (a, b) = (m.members[ks[0]].clone(), m.members[ks[1]].clone());
            if a.section != b.section || a.material != b.material || a.bim_id != b.bim_id || a.kind != b.kind || a.roll != b.roll {
                continue;
            }
            // orient: a ends at the node, b starts there
            let a_end = if a.j == *node { a.i } else { a.j };
            let b_end = if b.i == *node { b.j } else { b.i };
            let hinge_at_node = (if a.j == *node { a.hinge_j } else { a.hinge_i }) || (if b.i == *node { b.hinge_i } else { b.hinge_j });
            if hinge_at_node {
                continue;
            }
            let (Some(p0), Some(p1), Some(p2)) = (m.node(a_end), m.node(*node), m.node(b_end)) else { continue };
            let (p0, p1, p2) = (p0.p(), p1.p(), p2.p());
            let la = dist(p0, p1);
            let lb = dist(p1, p2);
            let lt = dist(p0, p2);
            if (la + lb - lt).abs() > 1e-4 * lt.max(1.0) {
                continue;
            }
            let hinge_start = if a.i == a_end { a.hinge_i } else { a.hinge_j };
            let hinge_end = if b.j == b_end { b.hinge_j } else { b.hinge_i };
            // move the loads: positions measured from the new start (a_end)
            for l in m.loads.iter_mut() {
                match &mut l.kind {
                    LoadKind::Line { member, wi, wj, a: la_, b: lb_ } if *member == a.id || *member == b.id => {
                        let (len, offset, flip) = if *member == a.id { (la, 0.0, a.i != a_end) } else { (lb, la, b.i != *node) };
                        let s0 = la_.unwrap_or(0.0);
                        let s1 = lb_.unwrap_or(len);
                        let (x0, x1) = if flip { (offset + len - s1, offset + len - s0) } else { (offset + s0, offset + s1) };
                        if flip {
                            std::mem::swap(wi, wj);
                        }
                        *la_ = Some(x0);
                        *lb_ = Some(x1);
                        *member = a.id;
                    }
                    LoadKind::Point { member, a: pa, .. } if *member == a.id || *member == b.id => {
                        let (len, offset, flip) = if *member == a.id { (la, 0.0, a.i != a_end) } else { (lb, la, b.i != *node) };
                        *pa = if flip { offset + len - *pa } else { offset + *pa };
                        *member = a.id;
                    }
                    _ => {}
                }
            }
            let ka = ks[0];
            let mm = &mut m.members[ka];
            mm.i = a_end;
            mm.j = b_end;
            mm.hinge_i = hinge_start;
            mm.hinge_j = hinge_end;
            m.members.remove(ks[1]);
            merged += 1;
            done = true;
            break;
        }
        if !done {
            return merged;
        }
    }
}

/// Writes the model as a Stabileo 3D model: members as elements, loads in global axes (slab
/// loads as the beam loads they make), self-weight as the editor's self-weight setting.
pub fn export(model: &Model) -> Value {
    let mut warnings = vec![];
    let (slab, _, _) = crate::slabload::loads(model, &mut warnings);
    let mut line: Vec<(Id, Id, f64, f64, [f64; 3], [f64; 3])> = slab.iter().map(|l| (l.case, l.member, l.a, l.b, l.wa, l.wb)).collect();
    let mut loads = vec![];
    let mut lid = 0;
    for l in &model.loads {
        match &l.kind {
            LoadKind::Nodal { node, f } => {
                lid += 1;
                loads.push(json!({"type": "nodal3d", "data": {"id": lid, "nodeId": node, "fx": f[0], "fy": f[1], "fz": f[2], "mx": f[3], "my": f[4], "mz": f[5], "caseId": l.case}}));
            }
            LoadKind::Line { member, wi, wj, a, b } => {
                let len = model.member(*member).map(|m| model.length(m)).unwrap_or(0.0);
                line.push((l.case, *member, a.unwrap_or(0.0), b.unwrap_or(len), *wi, *wj));
            }
            LoadKind::Point { member, a, p } => {
                let Some(mm) = model.member(*member) else { continue };
                let Some((_, ey, ez)) = crate::analysis::axes(model, mm) else { continue };
                let d = |v: [f64; 3]| v[0] * p[0] + v[1] * p[1] + v[2] * p[2];
                lid += 1;
                loads.push(json!({"type": "pointOnElement3d", "data": {"id": lid, "elementId": member, "a": a, "py": d(ey), "pz": d(ez), "caseId": l.case}}));
            }
            LoadKind::Thermal { member, dt, dt_y, dt_z } => {
                lid += 1;
                loads.push(json!({"type": "thermal3d", "data": {"id": lid, "elementId": member, "dtUniform": dt, "dtGradientY": dt_y, "dtGradientZ": dt_z, "caseId": l.case}}));
            }
            LoadKind::Settlement { .. } => {}
        }
    }
    for (case, member, a, b, wa, wb) in line {
        lid += 1;
        loads.push(json!({"type": "distributed3d", "data": {"id": lid, "elementId": member, "qXI": wa[0], "qXJ": wb[0], "qYI": wa[1], "qYJ": wb[1], "qZI": wa[2], "qZJ": wb[2], "a": a, "b": b, "frame": "global", "caseId": case}}));
    }
    let sw: Vec<Id> = model.cases.iter().filter(|c| c.self_weight).map(|c| c.id).collect();
    let case_type = |k: CaseKind| match k {
        CaseKind::D => "D",
        CaseKind::L => "L",
        CaseKind::Lr => "Lr",
        CaseKind::W => "W",
        CaseKind::E => "E",
        CaseKind::Other => "D",
    };
    json!({"model": {
        "name": model.name,
        "materials": model.materials.iter().map(|m| json!({"id": m.id, "name": m.name, "e": m.e, "nu": m.nu, "rho": m.gamma, "fy": if m.fy > 0.0 { json!(m.fy) } else { Value::Null }})).collect::<Vec<_>>(),
        "sections": model.sections.iter().map(|s| {
            let p = s.props();
            let mut v = json!({"id": s.id, "name": s.name, "a": p.a, "iy": p.iy, "iz": p.iz, "j": p.j.max(1e-9)});
            match s.shape {
                Shape::Rect { b, h } => { v["b"] = json!(b); v["h"] = json!(h); v["shape"] = json!("rect"); }
                Shape::H { d, bf, tw, tf, .. } => { v["h"] = json!(d); v["b"] = json!(bf); v["tw"] = json!(tw); v["tf"] = json!(tf); v["shape"] = json!("I"); }
                Shape::LippedC { h, b, t, .. } => { v["h"] = json!(h); v["b"] = json!(b); v["t"] = json!(t); v["shape"] = json!("C"); }
                Shape::General { .. } => {}
            }
            v
        }).collect::<Vec<_>>(),
        "nodes": model.nodes.iter().map(|n| json!({"id": n.id, "x": n.x, "y": n.y, "z": n.z})).collect::<Vec<_>>(),
        "elements": model.members.iter().map(|m| {
            let mut v = json!({"id": m.id, "type": "frame", "nodeI": m.i, "nodeJ": m.j, "materialId": m.material, "sectionId": m.section, "hingeStart": m.hinge_i, "hingeEnd": m.hinge_j});
            if let Some(b) = &m.bim_id { v["bimId"] = json!(b); }
            if m.roll != 0.0 { v["rollAngle"] = json!(m.roll); }
            v
        }).collect::<Vec<_>>(),
        "supports": model.supports.iter().enumerate().map(|(k, s)| {
            let t = match s.kind() { "fixed" => "fixed3d", "pinned" => "pinned3d", _ => "fixed3d" };
            json!({"id": k + 1, "nodeId": s.node, "type": t})
        }).collect::<Vec<_>>(),
        "loads": loads,
        "plates": [], "quads": [], "constraints": [],
        "loadCases": model.cases.iter().map(|c| json!({"id": c.id, "type": case_type(c.kind), "name": c.name})).collect::<Vec<_>>(),
        "combinations": model.combos.iter().map(|c| json!({"id": c.id, "name": c.name, "factors": c.factors.iter().map(|(id, f)| json!({"caseId": id, "factor": f})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "analysis": {"selfWeight": sw.iter().map(|c| json!({"caseId": c, "direction": "Z", "factor": -1})).collect::<Vec<_>>()},
    }})
}
