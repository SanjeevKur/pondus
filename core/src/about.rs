// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The licence notice and the source link, shown by the app (Help › About), the command line
//! (`pondus --about`), the MCP server (serverInfo and its instructions) and the calculation
//! report, as AGPL-3.0 section 13 asks of a program that users reach over a network.

/// Where the complete corresponding source of this version is published.
pub const SOURCE_URL: &str = "https://github.com/Sompote/pondus";
/// The vendored engine's upstream.
pub const ENGINE_URL: &str = "https://github.com/lambdaclass/stabileo";
pub const ENGINE_COMMIT: &str = "7985e8c59605e26fcff75516ccf02f580809c570";

/// One line for footers and server descriptions.
pub fn short() -> String {
    format!("Pondus {} · AGPL-3.0-only · source: {SOURCE_URL}", env!("CARGO_PKG_VERSION"))
}

/// The full notice.
pub fn notice() -> String {
    format!(
        "Pondus {ver}\n\
         Copyright (C) 2026 Sompote Youwai\n\
         \n\
         This program is free software: you can redistribute it and/or modify it under the terms of the\n\
         GNU Affero General Public License as published by the Free Software Foundation, version 3\n\
         (AGPL-3.0-only).\n\
         \n\
         This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;\n\
         without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.\n\
         See the GNU Affero General Public License for more details: https://www.gnu.org/licenses/agpl-3.0.html\n\
         \n\
         Source code: {SOURCE_URL}\n\
         \n\
         Pondus includes the Stabileo analysis engine (dedaliano-engine), Copyright (C) the Stabileo\n\
         authors (Bautista Chesta, Diego Kingston, Federico Carrone and contributors at LambdaClass),\n\
         AGPL-3.0-only, {ENGINE_URL} at commit {short_commit}, modified in solver/winkler.rs,\n\
         solver/plastic_3d.rs and solver/moving_loads.rs (marked [Pondus]). It also includes TigrimOS (Apache-2.0), craft-ui\n\
         (MIT or Apache-2.0), Lucide icons (ISC) and the Inter, JetBrains Mono and Noto Sans Thai fonts\n\
         (SIL OFL 1.1). See NOTICE.md in the source.\n\
         \n\
         Structural results are produced by software and must be checked by a qualified engineer.",
        ver = env!("CARGO_PKG_VERSION"),
        short_commit = &ENGINE_COMMIT[..7],
    )
}
