//! The editor transforms (split, merge, copy, mirror, rotate) and the Excel round trip.

use pondus_core::analysis::analyze;
use pondus_core::model::*;
use pondus_core::transform::*;
use serde_json::json;

fn mem(m: &mut Model, a: [f64; 3], b: [f64; 3], sec: &str) -> Id {
    let s = m.section_named(sec).unwrap();
    let (i, j) = (m.node_at(a), m.node_at(b));
    let id = m.next_member_id();
    m.members.push(Member { id, i, j, section: s, material: 1, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    id
}

/// A 6 m simple beam with 10 kN/m over the span, 4 kN/m over 1–2.5 m and 20 kN at 4.2 m (live).
fn ss_beam() -> Model {
    let mut m = Model::new("ss");
    m.cases[0].self_weight = false;
    mem(&mut m, [0.0, 0.0, 0.0], [6.0, 0.0, 0.0], "300 x 600");
    let (a, b) = (m.find_node([0.0; 3]).unwrap(), m.find_node([6.0, 0.0, 0.0]).unwrap());
    m.supports.push(Support { node: a, fix: [true, true, true, true, false, false], springs: [0.0; 6], uplift: false });
    m.supports.push(Support { node: b, fix: [false, true, true, false, false, false], springs: [0.0; 6], uplift: false });
    m.loads.push(Load { case: 1, kind: LoadKind::Line { member: 1, wi: [0.0, 0.0, -10.0], wj: [0.0, 0.0, -10.0], a: None, b: None } });
    m.loads.push(Load { case: 2, kind: LoadKind::Line { member: 1, wi: [0.0, 0.0, -4.0], wj: [0.0, 0.0, -8.0], a: Some(1.0), b: Some(4.5) } });
    m.loads.push(Load { case: 2, kind: LoadKind::Point { member: 1, a: 4.2, p: [0.0, 0.0, -20.0] } });
    m
}

/// My along the line of members, sampled at x (m from the start) for case `c`.
fn my_at(m: &Model, c: Id, xs: &[f64]) -> (Vec<f64>, [f64; 6]) {
    let a = analyze(m).unwrap();
    let r = a.case(c).unwrap();
    let mut out = vec![];
    for x in xs {
        // the member holding x, measured from node (0,0,0)
        let mut v = f64::NAN;
        for mr in &r.members {
            let mb = m.member(mr.member).unwrap();
            let (pi, pj) = (m.node(mb.i).unwrap().p(), m.node(mb.j).unwrap().p());
            let (s0, s1) = (pi[0].min(pj[0]), pi[0].max(pj[0]));
            if *x >= s0 - 1e-9 && *x <= s1 + 1e-9 {
                let local = if pi[0] <= pj[0] { x - pi[0] } else { pi[0] - x };
                // linear between the stations either side (My is linear where no point load acts)
                for w in mr.stations.windows(2) {
                    if local >= w[0].x - 1e-9 && local <= w[1].x + 1e-9 && w[1].x > w[0].x {
                        let f = (local - w[0].x) / (w[1].x - w[0].x);
                        v = w[0].my + f * (w[1].my - w[0].my);
                    }
                }
            }
        }
        out.push(v);
    }
    let mut re = [0.0; 6];
    for (n, f) in &r.reactions {
        let k = if m.node(*n).unwrap().x < 3.0 { 0 } else { 3 };
        re[k] += f[2];
    }
    (out, re)
}

#[test]
fn split_and_merge_keep_the_answer() {
    let m0 = ss_beam();
    let xs = [1.5, 2.0, 3.0, 4.0, 4.5];
    let (d0, r0) = my_at(&m0, 1, &xs);
    let (l0, rl0) = my_at(&m0, 2, &xs);
    // D: wL²/8 at midspan
    assert!((d0[2] - 45.0).abs() < 0.2, "{d0:?}");
    let mut m = m0.clone();
    let e = split_member(&mut m, 1, &[], 3).unwrap();
    assert_eq!(e.members.len(), 3);
    assert_eq!(m.members.len(), 3);
    eprintln!("split loads: {}", serde_json::to_string(&m.loads).unwrap());
    let (d1, r1) = my_at(&m, 1, &xs);
    let (l1, rl1) = my_at(&m, 2, &xs);
    eprintln!("D: one member {d0:?}, three {d1:?}; reactions {r0:?} {r1:?}");
    eprintln!("L: one member {l0:?}, three {l1:?}; reactions {rl0:?} {rl1:?}");
    for k in 0..xs.len() {
        assert!((d0[k] - d1[k]).abs() < 0.3 && (l0[k] - l1[k]).abs() < 0.3, "x {}", xs[k]);
    }
    assert!((r0[0] - r1[0]).abs() < 1e-9 && (rl0[3] - rl1[3]).abs() < 1e-9);
    // merge back, piece by piece
    let p = e.members.clone();
    merge_members(&mut m, p[0], p[1]).unwrap();
    merge_members(&mut m, p[0], p[2]).unwrap();
    assert_eq!(m.members.len(), 1);
    assert_eq!(m.nodes.len(), 2);
    let (d2, r2) = my_at(&m, 1, &xs);
    let (l2, rl2) = my_at(&m, 2, &xs);
    assert!((r0[0] - r2[0]).abs() < 1e-9 && (rl0[0] - rl2[0]).abs() < 1e-9 && (rl0[3] - rl2[3]).abs() < 1e-9, "{rl0:?} {rl2:?}");
    for k in 0..xs.len() {
        assert!((d0[k] - d2[k]).abs() < 0.3 && (l0[k] - l2[k]).abs() < 0.3, "merged, x {}", xs[k]);
    }
    eprintln!("merged loads: {}", serde_json::to_string(&m.loads).unwrap());
    // the cut line loads are one load each again, as before the split
    assert_eq!(m.loads.len(), 3);
    assert_eq!(m.loads[0].kind, m0.loads[0].kind);
    match (&m.loads[1].kind, &m0.loads[1].kind) {
        (LoadKind::Line { wi, wj, a, b, .. }, LoadKind::Line { wi: wi0, wj: wj0, a: a0, b: b0, .. }) => {
            assert!((0..3).all(|c| (wi[c] - wi0[c]).abs() < 1e-9 && (wj[c] - wj0[c]).abs() < 1e-9) && (a.unwrap() - a0.unwrap()).abs() < 1e-9 && (b.unwrap() - b0.unwrap()).abs() < 1e-9);
        }
        _ => panic!(),
    }
    // merging over a node with another member fails
    let mut m3 = m0.clone();
    split_member(&mut m3, 1, &[0.5], 0).unwrap();
    let mid = m3.find_node([3.0, 0.0, 0.0]).unwrap();
    let p = m3.node(mid).unwrap().p();
    mem(&mut m3, p, [3.0, 0.0, -3.0], "300 x 300");
    assert!(merge_members(&mut m3, 1, 2).is_err());
}

#[test]
fn reversed_merge_maps_loads() {
    // two members pointing at each other: 0→3 and 6→3, a point load on the second at 1 m
    // from its start (x = 5 along the merged member)
    let mut m = Model::new("rev");
    m.cases[0].self_weight = false;
    mem(&mut m, [0.0, 0.0, 0.0], [3.0, 0.0, 0.0], "300 x 600");
    mem(&mut m, [6.0, 0.0, 0.0], [3.0, 0.0, 0.0], "300 x 600");
    m.loads.push(Load { case: 1, kind: LoadKind::Point { member: 2, a: 1.0, p: [0.0, 0.0, -10.0] } });
    m.loads.push(Load { case: 1, kind: LoadKind::Line { member: 2, wi: [0.0, 0.0, -1.0], wj: [0.0, 0.0, -2.0], a: Some(0.5), b: Some(2.0) } });
    merge_members(&mut m, 1, 2).unwrap();
    let mb = m.member(1).unwrap();
    assert_eq!(m.node(mb.j).unwrap().x, 6.0);
    match &m.loads[0].kind {
        LoadKind::Point { a, .. } => assert!((a - 5.0).abs() < 1e-12, "{a}"),
        _ => panic!(),
    }
    match &m.loads[1].kind {
        // 6 − 2 = 4 .. 6 − 0.5 = 5.5, the intensities turned round
        LoadKind::Line { wi, wj, a, b, .. } => assert!((a.unwrap() - 4.0).abs() < 1e-12 && (b.unwrap() - 5.5).abs() < 1e-12 && wi[2] == -2.0 && wj[2] == -1.0),
        _ => panic!(),
    }
}

/// A plane portal 6 m x 4 m in the x-z plane at y, fixed bases, a beam load.
fn portal(m: &mut Model, y: f64) -> Vec<Id> {
    let c1 = mem(m, [0.0, y, 0.0], [0.0, y, 4.0], "400 x 400");
    let c2 = mem(m, [6.0, y, 0.0], [6.0, y, 4.0], "400 x 400");
    let b = mem(m, [0.0, y, 4.0], [6.0, y, 4.0], "300 x 600");
    for x in [0.0, 6.0] {
        let n = m.find_node([x, y, 0.0]).unwrap();
        m.supports.push(Support::of_kind(n, "fixed").unwrap());
    }
    m.loads.push(Load { case: 1, kind: LoadKind::Line { member: b, wi: [0.0, 0.0, -12.0], wj: [0.0, 0.0, -12.0], a: None, b: None } });
    vec![c1, c2, b]
}

#[test]
fn array_copy_joins_the_frame() {
    let mut m = Model::new("bays");
    let p = portal(&mut m, 0.0);
    // four frames 5 m apart
    let e = move_members(&mut m, &p, [0.0, 5.0, 0.0], true, 3).unwrap();
    assert_eq!(e.members.len(), 9);
    assert_eq!(m.members.len(), 12);
    assert_eq!(m.nodes.len(), 16);
    assert_eq!(m.supports.len(), 8);
    assert_eq!(m.loads.len(), 4);
    // ties along y at the column tops; then copying a frame onto itself makes nothing
    for x in [0.0, 6.0] {
        for k in 0..3 {
            mem(&mut m, [x, 5.0 * k as f64, 4.0], [x, 5.0 * (k + 1) as f64, 4.0], "300 x 600");
        }
    }
    assert_eq!(m.nodes.len(), 16, "the ties use the copied nodes");
    let again = move_members(&mut m, &p, [0.0, 5.0, 0.0], true, 1).unwrap();
    assert!(again.members.is_empty() && again.notes.len() == 3, "{:?}", again.notes);
    // a slab on the first bay goes with a copy of the bay's four beams
    m.slabs.push(Slab { id: 1, name: "S1".into(), x0: 0.0, y0: 0.0, x1: 6.0, y1: 5.0, z: 4.0, h: 0.15, dead: 1.5, live: 2.0, bim_id: None });
    let bay: Vec<Id> = m.members.iter().filter(|q| {
        let (a, b) = (m.node(q.i).unwrap(), m.node(q.j).unwrap());
        a.z == 4.0 && b.z == 4.0 && a.y.max(b.y) <= 5.0 && a.y.min(b.y) >= 0.0
    }).map(|q| q.id).collect();
    assert_eq!(bay.len(), 4);
    // copied onto the next bay: the beams are there already, the slab is new
    let e = move_members(&mut m, &bay, [0.0, 5.0, 0.0], true, 2).unwrap();
    assert!(e.members.is_empty() && e.slabs.len() == 2, "{:?}", e);
    assert_eq!((m.slabs[1].y0, m.slabs[1].y1, m.slabs[2].y1), (5.0, 10.0, 15.0));
    let a = analyze(&m).unwrap();
    let r = a.case(1).unwrap().reaction_sum();
    eprintln!("4 frames + ties + upper bay: {} members, {} nodes, D reactions {:?}", m.members.len(), m.nodes.len(), r);
    assert!(r[2] > 4.0 * 12.0 * 6.0);
}

#[test]
fn mirror_and_rotate() {
    // a sloped rafter from (0,0,4) to (6,0,6) with roll 30 and a load with a horizontal part
    let mut m = Model::new("mirror");
    let r = mem(&mut m, [0.0, 0.0, 4.0], [6.0, 0.0, 6.0], "H 300x150x6.5x9");
    m.members[0].roll = 30.0;
    m.loads.push(Load { case: 2, kind: LoadKind::Line { member: r, wi: [2.0, 0.0, -5.0], wj: [2.0, 0.0, -5.0], a: None, b: None } });
    let e = mirror_members(&mut m, &[r], [6.0, 0.0, 0.0], [1.0, 0.0, 0.0], true).unwrap();
    let c = m.member(e.members[0]).unwrap().clone();
    let (a, b) = (m.node(c.i).unwrap().p(), m.node(c.j).unwrap().p());
    eprintln!("mirrored rafter {a:?} → {b:?}, roll {}", c.roll);
    assert_eq!(a, [12.0, 0.0, 4.0]);
    assert_eq!(b, [6.0, 0.0, 6.0]);
    assert_eq!(m.nodes.len(), 3, "the ridge node is shared");
    // the section axes are the mirror images: local y of the copy ∥ mirrored local y
    use dedaliano_engine::element::compute_local_axes_3d;
    let (_, y0, _) = compute_local_axes_3d(0.0, 0.0, 4.0, 6.0, 0.0, 6.0, None, None, None, Some(30.0), false);
    let (_, y1, _) = compute_local_axes_3d(a[0], a[1], a[2], b[0], b[1], b[2], None, None, None, Some(c.roll), false);
    let my0 = [-y0[0], y0[1], y0[2]];
    let dot = (my0[0] * y1[0] + my0[1] * y1[1] + my0[2] * y1[2]).abs();
    assert!((dot - 1.0).abs() < 1e-9, "{dot}");
    match &m.loads[1].kind {
        LoadKind::Line { wi, .. } => assert_eq!(*wi, [-2.0, 0.0, -5.0]),
        _ => panic!(),
    }
    // rotate a column with roll 0 by 90°: its strong axis turns from x to y, so roll ±90
    let mut m = Model::new("rot");
    let col = mem(&mut m, [2.0, 0.0, 0.0], [2.0, 0.0, 3.0], "H 300x150x6.5x9");
    let n0 = m.find_node([2.0, 0.0, 0.0]).unwrap();
    m.supports.push(Support::of_kind(n0, "fixed").unwrap());
    let top = m.find_node([2.0, 0.0, 3.0]).unwrap();
    m.loads.push(Load { case: 2, kind: LoadKind::Nodal { node: top, f: [10.0, 0.0, 0.0, 0.0, 0.0, 0.0] } });
    let (_, y0, _) = compute_local_axes_3d(2.0, 0.0, 0.0, 2.0, 0.0, 3.0, None, None, None, None, false);
    rotate_members(&mut m, &[col], [0.0; 3], 90.0, false).unwrap();
    let c = m.member(col).unwrap();
    let p = m.node(c.i).unwrap().p();
    let (_, y1, _) = compute_local_axes_3d(p[0], p[1], p[2], p[0], p[1], p[2] + 3.0, None, None, None, (c.roll != 0.0).then_some(c.roll), false);
    eprintln!("rotated column at {p:?}, roll {} (local y {y0:?} → {y1:?})", c.roll);
    assert!((p[0]).abs() < 1e-9 && (p[1] - 2.0).abs() < 1e-9);
    assert!((c.roll.abs() - 90.0).abs() < 1e-6);
    let ry = [-y0[1], y0[0], y0[2]];
    assert!(((ry[0] * y1[0] + ry[1] * y1[1] + ry[2] * y1[2]).abs() - 1.0).abs() < 1e-9);
    assert_eq!(m.supports[0].node, n0);
    match &m.loads[0].kind {
        LoadKind::Nodal { f, .. } => assert!(f[0].abs() < 1e-12 && (f[1] - 10.0).abs() < 1e-12, "{f:?}"),
        _ => panic!(),
    }
}

#[test]
fn excel_round_trip() {
    let mut m = Model::new("RC");
    pondus_core::commands::run(&mut m, "add_frame", &json!({"x_spans": [6.0, 5.0], "y_spans": [5.0], "storeys": [4.0, 3.5], "column": "400 x 400", "beam": "250 x 500", "slab": {"h": 0.14, "dead": 1.5, "live": 3.0, "roof_live": 1.0}})).unwrap();
    let beam = m.members.iter().find(|q| m.kind(q) == MemberKind::Beam).unwrap().id;
    let top = m.nodes.iter().max_by(|a, b| a.z.total_cmp(&b.z)).unwrap().id;
    let base = m.supports[0].node;
    m.loads.push(Load { case: 2, kind: LoadKind::Point { member: beam, a: 1.25, p: [0.0, 0.0, -15.0] } });
    m.loads.push(Load { case: 1, kind: LoadKind::Nodal { node: top, f: [5.0, -2.0, 0.0, 0.0, 1.5, 0.0] } });
    m.loads.push(Load { case: 1, kind: LoadKind::Settlement { node: base, d: [0.0, 0.0, -0.005, 0.0, 0.0, 0.0] } });
    m.loads.push(Load { case: 2, kind: LoadKind::Thermal { member: beam, dt: 20.0, dt_y: 0.0, dt_z: 5.0 } });
    m.loads.push(Load { case: 2, kind: LoadKind::Line { member: beam, wi: [0.0, 0.0, -3.0], wj: [0.0, 0.0, -6.0], a: Some(0.5), b: Some(3.0) } });
    m.members[0].bim_id = Some("12345".into());
    m.members[1].behaviour = Behaviour::TensionOnly;
    m.members[1].behaviour = Behaviour::Normal;
    m.members[2].group = Some("C-1".into());
    m.members[2].roll = 90.0;
    m.supports[1].uplift = false;
    m.supports[2].springs = [0.0; 6];
    let a = analyze(&m).unwrap();
    let rep = pondus_core::design::run(&mut m.clone(), &Default::default()).ok().map(|r| r.0);
    let dir = std::env::temp_dir().join(format!("pondus-xl-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("RC.xlsx");
    pondus_core::excel::export(&m, Some(&a), rep.as_ref(), &path).unwrap();
    let back = pondus_core::excel::import(&path).unwrap();
    let (j0, j1) = (serde_json::to_value(&m).unwrap(), serde_json::to_value(&back).unwrap());
    for k in j0.as_object().unwrap().keys() {
        assert_eq!(j0[k], j1[k], "{k} differs");
    }
    assert_eq!(m, back);
    let b = analyze(&back).unwrap();
    let (c0, c1) = (a.result(&m, "ULS 1.2D + 1.6L").unwrap(), b.result(&back, "ULS 1.2D + 1.6L").unwrap());
    let worst = c0.members.iter().zip(&c1.members).flat_map(|(p, q)| p.stations.iter().zip(&q.stations).map(|(s, t)| (s.my - t.my).abs())).fold(0.0, f64::max);
    eprintln!("Excel round trip: {} members, {} loads, {} sheets of design; largest My change {worst:e}", back.members.len(), back.loads.len(), rep.as_ref().map(|r| r.beams.len()).unwrap_or(0));
    assert_eq!(worst, 0.0);
    let mut wb = calamine::open_workbook_auto(&path).unwrap();
    let names = calamine::Reader::sheet_names(&mut wb);
    eprintln!("sheets: {names:?}");
    assert!(names.iter().any(|s| s == "Results") && names.iter().any(|s| s == "Design beams"));
    let _ = std::fs::remove_dir_all(&dir);
}
