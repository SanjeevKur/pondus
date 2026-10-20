//! Shear walls: the wide-column model against the cantilever with bending and shear
//! deformation (Timoshenko), the frame connection along the wall, and the ACI 318-19 design.

use pondus_core::analysis::analyze;
use pondus_core::commands::run;
use pondus_core::model::*;
use serde_json::json;

fn cantilever_wall(along_y: bool) -> (Model, Id) {
    let mut m = Model::new("wall");
    let (a, b) = if along_y { ([0.0, 0.0], [0.0, 3.0]) } else { ([0.0, 0.0], [3.0, 0.0]) };
    run(&mut m, "add_walls", &json!({"walls": [{"from": a, "to": b, "t": 0.3, "z0": 0.0, "z1": 10.0}], "storey_h": 2.5})).unwrap();
    m.cases[0].self_weight = false;
    let top = m.find_node([b[0], b[1], 10.0]).expect("the wall's end at the top");
    (m, top)
}

#[test]
fn cantilever_wall_bending_and_shear() {
    for along_y in [false, true] {
        let (mut m, top) = cantilever_wall(along_y);
        assert_eq!(m.members.iter().filter(|x| matches!(x.role, Role::WallPier(_))).count(), 4);
        assert_eq!(m.supports.len(), 1);
        // 100 kN in the wall's plane at its top end
        let p = 100.0;
        let f = if along_y { [0.0, p, 0.0, 0.0, 0.0, 0.0] } else { [p, 0.0, 0.0, 0.0, 0.0, 0.0] };
        m.loads.push(Load { case: 2, kind: LoadKind::Nodal { node: top, f } });
        let a = analyze(&m).unwrap();
        let c = a.case(2).unwrap();
        let mat = &m.materials[0];
        let (e, nu) = (mat.e * 1e3, mat.nu);
        let g = e / (2.0 * (1.0 + nu));
        let (t, l, h): (f64, f64, f64) = (0.3, 3.0, 10.0);
        let i = t * l * l * l / 12.0;
        let bend = p * h.powi(3) / (3.0 * e * i);
        let shear = p * h / (5.0 / 6.0 * t * l * g);
        let d = c.displacements[&top][if along_y { 1 } else { 0 }];
        eprintln!("wall along {}: top {:.4} mm, PH³/3EI + PH/κGA = {:.4} + {:.4} = {:.4} mm", if along_y { "y" } else { "x" }, d * 1e3, bend * 1e3, shear * 1e3, (bend + shear) * 1e3);
        assert!((d - (bend + shear)).abs() / (bend + shear) < 0.005, "{d} vs {}", bend + shear);
        // the base takes P and P·H
        let r = c.reactions.iter().find(|r| r.0 != top).unwrap().1;
        let (fh, mb) = if along_y { (r[1], r[3]) } else { (r[0], r[4]) };
        assert!((fh.abs() - p).abs() < 1e-6 * p && (mb.abs() - p * h).abs() < 1e-6 * p * h, "{r:?}");
    }
}

#[test]
fn wall_out_of_plane_is_the_weak_axis() {
    let (mut m, top) = cantilever_wall(false);
    m.loads.push(Load { case: 2, kind: LoadKind::Nodal { node: top, f: [0.0, 10.0, 0.0, 0.0, 0.0, 0.0] } });
    let a = analyze(&m).unwrap();
    let d = a.case(2).unwrap().displacements[&top][1];
    let e = m.materials[0].e * 1e3;
    let g = e / (2.0 * (1.0 + m.materials[0].nu));
    let (t, l, h): (f64, f64, f64) = (0.3, 3.0, 10.0);
    let want = 10.0 * h.powi(3) / (3.0 * e * l * t.powi(3) / 12.0) + 10.0 * h / (5.0 / 6.0 * t * l * g);
    // the load at the end also twists the wall: compare the centre line's sway
    let centre = m.find_node([1.5, 0.0, 10.0]).unwrap();
    let dc = a.case(2).unwrap().displacements[&centre][1];
    eprintln!("out of plane: centre {:.3} mm (end {:.3}), closed form {:.3} mm", dc * 1e3, d * 1e3, want * 1e3);
    assert!((dc - want).abs() / want < 0.005);
}

#[test]
fn frame_with_a_wall_connects_and_designs() {
    // a 2 x 1 bay, 4-storey frame; a wall on the grid line x = 0 between the columns at y = 0
    // and y = 5 replaces nothing: the beams along x = 0 meet its arms
    let mut m = Model::new("frame + wall");
    run(&mut m, "add_frame", &json!({"x_spans": [6.0, 6.0], "y_spans": [5.0], "storeys": [3.5, 3.5, 3.5, 3.5], "column": "400 x 400", "beam": "300 x 600", "slab": {"h": 0.15, "dead": 1.5, "live": 3.0}})).unwrap();
    let r = run(&mut m, "add_walls", &json!({"walls": [{"from": [0.0, 0.0], "to": [0.0, 5.0], "t": 0.25}]})).unwrap();
    eprintln!("add_walls: {r}");
    assert_eq!(m.members.iter().filter(|x| matches!(x.role, Role::WallPier(_))).count(), 4);
    // a lateral load case along y: the wall, stiff in y, takes most of it
    let wcase = m.cases.len() as Id + 1;
    m.cases.push(LoadCase { id: wcase, name: "Wy".into(), kind: CaseKind::W, self_weight: false });
    // first on the wall's own line (x = 0): no eccentricity
    let tops: Vec<Id> = [[0.0, 0.0, 14.0], [0.0, 5.0, 14.0]].iter().map(|p| m.find_node(*p).unwrap()).collect();
    for n in &tops {
        m.loads.push(Load { case: wcase, kind: LoadKind::Nodal { node: *n, f: [0.0, 45.0, 0.0, 0.0, 0.0, 0.0] } });
    }
    m.diaphragms = vec![3.5, 7.0, 10.5, 14.0];
    let pier = m.members.iter().filter(|x| matches!(x.role, Role::WallPier(_))).min_by(|a, b| m.node(a.i).unwrap().z.total_cmp(&m.node(b.i).unwrap().z)).unwrap().id;
    let share = |m: &Model, case: Id| -> f64 {
        let a = analyze(m).unwrap();
        let st = &a.case(case).unwrap().member(pier).unwrap().stations[0];
        st.vz.abs().max(st.vy.abs())
    };
    let on_line = share(&m, wcase);
    eprintln!("lateral y on the wall's line: the wall takes {on_line:.1} of 90 kN");
    assert!(on_line > 0.9 * 90.0, "{on_line}");
    // the same 90 kN at the far edge (x = 12 m) twists the floor: the frames across take a share
    let far: Vec<Id> = [[12.0, 0.0, 14.0], [12.0, 5.0, 14.0]].iter().map(|p| m.find_node(*p).unwrap()).collect();
    let mut m2 = m.clone();
    m2.loads.retain(|l| l.case != wcase);
    for n in &far {
        m2.loads.push(Load { case: wcase, kind: LoadKind::Nodal { node: *n, f: [0.0, 45.0, 0.0, 0.0, 0.0, 0.0] } });
    }
    // the floor turns about a point near the stiff wall, so the frames away from it take the
    // load: the x = 12 m frame carries most of it, the wall little (asymmetric stiffness)
    let a2 = analyze(&m2).unwrap();
    let c2 = a2.case(wcase).unwrap();
    let far_frame: f64 = c2.reactions.iter().filter(|r| m2.node(r.0).is_some_and(|n| (n.x - 12.0).abs() < 1e-6)).map(|r| -r.1[1]).sum();
    let off = share(&m2, wcase);
    eprintln!("the same at the far edge: the x = 12 m frame takes {far_frame:.1} kN, the wall {off:.1} kN (the floor turns about the wall)");
    assert!(far_frame > 0.5 * 90.0 && off < 0.2 * 90.0, "{far_frame} {off}");
    let a = analyze(&m).unwrap();
    let c = a.case(wcase).unwrap();
    let rs = c.reaction_sum();
    assert!((rs[1] + 90.0).abs() < 1e-6, "{rs:?}");

    // gravity: the slab load still reaches the beams (and the arms along the wall)
    assert!((a.slab_applied[0] - a.slab_expected[0]).abs() < 1e-6 * a.slab_expected[0], "{:?} {:?}", a.slab_applied, a.slab_expected);
    // design: one row per storey, with bars, a shear check and no column design of the piers
    m.combos.push(Combination { id: 10, name: "ULS 1.2D + 1.0Wy + 1.0L".into(), kind: ComboKind::Uls, factors: vec![(1, 1.2), (2, 1.0), (wcase, 1.0)] });
    let a = analyze(&m).unwrap();
    let rep = pondus_core::design::design_pass(&m, &a, &Default::default());
    assert_eq!(rep.walls.len(), 4, "{:?}", rep.walls.iter().map(|w| w.z0).collect::<Vec<_>>());
    assert!(rep.columns.iter().all(|c| m.member(c.member).unwrap().role.is_frame()));
    assert!(rep.beams.iter().all(|c| m.member(c.member).unwrap().role.is_frame()));
    for w in &rep.walls {
        eprintln!("wall {} z {:.1}-{:.1}: {} / {}, Vu {:.0} φVn {:.0} kN, ratio {:.2} ({})", w.name, w.z0, w.z1, w.vertical, w.horizontal, w.vu, w.phi_vn, w.ratio, w.governs);
        assert!(w.ok, "{:?}", w.checks);
        assert!(w.rho_t >= 0.0020 - 1e-9 && w.rho_l >= 0.0012 - 1e-9);
    }
    // the base storey: φVn by hand from the reported ρt
    let w = &rep.walls[0];
    let acv = 250.0 * 5000.0;
    let fc: f64 = 24.0;
    let hw_lw: f64 = 14.0 / 5.0;
    let ac = if hw_lw >= 2.0 { 0.17 } else { 0.25 };
    let want = 0.75 * ((ac * fc.sqrt() + w.rho_t * 390.0) * acv).min(0.66 * fc.sqrt() * acv) / 1e3;
    assert!((w.phi_vn - want).abs() < 1e-6 * want, "{} vs {want}", w.phi_vn);
    // BIM: one wall, not its piers and arms
    let bim = pondus_core::design::bim_export(&m, &rep);
    assert_eq!(bim["walls"].as_array().unwrap().len(), 1);
    assert!(bim["members"].as_array().unwrap().iter().all(|e| e["section"].as_str().map(|s| !s.starts_with("Wall ") && !s.starts_with("Rigid")).unwrap_or(true)));
}

#[test]
fn walls_follow_new_floors_and_delete() {
    let mut m = Model::new("w");
    run(&mut m, "add_walls", &json!({"walls": [{"from": [0.0, 0.0], "to": [4.0, 0.0], "t": 0.2, "z0": 0.0, "z1": 6.0}], "storey_h": 3.0})).unwrap();
    assert_eq!(m.members.iter().filter(|x| matches!(x.role, Role::WallPier(_))).count(), 2);
    // a floor at 4.5 m appears: the wall gets a joint there
    run(&mut m, "add_members", &json!({"members": [{"from": [4.0, 0.0, 4.5], "to": [8.0, 0.0, 4.5], "section": "300 x 500"}]})).unwrap();
    assert_eq!(m.members.iter().filter(|x| matches!(x.role, Role::WallPier(_))).count(), 3);
    run(&mut m, "set_walls", &json!({"all": true, "t": 0.3})).unwrap();
    assert!(m.members.iter().filter(|x| matches!(x.role, Role::WallPier(_))).all(|x| m.section(x.section).unwrap().name == "Wall 300 x 4000 mm"));
    run(&mut m, "delete", &json!({"walls": [1]})).unwrap();
    assert!(m.walls.is_empty() && m.members.iter().all(|x| x.role.is_frame()), "{:?}", m.members.iter().map(|x| x.role).collect::<Vec<_>>());
}

#[test]
fn loads_by_height_skip_wall_arms() {
    // a floor beam meeting a wall, and a line load on "all beams at z": the wall's rigid arms at
    // that level are no floor beams and must not take the load
    use pondus_core::commands::run;
    use pondus_core::model::*;
    use serde_json::json;
    let mut m = Model::new("arms");
    run(&mut m, "add_walls", &json!({"walls": [{"from": [0.0, 0.0], "to": [4.0, 0.0], "t": 0.3, "z0": 0.0, "z1": 3.0}], "levels": [0.0, 3.0]})).unwrap();
    run(&mut m, "add_members", &json!({"members": [{"from": [2.0, 0.0, 3.0], "to": [2.0, 6.0, 3.0], "section": "300 x 600", "kind": "beam"}, {"from": [2.0, 6.0, 0.0], "to": [2.0, 6.0, 3.0], "section": "400 x 400", "kind": "column"}]})).unwrap();
    run(&mut m, "add_loads", &json!({"case": "D", "loads": [{"type": "line", "z": 3.0, "w": [0.0, 0.0, -10.0]}]})).unwrap();
    let loaded: Vec<Id> = m.loads.iter().filter_map(|l| match l.kind { LoadKind::Line { member, .. } => Some(member), _ => None }).collect();
    assert_eq!(loaded.len(), 1, "{loaded:?}");
    assert!(m.member(loaded[0]).unwrap().role.is_frame());
}
