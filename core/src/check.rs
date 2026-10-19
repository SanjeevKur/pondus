// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Model checks before analysis: what is missing or suspicious, in words an engineer acts on.

use crate::model::*;

pub fn problems(m: &Model) -> Vec<String> {
    let mut out = vec![];
    for mm in &m.members {
        if m.node(mm.i).is_none() || m.node(mm.j).is_none() {
            out.push(format!("member {} uses a node that does not exist", mm.id));
        } else if m.length(mm) < 1e-6 {
            out.push(format!("member {} has no length", mm.id));
        }
        if m.section(mm.section).is_none() {
            out.push(format!("member {} has no section (id {})", mm.id, mm.section));
        }
        if m.material(mm.material).is_none() {
            out.push(format!("member {} has no material (id {})", mm.id, mm.material));
        }
        if let (Some(s), Some(mt)) = (m.section(mm.section), m.material(mm.material)) {
            match (&s.shape, mt.kind) {
                (Shape::H { .. }, MaterialKind::Concrete) => out.push(format!("member {} is concrete with the steel shape {}", mm.id, s.name)),
                (Shape::Rect { .. }, MaterialKind::Steel) => out.push(format!("member {} is steel with the rectangle {} (solid steel?)", mm.id, s.name)),
                _ => {}
            }
        }
    }
    for (k, a) in m.members.iter().enumerate() {
        for b in &m.members[k + 1..] {
            if (a.i == b.i && a.j == b.j) || (a.i == b.j && a.j == b.i) {
                out.push(format!("members {} and {} lie on each other", a.id, b.id));
            }
        }
    }
    let used: std::collections::HashSet<Id> = m.members.iter().flat_map(|x| [x.i, x.j]).collect();
    for s in &m.supports {
        if !used.contains(&s.node) {
            out.push(format!("the support at node {} holds no member", s.node));
        }
    }
    for c in &m.combos {
        for (id, _) in &c.factors {
            if m.case(*id).is_none() {
                out.push(format!("combination '{}' uses case {id}, which does not exist", c.name));
            }
        }
    }
    if !m.combos.iter().any(|c| c.kind == ComboKind::Uls) {
        out.push("no strength (ULS) combination: nothing can be designed".into());
    }
    for s in &m.slabs {
        if s.lx() < 0.1 || s.ly() < 0.1 {
            out.push(format!("slab {} is too small ({:.2} x {:.2} m)", s.id, s.lx(), s.ly()));
        }
    }
    out
}
