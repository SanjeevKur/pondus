// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The GPU factorisation (cuDSS) against the engine's CPU Cholesky. Runs where an NVIDIA GPU
//! and cuDSS are found (LD_LIBRARY_PATH, or PONDUS_CUDART / PONDUS_CUDSS); elsewhere it passes
//! without checking anything and says so.

use dedaliano_engine::linalg::gpu;
use dedaliano_engine::linalg::sparse::CscMatrix;
use dedaliano_engine::linalg::sparse_chol;

/// The 7-point Laplacian of an m×m×m grid plus a small diagonal shift, lower triangle.
fn laplacian(m: usize) -> CscMatrix {
    let idx = |i: usize, j: usize, k: usize| (k * m + j) * m + i;
    let (mut r, mut c, mut v) = (vec![], vec![], vec![]);
    for k in 0..m {
        for j in 0..m {
            for i in 0..m {
                let p = idx(i, j, k);
                r.push(p);
                c.push(p);
                v.push(6.01);
                for (di, dj, dk) in [(1, 0, 0), (0, 1, 0), (0, 0, 1)] {
                    let (a, b, d) = (i + di, j + dj, k + dk);
                    if a < m && b < m && d < m {
                        r.push(idx(a, b, d));
                        c.push(p);
                        v.push(-1.0);
                    }
                }
            }
        }
    }
    CscMatrix::from_triplets(m * m * m, &r, &c, &v)
}

/// On a machine with an NVIDIA GPU and cuDSS: the GPU factor solves as the CPU one does, and
/// gives the same bits twice. Skipped (passes) where there is none.
#[test]
fn gpu_matches_cpu_cholesky() {
    if !gpu::available() {
        eprintln!("no GPU: {}", gpu::summary());
        return;
    }
    let a = laplacian(32); // 32,768 unknowns
    let b: Vec<f64> = (0..a.n).map(|i| ((i * 7919) % 101) as f64 - 50.0).collect();
    let sym = std::rc::Rc::new(sparse_chol::symbolic_cholesky(&a));
    let cpu = sparse_chol::sparse_cholesky_solve(&sparse_chol::numeric_cholesky(&sym, &a).unwrap(), &b);
    let g1 = gpu::Factor::new(&a).unwrap().solve(&b).unwrap();
    let g2 = gpu::Factor::new(&a).unwrap().solve(&b).unwrap();
    let scale = cpu.iter().fold(0.0f64, |s, x| s.max(x.abs()));
    let diff = cpu.iter().zip(&g1).fold(0.0f64, |s, (x, y)| s.max((x - y).abs())) / scale;
    assert!(diff < 1e-10, "GPU and CPU differ by {diff:e}");
    assert_eq!(g1, g2, "the GPU solve is not deterministic");
    // a matrix that is not positive definite is refused as such, for the CPU to report
    let mut bad = a.clone();
    bad.values[0] = -1.0;
    assert!(matches!(gpu::Factor::new(&bad), Err(None)));
}
