// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The tool and panel icons: Lucide SVGs prepared once by craft-ui's `SvgIconSet` and tinted
//! when drawn, as PhotoCraft draws its tool strip.

use std::sync::OnceLock;

use craft_ui::icons::{SvgIconSet, SvgStyle};
use eframe::egui;

static SET: OnceLock<SvgIconSet> = OnceLock::new();

fn set() -> &'static SvgIconSet {
    SET.get_or_init(|| SvgIconSet::new(crate::icon_data::ICONS, SvgStyle { current_color: "white", stroke_width: Some("1.75") }, "bytes://pondus/icons/", "square"))
}

/// Paints an icon into `rect`, tinted.
pub fn paint(ui: &egui::Ui, name: &str, rect: egui::Rect, tint: egui::Color32) {
    egui::Image::new(set().source(name)).tint(tint).paint_at(ui, rect);
}

/// An icon as a small widget beside text.
pub fn image(name: &str, size: f32, tint: egui::Color32) -> egui::Image<'static> {
    egui::Image::new(set().source(name)).fit_to_exact_size(egui::vec2(size, size)).tint(tint)
}
