// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Embeds the Pondus icon and version information in the Windows executable.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Pondus");
        res.set("FileDescription", "Pondus: agentic structural analysis and design");
        res.set("LegalCopyright", "Copyright (C) 2026 Sompote Youwai. AGPL-3.0-only.");
        if let Err(e) = res.compile() {
            println!("cargo:warning=no Windows icon embedded: {e}");
        }
    }
}
