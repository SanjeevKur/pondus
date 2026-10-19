//! Plastic collapse, pushover, moving loads, influence lines and staged construction against
//! closed-form results.

use pondus_core::advanced::*;
use pondus_core::analysis::analyze;
use pondus_core::model::*;

fn mem(m: &mut Model, a: [f64; 3], b: [f64; 3], s: Id, mat: Id) -> Id {
    let (i, j) = (m.node_at(a), m.node_at(b));
    let id = m.next_member_id();
    m.members.push(Member { id, i, j, section: s, material: mat, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, group: None });
    id
}

fn steel_beam(sup: &str) -> (Model, f64) {
    let mut m = Model::new("beam");
    m.cases[0].self_weight = false;
    let s = m.section_named("H 300x150x6.5x9").unwrap();
    let b = mem(&mut m, [0.0, 0.0, 0.0], [6.0, 0.0, 0.0], s, 2);
    for (k, x) in [0.0, 6.0].iter().enumerate() {
        let n = m.find_node([*x, 0.0, 0.0]).unwrap();
        let mut sp = Support::of_kind(n, if sup == "fixed" { "fixed" } else if k == 0 { "pinned" } else { "roller" }).unwrap();
        sp.fix[1] = true;
        sp.fix[3] = true;
        sp.fix[5] = true;
        m.supports.push(sp);
    }
    m.combos = vec![Combination { id: 1, name: "ULS L".into(), kind: ComboKind::Uls, factors: vec![(2, 1.0)] }, Combination { id: 2, name: "SLS L".into(), kind: ComboKind::Sls, factors: vec![(2, 1.0)] }];
    let mp = 245.0e3 * m.section(s).unwrap().props().zy;
    let _ = b;
    (m, mp)
}

#[test]
fn plastic_collapse_fixed_beam() {
    // fixed both ends, 10 kN/m: collapse at w = 16 Mp / L²
    let (mut m, mp) = steel_beam("fixed");
    m.loads.push(Load { case: 2, kind: LoadKind::Line { member: 1, wi: [0.0, 0.0, -10.0], wj: [0.0, 0.0, -10.0], a: None, b: None } });
    let a = analyze(&m).unwrap();
    let r = pushover(&m, &a, None, &PushoverSpec { combination: Some("ULS L".into()), ..Default::default() }).unwrap();
    let want = 16.0 * mp / 36.0 / 10.0;
    eprintln!("collapse λ {:.4}, 16Mp/(wL²) {want:.4}; hinges {:?}", r.collapse_factor, r.hinges.iter().map(|h| (h.member, (h.position * 100.0).round() / 100.0)).collect::<Vec<_>>());
    assert!((r.collapse_factor - want).abs() / want < 0.01, "{} vs {want}", r.collapse_factor);
    assert_eq!(r.hinges.len(), 3);
}

#[test]
fn sway_mechanism() {
    // pinned-base portal 6 x 4, the same H in columns and beam, lateral pattern: the sway
    // mechanism of hinges at the column tops, H·h = 2 Mp
    let mut m = Model::new("portal");
    let s = m.section_named("H 300x150x6.5x9").unwrap();
    mem(&mut m, [0.0, 0.0, 0.0], [0.0, 0.0, 4.0], s, 2);
    mem(&mut m, [6.0, 0.0, 0.0], [6.0, 0.0, 4.0], s, 2);
    mem(&mut m, [0.0, 0.0, 4.0], [6.0, 0.0, 4.0], s, 2);
    for x in [0.0, 6.0] {
        let n = m.find_node([x, 0.0, 0.0]).unwrap();
        m.supports.push(Support { node: n, fix: [true, true, true, true, false, true], springs: [0.0; 6], uplift: false });
    }
    let a = analyze(&m).unwrap();
    let r = pushover(&m, &a, None, &PushoverSpec { combination: None, direction: 0, pattern: "uniform".into() }).unwrap();
    let mp = 245.0e3 * m.section(s).unwrap().props().zy;
    // the uniform pattern puts the whole W on the top nodes: H = λ W at h = 4
    let want = 2.0 * mp / 4.0 / r.weight;
    eprintln!("pushover λ {:.4} (V {:.1} kN of W {:.1}), 2Mp/(hW) {want:.4}; curve points {}", r.collapse_factor, r.base_shear, r.weight, r.curve.len());
    assert!((r.collapse_factor - want).abs() / want < 0.02, "{} vs {want}", r.collapse_factor);
    // the capacity curve: load and roof displacement both grow event by event
    assert!(r.curve.windows(2).all(|w| w[1].0 >= w[0].0 - 1e-12 && w[1].1.abs() >= w[0].1.abs() - 1e-12), "{:?}", r.curve);
}

#[test]
fn moving_axle_and_influence() {
    // one 100 kN axle over a simple 6 m span: the largest moment PL/4 at midspan
    let (mut m, _) = steel_beam("simple");
    m.moving = Some(MovingSpec { name: "Axle".into(), path: vec![1], axles: vec![(0.0, 100.0)], step: 0.25, impact: 0.0, factor: 1.6 });
    let a = analyze(&m).unwrap();
    let (hi, lo) = a.moving.clone().expect("the moving envelope");
    let mmax = hi.members[0].stations.iter().map(|s| s.my).fold(f64::MIN, f64::max);
    eprintln!("moving: largest My {mmax:.3} kN·m, PL/4 = 150, least {:.3}", lo.members[0].stations.iter().map(|s| s.my).fold(f64::MAX, f64::min));
    assert!((mmax - 150.0).abs() < 1.0, "{mmax}");
    // its design combinations are there
    let names: Vec<String> = moving_combos(&m, &a).into_iter().map(|c| c.0).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    // the left reaction's influence line: 1 − x/L
    let n0 = m.find_node([0.0, 0.0, 0.0]).unwrap();
    let il = influence(&m, &a, "Fz", None, 0.0, Some(n0)).unwrap();
    let worst = il.iter().map(|p| (p.value.abs() - (1.0 - p.x / 6.0)).abs()).fold(0.0, f64::max);
    eprintln!("influence line of R_A: {} points, largest error {worst:.2e}", il.len());
    assert!(worst < 1e-6);
}

#[test]
fn staged_equals_linear_when_built_at_once() {
    let (mut m, _) = steel_beam("fixed");
    m.cases[0].self_weight = true;
    m.loads.push(Load { case: 2, kind: LoadKind::Line { member: 1, wi: [0.0, 0.0, -10.0], wj: [0.0, 0.0, -10.0], a: None, b: None } });
    m.stages = vec![Stage { name: "frame + D".into(), members: vec![1], cases: vec![1] }, Stage { name: "L".into(), members: vec![], cases: vec![2] }];
    let a = analyze(&m).unwrap();
    let (stages, fin) = staged(&m, &a).unwrap();
    let lin = a.combine("D + L", &[(1, 1.0), (2, 1.0)]);
    let (ms, ml) = (fin.members[0].stations[0].my, lin.members[0].stations[0].my);
    eprintln!("stages {:?}; final end moment {ms:.3}, linear D + L {ml:.3}", stages.iter().map(|s| s.name.clone()).collect::<Vec<_>>());
    assert!((ms - ml).abs() < 1e-6 * ml.abs().max(1.0));
}

/// Timing of the advanced analyses on the UI test's building (3 x 2 bays, 3 storeys).
#[test]
fn timing_on_a_building() {
    use std::time::Instant;
    let mut m = Model::new("timing");
    pondus_core::commands::run(&mut m, "add_frame", &serde_json::json!({"x_spans": [6.0, 6.0, 6.0], "y_spans": [5.0, 5.0], "storeys": [4.0, 3.5, 3.5], "column": "400 x 400", "beam": "300 x 600", "slab": {"h": 0.15, "dead": 1.5, "live": 3.0, "roof_live": 1.0}})).unwrap();
    let t = Instant::now();
    let a = analyze(&m).unwrap();
    eprintln!("analyze {:.2} s", t.elapsed().as_secs_f64());
    let t = Instant::now();
    let rep = pondus_core::design::design_pass(&m, &a, &Default::default());
    eprintln!("design pass {:.2} s", t.elapsed().as_secs_f64());
    let t = Instant::now();
    let p = pushover(&m, &a, Some(&rep), &PushoverSpec { combination: None, direction: 0, pattern: "triangular".into() }).unwrap();
    eprintln!("pushover {:.2} s: λ {:.4}, {} hinges, curve {:?}", t.elapsed().as_secs_f64(), p.collapse_factor, p.hinges.len(), p.curve.iter().map(|c| ((c.0 * 1e4).round() / 1e4, (c.1 * 1e6).round() / 1e3)).collect::<Vec<_>>());
    for h in &p.hinges {
        let mm = m.member(h.member).unwrap();
        let (a0, b0) = (m.node(mm.i).unwrap(), m.node(mm.j).unwrap());
        eprintln!("  hinge member {} {:?} z {:.1}-{:.1} at {:.2} λ {:.4}", h.member, m.kind(mm), a0.z, b0.z, h.position, h.at);
    }
    for w in &p.warnings {
        eprintln!("  warning: {w}");
    }
    if let Some(c) = rep.columns.iter().find(|c| c.member == 1) {
        eprintln!("  column 1: {} n_b {} n_h {} db {} ; mp {:?}", c.bars, c.n_b, c.n_h, c.db, pondus_core::advanced::plastic_moments_of(&m, m.member(1).unwrap(), Some(&rep)));
    }
    let beams: Vec<Id> = m.members.iter().filter(|x| m.kind(x) == MemberKind::Beam && [x.i, x.j].iter().all(|n| m.node(*n).is_some_and(|n| n.z == 4.0 && n.y == 0.0))).map(|x| x.id).collect();
    m.moving = Some(MovingSpec { name: "Truck".into(), path: beams.clone(), axles: vec![(0.0, 50.0), (2.0, 50.0)], step: 0.25, impact: 0.0, factor: 1.6 });
    let t = Instant::now();
    let a2 = analyze(&m).unwrap();
    eprintln!("analyze with moving {:.2} s", t.elapsed().as_secs_f64());
    let t = Instant::now();
    let il = influence(&m, &a2, "My", Some(beams[0]), 1.0, None).unwrap();
    let pk = il.iter().max_by(|a, b| a.value.total_cmp(&b.value)).unwrap();
    eprintln!("influence {:.2} s, {} points, peak {:.4} at member {} x {:.3}", t.elapsed().as_secs_f64(), il.len(), pk.value, pk.member, pk.x);
    // the moment line of a section peaks with the unit load over the section
    assert!(pk.member == beams[0] && (pk.x - 1.0).abs() < 1e-6, "{} {}", pk.member, pk.x);
    let t = Instant::now();
    m.stages = vec![Stage { name: "all".into(), members: m.members.iter().map(|x| x.id).collect(), cases: m.cases.iter().map(|c| c.id).collect() }];
    let _ = staged(&m, &a2).unwrap();
    eprintln!("staged {:.2} s", t.elapsed().as_secs_f64());
}
