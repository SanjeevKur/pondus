//! Mats and pile caps: a plate panel resting on supports with no beams (add_mat), and a plate
//! on a Winkler subgrade. Checked against statics, symmetry, the one-way strip wL²/8 and the
//! uniform settlement q / ks.

use pondus_core::analysis::analyze;
use pondus_core::commands::run;
use pondus_core::design;
use pondus_core::model::*;
use serde_json::json;

fn plain(name: &str) -> Model {
    let mut m = Model::new(name);
    m.materials[0].e = 25_000.0;
    m.materials[0].nu = 0.2;
    m
}

/// The pile cap of the prompt: 9 piles at 1.2 m, a 0.7 m cap, 100 t (981 kN) at the centre,
/// built with add_mat as an agent would, then loaded, analysed and designed.
#[test]
fn nine_pile_cap() {
    let mut m = plain("cap");
    let r = run(&mut m, "add_mat", &json!({"pile_grid": {"nx": 3, "ny": 3, "sx": 1.2, "sy": 1.2, "edge": 0.6}, "h": 0.7, "pile_size": 0.3})).unwrap();
    eprintln!("{r}");
    assert_eq!(m.slabs.len(), 1);
    assert_eq!(m.supports.len(), 9);
    assert!(m.members.is_empty());
    let s = m.slabs[0].clone();
    assert!((s.lx() - 3.6).abs() < 1e-9 && (s.ly() - 3.6).abs() < 1e-9 && s.mat && (s.h - 0.7).abs() < 1e-12, "{s:?}");
    assert!((m.design.slab_mesh - 0.3).abs() < 1e-9, "mesh {}", m.design.slab_mesh);
    // the column load at the centre: the centre pile's node (a node is made when none exists)
    run(&mut m, "add_loads", &json!({"case": "D", "loads": [{"type": "nodal", "at": [0.0, 0.0, 0.0], "f": [0.0, 0.0, -981.0, 0.0, 0.0, 0.0]}]})).unwrap();
    let problems = pondus_core::check::problems(&m);
    assert!(problems.iter().all(|w| !w.contains("holds no member")), "{problems:?}");
    let a = analyze(&m).unwrap();
    let d = a.case(1).unwrap();
    // balance: the cap's weight 24 x 0.7 x 3.6² = 217.7 kN and the 981 kN all reach the piles
    let w = 24.0 * 0.7 * 3.6 * 3.6 + 981.0;
    assert!((d.reaction_sum()[2] - w).abs() < 0.5, "{:?} vs {w}", d.reaction_sum());
    assert!((d.applied[2] + w).abs() < 1e-6);
    let rz = |x: f64, y: f64| {
        let n = m.find_node([x, y, 0.0]).unwrap();
        d.reactions.iter().find(|(id, _)| *id == n).unwrap().1[2]
    };
    let corners = [rz(-1.2, -1.2), rz(1.2, -1.2), rz(1.2, 1.2), rz(-1.2, 1.2)];
    let edges = [rz(1.2, 0.0), rz(-1.2, 0.0), rz(0.0, 1.2), rz(0.0, -1.2)];
    eprintln!("centre {:.1} kN, edge {:.1} kN, corner {:.1} kN", rz(0.0, 0.0), edges[0], corners[0]);
    // symmetry; the centre pile under the load takes the 981 kN (a load on a pinned node goes
    // straight into it) plus its share of the cap's weight, and of that weight the corner piles
    // take more than the edge piles, as the three-moment equation says (see the strip test)
    for c in corners {
        assert!((c - corners[0]).abs() < 1e-3 * w, "{corners:?}");
    }
    for e in edges {
        assert!((e - edges[0]).abs() < 1e-3 * w, "{edges:?}");
    }
    assert!(rz(0.0, 0.0) > 981.0 && rz(0.0, 0.0) < 981.0 + 217.7 / 9.0 * 1.2, "centre {}", rz(0.0, 0.0));
    assert!(corners[0] > edges[0] && edges[0] > 0.0);
    assert!((rz(0.0, 0.0) + 4.0 * edges[0] + 4.0 * corners[0] - w).abs() < 0.5);
    // design: both faces over the whole panel, punching at the nine piles
    let (rep, _) = design::run(&mut m, &design::Options::default()).unwrap();
    let sd = rep.slabs.iter().find(|x| x.slab == s.id).expect("the mat is designed");
    eprintln!("{} · ratio {:.2} ({})", sd.method, sd.ratio, sd.governs);
    for st in &sd.strips {
        eprintln!("  {} {}: Mu {:.1} kN·m/m, {} ({:.2})", st.direction, st.location, st.mu, st.bars, st.ratio);
    }
    for n in &sd.notes {
        eprintln!("  {n}");
    }
    assert!(sd.method.starts_with("mat FE"), "{}", sd.method);
    assert!(sd.strips.iter().any(|x| x.location.contains("over supports")) && sd.strips.iter().any(|x| x.location.contains("between supports")));
    assert_eq!(sd.notes.iter().filter(|n| n.starts_with("punching at support")).count(), 9);
    // and the 981 kN column load itself punches the cap from above (1.4 x 981 = 1373 kN)
    let load_note = sd.notes.iter().find(|n| n.starts_with("punching at load at node")).expect("the load point");
    assert!(load_note.contains("Vu 1373 kN"), "{load_note}");
    assert!(!sd.governs.starts_with("one-way shear"), "the shear next to the point load is its punching, not one-way shear: {}", sd.governs);
    assert!(sd.notes.iter().all(|n| !n.contains("NOT checked")));
    // the hogging over the centre pile exceeds the sagging between the piles under a point load
    let top = sd.strips.iter().find(|x| x.location.contains("over supports") && x.direction == "x").unwrap().mu;
    let bot = sd.strips.iter().find(|x| x.location.contains("between supports") && x.direction == "x").unwrap().mu;
    assert!(top > 0.0 && bot >= 0.0, "top {top} bottom {bot}");
    // the governing punching perimeter is the centre pile's, an interior one (4 sides, αs 40)
    let centre_note = sd.notes.iter().find(|n| n.contains("punching at support") && n.contains("(0.00, 0.00)")).unwrap();
    assert!(centre_note.contains("4 sides, αs 40"), "{centre_note}");

    // the same cap on piles of finite stiffness (EA/L of a 0.3 m, 20 m concrete pile is about
    // 100,000 kN/m): the cap bends and shares the column load with the other piles
    let mut m2 = plain("cap on springs");
    run(&mut m2, "add_mat", &json!({"pile_grid": {"nx": 3, "ny": 3, "sx": 1.2, "sy": 1.2, "edge": 0.6}, "h": 0.7, "pile_k": 100_000.0})).unwrap();
    run(&mut m2, "add_loads", &json!({"case": "D", "loads": [{"type": "nodal", "at": [0.0, 0.0, 0.0], "f": [0.0, 0.0, -981.0, 0.0, 0.0, 0.0]}]})).unwrap();
    assert!(m2.supports.iter().all(|s| s.springs[2] == 100_000.0 && !s.fix[2] && s.fix[0] && s.fix[1]));
    let a2 = analyze(&m2).unwrap();
    let d2 = a2.case(1).unwrap();
    assert!((d2.reaction_sum()[2] - w).abs() < 0.5, "{:?}", d2.reaction_sum());
    let rz2 = |x: f64, y: f64| {
        let n = m2.find_node([x, y, 0.0]).unwrap();
        d2.reactions.iter().find(|(id, _)| *id == n).unwrap().1[2]
    };
    eprintln!("on springs: centre {:.1} kN, edge {:.1} kN, corner {:.1} kN", rz2(0.0, 0.0), rz2(1.2, 0.0), rz2(1.2, 1.2));
    assert!(rz2(0.0, 0.0) < 0.8 * 981.0 && rz2(1.2, 0.0) > 60.0 && rz2(0.0, 0.0) > rz2(1.2, 0.0) && rz2(1.2, 0.0) > rz2(1.2, 1.2));
    let (rep2, _) = design::run(&mut m2, &design::Options::default()).unwrap();
    let sd2 = &rep2.slabs[0];
    let top2 = sd2.strips.iter().find(|x| x.location.contains("over supports") && x.direction == "x").unwrap().mu;
    eprintln!("on springs: top Mu {top2:.1} kN·m/m, ratio {:.2} ({})", sd2.ratio, sd2.governs);
    assert!(top2 > top, "the cap on springs bends: {top2} vs {top} on pinned piles");
}

/// A one-way strip: a 6 x 1 m plate 0.3 m thick on rows of pinned supports along its short
/// edges (supports placed on the slab with add_supports), under 20 kPa plus its own 7.2 kPa.
/// The midspan moment per metre is wL²/8 = 122.4 kN·m/m and the reactions sum to 163.2 kN.
#[test]
fn strip_on_support_rows_gives_wl2_over_8() {
    let mut m = plain("strip");
    m.cases[0].self_weight = false;
    run(&mut m, "add_slabs", &json!({"panels": [{"x0": 0.0, "y0": 0.0, "x1": 6.0, "y1": 1.0, "z": 0.0, "h": 0.3, "dead": 20.0, "live": 0.0, "mat": true}]})).unwrap();
    run(&mut m, "set_design", &json!({"slab_method": "plate", "slab_mesh": 0.125})).unwrap();
    // without the plate method a support on the slab would hold nothing: the check says so
    let pts: Vec<[f64; 3]> = (0..=8).flat_map(|k| [[0.0, k as f64 / 8.0, 0.0], [6.0, k as f64 / 8.0, 0.0]]).collect();
    run(&mut m, "add_supports", &json!({"points": pts, "type": "pinned"})).unwrap();
    assert_eq!(m.supports.len(), 18);
    let a = analyze(&m).unwrap();
    let d = a.case(1).unwrap();
    let w = 20.0 + 24.0 * 0.3;
    assert!((d.reaction_sum()[2] - w * 6.0).abs() < 1e-6, "{:?}", d.reaction_sum());
    // the element nearest midspan
    let c = d.plates.iter().min_by(|p, q| (p.x - 3.0).abs().total_cmp(&(q.x - 3.0).abs())).unwrap();
    let exact = w * 36.0 / 8.0;
    eprintln!("midspan element at x {:.3}: mx {:.2} kN·m/m (wL²/8 = {exact:.2}), my {:.2}", c.x, c.mx, c.my);
    // the element centre sits 1/16 of the span from midspan: M there is 0.9961 of the peak
    assert!((c.mx - exact * (1.0 - 1.0 / 256.0)).abs() / exact < 0.02, "mx {} vs {exact}", c.mx);
    // design: a mat panel, no 'minimum thickness' of a beam-supported floor
    let (rep, _) = design::run(&mut m, &design::Options::default()).unwrap();
    let sd = &rep.slabs[0];
    assert!(sd.h_min == 0.0 && sd.notes.iter().all(|n| !n.contains("Table 8.3.1.2")), "{:?}", sd.notes);
    let bot = sd.strips.iter().find(|x| x.location.contains("between supports") && x.direction == "x").unwrap();
    // the governing ULS combination of a new model is 1.4 D
    assert!((bot.mu - 1.4 * exact * (1.0 - 1.0 / 256.0)).abs() / (1.4 * exact) < 0.03, "Mu {} under 1.4 D", bot.mu);
}

/// The 1D analogue of a pile cap: a thin 1 m wide strip on three rows of supports at 1.2 m
/// with 0.6 m overhangs, under a uniform load. By the three-moment equation the overhangs put
/// M = -wL²/8 on the outer rows and -wL²/16 on the middle one, so R_mid / R_end = 1.05 / 1.275
/// = 0.824: the outer supports carry more than the middle one, which is why the corner piles of
/// a cap take more of its weight than the edge piles.
#[test]
fn strip_on_three_rows_matches_the_three_moment_equation() {
    let mut m = plain("rows");
    m.materials[0].nu = 0.0;
    run(&mut m, "add_slabs", &json!({"panels": [{"x0": 0.0, "y0": 0.0, "x1": 3.6, "y1": 1.0, "z": 0.0, "h": 0.1, "dead": 10.0, "live": 0.0, "mat": true}]})).unwrap();
    run(&mut m, "set_design", &json!({"slab_method": "plate", "slab_mesh": 0.1})).unwrap();
    let pts: Vec<[f64; 3]> = [0.6, 1.8, 3.0].iter().flat_map(|x| (0..=4).map(move |k| [*x, k as f64 / 4.0, 0.0])).collect();
    run(&mut m, "add_supports", &json!({"points": pts, "type": "pinned"})).unwrap();
    let a = analyze(&m).unwrap();
    let d = a.case(1).unwrap();
    let row = |x: f64| -> f64 { d.reactions.iter().filter(|(id, _)| m.node(*id).is_some_and(|n| (n.x - x).abs() < 1e-6)).map(|(_, r)| r[2]).sum() };
    let (end, mid) = (row(0.6), row(1.8));
    eprintln!("rows: end {end:.2} kN, mid {mid:.2} kN, ratio {:.3} (beam theory 0.824)", mid / end);
    assert!((row(3.0) - end).abs() < 1e-6);
    assert!((mid / end - 1.05 / 1.275).abs() < 0.03, "{}", mid / end);
}

/// Without the plate method a support on a slab holds nothing, and the check says what to set.
#[test]
fn support_on_slab_needs_the_plate_method() {
    let mut m = plain("check");
    run(&mut m, "add_slabs", &json!({"panels": [{"x0": 0.0, "y0": 0.0, "x1": 3.0, "y1": 3.0, "z": 0.0, "h": 0.5}]})).unwrap();
    run(&mut m, "add_supports", &json!({"points": [[1.5, 1.5, 0.0]], "type": "pinned"})).unwrap();
    let w = pondus_core::check::problems(&m);
    assert!(w.iter().any(|x| x.contains("slab_method 'plate'")), "{w:?}");
    assert!(analyze(&m).is_err());
    // a support off the slab is still refused
    assert!(run(&mut m, "add_supports", &json!({"points": [[9.0, 9.0, 0.0]], "type": "pinned"})).is_err());
}

/// A mat on a Winkler subgrade under a uniform pressure (50 kPa plus its own 19.2 kPa) settles
/// uniformly by q / ks with no bending, and the soil takes the whole load (reported under node 0).
#[test]
fn mat_on_subgrade_settles_uniformly() {
    let mut m = plain("raft");
    m.cases[0].self_weight = false;
    run(&mut m, "add_mat", &json!({"x0": 0.0, "y0": 0.0, "x1": 8.0, "y1": 6.0, "h": 0.8, "subgrade": 20_000.0, "dead": 50.0})).unwrap();
    assert!(m.supports.is_empty() && m.slabs[0].subgrade == 20_000.0);
    let a = analyze(&m).unwrap();
    let d = a.case(1).unwrap();
    // q / ks = 69.2 / 20000 = 3.46 mm everywhere
    let uz: Vec<f64> = d.plates.iter().map(|p| p.uz * 1000.0).collect();
    let (lo, hi) = (uz.iter().copied().fold(f64::MAX, f64::min), uz.iter().copied().fold(f64::MIN, f64::max));
    eprintln!("settlement {:.4} to {:.4} mm", -hi, -lo);
    let s = (50.0 + 24.0 * 0.8) / 20_000.0 * 1000.0;
    assert!((hi + s).abs() < 0.01 && (lo + s).abs() < 0.01, "{lo} {hi} vs {s}");
    let mmax = d.plates.iter().map(|p| p.mx.abs().max(p.my.abs())).fold(0.0, f64::max);
    assert!(mmax < 0.5, "moments {mmax} in a uniformly settling mat");
    // the soil reaction balances the 69.2 x 48 = 3321.6 kN and is reported under node 0
    let total = (50.0 + 24.0 * 0.8) * 48.0;
    let soil = d.reactions.iter().find(|(id, _)| *id == 0).expect("soil reactions").1;
    assert!((soil[2] - total).abs() < 1e-3 && (d.reaction_sum()[2] - total).abs() < 1e-3, "{soil:?} vs {total}");
    // a column load at the centre: the mat bends and the settlement peaks under it
    run(&mut m, "add_loads", &json!({"case": "D", "loads": [{"type": "nodal", "at": [4.0, 3.0, 0.0], "f": [0.0, 0.0, -1500.0, 0.0, 0.0, 0.0]}]})).unwrap();
    let a = analyze(&m).unwrap();
    let d = a.case(1).unwrap();
    assert!((d.reaction_sum()[2] - total - 1500.0).abs() < 1e-3, "{:?}", d.reaction_sum());
    let c = d.plates.iter().min_by(|p, q| ((p.x - 4.0).hypot(p.y - 3.0)).total_cmp(&(q.x - 4.0).hypot(q.y - 3.0))).unwrap();
    let corner = d.plates.iter().min_by(|p, q| (p.x + p.y).total_cmp(&(q.x + q.y))).unwrap();
    eprintln!("under the load {:.2} mm, corner {:.2} mm; mx under the load {:.1} kN·m/m", -c.uz * 1e3, -corner.uz * 1e3, c.mx);
    assert!(-c.uz > -corner.uz && c.mx > 0.0, "a column load on a mat sags under it (tension at the bottom)");
    let (rep, _) = design::run(&mut m, &design::Options::default()).unwrap();
    let sd = &rep.slabs[0];
    assert!(sd.notes.iter().any(|n| n.starts_with("settlement under D + L")), "{:?}", sd.notes);
    // the column load itself punches the mat: 1.4 x 1500 kN through a support_size square
    let note = sd.notes.iter().find(|n| n.starts_with("punching at load at node")).expect("the load point");
    assert!(note.contains("Vu 2100 kN"), "{note}");
}
