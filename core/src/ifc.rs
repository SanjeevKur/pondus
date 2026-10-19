// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! IFC (ISO 10303-21 STEP physical files) in and out, so the frame goes to and comes from
//! BIM tools. Export writes IFC4 in metres: project, site, building and one storey per level,
//! each member an IfcColumn, IfcBeam or IfcMember (braces) with its section swept along its
//! axis (IfcRectangleProfileDef, IfcIShapeProfileDef or IfcCShapeProfileDef; the placement's
//! z axis along the member and its x axis along the section's local y, so sloped members and
//! the roll come out right) and an 'Axis' polyline, each slab an IfcSlab, the materials by
//! IfcRelAssociatesMaterial, and a property set Pondus_Design with the member id, section and,
//! with a design report, the design ratio. General sections go out as the rectangle of the
//! same area and Iy.
//! Import reads IFC2x3 and IFC4 files from Revit, Aedifex and the like: IfcColumn, IfcBeam
//! and IfcMember (or, when the file has them, the analytical IfcStructuralCurveMember edges
//! instead), their placements resolved through every IfcLocalPlacement and
//! IfcAxis2Placement3D, the body's IfcExtrudedAreaSolid (also inside boolean clippings and
//! mapped items) or an 'Axis' polyline, and the profile: a rectangle (b x h), an I shape (the
//! nearest catalogue H within 5 %) or any closed polyline (its bounding rectangle). Length
//! units in mm or feet are scaled to m. Physical members stop at faces and sit at their
//! centroid, so the axes are snapped to an analytical model: a horizontal member moves up to
//! the level its top is at, and a member's end is extended (or trimmed) to the axis of the
//! member it meets, within `snap`. Slabs come in as outlines (IfcSlab, extruded profiles).
//! Footings, walls and other elements are counted and skipped. Each element's GlobalId is
//! kept as the member's bim_id.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

pub use crate::dxf::{apply, Applied, ImportResult, ImportedMember, ImportedSlab};
use crate::design::Report;
use crate::model::*;

// ---------------------------------------------------------------- STEP values

#[derive(Clone, Debug, PartialEq)]
pub enum P {
    Ref(usize),
    Num(f64),
    Str(String),
    Enum(String),
    List(Vec<P>),
    /// A typed value such as IFCLABEL('x').
    Typed(String, Vec<P>),
    Null,
}

impl P {
    fn r(&self) -> Option<usize> {
        if let P::Ref(i) = self { Some(*i) } else { None }
    }
    fn f(&self) -> Option<f64> {
        match self {
            P::Num(x) => Some(*x),
            P::Typed(_, v) => v.first().and_then(P::f),
            _ => None,
        }
    }
    fn s(&self) -> Option<&str> {
        match self {
            P::Str(s) => Some(s),
            P::Typed(_, v) => v.first().and_then(P::s),
            _ => None,
        }
    }
    fn e(&self) -> Option<&str> {
        if let P::Enum(s) = self { Some(s) } else { None }
    }
    fn list(&self) -> &[P] {
        if let P::List(v) = self { v } else { &[] }
    }
}

/// The DATA section of a STEP file: entity number → (TYPE in capitals, attributes).
pub struct Step {
    pub ents: HashMap<usize, (String, Vec<P>)>,
}

impl Step {
    pub fn parse(text: &str) -> Result<Step, String> {
        let up = text.find("DATA;").ok_or("not a STEP file: no DATA section")?;
        let body = &text[up + 5..];
        let mut ents = HashMap::new();
        // statements end at ';' outside strings
        let mut stmt = String::new();
        let mut in_str = false;
        let mut chars = body.chars().peekable();
        while let Some(c) = chars.next() {
            if in_str {
                stmt.push(c);
                if c == '\'' {
                    if chars.peek() == Some(&'\'') {
                        stmt.push(chars.next().unwrap());
                    } else {
                        in_str = false;
                    }
                }
                continue;
            }
            match c {
                '\'' => {
                    in_str = true;
                    stmt.push(c);
                }
                ';' => {
                    let t = stmt.trim();
                    if t == "ENDSEC" {
                        break;
                    }
                    if let Some(rest) = t.strip_prefix('#') {
                        if let Some((id, def)) = rest.split_once('=') {
                            if let (Ok(id), Some((ty, args))) = (id.trim().parse::<usize>(), parse_entity(def.trim())) {
                                ents.insert(id, (ty, args));
                            }
                        }
                    }
                    stmt.clear();
                }
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    let mut prev = ' ';
                    for d in chars.by_ref() {
                        if prev == '*' && d == '/' {
                            break;
                        }
                        prev = d;
                    }
                }
                _ => stmt.push(c),
            }
        }
        if ents.is_empty() {
            return Err("no entities in the DATA section".into());
        }
        Ok(Step { ents })
    }

    pub fn get(&self, id: usize) -> Option<(&str, &[P])> {
        self.ents.get(&id).map(|(t, a)| (t.as_str(), a.as_slice()))
    }
    fn of(&self, ty: &str) -> Vec<usize> {
        let mut v: Vec<usize> = self.ents.iter().filter(|(_, e)| e.0 == ty).map(|(k, _)| *k).collect();
        v.sort_unstable();
        v
    }
    fn arg(&self, id: usize, k: usize) -> Option<&P> {
        self.ents.get(&id).and_then(|e| e.1.get(k))
    }
    fn pt(&self, p: &P) -> Option<[f64; 3]> {
        let (t, a) = self.get(p.r()?)?;
        if t != "IFCCARTESIANPOINT" && t != "IFCDIRECTION" {
            return None;
        }
        let v: Vec<f64> = a.first()?.list().iter().filter_map(P::f).collect();
        Some([v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(0.0)])
    }
}

fn parse_entity(s: &str) -> Option<(String, Vec<P>)> {
    let open = s.find('(')?;
    let ty = s[..open].trim().to_uppercase();
    let chars: Vec<char> = s[open..].chars().collect();
    let mut k = 0;
    let v = parse_value(&chars, &mut k)?;
    match v {
        P::List(a) => Some((ty, a)),
        _ => None,
    }
}

fn parse_value(c: &[char], k: &mut usize) -> Option<P> {
    while *k < c.len() && c[*k].is_whitespace() {
        *k += 1;
    }
    let ch = *c.get(*k)?;
    match ch {
        '(' => {
            *k += 1;
            let mut out = vec![];
            loop {
                while *k < c.len() && c[*k].is_whitespace() {
                    *k += 1;
                }
                if *c.get(*k)? == ')' {
                    *k += 1;
                    return Some(P::List(out));
                }
                out.push(parse_value(c, k)?);
                while *k < c.len() && c[*k].is_whitespace() {
                    *k += 1;
                }
                match c.get(*k)? {
                    ',' => *k += 1,
                    ')' => {
                        *k += 1;
                        return Some(P::List(out));
                    }
                    _ => return None,
                }
            }
        }
        '\'' => {
            *k += 1;
            let mut s = String::new();
            while *k < c.len() {
                if c[*k] == '\'' {
                    if c.get(*k + 1) == Some(&'\'') {
                        s.push('\'');
                        *k += 2;
                        continue;
                    }
                    *k += 1;
                    break;
                }
                s.push(c[*k]);
                *k += 1;
            }
            Some(P::Str(decode(&s)))
        }
        '#' => {
            *k += 1;
            let st = *k;
            while *k < c.len() && c[*k].is_ascii_digit() {
                *k += 1;
            }
            c[st..*k].iter().collect::<String>().parse().ok().map(P::Ref)
        }
        '$' | '*' => {
            *k += 1;
            Some(P::Null)
        }
        '.' => {
            *k += 1;
            let st = *k;
            while *k < c.len() && c[*k] != '.' {
                *k += 1;
            }
            let e: String = c[st..*k].iter().collect();
            *k += 1;
            Some(P::Enum(e))
        }
        c0 if c0.is_ascii_alphabetic() => {
            let st = *k;
            while *k < c.len() && (c[*k].is_ascii_alphanumeric() || c[*k] == '_') {
                *k += 1;
            }
            let name: String = c[st..*k].iter().collect::<String>().to_uppercase();
            match parse_value(c, k)? {
                P::List(v) => Some(P::Typed(name, v)),
                _ => None,
            }
        }
        _ => {
            let st = *k;
            while *k < c.len() && (c[*k].is_ascii_digit() || matches!(c[*k], '.' | '-' | '+' | 'E' | 'e')) {
                *k += 1;
            }
            let t: String = c[st..*k].iter().collect();
            t.parse::<f64>().ok().or_else(|| t.replace(".E", ".0E").replace(".e", ".0e").parse().ok()).map(P::Num)
        }
    }
}

/// \X2\hhhh\X0\ (UTF-16) and \X\hh escapes of STEP strings.
fn decode(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(r) = rest.strip_prefix("\\X2\\") {
            let end = r.find("\\X0\\").unwrap_or(r.len());
            let hex = &r[..end];
            let units: Vec<u16> = (0..hex.len() / 4).filter_map(|q| u16::from_str_radix(&hex[q * 4..q * 4 + 4], 16).ok()).collect();
            out.push_str(&String::from_utf16_lossy(&units));
            rest = &r[(end + 4).min(r.len())..];
        } else if let Some(r) = rest.strip_prefix("\\X\\") {
            if let Some(b) = r.get(..2).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b as char);
            }
            rest = r.get(2..).unwrap_or("");
        } else if let Some(r) = rest.strip_prefix("\\\\") {
            out.push('\\');
            rest = r;
        } else {
            out.push('\\');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------- frames

/// A rigid placement: world = o + r[0]·x + r[1]·y + r[2]·z (r the unit axes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub o: [f64; 3],
    pub r: [[f64; 3]; 3],
}

impl Frame {
    pub const ID: Frame = Frame { o: [0.0; 3], r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] };
    pub fn pt(&self, p: [f64; 3]) -> [f64; 3] {
        let d = self.dir(p);
        [self.o[0] + d[0], self.o[1] + d[1], self.o[2] + d[2]]
    }
    pub fn dir(&self, p: [f64; 3]) -> [f64; 3] {
        [0, 1, 2].map(|q| self.r[0][q] * p[0] + self.r[1][q] * p[1] + self.r[2][q] * p[2])
    }
    /// self ∘ b: b's coordinates are in self's frame.
    pub fn then(&self, b: &Frame) -> Frame {
        Frame { o: self.pt(b.o), r: [self.dir(b.r[0]), self.dir(b.r[1]), self.dir(b.r[2])] }
    }
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (l > 1e-12).then(|| v.map(|x| x / l))
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}

/// A right-handed frame from its z axis and a reference x direction.
fn frame_from(o: [f64; 3], z: Option<[f64; 3]>, x: Option<[f64; 3]>) -> Frame {
    let z = z.and_then(unit).unwrap_or([0.0, 0.0, 1.0]);
    let mut x = x.and_then(unit).unwrap_or(if z[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] });
    x = unit(add(x, z, -dot(x, z))).unwrap_or(if z[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] });
    let x = unit(add(x, z, -dot(x, z))).unwrap_or([1.0, 0.0, 0.0]);
    Frame { o, r: [x, cross(z, x), z] }
}

struct Reader<'a> {
    st: &'a Step,
    scale: f64,
    placements: HashMap<usize, Frame>,
}

impl Reader<'_> {
    fn p(&self, v: &P) -> Option<[f64; 3]> {
        self.st.pt(v).map(|p| p.map(|x| x * self.scale))
    }
    fn d(&self, v: &P) -> Option<[f64; 3]> {
        self.st.pt(v)
    }
    /// IfcAxis2Placement3D / 2D, scaled.
    fn axis2(&self, id: Option<usize>) -> Frame {
        let Some((t, a)) = id.and_then(|i| self.st.get(i)) else { return Frame::ID };
        match t {
            "IFCAXIS2PLACEMENT3D" => frame_from(a.first().and_then(|v| self.p(v)).unwrap_or([0.0; 3]), a.get(1).and_then(|v| self.d(v)), a.get(2).and_then(|v| self.d(v))),
            "IFCAXIS2PLACEMENT2D" => frame_from(a.first().and_then(|v| self.p(v)).unwrap_or([0.0; 3]), None, a.get(1).and_then(|v| self.d(v))),
            _ => Frame::ID,
        }
    }
    /// An IfcLocalPlacement in world coordinates, through its PlacementRelTo chain.
    fn placement(&mut self, id: Option<usize>, depth: usize) -> Frame {
        let Some(id) = id else { return Frame::ID };
        if let Some(f) = self.placements.get(&id) {
            return *f;
        }
        let Some((t, a)) = self.st.get(id) else { return Frame::ID };
        if t != "IFCLOCALPLACEMENT" || depth > 64 {
            return Frame::ID;
        }
        let (rel, ax) = (a.first().and_then(P::r), a.get(1).and_then(P::r));
        let parent = self.placement(rel, depth + 1);
        let f = parent.then(&self.axis2(ax));
        self.placements.insert(id, f);
        f
    }
    /// The 2D cartesian transformation of a mapped item's target (axes, origin, scale).
    fn operator(&self, id: Option<usize>) -> Frame {
        let Some((t, a)) = id.and_then(|i| self.st.get(i)) else { return Frame::ID };
        if !t.starts_with("IFCCARTESIANTRANSFORMATIONOPERATOR3D") {
            return Frame::ID;
        }
        let x = a.first().and_then(|v| self.d(v));
        let o = a.get(2).and_then(|v| self.p(v)).unwrap_or([0.0; 3]);
        let z = a.get(4).and_then(|v| self.d(v));
        frame_from(o, z.or(Some([0.0, 0.0, 1.0])), x)
    }
}

/// A profile in its own 2D frame (already moved by its Position).
#[derive(Clone, Debug, PartialEq)]
enum Profile {
    Rect { b: f64, h: f64, c: [f64; 2], x: [f64; 2] },
    I { bf: f64, d: f64, tw: f64, tf: f64, r: f64, c: [f64; 2], x: [f64; 2] },
    Outline { pts: Vec<[f64; 2]> },
}

impl Profile {
    /// Width and depth along the profile's own x and y, its centre, its x direction.
    fn frame(&self) -> (f64, f64, [f64; 2], [f64; 2]) {
        match self {
            Profile::Rect { b, h, c, x } => (*b, *h, *c, *x),
            Profile::I { bf, d, c, x, .. } => (*bf, *d, *c, *x),
            Profile::Outline { pts } => {
                let (x0, y0, x1, y1) = crate::dxf::bbox(pts);
                (x1 - x0, y1 - y0, [(x0 + x1) / 2.0, (y0 + y1) / 2.0], [1.0, 0.0])
            }
        }
    }
}

impl Reader<'_> {
    fn profile(&self, id: usize) -> Option<Profile> {
        let (t, a) = self.st.get(id)?;
        let pos = || {
            let f = self.axis2(a.get(2).and_then(P::r));
            ([f.o[0], f.o[1]], [f.r[0][0], f.r[0][1]])
        };
        let len = |k: usize| a.get(k).and_then(P::f).map(|x| x * self.scale);
        match t {
            "IFCRECTANGLEPROFILEDEF" | "IFCRECTANGLEHOLLOWPROFILEDEF" | "IFCROUNDEDRECTANGLEPROFILEDEF" => {
                let (c, x) = pos();
                Some(Profile::Rect { b: len(3)?, h: len(4)?, c, x })
            }
            "IFCISHAPEPROFILEDEF" | "IFCASYMMETRICISHAPEPROFILEDEF" => {
                let (c, x) = pos();
                Some(Profile::I { bf: len(3)?, d: len(4)?, tw: len(5)?, tf: len(6)?, r: len(7).unwrap_or(0.0), c, x })
            }
            "IFCCIRCLEPROFILEDEF" | "IFCCIRCLEHOLLOWPROFILEDEF" => {
                // the square of the same area
                let (c, x) = pos();
                let s = len(3)? * std::f64::consts::PI.sqrt();
                Some(Profile::Rect { b: s, h: s, c, x })
            }
            "IFCARBITRARYCLOSEDPROFILEDEF" | "IFCARBITRARYPROFILEDEFWITHVOIDS" => {
                let pts = self.curve(a.get(2).and_then(P::r)?)?;
                (pts.len() >= 3).then_some(Profile::Outline { pts })
            }
            "IFCDERIVEDPROFILEDEF" => {
                let p = self.profile(a.get(2).and_then(P::r)?)?;
                let f = self.operator2(a.get(3).and_then(P::r));
                Some(match p {
                    Profile::Rect { b, h, c, x } => Profile::Rect { b, h, c: f(c), x: rot2(&f, x) },
                    Profile::I { bf, d, tw, tf, r, c, x } => Profile::I { bf, d, tw, tf, r, c: f(c), x: rot2(&f, x) },
                    Profile::Outline { pts } => Profile::Outline { pts: pts.into_iter().map(&f).collect() },
                })
            }
            _ => {
                // other parameterised profiles (C, L, T, Z, U): their overall width and depth
                let (c, x) = pos();
                let b = len(3)?;
                let h = len(4).unwrap_or(b);
                Some(Profile::Rect { b, h, c, x })
            }
        }
    }
    fn operator2(&self, id: Option<usize>) -> impl Fn([f64; 2]) -> [f64; 2] {
        let f = match id.and_then(|i| self.st.get(i)) {
            Some((t, a)) if t.starts_with("IFCCARTESIANTRANSFORMATIONOPERATOR2D") => frame_from(a.get(2).and_then(|v| self.p(v)).unwrap_or([0.0; 3]), None, a.first().and_then(|v| self.d(v))),
            _ => Frame::ID,
        };
        move |p: [f64; 2]| {
            let q = f.pt([p[0], p[1], 0.0]);
            [q[0], q[1]]
        }
    }
    /// The points of a polyline or an indexed polycurve, 2D.
    fn curve(&self, id: usize) -> Option<Vec<[f64; 2]>> {
        let (t, a) = self.st.get(id)?;
        let mut pts: Vec<[f64; 2]> = match t {
            "IFCPOLYLINE" => a.first()?.list().iter().filter_map(|v| self.p(v)).map(|p| [p[0], p[1]]).collect(),
            "IFCINDEXEDPOLYCURVE" => {
                let (_, la) = self.st.get(a.first()?.r()?)?;
                la.first()?.list().iter().map(|q| {
                    let v: Vec<f64> = q.list().iter().filter_map(P::f).collect();
                    [v.first().copied().unwrap_or(0.0) * self.scale, v.get(1).copied().unwrap_or(0.0) * self.scale]
                }).collect()
            }
            "IFCCOMPOSITECURVE" => {
                let mut out = vec![];
                for seg in a.first()?.list() {
                    if let Some(c) = self.st.get(seg.r()?).and_then(|(_, sa)| sa.get(2)).and_then(P::r).and_then(|c| self.curve(c)) {
                        out.extend(c);
                    }
                }
                out
            }
            _ => return None,
        };
        if pts.len() > 1 && (pts[0][0] - pts[pts.len() - 1][0]).abs() < 1e-9 && (pts[0][1] - pts[pts.len() - 1][1]).abs() < 1e-9 {
            pts.pop();
        }
        Some(pts)
    }
}

fn rot2(f: &impl Fn([f64; 2]) -> [f64; 2], x: [f64; 2]) -> [f64; 2] {
    let (a, b) = (f([0.0, 0.0]), f(x));
    [b[0] - a[0], b[1] - a[1]]
}

/// A swept body in world coordinates: its axis, the profile and the profile's axes.
#[derive(Clone, Debug)]
struct Body {
    a: [f64; 3],
    b: [f64; 3],
    profile: Profile,
    /// World directions of the profile's x and y.
    px: [f64; 3],
    py: [f64; 3],
    /// The profile's corners at the start, world (slabs).
    outline: Vec<[f64; 3]>,
}

impl Reader<'_> {
    /// The first extruded solid of a representation item, through clippings and mapped
    /// items, in the coordinates of `f`.
    fn solid(&self, id: usize, f: Frame, depth: usize) -> Option<Body> {
        if depth > 12 {
            return None;
        }
        let (t, a) = self.st.get(id)?;
        match t {
            "IFCEXTRUDEDAREASOLID" | "IFCEXTRUDEDAREASOLIDTAPERED" => {
                let profile = self.profile(a.first()?.r()?)?;
                let pos = f.then(&self.axis2(a.get(1).and_then(P::r)));
                let dir = a.get(2).and_then(|v| self.d(v)).and_then(unit).unwrap_or([0.0, 0.0, 1.0]);
                let depth = a.get(3)?.f()? * self.scale;
                let (_, _, c, x) = profile.frame();
                let start = pos.pt([c[0], c[1], 0.0]);
                let end = pos.pt(add([c[0], c[1], 0.0], dir, depth));
                let px = pos.dir([x[0], x[1], 0.0]);
                let py = pos.dir([-x[1], x[0], 0.0]);
                let outline = match &profile {
                    Profile::Outline { pts } => pts.iter().map(|p| pos.pt([p[0], p[1], 0.0])).collect(),
                    _ => {
                        let (w, h, c, _) = profile.frame();
                        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().map(|(u, v)| add(add(pos.pt([c[0], c[1], 0.0]), px, u * w / 2.0), py, v * h / 2.0)).collect()
                    }
                };
                Some(Body { a: start, b: end, profile, px, py, outline })
            }
            "IFCBOOLEANCLIPPINGRESULT" | "IFCBOOLEANRESULT" => self.solid(a.get(1)?.r()?, f, depth + 1),
            "IFCMAPPEDITEM" => {
                let (_, ma) = self.st.get(a.first()?.r()?)?;
                let origin = self.axis2(ma.first().and_then(P::r));
                let g = f.then(&self.operator(a.get(1).and_then(P::r))).then(&origin);
                let (_, ra) = self.st.get(ma.get(1)?.r()?)?;
                ra.get(3)?.list().iter().filter_map(P::r).find_map(|i| self.solid(i, g, depth + 1))
            }
            _ => None,
        }
    }
    /// A product's axis polyline ('Axis' representation), world.
    fn axis_rep(&self, shape: usize, f: Frame) -> Option<([f64; 3], [f64; 3])> {
        let (_, a) = self.st.get(shape)?;
        for rep in a.get(2)?.list().iter().filter_map(P::r) {
            let (_, ra) = self.st.get(rep)?;
            if ra.get(1).and_then(P::s).is_some_and(|s| s.eq_ignore_ascii_case("Axis")) {
                for item in ra.get(3)?.list().iter().filter_map(P::r) {
                    let (t, ia) = self.st.get(item)?;
                    if t == "IFCPOLYLINE" {
                        let pts: Vec<[f64; 3]> = ia.first()?.list().iter().filter_map(|v| self.p(v)).collect();
                        if pts.len() >= 2 {
                            return Some((f.pt(pts[0]), f.pt(pts[pts.len() - 1])));
                        }
                    }
                }
            }
        }
        None
    }
    fn body(&self, shape: usize, f: Frame) -> Option<Body> {
        let (_, a) = self.st.get(shape)?;
        let reps: Vec<usize> = a.get(2)?.list().iter().filter_map(P::r).collect();
        // the Body representation first, then any
        let mut order: Vec<usize> = reps.iter().copied().filter(|r| self.st.arg(*r, 1).and_then(P::s).is_some_and(|s| s.eq_ignore_ascii_case("Body"))).collect();
        let rest: Vec<usize> = reps.iter().copied().filter(|r| !order.contains(r)).collect();
        order.extend(rest);
        for rep in order {
            let Some((_, ra)) = self.st.get(rep) else { continue };
            for item in ra.get(3).map(P::list).unwrap_or(&[]).iter().filter_map(P::r) {
                if let Some(b) = self.solid(item, f, 0) {
                    return Some(b);
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------- import

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct IfcImport {
    /// How far a member's end may move to meet another member's axis, and a beam's axis to
    /// reach the level at its top, m (0: the axes as drawn).
    pub snap: f64,
    /// Ends closer than this merge, m.
    pub tol: f64,
    pub default_section: String,
    pub slab_dead: f64,
    pub slab_live: f64,
}

impl Default for IfcImport {
    fn default() -> Self {
        IfcImport { snap: 0.75, tol: 0.001, default_section: "300 x 300".into(), slab_dead: 1.5, slab_live: 2.0 }
    }
}

/// The length unit of the file, m per unit.
fn length_scale(st: &Step) -> f64 {
    for id in st.of("IFCUNITASSIGNMENT") {
        for u in st.arg(id, 0).map(P::list).unwrap_or(&[]).iter().filter_map(P::r) {
            let Some((t, a)) = st.get(u) else { continue };
            let is_len = |k: usize| a.get(k).and_then(P::e) == Some("LENGTHUNIT");
            if t == "IFCSIUNIT" && is_len(1) {
                return match a.get(2).and_then(P::e) {
                    Some("MILLI") => 0.001,
                    Some("CENTI") => 0.01,
                    Some("DECI") => 0.1,
                    Some("KILO") => 1000.0,
                    _ => 1.0,
                };
            }
            if t == "IFCCONVERSIONBASEDUNIT" && is_len(1) {
                let name = a.get(2).and_then(P::s).unwrap_or("").to_uppercase();
                let f = a.get(3).and_then(P::r).and_then(|m| st.arg(m, 0)).and_then(P::f);
                return match (f, name.as_str()) {
                    (Some(f), _) if f > 0.0 => f,
                    (_, "FOOT") => 0.3048,
                    (_, "INCH") => 0.0254,
                    _ => 1.0,
                };
            }
        }
    }
    1.0
}

/// Reads an IFC file's structural members and slabs (see the module notes).
pub fn import(text: &str, opts: &IfcImport) -> Result<ImportResult, String> {
    let st = Step::parse(text)?;
    let mut rd = Reader { st: &st, scale: length_scale(&st), placements: HashMap::new() };
    let mut r = ImportResult { fallback_section: opts.default_section.clone(), tol: opts.tol, ..Default::default() };
    for (t, _) in st.ents.values() {
        if t.starts_with("IFC") && !t.contains("RELATIONSHIP") {
            match t.as_str() {
                "IFCCOLUMN" | "IFCBEAM" | "IFCMEMBER" | "IFCSLAB" | "IFCFOOTING" | "IFCWALL" | "IFCWALLSTANDARDCASE" | "IFCPLATE" | "IFCPILE" | "IFCSTRUCTURALCURVEMEMBER" | "IFCCOLUMNSTANDARDCASE" | "IFCBEAMSTANDARDCASE" | "IFCMEMBERSTANDARDCASE" | "IFCSLABSTANDARDCASE" => *r.counts.entry(t.clone()).or_default() += 1,
                _ => {}
            }
        }
    }
    // materials and profiles by relationship
    let mut material: HashMap<usize, String> = HashMap::new();
    let mut profile_of: HashMap<usize, usize> = HashMap::new();
    for rel in st.of("IFCRELASSOCIATESMATERIAL").into_iter().chain(st.of("IFCRELASSOCIATESPROFILEPROPERTIES")) {
        let Some((_, a)) = st.get(rel) else { continue };
        let objs: Vec<usize> = a.get(4).map(P::list).unwrap_or(&[]).iter().filter_map(P::r).collect();
        let Some(target) = a.get(5).and_then(P::r) else { continue };
        let name = find(&st, target, 0, &|t| t == "IFCMATERIAL").and_then(|m| st.arg(m, 0)).and_then(P::s).map(String::from);
        let prof = find(&st, target, 0, &|t| t.ends_with("PROFILEDEF"));
        for o in objs {
            if let Some(n) = &name {
                material.entry(o).or_insert_with(|| n.clone());
            }
            if let Some(p) = prof {
                profile_of.entry(o).or_insert(p);
            }
        }
    }
    let mat_kind = |id: usize, prof: Option<&Profile>| -> Option<MaterialKind> {
        let n = material.get(&id).map(|s| s.to_lowercase()).unwrap_or_default();
        if n.contains("steel") || n.contains("stahl") || n.contains("ss400") || n.contains("s235") || n.contains("s275") || n.contains("s355") || n.contains("a992") || n.contains("metal") {
            Some(MaterialKind::Steel)
        } else if n.contains("concrete") || n.contains("beton") || n.starts_with("c2") || n.starts_with("c3") || n.contains("rc") {
            Some(MaterialKind::Concrete)
        } else if matches!(prof, Some(Profile::I { .. })) {
            Some(MaterialKind::Steel)
        } else {
            None
        }
    };
    let analytical = st.of("IFCSTRUCTURALCURVEMEMBER");
    let mut levels: Vec<f64> = vec![];
    for s in st.of("IFCBUILDINGSTOREY") {
        let pl = st.arg(s, 5).and_then(P::r);
        levels.push(rd.placement(pl, 0).o[2]);
    }
    let mut depth_of: Vec<f64> = vec![];
    if !analytical.is_empty() {
        r.warnings.push(format!("the file has an analytical model: its {} structural curve members are read, the physical members are not", analytical.len()));
        for id in &analytical {
            let Some((_, a)) = st.get(*id) else { continue };
            let f = rd.placement(a.get(5).and_then(P::r), 0);
            let Some(shape) = a.get(6).and_then(P::r) else { continue };
            let Some(edge) = find(&st, shape, 0, &|t| t == "IFCEDGE" || t == "IFCORIENTEDEDGE" || t == "IFCEDGECURVE") else { continue };
            let ends: Vec<[f64; 3]> = (0..2).filter_map(|k| st.arg(edge, k).and_then(P::r)).filter_map(|v| st.arg(v, 0)).filter_map(|p| rd.p(p)).map(|p| f.pt(p)).collect();
            if ends.len() < 2 {
                continue;
            }
            let prof = profile_of.get(id).and_then(|p| rd.profile(*p));
            let (section, h) = prof.as_ref().map(section_of).unwrap_or((opts.default_section.clone(), 0.0));
            let dz = (ends[1][2] - ends[0][2]).abs() / dist(ends[0], ends[1]).max(1e-9);
            let kind = if dz > 0.999 { MemberKind::Column } else if dz < 0.02 { MemberKind::Beam } else { MemberKind::Brace };
            r.members.push(ImportedMember { from: ends[0], to: ends[1], kind, section, material: mat_kind(*id, prof.as_ref()), roll: 0.0, bim_id: a.first().and_then(P::s).map(String::from), name: a.get(2).and_then(P::s).map(String::from) });
            depth_of.push(h);
        }
    } else {
        let classes = [("IFCCOLUMN", MemberKind::Column), ("IFCCOLUMNSTANDARDCASE", MemberKind::Column), ("IFCBEAM", MemberKind::Beam), ("IFCBEAMSTANDARDCASE", MemberKind::Beam), ("IFCMEMBER", MemberKind::Brace), ("IFCMEMBERSTANDARDCASE", MemberKind::Brace)];
        for (cls, kind) in classes {
            for id in st.of(cls) {
                let Some((_, a)) = st.get(id) else { continue };
                let f = rd.placement(a.get(5).and_then(P::r), 0);
                let Some(shape) = a.get(6).and_then(P::r) else {
                    r.warnings.push(format!("#{id} {cls}: no geometry"));
                    continue;
                };
                let body = rd.body(shape, f);
                let axis = rd.axis_rep(shape, f);
                let Some((from, to)) = axis.or(body.as_ref().map(|b| (b.a, b.b))) else {
                    r.warnings.push(format!("#{id} {cls}: its geometry is not an extrusion or an axis: skipped"));
                    continue;
                };
                let prof = body.as_ref().map(|b| b.profile.clone()).or_else(|| profile_of.get(&id).and_then(|p| rd.profile(*p)));
                let (section, h) = prof.as_ref().map(section_of).unwrap_or((opts.default_section.clone(), 0.0));
                let roll = body.as_ref().map(|b| roll_of(from, to, b.py)).unwrap_or(0.0);
                let kind = if kind == MemberKind::Brace {
                    let dz = (to[2] - from[2]).abs() / dist(from, to).max(1e-9);
                    if dz > 0.999 { MemberKind::Column } else if dz < 0.02 { MemberKind::Beam } else { MemberKind::Brace }
                } else {
                    kind
                };
                let name = a.get(2).and_then(P::s).map(String::from);
                r.members.push(ImportedMember { from, to, kind, section, material: mat_kind(id, prof.as_ref()), roll, bim_id: a.first().and_then(P::s).map(String::from), name });
                depth_of.push(h);
            }
        }
    }
    // slabs: the outline of their body, top at the solid's upper face
    for cls in ["IFCSLAB", "IFCSLABSTANDARDCASE"] {
        for id in st.of(cls) {
            let Some((_, a)) = st.get(id) else { continue };
            if a.get(8).and_then(P::e).is_some_and(|e| e == "LANDING" || e == "BASESLAB") {
                continue;
            }
            let f = rd.placement(a.get(5).and_then(P::r), 0);
            let Some(b) = a.get(6).and_then(P::r).and_then(|s| rd.body(s, f)) else { continue };
            let up = sub(b.b, b.a);
            if (up[2].abs() / dist(b.a, b.b).max(1e-9)) < 0.9 {
                r.warnings.push(format!("#{id} slab: not extruded vertically, skipped"));
                continue;
            }
            let h = up[2].abs();
            let z = b.outline.iter().map(|p| p[2]).fold(f64::MIN, f64::max).max(b.outline.iter().map(|p| p[2] + up[2]).fold(f64::MIN, f64::max));
            let name = a.get(2).and_then(P::s).unwrap_or("").to_string();
            r.slabs.push(ImportedSlab { outline: b.outline.iter().map(|p| [p[0], p[1]]).collect(), z, h, dead: opts.slab_dead, live: opts.slab_live, bim_id: a.first().and_then(P::s).map(String::from), name });
        }
    }
    if opts.snap > 0.0 {
        snap(&mut r, &depth_of, &levels, opts.snap);
    }
    for (k, v) in [("IFCFOOTING", "footings"), ("IFCWALL", "walls"), ("IFCWALLSTANDARDCASE", "walls"), ("IFCPLATE", "plates"), ("IFCPILE", "piles")] {
        if let Some(n) = r.counts.get(k) {
            r.warnings.push(format!("{n} {v} ({k}) not read"));
        }
    }
    if r.members.is_empty() {
        r.warnings.push("no columns, beams or members with geometry in the file".into());
    }
    Ok(r)
}

/// The first entity of a type reachable through references, a few levels deep.
fn find(st: &Step, id: usize, depth: usize, want: &dyn Fn(&str) -> bool) -> Option<usize> {
    let (t, a) = st.get(id)?;
    if want(t) {
        return Some(id);
    }
    if depth > 5 {
        return None;
    }
    fn refs(v: &P, out: &mut Vec<usize>) {
        match v {
            P::Ref(i) => out.push(*i),
            P::List(l) | P::Typed(_, l) => l.iter().for_each(|x| refs(x, out)),
            _ => {}
        }
    }
    let mut out = vec![];
    a.iter().for_each(|v| refs(v, &mut out));
    out.into_iter().find_map(|i| find(st, i, depth + 1, want))
}

/// A section name for a profile, and its depth (m).
fn section_of(p: &Profile) -> (String, f64) {
    match p {
        Profile::I { bf, d, tw, tf, .. } => {
            let best = crate::sections::catalogue().into_iter().map(|s| {
                let e = ((s.d / 1000.0 - d) / d).powi(2) + ((s.bf / 1000.0 - bf) / bf).powi(2) + 0.25 * ((s.tw / 1000.0 - tw) / tw).powi(2) + 0.25 * ((s.tf / 1000.0 - tf) / tf).powi(2);
                (e, s)
            }).min_by(|a, b| a.0.total_cmp(&b.0));
            match best {
                Some((e, s)) if e.sqrt() < 0.05 => (s.name, *d),
                _ => (rect_name((bf * 1000.0).round() / 1000.0, (d * 1000.0).round() / 1000.0), *d),
            }
        }
        _ => {
            let (b, h, _, _) = p.frame();
            let mm = |x: f64| (x * 1000.0).round() / 1000.0;
            (rect_name(mm(b), mm(h)), h)
        }
    }
}

/// The member's roll (degrees) that turns its local z onto the profile's y (its depth), in
/// (−90, 90]: the sections Pondus imports are symmetric, so a half turn changes nothing.
fn roll_of(a: [f64; 3], b: [f64; 3], py: [f64; 3]) -> f64 {
    let (_, ey, ez) = dedaliano_engine::element::compute_local_axes_3d(a[0], a[1], a[2], b[0], b[1], b[2], None, None, None, None, false);
    let (c, s) = (dot(py, ez), -dot(py, ey));
    let mut t = s.atan2(c).to_degrees();
    if t > 90.0 + 1e-6 {
        t -= 180.0;
    }
    if t <= -90.0 + 1e-6 {
        t += 180.0;
    }
    if t.abs() < 1e-6 {
        0.0
    } else {
        (t * 1e6).round() / 1e6
    }
}

/// Snaps physical member axes to an analytical model: horizontal members up to the level at
/// their top (a storey, or a column end), then each end along its member to the nearest axis
/// it meets within `snap`.
fn snap(r: &mut ImportResult, depth: &[f64], storeys: &[f64], snap: f64) {
    let mut levels: Vec<f64> = storeys.to_vec();
    for m in r.members.iter().filter(|m| m.kind == MemberKind::Column) {
        levels.push(m.from[2]);
        levels.push(m.to[2]);
    }
    levels.sort_by(f64::total_cmp);
    levels.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    let mut moved = 0;
    for (k, m) in r.members.iter_mut().enumerate() {
        let horizontal = (m.to[2] - m.from[2]).abs() < 1e-4 * dist(m.from, m.to).max(1.0);
        if !horizontal || m.kind == MemberKind::Column {
            continue;
        }
        let half = depth.get(k).copied().unwrap_or(0.0) / 2.0;
        let top = m.from[2] + half;
        if let Some(l) = levels.iter().copied().filter(|l| (*l - top).abs() <= half + 0.01 && (*l - m.from[2]).abs() <= snap).min_by(|a, b| (*a - top).abs().total_cmp(&(*b - top).abs())) {
            if (l - m.from[2]).abs() > 1e-9 {
                moved += 1;
            }
            m.from[2] = l;
            m.to[2] = l;
        }
    }
    // ends to the axes they meet
    let lines: Vec<([f64; 3], [f64; 3])> = r.members.iter().map(|m| (m.from, m.to)).collect();
    let mut extended = 0;
    for (k, m) in r.members.iter_mut().enumerate() {
        let Some(u) = unit(sub(m.to, m.from)) else { continue };
        for end in 0..2 {
            let p = if end == 0 { m.from } else { m.to };
            let mut best: Option<(f64, [f64; 3])> = None;
            for (q, (a, b)) in lines.iter().enumerate() {
                if q == k {
                    continue;
                }
                let Some(v) = unit(sub(*b, *a)) else { continue };
                let w = cross(u, v);
                let wl = dot(w, w).sqrt();
                if wl < 1e-6 {
                    continue;
                }
                // the closest points of the two lines
                let d0 = sub(*a, p);
                let s = dot(cross(d0, v), w) / (wl * wl);
                let t = dot(cross(d0, u), w) / (wl * wl);
                let pc = add(p, u, s);
                let qc = add(*a, v, t);
                let len = dist(*a, *b);
                // outwards from the member's end only (a trim of up to 10 mm at most): a member
                // that runs on past another keeps its length
                let out = if end == 0 { -s } else { s };
                if dist(pc, qc) > 0.01 || out > snap || out < -0.01 || t < -0.01 || t > len + 0.01 {
                    continue;
                }
                if best.is_none_or(|bst| s.abs() < bst.0) {
                    best = Some((s.abs(), qc));
                }
            }
            if let Some((dd, q)) = best {
                if dd > 1e-6 {
                    extended += 1;
                }
                if end == 0 {
                    m.from = q;
                } else {
                    m.to = q;
                }
            }
        }
    }
    if moved + extended > 0 {
        r.warnings.push(format!("analytical axes: {moved} horizontal members moved to their level, {extended} member ends extended or trimmed to the axes they meet (snap {snap} m)"));
    }
}

// ---------------------------------------------------------------- export

struct W {
    out: String,
    n: usize,
}

impl W {
    fn add(&mut self, e: String) -> usize {
        self.n += 1;
        let _ = writeln!(self.out, "#{}={};", self.n, e);
        self.n
    }
}

fn fnv(b: &[u8], seed: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ seed;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

const B64: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// A deterministic IFC GlobalId (22 characters of IFC's base 64).
fn guid(seed: u64, key: &str) -> String {
    let h1 = fnv(key.as_bytes(), seed);
    let h2 = fnv(key.as_bytes(), h1 ^ 0x9e37_79b9_7f4a_7c15);
    let v = ((h1 as u128) << 64) | h2 as u128;
    let mut s = String::with_capacity(22);
    s.push(B64[((v >> 126) & 3) as usize] as char);
    for i in 1..22 {
        s.push(B64[((v >> (126 - 6 * i)) & 63) as usize] as char);
    }
    s
}

fn is_guid(s: &str) -> bool {
    s.len() == 22 && s.bytes().all(|c| B64.contains(&c)) && matches!(s.as_bytes()[0], b'0'..=b'3')
}

fn st(t: &str) -> String {
    let mut o = String::from("'");
    for c in t.chars() {
        match c {
            '\'' => o.push_str("''"),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push(' '),
            c if c.is_ascii() => o.push(c),
            c => {
                let mut b = [0u16; 2];
                let _ = write!(o, "\\X2\\");
                for u in c.encode_utf16(&mut b) {
                    let _ = write!(o, "{u:04X}");
                }
                let _ = write!(o, "\\X0\\");
            }
        }
    }
    o.push('\'');
    o
}

fn f(x: f64) -> String {
    let x = if x.abs() < 1e-12 { 0.0 } else { x };
    let t = format!("{x:.9}");
    let t = t.trim_end_matches('0');
    t.to_string()
}

fn refs(v: &[usize]) -> String {
    v.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",")
}

/// The model as an IFC4 file (m); with a report, each member's and slab's design ratio.
pub fn export(model: &Model, rep: Option<&Report>) -> String {
    let seed = fnv(model.name.as_bytes(), 11);
    let g = |k: &str| st(&guid(seed, k));
    let mut w = W { out: String::new(), n: 0 };
    let pt3 = |w: &mut W, p: [f64; 3]| w.add(format!("IFCCARTESIANPOINT(({},{},{}))", f(p[0]), f(p[1]), f(p[2])));
    let dir3 = |w: &mut W, p: [f64; 3]| w.add(format!("IFCDIRECTION(({},{},{}))", f(p[0]), f(p[1]), f(p[2])));
    let units: Vec<usize> = [("LENGTHUNIT", "METRE"), ("AREAUNIT", "SQUARE_METRE"), ("VOLUMEUNIT", "CUBIC_METRE"), ("PLANEANGLEUNIT", "RADIAN")].iter().map(|(k, u)| w.add(format!("IFCSIUNIT(*,.{k}.,$,.{u}.)"))).collect();
    let ua = w.add(format!("IFCUNITASSIGNMENT(({}))", refs(&units)));
    let o = pt3(&mut w, [0.0; 3]);
    let wcs = w.add(format!("IFCAXIS2PLACEMENT3D(#{o},$,$)"));
    let ctx = w.add(format!("IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#{wcs},$)"));
    let body = w.add(format!("IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Body','Model',*,*,*,*,#{ctx},$,.MODEL_VIEW.,$)"));
    let axisctx = w.add(format!("IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Axis','Model',*,*,*,*,#{ctx},$,.GRAPH_VIEW.,$)"));
    let proj = w.add(format!("IFCPROJECT({},$,{},$,$,$,$,(#{ctx}),#{ua})", g("project"), st(&model.name)));
    let place = |w: &mut W, rel: Option<usize>, o: [f64; 3], z: Option<[f64; 3]>, x: Option<[f64; 3]>| -> usize {
        let op = pt3(w, o);
        let ax = match (z, x) {
            (Some(z), Some(x)) => {
                let (zi, xi) = (dir3(w, z), dir3(w, x));
                w.add(format!("IFCAXIS2PLACEMENT3D(#{op},#{zi},#{xi})"))
            }
            _ => w.add(format!("IFCAXIS2PLACEMENT3D(#{op},$,$)")),
        };
        let r = rel.map_or("$".to_string(), |r| format!("#{r}"));
        w.add(format!("IFCLOCALPLACEMENT({r},#{ax})"))
    };
    let site_pl = place(&mut w, None, [0.0; 3], None, None);
    let site = w.add(format!("IFCSITE({},$,'Site',$,$,#{site_pl},$,$,.ELEMENT.,$,$,$,$,$)", g("site")));
    let bpl = place(&mut w, Some(site_pl), [0.0; 3], None, None);
    let bld = w.add(format!("IFCBUILDING({},$,{},$,$,#{bpl},$,$,.ELEMENT.,$,$,$)", g("building"), st(&model.name)));
    w.add(format!("IFCRELAGGREGATES({},$,$,$,#{proj},(#{site}))", g("rel-project-site")));
    w.add(format!("IFCRELAGGREGATES({},$,$,$,#{site},(#{bld}))", g("rel-site-building")));
    // storeys: every level with a node or a slab
    let mut levels: Vec<f64> = model.nodes.iter().map(|n| n.z).chain(model.slabs.iter().map(|s| s.z)).map(|z| (z * 1000.0).round() / 1000.0).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    if levels.is_empty() {
        levels.push(0.0);
    }
    let mut storeys: Vec<(f64, usize, usize)> = vec![];
    for (k, z) in levels.iter().enumerate() {
        let pl = place(&mut w, Some(bpl), [0.0, 0.0, *z], None, None);
        let s = w.add(format!("IFCBUILDINGSTOREY({},$,{},$,$,#{pl},$,$,.ELEMENT.,{})", g(&format!("level-{z}")), st(&format!("Level {} (z {:.3})", k + 1, z)), f(*z)));
        storeys.push((*z, s, pl));
    }
    w.add(format!("IFCRELAGGREGATES({},$,$,$,#{bld},({}))", g("rel-building-storeys"), refs(&storeys.iter().map(|s| s.1).collect::<Vec<_>>())));
    let storey_of = |z: f64| -> usize {
        let k = storeys.iter().rposition(|s| s.0 <= z + 1e-6).unwrap_or(0);
        k
    };
    let mut contained: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut by_material: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut profiles: HashMap<Id, usize> = HashMap::new();
    for m in &model.members {
        let (Some(a), Some(b), Some((ex, ey, _))) = (model.node(m.i), model.node(m.j), crate::analysis::axes(model, m)) else { continue };
        let Some(sec) = model.section(m.section) else { continue };
        let lo = storey_of(a.z.min(b.z));
        let (sz, _, spl) = storeys[lo];
        let prof = *profiles.entry(sec.id).or_insert_with(|| {
            let o2 = w.add("IFCCARTESIANPOINT((0.,0.))".into());
            let p2 = w.add(format!("IFCAXIS2PLACEMENT2D(#{o2},$)"));
            match sec.shape {
                Shape::Rect { b, h } => w.add(format!("IFCRECTANGLEPROFILEDEF(.AREA.,{},#{p2},{},{})", st(&sec.name), f(b), f(h))),
                Shape::H { d, bf, tw, tf, r } => w.add(format!("IFCISHAPEPROFILEDEF(.AREA.,{},#{p2},{},{},{},{},{},$,$)", st(&sec.name), f(bf), f(d), f(tw), f(tf), f(r))),
                Shape::LippedC { h, b, d, t } => w.add(format!("IFCCSHAPEPROFILEDEF(.AREA.,{},#{p2},{},{},{},{},$)", st(&sec.name), f(h), f(b), f(t), f(d))),
                Shape::General { a, iy, .. } => {
                    let h = (12.0 * iy / a.max(1e-12)).sqrt();
                    w.add(format!("IFCRECTANGLEPROFILEDEF(.AREA.,{},#{p2},{},{})", st(&sec.name), f(a / h.max(1e-9)), f(h)))
                }
            }
        });
        let len = model.length(m);
        let pl = place(&mut w, Some(spl), [a.x, a.y, a.z - sz], Some(ex), Some(ey));
        let o0 = pt3(&mut w, [0.0; 3]);
        let pos = w.add(format!("IFCAXIS2PLACEMENT3D(#{o0},$,$)"));
        let up = dir3(&mut w, [0.0, 0.0, 1.0]);
        let solid = w.add(format!("IFCEXTRUDEDAREASOLID(#{prof},#{pos},#{up},{})", f(len)));
        let brep = w.add(format!("IFCSHAPEREPRESENTATION(#{body},'Body','SweptSolid',(#{solid}))"));
        let o1 = pt3(&mut w, [0.0, 0.0, len]);
        let line = w.add(format!("IFCPOLYLINE((#{o0},#{o1}))"));
        let arep = w.add(format!("IFCSHAPEREPRESENTATION(#{axisctx},'Axis','Curve3D',(#{line}))"));
        let shape = w.add(format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{arep},#{brep}))"));
        let gid = m.bim_id.clone().filter(|s| is_guid(s)).unwrap_or_else(|| guid(seed, &format!("member-{}", m.id)));
        let (class, pt, word) = match model.kind(m) {
            MemberKind::Column => ("IFCCOLUMN", ".COLUMN.", "Column"),
            MemberKind::Brace => ("IFCMEMBER", ".BRACE.", "Brace"),
            _ => ("IFCBEAM", ".BEAM.", "Beam"),
        };
        let e = w.add(format!("{class}({},$,{},$,{},#{pl},#{shape},{},{pt})", st(&gid), st(&format!("{word} {}", m.id)), st(&sec.name), st(&m.id.to_string())));
        contained.entry(lo).or_default().push(e);
        let mat = model.material(m.material).map(|x| x.name.clone()).unwrap_or_else(|| "Material".into());
        by_material.entry(mat).or_default().push(e);
        // the design ratio and what the member is in Pondus
        let mut props = vec![
            w.add(format!("IFCPROPERTYSINGLEVALUE('PondusMember',$,IFCINTEGER({}),$)", m.id)),
            w.add(format!("IFCPROPERTYSINGLEVALUE('Section',$,IFCLABEL({}),$)", st(&sec.name))),
        ];
        if let Some(ratio) = rep.and_then(|r| crate::scene::member_ratio(r, m.id)) {
            props.push(w.add(format!("IFCPROPERTYSINGLEVALUE('DesignRatio',$,IFCRATIOMEASURE({}),$)", f((ratio * 1e4).round() / 1e4))));
            props.push(w.add(format!("IFCPROPERTYSINGLEVALUE('DesignStatus',$,IFCLABEL({}),$)", st(if ratio <= 1.0 { "OK" } else { "FAILS" }))));
        }
        let ps = w.add(format!("IFCPROPERTYSET({},$,'Pondus_Design',$,({}))", g(&format!("pset-member-{}", m.id)), refs(&props)));
        w.add(format!("IFCRELDEFINESBYPROPERTIES({},$,$,$,(#{e}),#{ps})", g(&format!("rel-pset-member-{}", m.id))));
    }
    for s in &model.slabs {
        let k = storeys.iter().position(|q| (q.0 - s.z).abs() < 1e-3).unwrap_or_else(|| storey_of(s.z));
        let (sz, _, spl) = storeys[k];
        let (lo, hi) = (s.lo(), s.hi());
        let pl = place(&mut w, Some(spl), [0.0; 3], None, None);
        let c2 = w.add(format!("IFCCARTESIANPOINT(({},{}))", f((lo.0 + hi.0) / 2.0), f((lo.1 + hi.1) / 2.0)));
        let p2 = w.add(format!("IFCAXIS2PLACEMENT2D(#{c2},$)"));
        let prof = w.add(format!("IFCRECTANGLEPROFILEDEF(.AREA.,$,#{p2},{},{})", f(hi.0 - lo.0), f(hi.1 - lo.1)));
        let o = pt3(&mut w, [0.0, 0.0, s.z - sz - s.h]);
        let pos = w.add(format!("IFCAXIS2PLACEMENT3D(#{o},$,$)"));
        let up = dir3(&mut w, [0.0, 0.0, 1.0]);
        let solid = w.add(format!("IFCEXTRUDEDAREASOLID(#{prof},#{pos},#{up},{})", f(s.h)));
        let brep = w.add(format!("IFCSHAPEREPRESENTATION(#{body},'Body','SweptSolid',(#{solid}))"));
        let shape = w.add(format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{brep}))"));
        let gid = s.bim_id.clone().filter(|x| is_guid(x)).unwrap_or_else(|| guid(seed, &format!("slab-{}", s.id)));
        let e = w.add(format!("IFCSLAB({},$,{},$,{},#{pl},#{shape},{},.FLOOR.)", st(&gid), st(&s.name), st(&format!("Slab {:.0} mm", s.h * 1000.0)), st(&format!("S{}", s.id))));
        contained.entry(k).or_default().push(e);
        by_material.entry(model.material(model.concrete_id()).map(|x| x.name.clone()).unwrap_or_else(|| "Concrete".into())).or_default().push(e);
        if let Some(d) = rep.and_then(|r| r.slabs.iter().find(|d| d.slab == s.id)) {
            let p = w.add(format!("IFCPROPERTYSINGLEVALUE('DesignRatio',$,IFCRATIOMEASURE({}),$)", f((d.ratio * 1e4).round() / 1e4)));
            let ps = w.add(format!("IFCPROPERTYSET({},$,'Pondus_Design',$,(#{p}))", g(&format!("pset-slab-{}", s.id))));
            w.add(format!("IFCRELDEFINESBYPROPERTIES({},$,$,$,(#{e}),#{ps})", g(&format!("rel-pset-slab-{}", s.id))));
        }
    }
    for (name, els) in &by_material {
        let m = w.add(format!("IFCMATERIAL({},$,$)", st(name)));
        w.add(format!("IFCRELASSOCIATESMATERIAL({},$,$,$,({}),#{m})", g(&format!("rel-material-{name}")), refs(els)));
    }
    for (k, els) in &contained {
        w.add(format!("IFCRELCONTAINEDINSPATIALSTRUCTURE({},$,$,$,({}),#{})", g(&format!("rel-contained-{k}")), refs(els), storeys[*k].1));
    }
    format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [ReferenceView]'),'2;1');\nFILE_NAME({},'2026-01-01T00:00:00',(''),(''),'Pondus 0.1.0','Pondus','');\nFILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n{}ENDSEC;\nEND-ISO-10303-21;\n",
        st(&format!("{}.ifc", model.name)),
        w.out
    )
}
