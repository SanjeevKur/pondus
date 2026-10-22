# Notice

Pondus, Copyright (C) 2026 Sompote Youwai.

Pondus is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, version 3 only (AGPL-3.0-only). The full text is in [LICENSE](LICENSE). Pondus is distributed in the hope that it will be useful, but without any warranty, without even the implied warranty of merchantability or fitness for a particular purpose.

The complete corresponding source of every version is published at **https://github.com/Sompote/pondus**. The program shows this link in Help › About, in `pondus --about`, in the `serverInfo` and instructions of its MCP server and in the footer of every calculation report. AGPL-3.0 section 13 requires this, because users may reach Pondus over a network, for example as an MCP server called by another agent. If you modify Pondus and let others use it, over a network or otherwise, you must offer them the source of your modified version under the same licence.

## Why the whole program is AGPL-3.0

Pondus compiles the Stabileo analysis engine into the same binary as its own code, so the program as a whole is a combined work of that AGPL-3.0 engine. The code in `core/` and `app/` was written for Pondus and contains no Stabileo code, but it is distributed under AGPL-3.0 together with the engine. Programs that only exchange data with Pondus over MCP, JSON, Excel, DXF or IFC files, such as the Aedifex BIM modeller, are separate programs and are not affected. The models, results, drawings and reports that Pondus produces belong to their user and are not covered by the licence.

## Third-party components

| Component | Where | Licence | Copyright and source |
|---|---|---|---|
| Stabileo analysis engine (`dedaliano-engine`) | `engine/` | AGPL-3.0-only | The Stabileo authors: Bautista Chesta, Diego Kingston, Federico Carrone and contributors at LambdaClass. https://github.com/lambdaclass/stabileo, commit `7985e8c59605e26fcff75516ccf02f580809c570`. Its own third-party notices are in `engine/THIRD_PARTY_NOTICES.md`. |
| TigrimOS agent harness (`tigrim`) | `tigrimos/` | Apache-2.0 | `tigrimos/LICENSE`, from TigrimOSR, with the MCP-timeout, Python and vision patches that Aedifex also carries |
| craft-ui (PhotoCraft theme and widgets) | Cargo dependency | MIT or Apache-2.0 | https://github.com/storytold/craft-libs |
| Lucide icons | `app/assets/icons/` | ISC | `app/assets/icons/LICENSE-lucide.txt` |
| Inter, JetBrains Mono, Noto Sans Thai fonts | `app/assets/fonts/` | SIL Open Font License 1.1 | `app/assets/fonts/OFL-*.txt` |
| Rust crates (egui/eframe, krilla, ttf-parser, rust_xlsxwriter, calamine, serde and others) | `Cargo.lock` | MIT, Apache-2.0 or both | the crates' own licence files |

Apache-2.0, MIT, ISC and the OFL are compatible with distribution under AGPL-3.0.

## Changes to the Stabileo engine

As AGPL-3.0 section 5(a) asks, the modified files carry a prominent notice. Every change is marked `[Pondus]` in the source and listed in `engine/Cargo.toml`:

1. `engine/Cargo.toml`: the manifest was trimmed to a native library (no wasm `cdylib`, benches or dev-dependencies). October 2026.
2. `engine/src/solver/winkler.rs`: the sign of the w–θy coupling of the vertical Winkler foundation matrix was corrected. Without it, a uniformly loaded beam on a uniform foundation settled unevenly. 19 October 2026.
3. `engine/src/solver/plastic_3d.rs`: an unshifted sparse Cholesky test now runs before the dense rank test that checks for a mechanism at every hinge event. It gives the same answer and is faster. 20 October 2026.
4. `engine/src/solver/moving_loads.rs`: the 3D moving-load envelope also keeps the extremes at each end of an element (`start_max`, `start_min`, `end_max`, `end_min`). The per-element extremes take both ends together, so a point read from them carried the value one element away. 20 October 2026.
5. `engine/src/linalg/gpu.rs` (new), `engine/src/linalg/mod.rs`, `engine/src/solver/linear.rs`, `engine/src/solver/constraints.rs` and `engine/src/linalg/lanczos.rs`: the sparse factorisation on an NVIDIA GPU through cuDSS, loaded at run time, and a parallel reorthogonalisation and Ritz-vector assembly in the Lanczos solver. The results are unchanged to 6e-11. 20 October 2026.
6. `engine/src/solver/modal.rs`: `solve_modal_3d_with_masses`, the 3D modal analysis with lumped nodal masses (translations and rotational inertia) added to the diagonal of the mass matrix, with the mass ratios taken against each direction's own total. `solve_modal_3d` calls it with no nodal masses. 23 October 2026.
7. `engine/src/linalg/lanczos.rs`: when the sparse shift-invert iteration does not converge, a system of up to 3000 DOFs falls back to the dense solver for a semidefinite mass matrix instead of the dense Lanczos, which needs M positive definite and failed with lumped masses only. 23 October 2026.

## Disclaimer

Structural results are produced by software and must be checked by a qualified engineer before they are used for design or construction.
