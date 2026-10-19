// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! Look and feel: PhotoCraft's Pro theme (github.com/storytold/photocraft, MIT OR Apache-2.0)
//! as colour tokens, applied to egui's widget states with craft-ui's `WidgetPalette` (the
//! shared Craft component library), with Inter / JetBrains Mono / Noto Sans Thai (OFL).

use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Margin, Shadow, Stroke, TextStyle, Vec2};

/// Semantic colours; every widget reads these instead of hard-coding a colour.
#[derive(Clone, Copy)]
pub struct Tokens {
    pub dark: bool,
    /// Panels and bars.
    pub chrome: Color32,
    /// Menu bar / tab strips, a step darker than `chrome`.
    pub dock: Color32,
    /// Raised content: explorer bodies, popups.
    pub card: Color32,
    /// Explorer title bar and its outline: a steel-blue tint, so the titles stand apart
    /// from the grey panels by hue, not only by lightness.
    pub header: Color32,
    pub header_edge: Color32,
    /// Text fields and value boxes.
    pub field: Color32,
    pub field_border: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub separator: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_text: Color32,
    /// Drawing area around the model, its dot grid and the ink of rulers / labels on it.
    pub canvas: Color32,
    pub canvas_dot: Color32,
    pub ink: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub error: Color32,
    pub log_command: Color32,
    pub log_reply: Color32,
    pub tools: ToolHues,
}

/// Stroke colours of the tool-strip icons: each tool takes the colour of what it makes.
#[derive(Clone, Copy)]
pub struct ToolHues {
    pub wall: Color32,
    pub door: Color32,
    pub window: Color32,
    pub floor: Color32,
    pub column: Color32,
    pub grid: Color32,
    pub room: Color32,
    pub component: Color32,
    pub beam: Color32,
    pub foundation: Color32,
}

pub const RADIUS_SM: u8 = 3;
pub const RADIUS: u8 = 4;
/// Height of value fields, buttons and tab strips.
pub const ROW_H: f32 = 24.0;
pub const TAB_STRIP_H: f32 = 26.0;

impl Tokens {
    pub const DARK: Tokens = Tokens {
        dark: true,
        chrome: Color32::from_rgb(50, 50, 52),
        dock: Color32::from_rgb(34, 34, 36),
        card: Color32::from_rgb(42, 42, 44),
        header: Color32::from_rgb(38, 50, 68),
        header_edge: Color32::from_rgb(58, 76, 102),
        field: Color32::from_rgb(30, 30, 32),
        field_border: Color32::from_rgb(70, 70, 74),
        hover: Color32::from_rgb(66, 66, 70),
        pressed: Color32::from_rgb(78, 78, 84),
        separator: Color32::from_rgb(64, 64, 68),
        text: Color32::from_rgb(222, 222, 224),
        text_dim: Color32::from_rgb(178, 178, 182),
        text_faint: Color32::from_rgb(128, 128, 134),
        accent: Color32::from_rgb(55, 142, 240),
        accent_soft: Color32::from_rgb(36, 64, 98),
        accent_text: Color32::from_rgb(160, 205, 255),
        canvas: Color32::from_rgb(38, 38, 40),
        canvas_dot: Color32::from_rgb(58, 58, 62),
        ink: Color32::from_rgb(190, 190, 196),
        ok: Color32::from_rgb(110, 200, 120),
        warn: Color32::from_rgb(232, 170, 70),
        error: Color32::from_rgb(240, 100, 95),
        log_command: Color32::from_rgb(140, 190, 255),
        log_reply: Color32::from_rgb(120, 200, 190),
        tools: ToolHues {
            wall: Color32::from_rgb(214, 210, 200),
            door: Color32::from_rgb(226, 160, 94),
            window: Color32::from_rgb(120, 190, 240),
            floor: Color32::from_rgb(180, 180, 172),
            column: Color32::from_rgb(200, 200, 210),
            grid: Color32::from_rgb(130, 160, 230),
            room: Color32::from_rgb(140, 210, 150),
            component: Color32::from_rgb(230, 190, 120),
            beam: Color32::from_rgb(120, 200, 230),
            foundation: Color32::from_rgb(200, 160, 230),
        },
    };

    pub const LIGHT: Tokens = Tokens {
        dark: false,
        chrome: Color32::from_rgb(243, 243, 245),
        dock: Color32::from_rgb(230, 231, 235),
        card: Color32::from_rgb(251, 251, 252),
        header: Color32::from_rgb(210, 224, 245),
        header_edge: Color32::from_rgb(176, 197, 228),
        field: Color32::WHITE,
        field_border: Color32::from_rgb(204, 206, 212),
        hover: Color32::from_rgb(224, 226, 232),
        pressed: Color32::from_rgb(210, 213, 222),
        separator: Color32::from_rgb(218, 220, 226),
        text: Color32::from_rgb(30, 32, 36),
        text_dim: Color32::from_rgb(82, 86, 94),
        text_faint: Color32::from_rgb(132, 136, 146),
        accent: Color32::from_rgb(30, 118, 226),
        accent_soft: Color32::from_rgb(214, 230, 252),
        accent_text: Color32::from_rgb(18, 84, 170),
        canvas: Color32::from_rgb(226, 228, 233),
        canvas_dot: Color32::from_rgb(200, 203, 211),
        ink: Color32::from_rgb(70, 74, 82),
        ok: Color32::from_rgb(40, 150, 60),
        warn: Color32::from_rgb(196, 120, 0),
        error: Color32::from_rgb(210, 40, 40),
        log_command: Color32::from_rgb(20, 70, 170),
        log_reply: Color32::from_rgb(20, 120, 120),
        tools: ToolHues {
            wall: Color32::from_rgb(90, 86, 78),
            door: Color32::from_rgb(170, 100, 30),
            window: Color32::from_rgb(30, 120, 190),
            floor: Color32::from_rgb(110, 110, 104),
            column: Color32::from_rgb(90, 90, 100),
            grid: Color32::from_rgb(60, 90, 170),
            room: Color32::from_rgb(40, 140, 70),
            component: Color32::from_rgb(160, 100, 20),
            beam: Color32::from_rgb(20, 110, 150),
            foundation: Color32::from_rgb(120, 60, 160),
        },
    };

    pub fn get(ctx: &egui::Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(id())).unwrap_or(Tokens::DARK)
    }
}

fn id() -> Id {
    Id::new("pondus-theme")
}

/// Stores the tokens and maps them onto egui's visuals and spacing.
pub fn apply(ctx: &egui::Context, t: Tokens) {
    ctx.data_mut(|d| d.insert_temp(id(), t));
    let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let r = CornerRadius::same(RADIUS);
    v.panel_fill = t.chrome;
    v.window_fill = t.card;
    v.window_stroke = Stroke::new(1.0_f32, t.separator);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(6);
    v.extreme_bg_color = t.field;
    v.faint_bg_color = t.card;
    v.code_bg_color = t.field;
    v.override_text_color = Some(t.text);
    v.hyperlink_color = t.accent_text;
    v.warn_fg_color = t.warn;
    v.error_fg_color = t.error;
    v.selection.bg_fill = t.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, t.accent_text);
    v.window_shadow = Shadow { offset: [0, 10], blur: 32, spread: 0, color: Color32::from_black_alpha(if t.dark { 110 } else { 40 }) };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(if t.dark { 100 } else { 36 }) };
    v.handle_shape = egui::style::HandleShape::Circle;
    v.slider_trailing_fill = true;
    let w = &mut v.widgets;
    // the five widget states from the tokens, through craft-ui's shared mechanism
    craft_ui::theme::WidgetPalette { noninteractive: t.chrome, inactive: t.field, hovered: t.hover, active: t.pressed, open: t.hover }.apply_to(w, r, Stroke::new(1.0_f32, t.text));
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, t.separator);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, t.text_dim);
    for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.bg_stroke = Stroke::new(1.0_f32, t.field_border);
        s.expansion = 0.0;
    }
    w.inactive.fg_stroke = Stroke::new(1.0_f32, t.text_dim);
    w.hovered.bg_stroke = Stroke::new(1.0_f32, t.text_faint);
    w.active.bg_stroke = Stroke::new(1.0_f32, t.accent);

    // eframe follows the system light/dark setting, so both theme slots get this style
    let mut s = (*ctx.global_style()).clone();
    s.visuals = v;
    s.text_styles = [
        (TextStyle::Small, FontId::proportional(10.5)),
        (TextStyle::Body, FontId::proportional(12.5)),
        (TextStyle::Button, FontId::proportional(12.5)),
        (TextStyle::Heading, FontId::new(15.0, medium())),
        (TextStyle::Monospace, FontId::monospace(12.0)),
    ]
    .into();
    let sp = &mut s.spacing;
    sp.item_spacing = Vec2::new(8.0, 6.0);
    sp.button_padding = Vec2::new(10.0, 4.0);
    sp.interact_size = Vec2::new(40.0, ROW_H);
    sp.menu_margin = Margin::same(6);
    sp.window_margin = Margin::same(10);
    sp.indent = 16.0;
    sp.combo_width = 120.0;
    sp.scroll = egui::style::ScrollStyle::thin();
    s.interaction.tooltip_delay = 0.35;
    ctx.set_style_of(egui::Theme::Dark, s.clone());
    ctx.set_style_of(egui::Theme::Light, s);
    ctx.set_theme(if t.dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
}

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

/// Medium-weight Inter for headers and tabs.
pub fn medium_font(size: f32) -> FontId {
    FontId::new(size, medium())
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut f = FontDefinitions::default();
    let add = |f: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        f.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut f, "Inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(&mut f, "InterMedium", include_bytes!("../assets/fonts/Inter-Medium.ttf"));
    add(&mut f, "JetBrainsMono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"));
    // Thai script (Inter and JetBrains Mono have no Thai glyphs), tried after the main font
    add(&mut f, "NotoSansThai", include_bytes!("../assets/fonts/NotoSansThai-Regular.ttf"));
    // egui's own fonts stay as fallbacks (symbols, emoji)
    let fallback = f.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    f.families.entry(FontFamily::Proportional).or_default().splice(0..0, ["Inter".to_string(), "NotoSansThai".to_string()]);
    f.families.entry(FontFamily::Monospace).or_default().splice(0..0, ["JetBrainsMono".to_string(), "NotoSansThai".to_string()]);
    let mut med = vec!["InterMedium".to_string(), "Inter".to_string(), "NotoSansThai".to_string()];
    med.extend(fallback);
    f.families.insert(medium(), med);
    ctx.set_fonts(f);
}
