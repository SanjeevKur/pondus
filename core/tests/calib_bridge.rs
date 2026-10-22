//! Calibration of the moving-load analysis against a published bridge design: FHWA Steel Bridge
//! Design Handbook, Design Example 2A, two-span (90 ft - 90 ft) continuous composite steel
//! I-girder bridge (FHWA-HIF-16-002 Vol. 21, December 2015; AASHTO LRFD 7th ed., 2014).
//!
//! The example's Table 6 prints, at the tenth points of span 1, the unfactored and undistributed
//! (per lane) HL-93 live load moments: the vehicle (design truck with rear spacing 14-30 ft, or
//! the design tandem) positive and negative envelopes, 1.33 vehicle + lane in positive bending,
//! the "standard" negative 1.33 vehicle + lane, and the "special" negative loading of Article
//! 3.6.1.3.1, 90 % of two design trucks (rear spacing 14 ft, at least 50 ft apart) plus 90 % of the
//! lane, with IM 33 % on the trucks. The girder is analysed with the short-term composite moments of
//! inertia the example uses, 48,806 in.4 in the positive bending region and 50,027 in.4 from 63 to
//! 117 ft (Sec. 6.1 and p. 53), so the moments depend on that ratio and not on E.
//!
//! Every vehicle envelope is Pondus's moving-load analysis (advanced::moving_loads); the lane load,
//! which "need not be applied continuously" (Art. 3.6.1.2.4), is placed on the parts of Pondus's
//! influence line (advanced::influence) of the same sign. Rows go to calibration/bridge.csv.

mod calib;
use calib::{check, Row};

use pondus_core::advanced::{influence, moving_loads, MovingSpec};
use pondus_core::analysis::analyze;
use pondus_core::model::*;

const SUITE: &str = "bridge";
const FT: f64 = 0.3048;
const KIP: f64 = 4.448222;
const KIPFT: f64 = KIP * FT;
const IN4: f64 = 0.0254 * 0.0254 * 0.0254 * 0.0254;
const SRC: &str = "FHWA Steel Bridge Design Handbook, Design Example 2A (2015), Table 6";
const CASE: &str = "two-span 90 ft + 90 ft composite I-girder, HL-93 per lane";

#[allow(clippy::too_many_arguments)]
fn row(aspect: &str, case: &str, source: &str, quantity: &str, unit: &str, reference: f64, pondus: f64, tol: f64, kind: &str) {
    check(Row { suite: SUITE, aspect, case, source, quantity, unit, reference, pondus, tol, kind });
}

/// The tolerance of a value printed to the nearest kip-ft: 0.5 % or the rounding, whichever is
/// larger.
fn tol(r: f64) -> f64 {
    (0.5f64).max(0.6 / r.abs() * 100.0)
}

/// The girder: 20 members of 9 ft (every tenth point a node, and the section changes at 63 and
/// 117 ft with them), pinned at the left abutment, on rollers at the pier and the right abutment.
fn girder(spans: usize) -> Model {
    let mut m = Model::new("FHWA DE 2A girder");
    let id = m.next_material_id();
    m.materials.push(Material { id, name: "steel (weightless)".into(), kind: MaterialKind::Other, e: 199_948.0, nu: 0.3, gamma: 0.0, fc: 0.0, fy: 0.0, fu: 0.0, fb: 0.0, ft: 0.0, fv: 0.0, e_min: 0.0, alpha: 0.0 });
    let mut sec = |m: &mut Model, i: f64| {
        let s = m.next_section_id();
        m.sections.push(Section { id: s, name: format!("I {:.0} in4", i / IN4), shape: Shape::General { a: 0.06, iy: i, iz: i, j: 1e-4, shear: None } });
        s
    };
    let (s1, s2) = (sec(&mut m, 48_806.0 * IN4), sec(&mut m, 50_027.0 * IN4));
    for k in 0..10 * spans {
        let (x0, x1) = (k as f64 * 9.0, (k + 1) as f64 * 9.0);
        let s = if x0 >= 63.0 - 1e-6 && x1 <= 117.0 + 1e-6 { s2 } else { s1 };
        let (i, j) = (m.node_at([x0 * FT, 0.0, 0.0]), m.node_at([x1 * FT, 0.0, 0.0]));
        let mid = m.next_member_id();
        m.members.push(Member { id: mid, i, j, section: s, material: id, kind: MemberKind::Beam, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    }
    for (k, x) in (0..=spans).map(|k| k as f64 * 90.0).enumerate() {
        let n = m.find_node([x * FT, 0.0, 0.0]).unwrap();
        let fix = if k == 0 { [true, true, true, true, false, false] } else { [false, true, true, true, false, false] };
        m.supports.push(Support { node: n, fix, springs: [0.0; 6], uplift: false });
    }
    m.cases[0].self_weight = false;
    m
}

/// The moment (kip-ft) at the end of member `k` (tenth point k + 1 of span 1) of an envelope.
fn at(r: &CaseResultRef, k: usize) -> f64 {
    r.members[k].stations.last().unwrap().my / KIPFT
}
type CaseResultRef = pondus_core::analysis::CaseResult;

/// The envelope (max, min) at the tenth points 0.1 ... 1.0 of span 1 over several trains.
fn envelope(m: &Model, trains: &[Vec<(f64, f64)>]) -> (Vec<f64>, Vec<f64>) {
    let a = analyze(m).unwrap();
    let path: Vec<Id> = m.members.iter().map(|x| x.id).collect();
    let (mut hi, mut lo) = (vec![f64::MIN; 10], vec![f64::MAX; 10]);
    for axles in trains {
        let spec = MovingSpec { name: "HL-93".into(), path: path.clone(), axles: axles.clone(), step: 0.5 * FT, impact: 0.0, factor: 1.0 };
        let (h, l) = moving_loads(m, &a, &spec).unwrap();
        for k in 0..10 {
            hi[k] = hi[k].max(at(&h, k));
            lo[k] = lo[k].min(at(&l, k));
        }
    }
    (hi, lo)
}

/// A truck in both directions: lead axle 8 kip, 14 ft to the first 32-kip axle, `v` ft to the
/// second; weights scaled by `f`.
fn truck(v: f64, f: f64) -> Vec<Vec<(f64, f64)>> {
    let fwd = vec![(0.0, 8.0 * f * KIP), (14.0 * FT, 32.0 * f * KIP), ((14.0 + v) * FT, 32.0 * f * KIP)];
    let back = vec![(0.0, 32.0 * f * KIP), (v * FT, 32.0 * f * KIP), ((v + 14.0) * FT, 8.0 * f * KIP)];
    vec![fwd, back]
}

/// The lane load (0.64 klf) on the parts of the influence line of the moment at tenth point
/// k + 1 that have one sign: (positive, negative) moments in kip-ft.
fn lane(m: &Model, a: &pondus_core::analysis::Analysis, k: usize) -> (f64, f64) {
    let mem = m.members[k].id;
    let len = m.length(&m.members[k]);
    let pts = influence(m, a, "My", Some(mem), len, None).unwrap();
    // global position of each point along the girder
    let pos: Vec<(f64, f64)> = {
        let mut v: Vec<(f64, f64)> = pts.iter().map(|p| {
            let mm = m.member(p.member).unwrap();
            (m.node(mm.i).unwrap().x + p.x, p.value)
        }).collect();
        v.sort_by(|p, q| p.0.total_cmp(&q.0));
        v.dedup_by(|p, q| (p.0 - q.0).abs() < 1e-9);
        v
    };
    let (mut pos_area, mut neg_area) = (0.0, 0.0);
    for w in pos.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        let dx = x1 - x0;
        if y0 * y1 >= 0.0 {
            let a = 0.5 * (y0 + y1) * dx;
            if a > 0.0 { pos_area += a } else { neg_area += a }
        } else {
            // the sign changes inside: split at the zero
            let xz = dx * y0.abs() / (y0.abs() + y1.abs());
            let (a0, a1) = (0.5 * y0 * xz, 0.5 * y1 * (dx - xz));
            for a in [a0, a1] {
                if a > 0.0 { pos_area += a } else { neg_area += a }
            }
        }
    }
    let w = 0.64 * KIP / FT; // kN/m
    // a unit downward load gives the influence ordinate; the lane is downward too
    (w * pos_area / KIPFT, w * neg_area / KIPFT)
}

#[test]
fn fhwa_de2a_two_span_hl93() {
    let m = girder(2);
    let a = analyze(&m).unwrap();
    // the design truck with the rear spacing 14 to 30 ft, in both directions, and the tandem
    let mut trains: Vec<Vec<(f64, f64)>> = (0..=8).flat_map(|i| truck(14.0 + 2.0 * i as f64, 1.0)).collect();
    trains.push(vec![(0.0, 25.0 * KIP), (4.0 * FT, 25.0 * KIP)]);
    let (vpos, vneg) = envelope(&m, &trains);
    // the special negative loading: two design trucks (the 90 % is applied below), rear spacing
    // 14 ft, 50 ft or more apart
    let mut two = vec![];
    for gap in (50..=100).step_by(2) {
        for t in truck(14.0, 1.0) {
            let len = t.last().unwrap().0;
            let mut both = t.clone();
            both.extend(t.iter().map(|(o, w)| (o + len + gap as f64 * FT, *w)));
            two.push(both);
        }
    }
    let (_, sneg) = envelope(&m, &two);
    let lanes: Vec<(f64, f64)> = (0..10).map(|k| lane(&m, &a, k)).collect();

    // Table 6, span 1, tenth points 0.1 ... 1.0
    let veh_pos = [485.0, 816.0, 1002.0, 1083.0, 1059.0, 951.0, 743.0, 463.0, 192.0];
    let veh_neg = [-60.0, -120.0, -180.0, -240.0, -300.0, -359.0, -419.0, -479.0, -539.0, -599.0];
    let special = [-29.0, -59.0, -88.0, -118.0, -147.0, -176.0, -206.0, -967.0, -1250.0, -2020.0];
    let standard = [-112.0, -225.0, -337.0, -449.0, -562.0, -674.0, -786.0, -899.0, -1102.0, -1450.0];
    let plus_lane = [846.0, 1434.0, 1779.0, 1932.0, 1894.0, 1691.0, 1304.0, 770.0, 285.0];
    for k in 0..10 {
        let pt = format!("{:.1}L", (k + 1) as f64 / 10.0);
        if k < 9 {
            row("bridge moving load", CASE, SRC, &format!("vehicle M+ at {pt}"), "kip-ft", veh_pos[k], vpos[k], tol(veh_pos[k]), "published");
            // at 0.9L the influence line changes sign inside span 1; its positive part carries
            // 31.53 kip-ft of lane load (Pondus, and an independent integration at 0.1 ft), the
            // table's 285 implies 29.6, a coarser placement of the lane: 1 % allowed there
            let t = if k == 8 { 1.0 } else { tol(plus_lane[k]) };
            row("bridge moving load", CASE, SRC, &format!("1.33 vehicle + lane M+ at {pt}"), "kip-ft", plus_lane[k], 1.33 * vpos[k] + lanes[k].0, t, "published");
        }
        row("bridge moving load", CASE, SRC, &format!("vehicle M- at {pt}"), "kip-ft", veh_neg[k], vneg[k], tol(veh_neg[k]), "published");
        row("bridge moving load", CASE, SRC, &format!("standard M- (1.33 vehicle + lane) at {pt}"), "kip-ft", standard[k], 1.33 * vneg[k] + lanes[k].1, tol(standard[k]), "published");
        // the special loading applies between the points of dead-load contraflexure (Art.
        // 3.6.1.3.1). The values the table prints away from the pier (-29 at 0.1L, rising
        // linearly to -206 at 0.7L) are no two-truck placement, and the -967 at 0.8L would need a
        // two-truck effect of -611 kip-ft where no placement gives more than -479 (an independent
        // search over gaps of 50 to 250 ft agrees), so only 0.9L and the pier are compared
        if k >= 8 {
            let sp = 0.9 * (1.33 * sneg[k] + lanes[k].1);
            row("bridge moving load", CASE, SRC, &format!("special M- (90 % of two trucks + lane) at {pt}"), "kip-ft", special[k], sp, tol(special[k]), "published");
        }
    }
}
