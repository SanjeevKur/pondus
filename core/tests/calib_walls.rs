//! Calibration of the earthquake and dynamic analysis of wall structures. Every case is a
//! Pondus model with structural walls (`add_walls`: piers on the wall's centre line with rigid
//! arms), analysed through Pondus's seismic and time-history modules, and compared with:
//! - the natural frequencies and effective masses of a uniform flexural cantilever (Clough &
//!   Penzien 3e, Table 18-1; Chopra 5e Ch. 17);
//! - the continuum theory of wall-frame structures, a flexural cantilever coupled to a shear
//!   cantilever (Heidebrecht & Stafford Smith 1973; Miranda & Taghavi 2005), whose frequency
//!   equation is solved here independently of the engine for several stiffness ratios αH;
//! - the coupled lateral-torsional modes of a one-storey building with a wall on one side
//!   (Chopra 5e Ch. 9), from the 3 × 3 eigenproblem, and their response spectrum base shear by
//!   CQC (Der Kiureghian 1981);
//! - the step response of the wall by exact modal superposition (Chopra 5e Ch. 4 and 13).
//!
//! Each comparison is one row of calibration/walls.csv (PONDUS_CALIB).

mod calib;
use calib::{check, Row};

use pondus_core::analysis::analyze;
use pondus_core::commands::run;
use pondus_core::model::*;
use serde_json::json;
use std::f64::consts::PI;

const SUITE: &str = "walls";
const G: f64 = 9.81;

#[allow(clippy::too_many_arguments)]
fn row(aspect: &str, case: &str, source: &str, quantity: &str, unit: &str, reference: f64, pondus: f64, tol: f64, kind: &str) {
    check(Row { suite: SUITE, aspect, case, source, quantity, unit, reference, pondus, tol, kind });
}

/// A material without weight (the elements whose mass the benchmark leaves out).
fn weightless(m: &mut Model, e: f64) -> Id {
    let id = m.next_material_id();
    m.materials.push(Material { id, name: format!("weightless {id}"), kind: MaterialKind::Other, e, nu: 0.2, gamma: 0.0, fc: 0.0, fy: 0.0, fu: 0.0, fb: 0.0, ft: 0.0, fv: 0.0, e_min: 0.0, alpha: 0.0 });
    id
}

fn general(m: &mut Model, a: f64, iy: f64, iz: f64, j: f64) -> Id {
    let id = m.next_section_id();
    m.sections.push(Section { id, name: format!("G{id}"), shape: Shape::General { a, iy, iz, j } });
    id
}

fn mem(m: &mut Model, a: [f64; 3], b: [f64; 3], s: Id, mat: Id) -> Id {
    let (i, j) = (m.node_at(a), m.node_at(b));
    let id = m.next_member_id();
    m.members.push(Member { id, i, j, section: s, material: mat, kind: MemberKind::Auto, hinge_i: false, hinge_j: false, lb: None, k: None, roll: 0.0, truss: false, behaviour: Default::default(), winkler: None, rebar: None, bim_id: None, tension: None, role: Default::default(), group: None });
    id
}

fn modal_settings(m: &mut Model) {
    m.seismic.enabled = true;
    m.seismic.live_mass = 0.0;
    m.seismic.mass_target = 0.995;
    m.seismic.max_modes = 60;
    m.seismic.r = 1.0;
    m.seismic.ie = 1.0;
    m.seismic.elf_ratio = 0.0;
    m.seismic.spectrum = pondus_core::seismic::Spectrum::Table { points: vec![(0.0, 0.4), (50.0, 0.4)] };
}

/// The modes with mass in direction `d` (0 = x, 1 = y), by frequency: (f Hz, mass ratio).
fn modes_in(a: &pondus_core::analysis::Analysis, d: usize, min: f64) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = a.seismic.as_ref().unwrap().modes.iter().filter(|md| md.mass_ratio[d] > min).map(|md| (md.frequency, md.mass_ratio[d])).collect();
    v.sort_by(|p, q| p.0.total_cmp(&q.0));
    v
}

// ----------------------------------------------------------------- the continuum

/// The frequency determinant of a flexural cantilever (EI) coupled to a shear cantilever (GA)
/// with uniform mass, in the dimensionless wave numbers a, b (a² − b² = α², λ⁴ = a²b², with
/// α = H √(GA/EI)), scaled by cosh a. From u'''' − α²u'' − λ⁴u = 0, u(0) = u'(0) = 0,
/// u''(1) = 0 and u'''(1) − α²u'(1) = 0.
fn det(b: f64, alpha: f64) -> f64 {
    // expanded exactly (1 − tanh²a = 1/cosh²a, cos²b + sin²b = 1), so that nothing cancels
    // when tanh a rounds to 1 at large α
    let a = (b * b + alpha * alpha).sqrt();
    a * (a.powi(4) + b.powi(4)) * b.cos() + a * a * b * (a * a - b * b) * a.tanh() * b.sin() + 2.0 * a.powi(3) * b * b / a.cosh()
}

/// The first `n` modes of the continuum: (λ² = ab, effective mass ratio).
fn continuum(alpha: f64, n: usize) -> Vec<(f64, f64)> {
    let mut out = vec![];
    let (mut b0, mut d0) = (1e-6, det(1e-6, alpha));
    let mut b = 1e-6;
    while out.len() < n {
        b += 1e-3;
        assert!(b < 200.0, "no root of the frequency equation below b = 200");
        let d1 = det(b, alpha);
        if d0.signum() != d1.signum() && d0.is_finite() && d1.is_finite() {
            let (mut lo, mut hi) = (b0, b);
            for _ in 0..80 {
                let mid = (lo + hi) / 2.0;
                if det(mid, alpha).signum() == det(lo, alpha).signum() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let bb = (lo + hi) / 2.0;
            let a = (bb * bb + alpha * alpha).sqrt();
            // the mode shape and its effective mass (uniform mass): (∫u)² / ∫u²
            // u = cosh az − cos bz + c2 (sinh az − (a/b) sin bz), with the growing exponential
            // written as e^{a(z−1)} so that nothing overflows or cancels at large a
            let den = a * a * a.sinh() + a * bb * bb.sin();
            let c2 = -(a * a * a.cosh() + bb * bb * bb.cos()) / den;
            let grow = (-a * a * (-a).exp() + a * bb * bb.sin() - bb * bb * bb.cos()) / (a * a * (1.0 - (-2.0 * a).exp()) + 2.0 * a * bb * bb.sin() * (-a).exp());
            let u = |z: f64| grow * (a * (z - 1.0)).exp() + (1.0 - c2) / 2.0 * (-a * z).exp() - (bb * z).cos() - c2 * a / bb * (bb * z).sin();
            let k = 4000;
            let (mut s1, mut s2) = (0.0, 0.0);
            for i in 0..=k {
                let z = i as f64 / k as f64;
                let w = if i == 0 || i == k { 0.5 } else { 1.0 } / k as f64;
                s1 += w * u(z);
                s2 += w * u(z) * u(z);
            }
            out.push((a * bb, s1 * s1 / s2));
        }
        b0 = b;
        d0 = d1;
    }
    out
}

#[test]
fn continuum_solver_limits() {
    // the solver against its two known limits: the flexural cantilever (1 + cosh β cos β = 0,
    // β1 = 1.8751, effective mass 0.6131) and, for large αH, the shear cantilever
    // (λ² → (2n − 1)π αH/2, effective mass 8/π² = 0.8106)
    let f = continuum(0.0, 3);
    for (k, beta) in [1.875104, 4.694091, 7.854757].iter().enumerate() {
        assert!((f[k].0 - beta * beta).abs() < 1e-5 * beta * beta, "{:?}", f[k]);
    }
    assert!((f[0].1 - 0.6131).abs() < 2e-4, "{}", f[0].1);
    let s = continuum(200.0, 1);
    let shear = PI / 2.0 * 200.0;
    assert!((s[0].0 - shear).abs() / shear < 0.01, "{} vs {shear}", s[0].0);
    assert!((s[0].1 - 8.0 / (PI * PI)).abs() < 0.01, "{}", s[0].1);
}

// ----------------------------------------------------------------- cases

/// A slender cantilever wall, t 0.3 × lw 2 m, 60 m high in `storeys` storeys, its mass its own
/// weight.
fn wall_tower(storeys: usize) -> (Model, f64, f64) {
    let mut m = Model::new("wall");
    let h = 60.0;
    run(&mut m, "add_walls", &json!({"walls": [{"from": [0.0, 0.0], "to": [2.0, 0.0], "t": 0.3, "z0": 0.0, "z1": h}], "storey_h": h / storeys as f64})).unwrap();
    modal_settings(&mut m);
    let c = &m.materials[0];
    let ei = c.e * 1e3 * 0.3 * 8.0 / 12.0;
    let mbar = c.gamma * 0.3 * 2.0 / G;
    (m, ei, mbar)
}

#[test]
fn cantilever_wall_modes() {
    let (m, ei, mbar) = wall_tower(20);
    let a = analyze(&m).unwrap();
    let x = modes_in(&a, 0, 1e-3);
    let src = "Clough & Penzien 3e, Table 18-1 (βnL of a uniform cantilever)";
    let ratio = 3.0 * ei / (5.0 / 6.0 * 0.3 * 2.0 * m.materials[0].e * 1e3 / 2.4 * 3600.0);
    eprintln!("wall shear flexibility / bending flexibility at the top: {ratio:.2e}");
    for (k, (beta, tol)) in [(1.875104, 0.3), (4.694091, 0.6), (7.854757, 1.2)].iter().enumerate() {
        let f = beta * beta / (2.0 * PI * 60.0 * 60.0) * (ei / mbar).sqrt();
        row("wall modal", "cantilever wall 0.3 × 2 m, H 60 m, 20 storeys", src, &format!("f{} (in plane)", k + 1), "Hz", f, x[k].0, *tol, "published");
    }
    // the effective masses of the flexural cantilever: 0.6131, 0.1883, 0.0647 (of the total)
    let cont = continuum(0.0, 3);
    for (k, tol) in [0.5, 1.0, 2.0].iter().enumerate() {
        row("wall modal", "cantilever wall 0.3 × 2 m, H 60 m, 20 storeys", "Chopra 5e Ch. 17 (uniform cantilever, (∫mφ)²/∫mφ²)", &format!("effective mass ratio, mode {}", k + 1), "–", cont[k].1, x[k].1, *tol, "closed-form");
    }
}

/// A wall coupled to two shear frames (columns with rigid beams, one at y = −6 m and one at
/// y = +6 m, so that the x modes stay planar) by rigid diaphragms, the mass all in the wall.
fn wall_frame(alpha_h: f64, storeys: usize) -> (Model, f64, f64) {
    let (mut m, ei, mbar) = wall_tower(storeys);
    let h = 60.0;
    let hs = h / storeys as f64;
    // the frames are shear beams only: their columns are 1000 times stiffer axially than
    // concrete (with Ic scaled down to keep their bending stiffness), so the frames' own
    // overturning flexibility, which the continuum leaves out, is negligible
    let e = m.materials[0].e * 1000.0;
    let mat = weightless(&mut m, e);
    // the frames' total shear stiffness GA = αH² EI / H², per storey k = GA/hs, from four
    // columns fixed-fixed by rigid beams: 4 · 12 E Ic / hs³ = GA / hs
    let ga = alpha_h * alpha_h * ei / (h * h);
    let ic = ga * hs * hs / (4.0 * 12.0 * e * 1e3);
    let col = general(&mut m, 0.2, ic, ic, ic);
    // the beams 10⁴ times stiffer than the columns, enough to fix the column ends but not so
    // stiff that K becomes ill-conditioned (the diaphragm already makes the floor rigid in plan)
    let beam = general(&mut m, 0.2, 1e4 * ic, 1e4 * ic, 1e4 * ic);
    for k in 0..storeys {
        let (z0, z1) = (k as f64 * hs, (k + 1) as f64 * hs);
        for y in [-6.0, 6.0] {
            for x in [0.0, 6.0] {
                mem(&mut m, [x, y, z0], [x, y, z1], col, mat);
            }
            mem(&mut m, [0.0, y, z1], [6.0, y, z1], beam, mat);
        }
    }
    for y in [-6.0, 6.0] {
        for x in [0.0, 6.0] {
            let n = m.find_node([x, y, 0.0]).unwrap();
            m.supports.push(Support::of_kind(n, "fixed").unwrap());
        }
    }
    pondus_core::walls::sync(&mut m);
    m.diaphragms = (1..=storeys).map(|k| k as f64 * hs).collect();
    (m, ei, mbar)
}

#[test]
fn wall_frame_continuum() {
    let src = "Heidebrecht & Stafford Smith (1973); Miranda & Taghavi (2005): wall-frame continuum, frequency equation solved independently";
    for alpha_h in [0.5, 1.0, 3.0, 8.0] {
        let (m, ei, mbar) = wall_frame(alpha_h, 40);
        let a = analyze(&m).unwrap();
        let x = modes_in(&a, 0, 2e-3);
        let cont = continuum(alpha_h, 3);
        let case = format!("wall + shear frames, αH = {alpha_h}, 40 storeys");
        for k in 0..3 {
            let f = cont[k].0 / (2.0 * PI * 3600.0) * (ei / mbar).sqrt();
            row("wall-frame modal", &case, src, &format!("f{} (in plane)", k + 1), "Hz", f, x[k].0, 1.5, "independent");
        }
        row("wall-frame modal", &case, src, "effective mass ratio, mode 1", "–", cont[0].1, x[0].1, 1.0, "independent");
    }
    // the storeys converge on the continuum (the frames' shear is lumped per storey)
    let mut last = f64::INFINITY;
    for storeys in [10, 20, 40] {
        let (m, ei, mbar) = wall_frame(8.0, storeys);
        let a = analyze(&m).unwrap();
        let f1 = modes_in(&a, 0, 2e-3)[0].0;
        let fc = continuum(8.0, 1)[0].0 / (2.0 * PI * 3600.0) * (ei / mbar).sqrt();
        let err = ((f1 - fc) / fc).abs();
        eprintln!("αH 8, {storeys} storeys: f1 {f1:.5} Hz, continuum {fc:.5} Hz, error {:.3} %", err * 100.0);
        row("wall-frame modal", &format!("wall + shear frames, αH = 8, {storeys} storeys"), src, "f1 (convergence with the storeys)", "Hz", fc, f1, 3.0, "independent");
        assert!(err < last + 1e-6, "no convergence: {err} after {last}");
        last = err;
    }
}

/// The 3 × 3 eigenproblem of a rigid floor (ux, uy, θ about the mass centre): frequencies and
/// the share of the mass in y of each mode, by Jacobi rotations on M^-½ K M^-½.
fn floor_modes(k: [[f64; 3]; 3], mass: [f64; 3]) -> Vec<(f64, f64, f64)> {
    let s: [f64; 3] = mass.map(|m| 1.0 / m.sqrt());
    let mut a = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            a[i][j] = s[i] * k[i][j] * s[j];
        }
    }
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..100 {
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-15 * (a[p][p].abs() + a[q][q].abs()) {
                continue;
            }
            let th = 0.5 * (2.0 * a[p][q]).atan2(a[q][q] - a[p][p]);
            let (c, sn) = (th.cos(), th.sin());
            for r in 0..3 {
                let (arp, arq) = (a[r][p], a[r][q]);
                a[r][p] = c * arp - sn * arq;
                a[r][q] = sn * arp + c * arq;
            }
            for r in 0..3 {
                let (apr, aqr) = (a[p][r], a[q][r]);
                a[p][r] = c * apr - sn * aqr;
                a[q][r] = sn * apr + c * aqr;
            }
            for r in 0..3 {
                let (vrp, vrq) = (v[r][p], v[r][q]);
                v[r][p] = c * vrp - sn * vrq;
                v[r][q] = sn * vrp + c * vrq;
            }
        }
    }
    let mut out: Vec<(f64, f64, f64)> = (0..3)
        .map(|j| {
            // φ = M^-½ v: participation in x and y (mass shares)
            let phi = [s[0] * v[0][j], s[1] * v[1][j], s[2] * v[2][j]];
            let mn: f64 = (0..3).map(|i| mass[i] * phi[i] * phi[i]).sum();
            let (lx, ly) = (mass[0] * phi[0], mass[1] * phi[1]);
            (a[j][j].sqrt() / (2.0 * PI), lx * lx / mn / mass[0], ly * ly / mn / mass[1])
        })
        .collect();
    out.sort_by(|p, q| p.0.total_cmp(&q.0));
    out
}

#[test]
fn one_storey_wall_on_one_side() {
    // a 10 × 10 m floor of stiff, continuous beams carrying the mass (10 kN/m on the
    // three edges y = 0, y = 10 and x = 10), on four corner columns pinned at the top (so each
    // is a cantilever, k = 3EI/h³) and a wall on the fourth edge (x = 0, y 3 to 7), whose top
    // is free to rotate in its plane (no beam along it): the mass centre sits at x = 6.67 m,
    // the stiffness centre near the wall, and the y sway couples with torsion
    let h = 3.5;
    let mut m = Model::new("one storey");
    m.materials[0].gamma = 0.0;
    let e = m.materials[0].e;
    let cmat = weightless(&mut m, e);
    let col = m.rect_section(0.4, 0.4);
    // the beams carry the mass between the corners, where only the model's nodes join the
    // diaphragm: stiff enough in plan and axially (I 2000 m⁴, A 100 m²; their density then
    // comes from the 10 kN/m alone) that their own deformation leaves the floor modes (up to
    // 47 Hz) alone
    let beam = general(&mut m, 100.0, 2000.0, 2000.0, 500.0);
    let mut cols = vec![];
    for (x, y) in [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (10.0, 10.0)] {
        let id = mem(&mut m, [x, y, 0.0], [x, y, h], col, cmat);
        m.members.iter_mut().find(|q| q.id == id).unwrap().hinge_j = true;
        cols.push(id);
        let n = m.find_node([x, y, 0.0]).unwrap();
        m.supports.push(Support::of_kind(n, "fixed").unwrap());
    }
    let w = 10.0;
    for (a, b) in [([0.0, 0.0, h], [10.0, 0.0, h]), ([0.0, 10.0, h], [10.0, 10.0, h]), ([10.0, 0.0, h], [10.0, 10.0, h])] {
        let id = mem(&mut m, a, b, beam, cmat);
        m.loads.push(Load { case: 1, kind: LoadKind::Line { member: id, wi: [0.0, 0.0, -w], wj: [0.0, 0.0, -w], a: None, b: None } });
    }
    run(&mut m, "add_walls", &json!({"walls": [{"from": [0.0, 3.0], "to": [0.0, 7.0], "t": 0.25, "z0": 0.0, "z1": h}]})).unwrap();
    m.cases[0].self_weight = true;
    m.diaphragms = vec![h];
    modal_settings(&mut m);
    let a = analyze(&m).unwrap();
    // the reference: masses and polar inertia of the three loaded edges, about the mass centre
    let mu = w / G;
    let edges = [([0.0, 0.0], [10.0, 0.0]), ([0.0, 10.0], [10.0, 10.0]), ([10.0, 0.0], [10.0, 10.0])];
    let mass: f64 = edges.len() as f64 * mu * 10.0;
    let (xc, yc) = (edges.iter().map(|(p, q)| (p[0] + q[0]) / 2.0).sum::<f64>() / 3.0, edges.iter().map(|(p, q)| (p[1] + q[1]) / 2.0).sum::<f64>() / 3.0);
    let jp: f64 = edges.iter().map(|(p, q)| {
        let (cx, cy) = ((p[0] + q[0]) / 2.0 - xc, (p[1] + q[1]) / 2.0 - yc);
        mu * 10.0 * (cx * cx + cy * cy + 100.0 / 12.0)
    }).sum();
    // stiffnesses: cantilever columns 3EI/h³ both ways, torsion GJ/h; the wall in plane
    // 1/(h³/3EI + h/κGA), out of plane 3EI/h³ + its shear, its own torsion GJ/h
    let ee = e * 1e3;
    let gg = ee / 2.4;
    let pc = m.section(col).unwrap().props();
    let kc = 3.0 * ee * pc.iy / h.powi(3);
    let wsec = m.members.iter().find(|q| matches!(q.role, Role::WallPier(_))).map(|q| q.section).unwrap();
    let pw = m.section(wsec).unwrap().props();
    let (t, lw): (f64, f64) = (0.25, 4.0);
    let kw_in = 1.0 / (h.powi(3) / (3.0 * ee * t * lw.powi(3) / 12.0) + h / (5.0 / 6.0 * t * lw * gg));
    let kw_out = 1.0 / (h.powi(3) / (3.0 * ee * lw * t.powi(3) / 12.0) + h / (5.0 / 6.0 * t * lw * gg));
    let mut kk = [[0.0; 3]; 3];
    let mut add = |x: f64, y: f64, kx: f64, ky: f64, kt: f64| {
        let (dx, dy) = (x - xc, y - yc);
        kk[0][0] += kx;
        kk[1][1] += ky;
        kk[0][2] -= kx * dy;
        kk[2][0] -= kx * dy;
        kk[1][2] += ky * dx;
        kk[2][1] += ky * dx;
        kk[2][2] += kx * dy * dy + ky * dx * dx + kt;
    };
    for (x, y) in [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (10.0, 10.0)] {
        add(x, y, kc, kc, gg * pc.j / h);
    }
    add(0.0, 5.0, kw_out, kw_in, gg * pw.j / h);
    let refm = floor_modes(kk, [mass, mass, jp]);
    let sr = a.seismic.as_ref().unwrap();
    // Pondus's three floor modes: the lowest with horizontal mass or a torsion of the floor
    let mut pm: Vec<(f64, f64, f64)> = sr.modes.iter().filter(|md| md.mass_ratio[0] + md.mass_ratio[1] > 1e-4).map(|md| (md.frequency, md.mass_ratio[0], md.mass_ratio[1])).collect();
    pm.sort_by(|p, q| p.0.total_cmp(&q.0));
    eprintln!("mass {mass:.3} t, Jp {jp:.2} t·m², centre ({xc:.3}, {yc:.3}); reference {refm:?}; Pondus {:?}", &pm[..pm.len().min(4)]);
    let src = "Chopra 5e Ch. 9 (one-storey unsymmetric-plan system), 3 × 3 eigenproblem";
    let case = "one storey, wall on one side (x = 0), mass centre at x = 6.67 m";
    for k in 0..3 {
        row("torsional coupling", case, src, &format!("f{}", k + 1), "Hz", refm[k].0, pm[k].0, 0.5, "closed-form");
    }
    for k in 0..3 {
        if refm[k].2 > 0.02 {
            row("torsional coupling", case, src, &format!("mass ratio in y, mode {}", k + 1), "–", refm[k].2, pm[k].2, 1.0, "closed-form");
        }
    }
    // the response spectrum base shear in y, flat Sa = 0.4 g: modal V = (mass share) W Sa, CQC
    let wt = mass * G;
    let ys: Vec<(f64, f64)> = refm.iter().filter(|r| r.2 > 1e-6).map(|r| (2.0 * PI * r.0, r.2 * wt * 0.4)).collect();
    let xi: f64 = 0.05;
    let mut v2 = 0.0;
    for (wi, vi) in &ys {
        for (wj, vj) in &ys {
            let r = wj / wi;
            let rho = 8.0 * xi * xi * (1.0 + r) * r.powf(1.5) / ((1.0 - r * r).powi(2) + 4.0 * xi * xi * r * (1.0 + r).powi(2));
            v2 += rho * vi * vj;
        }
    }
    let dy = sr.dirs.iter().find(|d| d.name == "Ey").unwrap();
    row("wall RSA", case, "Chopra 5e Ch. 13; Der Kiureghian (1981) CQC, ξ = 5 %", "base shear V_RSA in y (flat Sa 0.4 g)", "kN", v2.sqrt(), dy.v_rsa, 0.5, "closed-form");
    row("wall RSA", case, "the loaded edges' weight", "seismic weight W", "kN", wt, sr.weight, 0.1, "closed-form");
}

#[test]
fn wall_step_response() {
    // an undamped step of ground acceleration a0 = 0.1 g along the wall: by modal superposition
    // u_top(t) = Σ Γn φn(H) a0/ωn² (1 − cos ωn t), the modes of the flexural cantilever
    let (m, ei, mbar) = wall_tower(20);
    let a = analyze(&m).unwrap();
    let h = 60.0;
    let beta = [1.875104, 4.694091, 7.854757, 10.995541, 14.137168, 17.278760];
    let a0 = 0.1 * G;
    let mut terms = vec![];
    for b in beta {
        let w = b * b / (h * h) * (ei / mbar).sqrt();
        // φ(z) = cosh − cos − σ(sinh − sin), σ = (cosh b + cos b)/(sinh b + sin b); Γφ(H) from
        // the exact integrals ∫φ and ∫φ² (numerically)
        let s = (b.cosh() + b.cos()) / (b.sinh() + b.sin());
        let phi = |z: f64| (b * z).cosh() - (b * z).cos() - s * ((b * z).sinh() - (b * z).sin());
        let k = 4000;
        let (mut s1, mut s2) = (0.0, 0.0);
        for i in 0..=k {
            let z = i as f64 / k as f64;
            let wq = if i == 0 || i == k { 0.5 } else { 1.0 } / k as f64;
            s1 += wq * phi(z);
            s2 += wq * phi(z) * phi(z);
        }
        terms.push((w, s1 / s2 * phi(1.0) * a0 / (w * w)));
    }
    let t1 = 2.0 * PI / terms[0].0;
    let dur = 1.2 * t1;
    let mut peak = 0.0f64;
    let n = 20000;
    for i in 0..=n {
        let t = dur * i as f64 / n as f64;
        let u: f64 = terms.iter().map(|(w, c)| c * (1.0 - (w * t).cos())).sum();
        peak = peak.max(u.abs());
    }
    let static_top = a0 * mbar * h.powi(4) / (8.0 * ei);
    eprintln!("step: T1 {t1:.3} s, modal peak {:.2} mm, static wL⁴/8EI {:.2} mm", peak * 1e3, static_top * 1e3);
    let rec = pondus_core::timehistory::Record { name: "step 0.1 g".into(), dt: 0.01, accel: vec![0.1; (dur / 0.01) as usize + 10] };
    let st = pondus_core::timehistory::Settings { direction: 0, scale: 1.0, dt: t1 / 400.0, duration: Some(dur), damping: 0.0, alpha: None };
    let th = pondus_core::timehistory::run(&m, &a, &rec, &st).unwrap();
    let src = "Chopra 5e Ch. 4 and 13: step response by modal superposition (6 modes)";
    row("wall time history", "cantilever wall 0.3 × 2 m, H 60 m, step a0 = 0.1 g, undamped", src, "peak top displacement", "mm", peak * 1e3, th.roof_peak * 1e3, 1.5, "closed-form");
    let ratio = th.roof_peak / static_top;
    row("wall time history", "cantilever wall 0.3 × 2 m, H 60 m, step a0 = 0.1 g, undamped", "the same, as a dynamic amplification of the static top displacement", "peak / (m a0 H⁴/8EI)", "–", peak / static_top, ratio, 1.5, "closed-form");
}
