// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Offscreen renders of the real window (egui_kittest on wgpu), as PhotoCraft checks its UI:
//! PONDUS_SNAPSHOTS=/some/dir cargo test -p pondus --release ui_ -- --nocapture

use crate::app::App;
use eframe::egui;
use egui_kittest::Harness;

fn shot(h: &mut Harness<'_, App>, name: &str) {
    h.run_steps(4);
    if let Some(d) = std::env::var_os("PONDUS_SNAPSHOTS").map(std::path::PathBuf::from) {
        let _ = std::fs::create_dir_all(&d);
        h.render().expect("render").save(d.join(format!("{name}.png"))).expect("save");
    }
}

#[test]
fn ui_main_window() {
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 940.0)).with_pixels_per_point(1.0).wgpu().build_eframe(|cc| App::new(cc, None));
    shot(&mut h, "01-empty");
    h.state_mut().test_frame();
    shot(&mut h, "02-frame");
    h.state_mut().test_analyze();
    shot(&mut h, "03-moment");
    h.state_mut().test_design();
    shot(&mut h, "04-utilisation");
    h.state_mut().test_select_first_beam();
    shot(&mut h, "05-beam-properties");
    h.state_mut().test_view("plan");
    shot(&mut h, "06-plan");
    h.state_mut().test_tool(crate::tools::Tool::Member);
    shot(&mut h, "07-member-tool");
    h.state_mut().test_tool(crate::tools::Tool::Load);
    shot(&mut h, "08-load-tool");
    h.state_mut().test_tab(crate::app::BottomTab::Members);
    shot(&mut h, "09-members-table");
    h.state_mut().test_plate();
    shot(&mut h, "11-plate-my");
    h.state_mut().test_phase1();
    shot(&mut h, "14-columns-buckling");
    h.state_mut().test_seismic();
    shot(&mut h, "13-seismic");
    h.state_mut().seismic_open = false;
    h.state_mut().test_plate();
    h.state_mut().test_plate_shear();
    shot(&mut h, "12-plate-qx-values");
    // the hover finds the element under a point, in the plan and in 3D
    let (x, y) = h.state().test_plate_under(1.3, 2.2).expect("an element under (1.3, 2.2)");
    assert!((x - 1.3).abs() <= 0.25 && (y - 2.2).abs() <= 0.25, "{x} {y}");
    h.state_mut().test_view("3d");
    h.state_mut().spec.z = None;
    let (x, y) = h.state().test_plate_under(4.6, 1.1).expect("in 3D too");
    assert!((x - 4.6).abs() <= 0.25 && (y - 1.1).abs() <= 0.25, "{x} {y}");
}

/// The drawing tools, clicked through their own code (a Picked as the view reports it).
#[test]
fn ui_tools_build_a_frame() {
    use crate::tools::{LoadType, Tool};
    use crate::view::Picked;
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 940.0)).with_pixels_per_point(1.0).wgpu().build_eframe(|cc| App::new(cc, None));
    let click = |plane: Option<[f64; 3]>, node: Option<usize>, member: Option<usize>| Picked { member, node, plane, clicked: true, secondary: false, proj: None, rect: egui::Rect::NOTHING, drag: None, inside: true };
    let a = h.state_mut();
    // a portal: two columns and a beam, drawn as a chain with the member tool
    a.set_tool(Tool::Member);
    a.opts.section = "400 x 400".into();
    for p in [[0.0, 0.0, 0.0], [0.0, 0.0, 4.0], [6.0, 0.0, 4.0], [6.0, 0.0, 0.0]] {
        a.tool_click(&click(Some(p), None, None));
    }
    assert_eq!(a.model.members.len(), 3, "{:?}", a.status);
    let base: Vec<usize> = a.model.nodes.iter().filter(|n| n.z == 0.0).map(|n| n.id).collect();
    a.set_tool(Tool::Support);
    for n in &base {
        a.tool_click(&click(None, Some(*n), None));
    }
    assert_eq!(a.model.supports.len(), 2);
    let beam = a.model.members.iter().find(|m| a.model.kind(m) == pondus_core::model::MemberKind::Beam).unwrap().id;
    a.set_tool(Tool::Load);
    a.opts.load_type = LoadType::Line;
    a.opts.w = 12.0;
    a.tool_click(&click(None, None, Some(beam)));
    a.opts.load_type = LoadType::Point;
    a.opts.p = 30.0;
    a.tool_click(&click(None, None, Some(beam)));
    a.opts.load_type = LoadType::Nodal;
    let top = a.model.find_node([0.0, 0.0, 4.0]).unwrap();
    a.opts.f = [5.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    a.tool_click(&click(None, Some(top), None));
    assert_eq!(a.model.loads.len(), 3, "{}", a.status);
    a.test_analyze();
    let r = a.analysis.as_ref().unwrap().case(1).unwrap().reaction_sum();
    // D: 12 x 6 + 30 down, 5 sideways, plus the members' own weight
    let sw: f64 = a.model.members.iter().map(|m| 24.0 * a.model.section(m.section).unwrap().props().a * a.model.length(m)).sum();
    assert!((r[2] - (72.0 + 30.0 + sw)).abs() < 1e-6 && (r[0] + 5.0).abs() < 1e-6, "{r:?}");
    // hinge and delete
    a.set_tool(Tool::Hinge);
    a.tool_click(&click(None, None, Some(beam)));
    assert!(a.model.member(beam).unwrap().hinge_i && a.model.member(beam).unwrap().hinge_j);
    a.undo();
    assert!(!a.model.member(beam).unwrap().hinge_i);
    a.set_tool(Tool::Delete);
    a.tool_click(&click(None, None, Some(beam)));
    assert_eq!(a.model.members.len(), 2);
    a.undo();
    // a slab in a bay: a 6 x 5 bay of beams at z 4
    a.set_tool(Tool::Member);
    a.opts.from = None;
    a.opts.section = "300 x 600".into();
    for p in [[6.0, 0.0, 4.0], [6.0, 5.0, 4.0], [0.0, 5.0, 4.0], [0.0, 0.0, 4.0]] {
        if a.opts.from.is_none() {
            a.tool_click(&click(Some([0.0, 0.0, 4.0]), None, None));
            a.tool_click(&click(Some([6.0, 0.0, 4.0]), None, None));
        }
        a.tool_click(&click(Some(p), None, None));
    }
    a.set_tool(Tool::Slab);
    a.tool_click(&click(Some([3.0, 2.5, 4.0]), None, None));
    assert_eq!(a.model.slabs.len(), 1, "{}", a.status);
    shot(&mut h, "10-drawn-portal");
}

/// The advanced analysis window: a pushover of the designed frame, a moving axle pair over the
/// first-floor beams, an influence line and two stages.
#[test]
fn ui_advanced() {
    use crate::advanced_ui::AdvTab;
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 940.0)).with_pixels_per_point(1.0).wgpu().build_eframe(|cc| App::new(cc, None));
    h.state_mut().test_frame();
    h.state_mut().test_analyze();
    h.state_mut().test_design();
    let a = h.state_mut();
    a.adv.open = true;
    a.adv.tab = AdvTab::Pushover;
    a.run_advanced(AdvTab::Pushover);
    let r = a.adv.push_result.as_ref().unwrap_or_else(|| panic!("{}", a.status));
    assert!(r.collapse_factor > 0.0 && !r.hinges.is_empty(), "{}", a.status);
    shot(&mut h, "15-pushover");
    let a = h.state_mut();
    let z1 = a.model.nodes.iter().map(|n| n.z).filter(|z| *z > 0.0).fold(f64::MAX, f64::min);
    let beams: Vec<usize> = a.model.members.iter().filter(|m| a.model.kind(m) == pondus_core::model::MemberKind::Beam && [m.i, m.j].iter().all(|n| a.model.node(*n).is_some_and(|n| (n.z - z1).abs() < 1e-6 && n.y == 0.0))).map(|m| m.id).collect();
    a.adv.tab = AdvTab::Moving;
    a.model.moving = Some(pondus_core::advanced::MovingSpec { name: "Truck".into(), path: beams.clone(), axles: vec![(0.0, 50.0), (2.0, 50.0)], step: 0.25, impact: 0.0, factor: 1.6 });
    a.run_advanced(AdvTab::Moving);
    assert!(a.analysis.as_ref().unwrap().moving.is_some(), "{}", a.status);
    assert_eq!(a.spec.result, "moving max");
    shot(&mut h, "16-moving");
    let a = h.state_mut();
    a.adv.tab = AdvTab::Influence;
    a.adv.il_member = beams[0];
    a.adv.il_x = 1.0;
    a.run_advanced(AdvTab::Influence);
    assert!(!a.adv.il_result.is_empty(), "{}", a.status);
    shot(&mut h, "17-influence");
    let a = h.state_mut();
    a.adv.tab = AdvTab::Stages;
    let cols: Vec<usize> = a.model.members.iter().filter(|m| a.model.kind(m) == pondus_core::model::MemberKind::Column).map(|m| m.id).collect();
    let rest: Vec<usize> = a.model.members.iter().map(|m| m.id).filter(|id| !cols.contains(id)).collect();
    a.model.stages = vec![pondus_core::advanced::Stage { name: "columns".into(), members: cols, cases: vec![] }, pondus_core::advanced::Stage { name: "floors".into(), members: rest, cases: a.model.cases.iter().map(|c| c.id).collect() }];
    a.run_advanced(AdvTab::Stages);
    assert!(a.analysis.as_ref().unwrap().staged.is_some(), "{}", a.status);
    shot(&mut h, "18-stages");
}

/// Building by hand: a bay dragged on the ground plane... then columns dragged as a rectangle,
/// a bay on top with its slab, a beam dragged and one dropped, supports dropped from the
/// palette, a node dragged, and a member copied by Ctrl-drag.
#[test]
fn ui_draw_by_hand() {
    use crate::tools::Tool;
    use crate::view::{Drag, Picked};
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 940.0)).with_pixels_per_point(1.0).wgpu().build_eframe(|cc| App::new(cc, None));
    shot(&mut h, "19-empty-with-palette");
    let a = h.state_mut();
    let drag = |from: [f64; 3], to: [f64; 3]| Drag { from, to, node: None, member: None };
    // four columns from a rectangle drag on z = 0, 3.5 m up
    a.set_tool(Tool::Column);
    a.opts.col_h = 3.5;
    a.tool_drag(&drag([0.0, 0.0, 0.0], [6.0, 5.0, 0.0]), false);
    assert_eq!(a.model.members.len(), 4, "{}", a.status);
    // a bay on top: four beams and a slab
    a.set_tool(Tool::Bay);
    a.tool_drag(&drag([0.0, 0.0, 3.5], [6.0, 5.0, 3.5]), false);
    assert_eq!(a.model.members.len(), 8, "{}", a.status);
    assert_eq!(a.model.slabs.len(), 1, "{}", a.status);
    // a cantilever beam dragged out of a corner, and one dropped at the far corner
    a.set_tool(Tool::Beam);
    a.tool_drag(&drag([6.0, 5.0, 3.5], [8.0, 5.0, 3.5]), false);
    assert_eq!(a.model.members.len(), 9);
    let pick = |plane: Option<[f64; 3]>, node: Option<usize>, member: Option<usize>| Picked { member, node, plane, clicked: false, secondary: false, proj: None, rect: egui::Rect::NOTHING, drag: None, inside: true };
    let n0 = a.model.find_node([0.0, 0.0, 0.0]).unwrap();
    // supports dropped from the palette on the four column bases
    for p in [[0.0, 0.0, 0.0], [6.0, 0.0, 0.0], [6.0, 5.0, 0.0], [0.0, 5.0, 0.0]] {
        let n = a.model.find_node(p).unwrap();
        a.tool_drop(Tool::Support, Some("fixed"), &pick(Some(p), Some(n), None));
    }
    assert_eq!(a.model.supports.len(), 4, "{}", a.status);
    assert!(a.model.supports.iter().any(|s| s.node == n0 && s.fix.iter().all(|f| *f)));
    // a beam dropped on an existing bay edge is not added twice
    a.tool_drop(Tool::Beam, None, &pick(Some([0.0, 5.0, 3.5]), None, None));
    assert_eq!(a.model.members.len(), 9, "{}", a.status);
    assert!(a.status.contains("already joins"), "{}", a.status);
    // the select tool: drag the cantilever's tip node to (8, 6, 3.5)
    let tip = a.model.find_node([8.0, 5.0, 3.5]).unwrap();
    a.set_tool(Tool::Select);
    a.tool_drag(&Drag { from: [8.0, 5.0, 3.5], to: [8.0, 6.0, 3.5], node: Some(tip), member: None }, false);
    assert!(a.model.find_node([8.0, 6.0, 3.5]).is_some(), "{}", a.status);
    // dragging that node onto the slab corner merges it, and the cantilever disappears
    a.tool_drag(&Drag { from: [8.0, 6.0, 3.5], to: [6.0, 5.0, 3.5], node: Some(tip), member: None }, false);
    assert_eq!(a.model.members.len(), 8, "{}", a.status);
    // Ctrl-drag copies a column 6 m along x
    let col = a.model.members.iter().find(|m| a.model.kind(m) == pondus_core::model::MemberKind::Column).unwrap().id;
    a.tool_drag(&Drag { from: [0.0, 0.0, 0.0], to: [12.0, 0.0, 0.0], node: None, member: Some(col) }, true);
    assert_eq!(a.model.members.len(), 9, "{}", a.status);
    a.undo();
    assert_eq!(a.model.members.len(), 8);
    // the slab load all reaches the beams (no duplicate beam doubles it)
    a.test_analyze();
    let an = a.analysis.as_ref().unwrap();
    assert!((an.slab_applied[0] - an.slab_expected[0]).abs() < 1e-6 * an.slab_expected[0].max(1.0), "{} vs {}", an.slab_applied[0], an.slab_expected[0]);
    a.view.fit = true;
    shot(&mut h, "20-drawn-by-hand");
}

/// A shear wall dragged with the wall tool along a frame line: it replaces the beams in it,
/// analyses, designs per storey and draws as panels.
#[test]
fn ui_wall_tool() {
    use crate::tools::Tool;
    use crate::view::Drag;
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 940.0)).with_pixels_per_point(1.0).wgpu().build_eframe(|cc| App::new(cc, None));
    h.state_mut().test_frame();
    let a = h.state_mut();
    a.set_tool(Tool::Wall);
    a.opts.wall_t = 0.3;
    a.tool_drag(&Drag { from: [0.0, 0.0, 0.0], to: [6.0, 0.0, 0.0], node: None, member: None }, false);
    assert_eq!(a.model.walls.len(), 1, "{}", a.status);
    assert_eq!(a.model.members.iter().filter(|m| matches!(m.role, pondus_core::model::Role::WallPier(_))).count(), 3);
    a.test_analyze();
    a.test_design();
    let rep = a.report.as_ref().unwrap();
    assert_eq!(rep.walls.len(), 3, "{}", a.status);
    a.spec.diagram = pondus_core::scene::Diagram::Utilisation;
    a.set_tool(Tool::Select);
    shot(&mut h, "21-shear-wall");
}

/// Pictures of any model file in the real window: PONDUS_OPEN=model.pondus PONDUS_SNAPSHOTS=dir
/// cargo test -p pondus --release ui_open_file -- --ignored. The model is shown, analysed, and its
/// plan and first two mode shapes drawn with the earthquake table.
#[test]
#[ignore]
fn ui_open_file() {
    let Some(path) = std::env::var_os("PONDUS_OPEN").map(std::path::PathBuf::from) else { return };
    let z: f64 = std::env::var("PONDUS_PLAN_Z").ok().and_then(|v| v.parse().ok()).unwrap_or(4.0);
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).with_pixels_per_point(1.0).wgpu().build_eframe(|cc| App::new(cc, Some(path.clone())));
    h.state_mut().view.fit = true;
    shot(&mut h, "open-1-model");
    h.state_mut().test_analyze();
    h.state_mut().spec.view = "plan".into();
    h.state_mut().spec.z = Some(z);
    h.state_mut().view.set_preset("plan");
    h.state_mut().view.fit = true;
    shot(&mut h, "open-2-plan");
    for (k, name) in [(0usize, "open-3-mode1"), (1, "open-4-mode2")] {
        let a = h.state_mut();
        a.spec.view = "3d".into();
        a.spec.z = None;
        a.view.set_preset("3d");
        a.view.fit = true;
        a.spec.diagram = pondus_core::scene::Diagram::Mode;
        a.spec.mode = k;
        a.animate = false;
        a.bottom = crate::app::BottomTab::Seismic;
        shot(&mut h, name);
    }
}
