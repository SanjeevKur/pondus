//! DXF and IFC in and out: round trips of a frame, hand-made files with known coordinates,
//! and the IFC4 file Aedifex writes for its 2 x 2 bay frame. PONDUS_INTERCHANGE_OUT=dir keeps
//! the written files (for a third-party reader).

use pondus_core::commands::run;
use pondus_core::model::*;
use pondus_core::{dxf, ifc};
use serde_json::json;

fn frame() -> Model {
    let mut m = Model::new("Interchange");
    run(&mut m, "add_frame", &json!({"x_spans": [6.0, 6.0], "y_spans": [5.0], "storeys": [4.0, 3.5], "column": "400 x 400", "beam": "300 x 600", "slab": {"h": 0.15, "dead": 1.5, "live": 3.0}})).unwrap();
    // a steel brace and a sloped steel rafter turned 30° about its axis
    run(&mut m, "add_members", &json!({"members": [
        {"from": [0, 0, 0], "to": [6, 0, 4], "section": "H 200x100x5.5x8", "kind": "brace"},
        {"from": [0, 5, 7.5], "to": [6, 5, 9.0], "section": "H 300x150x6.5x9", "roll": 30}]})).unwrap();
    m
}

fn keep(name: &str, text: &str) {
    if let Some(d) = std::env::var_os("PONDUS_INTERCHANGE_OUT") {
        let d = std::path::PathBuf::from(d);
        let _ = std::fs::create_dir_all(&d);
        std::fs::write(d.join(name), text).unwrap();
    }
}

/// Every member of `a` is in `b` with the same ends (1 mm), section, kind and, when asked,
/// roll (a half turn apart is the same for these symmetric sections).
fn same_members(a: &Model, b: &Model, roll: bool) {
    assert_eq!(a.members.len(), b.members.len(), "member count");
    for m in &a.members {
        let (p, q) = (a.node(m.i).unwrap().p(), a.node(m.j).unwrap().p());
        let n = b.members.iter().find(|x| {
            let (r, s) = (b.node(x.i).unwrap().p(), b.node(x.j).unwrap().p());
            (dist(p, r) < 1e-3 && dist(q, s) < 1e-3) || (dist(p, s) < 1e-3 && dist(q, r) < 1e-3)
        });
        let n = n.unwrap_or_else(|| panic!("member {} {p:?}–{q:?} not found", m.id));
        assert_eq!(a.section(m.section).unwrap().name, b.section(n.section).unwrap().name, "member {}", m.id);
        assert_eq!(a.kind(m), b.kind(n), "member {}", m.id);
        assert_eq!(a.is_steel(m), b.is_steel(n), "member {}", m.id);
        if roll {
            let d = (m.roll - n.roll).rem_euclid(180.0);
            assert!(d < 1e-4 || d > 180.0 - 1e-4, "member {}: roll {} vs {}", m.id, m.roll, n.roll);
        }
    }
}

fn same_slabs(a: &Model, b: &Model) {
    assert_eq!(a.slabs.len(), b.slabs.len(), "slab count");
    for s in &a.slabs {
        assert!(b.slabs.iter().any(|t| (t.lo().0 - s.lo().0).abs() < 1e-3 && (t.lo().1 - s.lo().1).abs() < 1e-3 && (t.hi().0 - s.hi().0).abs() < 1e-3 && (t.hi().1 - s.hi().1).abs() < 1e-3 && (t.z - s.z).abs() < 1e-3 && (t.h - s.h).abs() < 1e-6), "slab {} not found", s.id);
    }
}

#[test]
fn dxf_round_trip() {
    let m = frame();
    let a = pondus_core::analysis::analyze(&m).unwrap();
    let text = dxf::export(&m, Some(&a), &dxf::DxfOptions::default());
    keep("frame3d.dxf", &text);
    assert!(text.starts_with("0\nSECTION\n2\nHEADER") && text.ends_with("0\nEOF\n"));
    assert!(text.contains("COLUMN_400x400") && text.contains("BRACE_H300x150x6p5x9") && text.contains("BRACE_H200x100x5p5x8") && text.contains("DIAGRAM_MY"));
    let r = dxf::import(&text, &dxf::DxfImport::default()).unwrap();
    eprintln!("DXF: {} members, {} slabs, counts {:?}, warnings {:?}", r.members.len(), r.slabs.len(), r.counts, r.warnings);
    let mut b = Model::new("back");
    let ap = dxf::apply(&mut b, &r).unwrap();
    eprintln!("applied: {} members, {} nodes, {} slabs", ap.members.len(), ap.nodes, ap.slabs.len());
    same_members(&m, &b, false);
    same_slabs(&m, &b);
    assert_eq!(b.nodes.len(), m.nodes.len());
    // a plan of the first floor and an elevation of the front frame
    let plan = dxf::export(&m, None, &dxf::DxfOptions { view: dxf::DxfView::Plan { z: 4.0 }, ..Default::default() });
    keep("plan4.dxf", &plan);
    let rp = dxf::import(&plan, &dxf::DxfImport { view: dxf::DxfView::Plan { z: 4.0 }, ..Default::default() }).unwrap();
    let beams4 = m.members.iter().filter(|x| [x.i, x.j].iter().all(|n| (m.node(*n).unwrap().z - 4.0).abs() < 1e-6)).count();
    assert_eq!(rp.members.len(), beams4, "the plan's beams");
    assert!(rp.members.iter().all(|x| (x.from[2] - 4.0).abs() < 1e-9 && x.kind == MemberKind::Beam));
    assert!(plan.contains("COLUMNS_PLAN"));
    let el = dxf::export(&m, Some(&a), &dxf::DxfOptions { view: dxf::DxfView::ElevationXz { y: 0.0 }, ..Default::default() });
    keep("elevation_y0.dxf", &el);
    let re = dxf::import(&el, &dxf::DxfImport { view: dxf::DxfView::ElevationXz { y: 0.0 }, ..Default::default() }).unwrap();
    let front = m.members.iter().filter(|x| [x.i, x.j].iter().all(|n| m.node(*n).unwrap().y.abs() < 1e-6)).count();
    assert_eq!(re.members.len(), front);
    assert!(re.members.iter().any(|x| dist(x.from, [0.0, 0.0, 0.0]) < 1e-9 && dist(x.to, [6.0, 0.0, 4.0]) < 1e-9 && x.section == "H200x100x5.5x8"));
}

#[test]
fn dxf_in_mm_with_polylines() {
    // a 2D plan in mm: an LWPOLYLINE of three beams on layer 0 and a LINE on a section layer
    let text = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
0\nLWPOLYLINE\n8\n0\n90\n4\n70\n0\n10\n0\n20\n0\n10\n6000\n20\n0\n10\n6000\n20\n5000\n10\n0\n20\n5000\n\
0\nLINE\n8\nBEAM_250x500\n10\n0\n20\n0\n30\n0\n11\n0\n21\n5000\n31\n0\n\
0\nTEXT\n8\nNOTES\n10\n1\n20\n1\n40\n100\n1\nhello\n0\nENDSEC\n0\nEOF\n";
    let r = dxf::import(text, &dxf::DxfImport { view: dxf::DxfView::Plan { z: 3.0 }, default_section: "200 x 400".into(), ..Default::default() }).unwrap();
    assert_eq!(r.members.len(), 4);
    assert_eq!(r.members[0].from, [0.0, 0.0, 3.0]);
    assert_eq!(r.members[1].to, [6.0, 5.0, 3.0]);
    assert_eq!(r.members[3].section, "250x500");
    assert_eq!(r.members[3].kind, MemberKind::Beam);
    let mut m = Model::new("mm");
    let ap = dxf::apply(&mut m, &r).unwrap();
    assert_eq!((ap.members.len(), ap.nodes), (4, 4));
    let names: Vec<&str> = m.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["200 x 400 mm", "250 x 500 mm"]);
}

#[test]
fn ifc_round_trip() {
    let m = frame();
    let text = ifc::export(&m, None);
    keep("frame.ifc", &text);
    assert!(text.contains("FILE_SCHEMA(('IFC4'))") && text.contains("IFCISHAPEPROFILEDEF") && text.contains("IFCBUILDINGSTOREY"));
    let st = ifc::Step::parse(&text).unwrap();
    let count = |t: &str| st.ents.values().filter(|e| e.0 == t).count();
    eprintln!("IFC: {} entities, {} columns, {} beams, {} members, {} slabs, {} storeys", st.ents.len(), count("IFCCOLUMN"), count("IFCBEAM"), count("IFCMEMBER"), count("IFCSLAB"), count("IFCBUILDINGSTOREY"));
    assert_eq!(count("IFCCOLUMN") + count("IFCBEAM") + count("IFCMEMBER"), m.members.len());
    for snap in [0.0, 0.75] {
        let r = ifc::import(&text, &ifc::IfcImport { snap, ..Default::default() }).unwrap();
        eprintln!("snap {snap}: {} members, {} slabs, warnings {:?}", r.members.len(), r.slabs.len(), r.warnings);
        let mut b = Model::new("back");
        ifc::apply(&mut b, &r).unwrap();
        same_members(&m, &b, true);
        same_slabs(&m, &b);
        // the GlobalIds came along
        assert!(b.members.iter().all(|x| x.bim_id.as_ref().is_some_and(|g| g.len() == 22)));
    }
    // a report's ratios go into the property sets
    let mut d = m.clone();
    let (rep, _) = pondus_core::design::run(&mut d, &Default::default()).unwrap();
    let t2 = ifc::export(&d, Some(&rep));
    keep("frame_designed.ifc", &t2);
    let n = t2.matches("'DesignRatio'").count();
    eprintln!("design ratios written: {n}");
    assert!(n >= d.members.len() - 2, "{n}");
}

#[test]
fn ifc2x3_by_hand_in_mm() {
    // the building turned 90° about z and moved 1 m in x; the storey 3 m up in it; a beam
    // 200 x 500 placed in the storey with its axis along the storey's x, its top at the level,
    // and a steel column under a mapped representation
    let text = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('IFC2X3'));\nENDSEC;\nDATA;\n\
#1=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);\n#2=IFCUNITASSIGNMENT((#1));\n\
#10=IFCCARTESIANPOINT((1000.,0.,0.));\n#11=IFCDIRECTION((0.,0.,1.));\n#12=IFCDIRECTION((0.,1.,0.));\n\
#13=IFCAXIS2PLACEMENT3D(#10,#11,#12);\n#14=IFCLOCALPLACEMENT($,#13);\n\
#20=IFCCARTESIANPOINT((0.,0.,3000.));\n#21=IFCAXIS2PLACEMENT3D(#20,$,$);\n#22=IFCLOCALPLACEMENT(#14,#21);\n\
#23=IFCBUILDINGSTOREY('2Ss$cpf1D5xvW_w7ZGq3Ex',#99,'Level 2',$,$,#22,$,$,.ELEMENT.,3000.);\n\
#30=IFCCARTESIANPOINT((2000.,0.,-250.));\n#31=IFCDIRECTION((1.,0.,0.));\n#32=IFCDIRECTION((0.,1.,0.));\n\
#33=IFCAXIS2PLACEMENT3D(#30,#31,#32);\n#34=IFCLOCALPLACEMENT(#22,#33);\n\
#40=IFCCARTESIANPOINT((0.,0.));\n#41=IFCAXIS2PLACEMENT2D(#40,$);\n#42=IFCRECTANGLEPROFILEDEF(.AREA.,'B',#41,200.,500.);\n\
#43=IFCCARTESIANPOINT((0.,0.,0.));\n#44=IFCAXIS2PLACEMENT3D(#43,$,$);\n#45=IFCDIRECTION((0.,0.,1.));\n\
#46=IFCEXTRUDEDAREASOLID(#42,#44,#45,4000.);\n#47=IFCSHAPEREPRESENTATION(#98,'Body','SweptSolid',(#46));\n\
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#47));\n#49=IFCBEAM('0Kq9xVnHz8c9u3Ek1QdB7A',#99,'Beam 1',$,'B 200x500',#34,#48,'B1');\n\
#60=IFCISHAPEPROFILEDEF(.AREA.,'H',#41,150.,300.,6.5,9.,13.);\n#61=IFCEXTRUDEDAREASOLID(#60,#44,#45,3000.);\n\
#62=IFCSHAPEREPRESENTATION(#98,'Body','SweptSolid',(#61));\n#63=IFCREPRESENTATIONMAP(#44,#62);\n\
#64=IFCCARTESIANTRANSFORMATIONOPERATOR3D($,$,#43,$,$);\n#65=IFCMAPPEDITEM(#63,#64);\n\
#66=IFCSHAPEREPRESENTATION(#98,'Body','MappedRepresentation',(#65));\n#67=IFCPRODUCTDEFINITIONSHAPE($,$,(#66));\n\
#68=IFCLOCALPLACEMENT(#22,#44);\n#69=IFCCOLUMN('1Abc9xVnHz8c9u3Ek1QdB7A',#99,'Column 1',$,$,#68,#67,'C1');\n\
#70=IFCMATERIAL('Steel S275');\n#71=IFCRELASSOCIATESMATERIAL('3Abc9xVnHz8c9u3Ek1QdB7A',#99,$,$,(#69),#70);\n\
ENDSEC;\nEND-ISO-10303-21;\n";
    let r = ifc::import(text, &ifc::IfcImport { snap: 0.0, ..Default::default() }).unwrap();
    assert_eq!(r.members.len(), 2, "{:?}", r.warnings);
    let col = r.members.iter().find(|x| x.kind == MemberKind::Column).unwrap();
    let beam = r.members.iter().find(|x| x.kind == MemberKind::Beam).unwrap();
    // building x = world y, building y = world −x: the beam's start (2000, 0, −250) in the
    // storey is (1, 2, 2.75) m in the world, and it runs 4 m along world y
    let close = |a: [f64; 3], b: [f64; 3]| dist(a, b) < 1e-9;
    assert!(close(beam.from, [1.0, 2.0, 2.75]) && close(beam.to, [1.0, 6.0, 2.75]), "{:?} {:?}", beam.from, beam.to);
    assert_eq!(beam.section, "200 x 500 mm");
    assert_eq!(beam.bim_id.as_deref(), Some("0Kq9xVnHz8c9u3Ek1QdB7A"));
    assert!(close(col.from, [1.0, 0.0, 3.0]) && close(col.to, [1.0, 0.0, 6.0]), "{:?} {:?}", col.from, col.to);
    assert_eq!(col.section, "H 300x150x6.5x9");
    assert_eq!(col.material, Some(MaterialKind::Steel));
    // snapped: the beam rises to the level its top is at, 3 m
    let r2 = ifc::import(text, &ifc::IfcImport::default()).unwrap();
    let b2 = r2.members.iter().find(|x| x.kind == MemberKind::Beam).unwrap();
    assert!((b2.from[2] - 3.0).abs() < 1e-9 && (b2.to[2] - 3.0).abs() < 1e-9, "{:?}", b2.from);
    // the depths: the beam's along world z, the column's along world −x (the profile's y)
    let mut m = Model::new("hand");
    ifc::apply(&mut m, &r).unwrap();
    for x in &m.members {
        let (_, _, ez) = pondus_core::analysis::axes(&m, x).unwrap();
        let want = if m.kind(x) == MemberKind::Beam { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let c = ez[0] * want[0] + ez[1] * want[1] + ez[2] * want[2];
        assert!(c.abs() > 1.0 - 1e-9, "member {} ez {ez:?} roll {}", x.id, x.roll);
    }
    assert!(m.is_steel(m.members.iter().find(|x| m.kind(x) == MemberKind::Column).unwrap()));
}

#[test]
fn ifc_from_aedifex() {
    // Aedifex's 2 x 2 bay frame (core/examples/frame.rs): 9 columns 0 – 6.4 m, beams 300 x 600
    // with their tops at 3.2 and 6.4 m stopping at the column faces, two slabs over the four
    // bays, nine footings
    let text = include_str!("data/aedifex_frame.ifc");
    let r = ifc::import(text, &ifc::IfcImport::default()).unwrap();
    eprintln!("Aedifex: {} members, {} slabs, warnings {:?}", r.members.len(), r.slabs.len(), r.warnings);
    assert_eq!(r.members.len(), 33);
    let mut m = Model::new("from Aedifex");
    let ap = ifc::apply(&mut m, &r).unwrap();
    eprintln!("applied: {} members, {} nodes, {} slab panels, warnings {:?}", ap.members.len(), ap.nodes, ap.slabs.len(), ap.warnings);
    // the analytical model: every node on the 6 m grid at z 0, 3.2 or 6.4
    for n in &m.nodes {
        let on = |v: f64| (v / 6.0 - (v / 6.0).round()).abs() < 1e-6;
        assert!(on(n.x) && on(n.y) && [0.0, 3.2, 6.4].iter().any(|z| (n.z - z).abs() < 1e-6), "node {:?}", n.p());
    }
    assert_eq!(m.nodes.len(), 27);
    let beams: Vec<&Member> = m.members.iter().filter(|x| m.kind(x) == MemberKind::Beam).collect();
    assert_eq!(beams.len(), 24);
    assert!(beams.iter().all(|x| (m.length(x) - 6.0).abs() < 1e-6 && m.section(x.section).unwrap().name == "300 x 600 mm"));
    assert!(m.members.iter().filter(|x| m.kind(x) == MemberKind::Column).all(|x| m.section(x.section).unwrap().name == "400 x 400 mm" && (m.length(x) - 6.4).abs() < 1e-6));
    // each slab over four bays becomes four panels on the beams
    assert_eq!(m.slabs.len(), 8);
    assert!(m.slabs.iter().all(|s| (s.lx() - 6.0).abs() < 1e-6 && (s.ly() - 6.0).abs() < 1e-6));
    let zs: Vec<f64> = m.slabs.iter().map(|s| s.z).collect();
    assert!(zs.iter().all(|z| (z - 3.2).abs() < 1e-6 || (z - 6.4).abs() < 1e-6), "{zs:?}");
    assert!(r.warnings.iter().any(|w| w.contains("footings")));
    // and it analyses: supports under the columns, one load case of self weight
    run(&mut m, "add_supports", &json!({"points": [[0,0,0],[6,0,0],[12,0,0],[0,6,0],[6,6,0],[12,6,0],[0,12,0],[6,12,0],[12,12,0]], "type": "fixed"})).unwrap();
    let a = pondus_core::analysis::analyze(&m).unwrap();
    let d = a.case(1).unwrap().reaction_sum();
    eprintln!("self weight + slabs reaction: {:.1} kN", d[2]);
    assert!(d[2] > 0.0);
}
