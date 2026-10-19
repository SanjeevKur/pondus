//! Whole flows through the MCP server, as an agent drives them.

use serde_json::{json, Value};
use pondus_core::mcp::Server;
use pondus_core::model::Model;

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    let r = s.handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}})).unwrap();
    assert_eq!(r["result"]["isError"], false, "{name}: {r}");
    let t = r["result"]["content"][0]["text"].as_str().map(String::from);
    t.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(r["result"].clone())
}

#[test]
fn concrete_building_designs() {
    let mut s = Server::new(Model::new("RC"));
    let r = s.handle(&json!({"jsonrpc": "2.0", "id": 0, "method": "tools/list"})).unwrap();
    assert!(r["result"]["tools"].as_array().unwrap().len() >= 20);
    call(&mut s, "add_frame", json!({"x_spans": [6.0, 6.0], "y_spans": [5.0, 5.0], "storeys": [4.0, 3.5], "column": "400 x 400", "beam": "250 x 400", "slab": {"h": 0.13, "dead": 1.5, "live": 3.0, "roof_live": 1.0}}));
    let a = call(&mut s, "analyze", json!({}));
    assert!(a["cases"].as_array().unwrap().iter().all(|c| c["balanced"] == true), "{a}");
    assert!(a["warnings"].as_array().unwrap().is_empty(), "{a}");
    let sl = &a["slab_load_on_beams_kN"];
    assert!((sl["dead"][0].as_f64().unwrap() - sl["dead"][1].as_f64().unwrap()).abs() < 0.2, "{sl}");
    let d = call(&mut s, "design", json!({"resize": true}));
    eprintln!("rounds {} changes {} failing {}", d["rounds"], d["changes"], d["failing"]);
    for b in d["beams"].as_array().unwrap().iter().take(6) {
        eprintln!("  beam {}", b);
    }
    assert!(d["failing"].as_array().unwrap().is_empty(), "{}", d["failing"]);
    assert_eq!(d["beams"].as_array().unwrap().len(), 24);
    assert_eq!(d["slabs"].as_array().unwrap().len(), 8);
    eprintln!("beam 1: {}", d["beams"][0]);
    eprintln!("slab 1: {}", d["slabs"][0]);
    // the 130 mm slabs are under the Table 8.3.1.2 minimum (138 mm for 6 x 5 m): thickened
    assert!(d["changes"].as_array().unwrap().iter().any(|c| c.as_str().unwrap().starts_with("slab")), "{}", d["changes"]);
    let b = call(&mut s, "export_bim", json!({}));
    assert_eq!(b["members"][0]["family"].as_str().unwrap().is_empty(), false);
    let pic = s.handle(&json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": "show_picture", "arguments": {"diagram": "utilisation"}}})).unwrap();
    assert_eq!(pic["result"]["content"][0]["type"], "image");
}

#[test]
fn steel_frame_selects_shapes() {
    let mut s = Server::new(Model::new("Steel"));
    call(&mut s, "add_frame", json!({"x_spans": [8.0], "y_spans": [6.0], "storeys": [4.5], "material": "steel", "column": "H 200x200x8x12", "beam": "H 200x100x5.5x8", "slab": {"h": 0.12, "dead": 1.0, "live": 2.5}}));
    let d = call(&mut s, "design", json!({"select": true, "resize": true}));
    eprintln!("steel: rounds {} changes {} failing {}", d["rounds"], d["changes"], d["failing"]);
    for m in d["steel"].as_array().unwrap() {
        eprintln!("  {} {} ratio {} governs {}", m["member"], m["section"], m["ratio"], m["governs"]);
    }
    assert!(d["failing"].as_array().unwrap().is_empty());
    // the 8 m beams need more than the smallest shape
    assert!(d["steel"].as_array().unwrap().iter().any(|m| m["section"] != "H 200x100x5.5x8"));
}

#[test]
fn aedifex_style_import_merges_beams() {
    // a 6 m beam split in four (as Aedifex writes it) between two columns, bimIds on elements
    let mut nodes = vec![json!({"id": 1, "x": 0, "y": 0, "z": 0}), json!({"id": 2, "x": 6, "y": 0, "z": 0})];
    nodes.push(json!({"id": 3, "x": 0, "y": 0, "z": 3}));
    for k in 1..4 {
        nodes.push(json!({"id": 3 + k, "x": 1.5 * k as f64, "y": 0, "z": 3}));
    }
    nodes.push(json!({"id": 7, "x": 6, "y": 0, "z": 3}));
    let mut elements = vec![json!({"id": 1, "type": "frame", "nodeI": 1, "nodeJ": 3, "materialId": 2, "sectionId": 1, "bimId": "C-11"}), json!({"id": 2, "type": "frame", "nodeI": 2, "nodeJ": 7, "materialId": 2, "sectionId": 1, "bimId": "C-12"})];
    let chain = [3, 4, 5, 6, 7];
    for k in 0..4 {
        elements.push(json!({"id": 3 + k, "type": "frame", "nodeI": chain[k], "nodeJ": chain[k + 1], "materialId": 2, "sectionId": 2, "bimId": "B-20"}));
    }
    let loads: Vec<Value> = (0..4).map(|k| json!({"type": "distributed3d", "data": {"id": k + 1, "elementId": 3 + k, "qYI": 0, "qYJ": 0, "qZI": -20, "qZJ": -20, "a": 0, "b": 1.5, "frame": "global", "caseId": 2}})).collect();
    let fixture = json!({"model": {
        "name": "aedifex", "materials": [{"id": 2, "name": "Concrete C30", "e": 25743.0, "nu": 0.2, "rho": 24.0}],
        "sections": [{"id": 1, "name": "Concrete Rectangular Column: 400 x 400 mm", "a": 0.16, "iy": 0.002133, "iz": 0.002133, "j": 0.0036, "b": 0.4, "h": 0.4, "shape": "rect"}, {"id": 2, "name": "Concrete Rectangular Beam: 300 x 600 mm", "a": 0.18, "iy": 0.0054, "iz": 0.00135, "j": 0.0037, "b": 0.3, "h": 0.6, "shape": "rect"}],
        "nodes": nodes, "elements": elements, "supports": [{"id": 1, "nodeId": 1, "type": "fixed3d"}, {"id": 2, "nodeId": 2, "type": "fixed3d"}],
        "loads": loads, "plates": [], "quads": [], "constraints": [],
        "loadCases": [{"id": 1, "type": "D", "name": "D"}, {"id": 2, "type": "D", "name": "SD"}, {"id": 3, "type": "L", "name": "L"}],
        "combinations": [{"id": 1, "name": "ULS 1.4D", "factors": [{"caseId": 1, "factor": 1.4}, {"caseId": 2, "factor": 1.4}]}, {"id": 3, "name": "SLS D + L", "factors": [{"caseId": 1, "factor": 1.0}, {"caseId": 2, "factor": 1.0}, {"caseId": 3, "factor": 1.0}]}],
        "analysis": {"selfWeight": [{"caseId": 1, "direction": "Z", "factor": -1}]},
    }});
    let mut s = Server::new(Model::new("x"));
    let r = call(&mut s, "import_stabileo", json!({"model": fixture}));
    assert_eq!(r["import"]["merged"], 3, "{r}");
    assert_eq!(s.model.members.len(), 3);
    assert_eq!(s.model.materials[0].fc, 30.0);
    let a = call(&mut s, "analyze", json!({}));
    assert!(a["cases"].as_array().unwrap().iter().all(|c| c["balanced"] == true), "{a}");
    let sd = a["cases"].as_array().unwrap().iter().find(|c| c["case"] == "SD").unwrap().clone();
    assert!((sd["reactions_kN"][2].as_f64().unwrap() - 120.0).abs() < 1e-6, "{sd}");
    let d = call(&mut s, "design", json!({}));
    assert_eq!(d["beams"][0]["bim_id"], "B-20");
    let b = call(&mut s, "export_bim", json!({}));
    let beam = b["members"].as_array().unwrap().iter().find(|m| m["bim_id"] == "B-20").unwrap();
    assert_eq!(beam["family"], "Concrete Rectangular Beam");
    assert!(beam["reinforcement"]["bottom"].as_str().unwrap().contains("DB"), "{beam}");
    // round trip through Stabileo's format keeps the loads
    let back = pondus_core::stabileo::export(&s.model);
    let (m2, _) = pondus_core::stabileo::import(&back).unwrap();
    let a2 = pondus_core::analysis::analyze(&m2).unwrap();
    assert!((a2.case(2).unwrap().reaction_sum()[2] - 120.0).abs() < 1e-6);
}

#[test]
fn plate_slabs_design_the_building() {
    let mut s = Server::new(Model::new("RC plate"));
    call(&mut s, "add_frame", json!({"x_spans": [6.0, 6.0], "y_spans": [5.0, 5.0], "storeys": [4.0, 3.5], "column": "400 x 400", "beam": "300 x 600", "slab": {"h": 0.15, "dead": 1.5, "live": 3.0, "roof_live": 1.0}}));
    let t0 = std::time::Instant::now();
    let coef = call(&mut s, "design", json!({"slab_method": "coefficients"}));
    let t1 = t0.elapsed();
    let a = call(&mut s, "set_design", json!({"slab_method": "plate", "slab_mesh": 0.5}));
    assert_eq!(a["slab_method"], "plate");
    let an = call(&mut s, "analyze", json!({}));
    assert!(an["cases"].as_array().unwrap().iter().all(|c| c["balanced"] == true), "{an}");
    let t2 = std::time::Instant::now();
    let plate = call(&mut s, "design", json!({"resize": true}));
    eprintln!("coefficients {:.2} s, plate {:.2} s ({} elements)", t1.as_secs_f64(), t2.elapsed().as_secs_f64(), an["elements"]);
    for (c, p) in coef["slabs"].as_array().unwrap().iter().zip(plate["slabs"].as_array().unwrap()).take(2) {
        eprintln!("COEF  {} {}\n      {}", c["method"], c["ratio"], c["bars"]);
        eprintln!("PLATE {} {}\n      {}\n      {}", p["method"], p["ratio"], p["bars"], p["notes"][0]);
    }
    eprintln!("failing {} changes {} warnings {}", plate["failing"], plate["changes"], plate["warnings"]);
    for b in plate["beams"].as_array().unwrap().iter().filter(|b| b["ok"] == false).take(3) { eprintln!("NOTES {} {}", b["member"], b["notes"]); }
    assert!(plate["failing"].as_array().unwrap().is_empty(), "{}", plate["failing"]);
    assert!(plate["slabs"][0]["method"].as_str().unwrap().starts_with("plate"));
    let sr = call(&mut s, "get_slab_results", json!({"slabs": [1]}));
    eprintln!("SLAB RESULTS {}", sr["slabs"][0]);
    let s1 = &sr["slabs"][0];
    assert!(s1["my"]["max"][0].as_f64().unwrap() > 0.0 && s1["wood_armer_kNm_per_m"]["bottom_y"].as_f64().unwrap() >= s1["my"]["max"][0].as_f64().unwrap() - 1e-9);
    let pic = s.handle(&json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": "show_picture", "arguments": {"view": "plan", "z": 4.0, "diagram": "plateqx"}}})).unwrap();
    assert_eq!(pic["result"]["content"][0]["type"], "image");
}

#[test]
fn earthquake_on_a_building() {
    let mut s = Server::new(Model::new("RC seismic"));
    call(&mut s, "add_frame", json!({"x_spans": [6.0, 6.0, 6.0], "y_spans": [5.0, 5.0], "storeys": [4.0, 3.5, 3.5], "column": "500 x 500", "beam": "300 x 600", "slab": {"h": 0.15, "dead": 1.5, "live": 3.0, "roof_live": 1.0}}));
    call(&mut s, "set_seismic", json!({"enabled": true, "spectrum": {"kind": "code", "sds": 0.6, "sd1": 0.35, "tl": 6.0}, "r": 8.0, "cd": 5.5, "ie": 1.0}));
    let g = call(&mut s, "get_seismic", json!({}));
    eprintln!("weight {} kN, {} modes, first {}", g["weight_kN"], g["modes"].as_array().unwrap().len(), g["modes"][0]);
    for d in g["directions"].as_array().unwrap() {
        eprintln!("{} T1 {} T_ELF {} Cs {} V_ELF {} V_RSA {} scale {}", d["direction"], d["T1_s"], d["T_used_ELF_s"], d["Cs"], d["V_ELF_kN"], d["V_RSA_kN"], d["scale"]);
        for x in d["drifts"].as_array().unwrap() {
            eprintln!("   storey {} δ {} mm allowed {} ratio {}", x["storey_z"], x["delta_mm"], x["allowed_mm"], x["ratio"]);
        }
    }
    // the seismic weight: the D cases (frame, slabs, finishes) + 25 % of the live
    let an = call(&mut s, "analyze", json!({}));
    let d_kn = an["cases"][0]["reactions_kN"][2].as_f64().unwrap();
    let l_kn = an["cases"][1]["reactions_kN"][2].as_f64().unwrap();
    let w = g["weight_kN"].as_f64().unwrap();
    assert!((w - (d_kn + 0.25 * l_kn)).abs() / w < 0.01, "W {w} vs D {d_kn} + 0.25 L {l_kn}");
    let last = g["modes"].as_array().unwrap().last().unwrap()["cumulative_xyz"].clone();
    assert!(last[0].as_f64().unwrap() > 0.85 && last[1].as_f64().unwrap() > 0.85, "{last}");
    // after scaling the response spectrum base shear is at least the ELF one
    for d in g["directions"].as_array().unwrap() {
        let v = d["V_RSA_kN"].as_f64().unwrap() * d["scale"].as_f64().unwrap();
        assert!(v >= d["V_ELF_kN"].as_f64().unwrap() * 0.999);
    }
    let ex = call(&mut s, "get_results", json!({"result": "Ex", "members": [1]}));
    eprintln!("column 1 under Ex: {}", ex["members"][0]);
    let des = call(&mut s, "design", json!({}));
    let gov: Vec<String> = des["beams"].as_array().unwrap().iter().map(|b| b["governs"].as_str().unwrap().to_string()).collect();
    eprintln!("design: {} beams, failing {}, first beam {}", gov.len(), des["failing"], des["beams"][0]["ratio"]);
}

#[test]
fn columns_and_footings() {
    let mut s = Server::new(Model::new("RC cols"));
    call(&mut s, "add_frame", json!({"x_spans": [6.0, 6.0], "y_spans": [5.0, 5.0], "storeys": [4.0, 3.5, 3.5], "column": "400 x 400", "beam": "300 x 600", "slab": {"h": 0.15, "dead": 1.5, "live": 3.0, "roof_live": 1.0}}));
    let d = call(&mut s, "design", json!({"resize": true}));
    eprintln!("failing {} changes {}", d["failing"], d["changes"]);
    let cols = d["columns"].as_array().unwrap();
    assert_eq!(cols.len(), 27);
    for c in cols.iter().take(3) {
        eprintln!("COLUMN {} {} {} ρ {} % ties {} ratio {} governs {}\n   {}", c["member"], c["section_mm"], c["bars"], c["rho_percent"], c["ties"], c["ratio"], c["governs"], c["checks"]);
    }
    let f = d["footings"].as_array().unwrap();
    assert_eq!(f.len(), 9);
    for x in f.iter().take(2) {
        eprintln!("FOOTING {}", x);
    }
    assert!(d["failing"].as_array().unwrap().is_empty(), "{}", d["failing"]);
    let bim = call(&mut s, "export_bim", json!({}));
    assert_eq!(bim["footings"].as_array().unwrap().len(), 9);
    assert!(bim["members"].as_array().unwrap().iter().any(|m| m["reinforcement"]["longitudinal"].is_string()));
}

#[test]
fn phase1_tools() {
    let mut s = Server::new(Model::new("tools"));
    call(&mut s, "add_frame", json!({"x_spans": [6.0], "y_spans": [5.0], "storeys": [3.5, 3.5], "column": "350 x 350", "beam": "300 x 600", "slab": {"h": 0.15, "dead": 1.5, "live": 3.0}}));
    call(&mut s, "set_diaphragms", json!({"all": true}));
    let b = call(&mut s, "buckling", json!({"modes": 3}));
    eprintln!("buckling {}", b);
    assert!(b["load_factors"][0].as_f64().unwrap() > 1.0);
    call(&mut s, "set_design", json!({"second_order": true, "footing": {"q_allow": 120.0, "depth": 1.2}}));
    let d = call(&mut s, "design", json!({"resize": true}));
    eprintln!("second order design: failing {} footings {} first column {}", d["failing"], d["footings"].as_array().unwrap().len(), d["columns"][0]["bars"]);
    assert!(d["failing"].as_array().unwrap().is_empty());
    let rec: String = (0..=400).map(|k| format!("{} {}\n", k as f64 * 0.01, 0.2 * (2.0 * std::f64::consts::PI * 2.0 * k as f64 * 0.01).sin())).collect();
    let th = call(&mut s, "time_history", json!({"record": rec, "direction": "x", "dt": 0.01}));
    eprintln!("time history {}", th);
    assert!(th["roof_peak_mm"].as_f64().unwrap() > 0.0);
    call(&mut s, "add_loads", json!({"case": "L", "loads": [{"type": "thermal", "members": [9], "dt": 20.0}, {"type": "settlement", "at": [0.0, 0.0, 0.0], "d": [0.0, 0.0, -0.005, 0.0, 0.0, 0.0]}]}));
    call(&mut s, "add_supports", json!({"points": [[6.0, 5.0, 0.0]], "type": "spring", "springs": [1e5, 1e5, 2e5, 0.0, 0.0, 0.0]}));
    let an = call(&mut s, "analyze", json!({}));
    assert!(an["cases"].as_array().unwrap().iter().all(|c| c["balanced"] == true), "{an}");
}

#[test]
fn phase3_tools() {
    // a steel portal 6 x 4 driven through the agent's tools: pushover, a moving axle on the
    // beam, an influence line, two construction stages, uplift and a tension-only brace, and the
    // section analysis
    let mut s = Server::new(Model::new("portal"));
    call(&mut s, "add_members", json!({"members": [
        {"from": [0, 0, 0], "to": [0, 0, 4], "section": "H 300x150x6.5x9"},
        {"from": [6, 0, 0], "to": [6, 0, 4], "section": "H 300x150x6.5x9"},
        {"from": [0, 0, 4], "to": [6, 0, 4], "section": "H 300x150x6.5x9"}]}));
    call(&mut s, "add_supports", json!({"points": [[0, 0, 0], [6, 0, 0]], "type": "fixed"}));
    call(&mut s, "add_load_case", json!({"name": "L", "kind": "L"}));
    call(&mut s, "add_loads", json!({"case": "L", "loads": [{"type": "line", "members": [3], "w": [0, 0, -10]}]}));
    call(&mut s, "add_combination", json!({"name": "ULS", "kind": "uls", "factors": {"D": 1.2, "L": 1.6}}));
    let p = call(&mut s, "pushover", json!({"direction": "x"}));
    eprintln!("pushover: λ {} V {} kN, {} hinges", p["collapse_factor"], p["base_shear_kN"], p["hinges"].as_array().unwrap().len());
    // a fixed-base portal sways at four hinges: H h = 4 Mp
    let mp = 245e3 * 5.22e-4;
    let v = p["base_shear_kN"].as_f64().unwrap();
    assert!((v - 4.0 * mp / 4.0).abs() / (mp) < 0.05, "{v} vs {}", mp);
    let pc = call(&mut s, "pushover", json!({"combination": "ULS"}));
    assert!(pc["collapse_factor"].as_f64().unwrap() > 1.0, "{pc}");
    call(&mut s, "set_moving", json!({"path": [3], "axles": [[0, 50], [2, 50]], "impact": 0.0}));
    let a = call(&mut s, "analyze", json!({}));
    let g = call(&mut s, "get_results", json!({"result": "moving max", "members": [3]}));
    eprintln!("moving max on the beam: {}", g["members"][0]["My_sag_max [kNm, x]"]);
    let _ = a;
    let il = call(&mut s, "influence_line", json!({"quantity": "My", "member": 3, "x": 3.0}));
    assert!(il["max"]["value"].as_f64().unwrap() > 0.0, "{il}");
    call(&mut s, "set_stages", json!({"stages": [{"name": "columns", "members": [1, 2], "cases": [1]}, {"name": "beam", "members": [3], "cases": [2]}]}));
    let st = call(&mut s, "staged", json!({}));
    eprintln!("staged: {st}");
    let fin = call(&mut s, "get_results", json!({"result": "staged (final)", "members": [3]}));
    assert_eq!(fin["result"], "staged (final)");
    call(&mut s, "set_members", json!({"members": [3], "behaviour": "compression_only"}));
    call(&mut s, "set_members", json!({"members": [3], "behaviour": "normal"}));
    call(&mut s, "add_supports", json!({"points": [[6, 0, 0]], "type": "pinned", "uplift": true}));
    call(&mut s, "analyze", json!({}));
    let sec = call(&mut s, "section_analysis", json!({"name": "H 300x150x6.5x9"}));
    eprintln!("section FE: {sec}");
    let j = sec["J_m4"].as_f64().unwrap();
    assert!(j > 5e-8 && j < 1.5e-7, "{j}");
}

#[test]
fn calculation_report_pdf() {
    let mut s = Server::new(Model::new("RC report"));
    call(&mut s, "add_frame", json!({"x_spans": [6.0, 6.0], "y_spans": [5.0], "storeys": [4.0, 3.5], "column": "400 x 400", "beam": "250 x 500", "slab": {"h": 0.15, "dead": 1.5, "live": 3.0}}));
    call(&mut s, "design", json!({"resize": true}));
    let dir = std::env::var_os("PONDUS_REPORT_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let p = dir.join("pondus-test-report.pdf");
    let r = call(&mut s, "write_report", json!({"path": p.display().to_string(), "project": "Test building", "engineer": "Pondus test"}));
    eprintln!("report: {r}");
    let bytes = std::fs::read(&p).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    assert!(r["pages"].as_u64().unwrap() >= 4, "{r}");
    assert!(r["sections"].as_array().unwrap().iter().any(|x| x.as_str().unwrap().contains("Design")), "{r}");
}

#[test]
fn phase4_tools() {
    // copy a portal along y with the agent's transform, write Excel / DXF / IFC, read each back
    let mut s = Server::new(Model::new("portal4"));
    call(&mut s, "add_frame", json!({"x_spans": [6.0], "storeys": [4.0], "column": "400 x 400", "beam": "300 x 600"}));
    let n0 = s.model.members.len();
    let e = call(&mut s, "transform", json!({"all": true, "action": "move", "d": [0, 5, 0], "copy": true, "n": 2}));
    assert_eq!(s.model.members.len(), 3 * n0, "{e}");
    let sp = call(&mut s, "split_member", json!({"member": 3, "parts": 2}));
    eprintln!("split: {sp}");
    let dir = std::env::temp_dir().join(format!("pondus-io-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let before = s.model.members.len();
    for (fmt, ext) in [("excel", "xlsx"), ("dxf", "dxf"), ("ifc", "ifc")] {
        let p = dir.join(format!("m.{ext}"));
        call(&mut s, "export_file", json!({"format": fmt, "path": p.display().to_string()}));
        let mut t = Server::new(Model::new("x"));
        let r = call(&mut t, "import_file", json!({"format": fmt, "path": p.display().to_string(), "replace": true, "snap": 0.0}));
        eprintln!("{fmt}: {} members back of {before}; {}", t.model.members.len(), r["warnings"]);
        assert_eq!(t.model.members.len(), before, "{fmt}: {r}");
    }
}
