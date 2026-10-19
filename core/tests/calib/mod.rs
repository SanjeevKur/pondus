//! The calibration recorder: every comparison of Pondus against a published or closed-form
//! value is one row, asserted against its tolerance and, when PONDUS_CALIB names a directory,
//! appended to `<dir>/<suite>.csv` (aspect, case, source, quantity, unit, reference, pondus,
//! error %, tolerance %, kind) for the calibration paper's tables.
//!
//! kind: "published" (a value printed in the cited source), "closed-form" (a formula of the
//! cited source evaluated here, the formula named in `source`) or "independent" (another
//! program or method).

#![allow(dead_code)]

use std::io::Write;

pub struct Row<'a> {
    pub suite: &'a str,
    pub aspect: &'a str,
    pub case: &'a str,
    pub source: &'a str,
    pub quantity: &'a str,
    pub unit: &'a str,
    pub reference: f64,
    pub pondus: f64,
    /// Relative tolerance, % (or absolute when the reference is 0: then in the quantity's unit).
    pub tol: f64,
    pub kind: &'a str,
}

/// Records the row and asserts it.
pub fn check(r: Row) {
    let err = if r.reference.abs() > 1e-12 { (r.pondus - r.reference) / r.reference.abs() * 100.0 } else { r.pondus - r.reference };
    eprintln!("[{}] {} | {} | {} = {:.6} vs {:.6} {} ({:+.3} %, tol {} %) [{}]", r.aspect, r.case, r.source, r.quantity, r.pondus, r.reference, r.unit, err, r.tol, r.kind);
    if let Some(dir) = std::env::var_os("PONDUS_CALIB") {
        let p = std::path::Path::new(&dir).join(format!("{}.csv", r.suite));
        let new = !p.exists();
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
            if new {
                let _ = writeln!(f, "aspect,case,source,quantity,unit,reference,pondus,error_pct,tol_pct,kind");
            }
            let q = |s: &str| format!("\"{}\"", s.replace('"', "'"));
            let _ = writeln!(f, "{},{},{},{},{},{},{},{:.4},{},{}", q(r.aspect), q(r.case), q(r.source), q(r.quantity), q(r.unit), r.reference, r.pondus, err, r.tol, r.kind);
        }
    }
    assert!(err.abs() <= r.tol, "{} / {}: {} = {} vs {} ({:+.3} % > {} %)", r.aspect, r.case, r.quantity, r.pondus, r.reference, err, r.tol);
}
