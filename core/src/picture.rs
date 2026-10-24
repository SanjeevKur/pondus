// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The agent's pictures: the scene of `scene.rs` rasterised on the CPU (anti-aliased lines,
//! filled polygons drawn far to near, values in a small bitmap font) into a PNG, on the app's
//! dark canvas, with a colour-scale legend for the plate diagrams.

use crate::scene::{bounds, Camera, Prim, Rgba};

pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

impl Image {
    fn new(w: usize, h: usize, bg: Rgba) -> Image {
        let mut rgba = vec![0; w * h * 4];
        for px in rgba.chunks_mut(4) {
            px.copy_from_slice(&bg);
        }
        Image { w, h, rgba }
    }

    fn blend(&mut self, x: i64, y: i64, c: Rgba, cover: f32) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return;
        }
        let a = (c[3] as f32 / 255.0) * cover.clamp(0.0, 1.0);
        let i = (y as usize * self.w + x as usize) * 4;
        for k in 0..3 {
            let d = self.rgba[i + k] as f32;
            self.rgba[i + k] = (d + (c[k] as f32 - d) * a).round() as u8;
        }
    }

    /// A line of width `w` px: coverage from the distance to the segment.
    fn line(&mut self, a: (f64, f64), b: (f64, f64), c: Rgba, w: f32) {
        let r = (w as f64 / 2.0).max(0.5);
        let (x0, x1) = ((a.0.min(b.0) - r - 1.0).floor() as i64, (a.0.max(b.0) + r + 1.0).ceil() as i64);
        let (y0, y1) = ((a.1.min(b.1) - r - 1.0).floor() as i64, (a.1.max(b.1) + r + 1.0).ceil() as i64);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = (dx * dx + dy * dy).max(1e-12);
        for y in y0.max(0)..=y1.min(self.h as i64 - 1) {
            for x in x0.max(0)..=x1.min(self.w as i64 - 1) {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let t = (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0);
                let (qx, qy) = (a.0 + dx * t - px, a.1 + dy * t - py);
                let d = (qx * qx + qy * qy).sqrt();
                let cover = (r + 0.5 - d) as f32;
                if cover > 0.0 {
                    self.blend(x, y, c, cover);
                }
            }
        }
    }

    /// A filled polygon (even-odd scanline, convex or not).
    fn poly(&mut self, pts: &[(f64, f64)], c: Rgba) {
        if pts.len() < 3 {
            return;
        }
        let y0 = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min).floor().max(0.0) as i64;
        let y1 = pts.iter().map(|p| p.1).fold(f64::MIN, f64::max).ceil().min(self.h as f64 - 1.0) as i64;
        for y in y0..=y1 {
            let sy = y as f64 + 0.5;
            let mut xs = vec![];
            for k in 0..pts.len() {
                let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                if (a.1 <= sy && b.1 > sy) || (b.1 <= sy && a.1 > sy) {
                    xs.push(a.0 + (sy - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks(2) {
                if pair.len() == 2 {
                    for x in (pair[0].round() as i64)..(pair[1].round() as i64) {
                        self.blend(x, y, c, 1.0);
                    }
                }
            }
        }
    }
}

impl Image {
    /// A triangle with a colour at each corner, interpolated across it (Gouraud shading).
    fn tri_shaded(&mut self, p: [(f64, f64); 3], c: [Rgba; 3]) {
        let det = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if det.abs() < 1e-9 {
            return;
        }
        let x0 = p.iter().map(|q| q.0).fold(f64::MAX, f64::min).floor().max(0.0) as i64;
        let x1 = p.iter().map(|q| q.0).fold(f64::MIN, f64::max).ceil().min(self.w as f64 - 1.0) as i64;
        let y0 = p.iter().map(|q| q.1).fold(f64::MAX, f64::min).floor().max(0.0) as i64;
        let y1 = p.iter().map(|q| q.1).fold(f64::MIN, f64::max).ceil().min(self.h as f64 - 1.0) as i64;
        // a little tolerance on the edges so neighbouring triangles leave no seam
        let eps = 0.75 / det.abs().sqrt().max(1.0);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let l1 = ((p[1].0 - p[0].0) * (py - p[0].1) - (px - p[0].0) * (p[1].1 - p[0].1)) / det;
                let l2 = ((px - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (py - p[0].1)) / det;
                let (w2, w1) = (l1, l2);
                let w0 = 1.0 - w1 - w2;
                if w0 < -eps || w1 < -eps || w2 < -eps {
                    continue;
                }
                let (w0, w1, w2) = (w0.max(0.0), w1.max(0.0), w2.max(0.0));
                let s = w0 + w1 + w2;
                // opaque, so the pixels the two triangles of an element share show no seam
                let mut col = [0, 1, 2, 3].map(|k| ((c[0][k] as f64 * w0 + c[1][k] as f64 * w1 + c[2][k] as f64 * w2) / s).round() as u8);
                col[3] = 255;
                self.blend(x, y, col, 1.0);
            }
        }
    }
}

pub const BACKGROUND: Rgba = [30, 30, 32, 255];

pub fn render(prims: &[Prim], cam: &Camera, w: usize, h: usize) -> Image {
    let mut img = Image::new(w, h, BACKGROUND);
    let Some((x0, y0, x1, y1)) = bounds(prims, cam) else { return img };
    let margin = 30.0;
    let k = ((w as f64 - 2.0 * margin) / (x1 - x0).max(1e-9)).min((h as f64 - 2.0 * margin) / (y1 - y0).max(1e-9));
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let to = |p: [f64; 3]| -> ((f64, f64), f64) {
        let (x, y, d) = cam.project(p);
        (((x - cx) * k + w as f64 / 2.0, (y - cy) * k + h as f64 / 2.0), d)
    };
    // far to near by the mean depth, polygons first at equal depth
    let mut order: Vec<(f64, usize)> = prims
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let d = match p {
                Prim::Line { a, b, .. } => (to(*a).1 + to(*b).1) / 2.0,
                Prim::Poly { pts, .. } => pts.iter().map(|q| to(*q).1).sum::<f64>() / pts.len().max(1) as f64 + 1e-6,
                Prim::Shaded { pts, .. } => pts.iter().map(|q| to(*q).1).sum::<f64>() / 4.0 + 1e-6,
                Prim::Dot { p, .. } => to(*p).1,
                // values are written last, over the dots and lines at their point
                Prim::Text { p, .. } => to(*p).1 - 1e-3,
            };
            (-d, i)
        })
        .collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n_text = prims.iter().filter(|p| matches!(p, Prim::Text { .. })).count();
    for (_, i) in order {
        match &prims[i] {
            Prim::Line { a, b, color, width, .. } => img.line(to(*a).0, to(*b).0, *color, *width * 1.3),
            Prim::Poly { pts, color } => img.poly(&pts.iter().map(|q| to(*q).0).collect::<Vec<_>>(), *color),
            Prim::Shaded { pts, colors } => {
                let q = pts.map(|p| to(p).0);
                img.tri_shaded([q[0], q[1], q[2]], [colors[0], colors[1], colors[2]]);
                img.tri_shaded([q[0], q[2], q[3]], [colors[0], colors[2], colors[3]]);
            }
            Prim::Dot { p, color, r } => {
                let c = to(*p).0;
                img.line(c, c, *color, *r * 2.0);
            }
            Prim::Text { p, text, color } => {
                // many values (a fine plate mesh) get the small size so they do not overlap
                let scale = if n_text > 80 { 1 } else { 2 };
                let c = to(*p).0;
                let wpx = text_width(text, scale);
                img.text(c.0 - wpx as f64 / 2.0, c.1 - 3.5 * scale as f64, text, *color, scale, Some([236, 236, 236, 170]));
            }
        }
    }
    img
}

/// A 5 x 7 bitmap font (digits, capitals and the signs of a number): each glyph is seven rows
/// of five bits, top row first. Lowercase letters are drawn as capitals.
fn glyph(ch: char) -> Option<[u8; 7]> {
    Some(match ch.to_ascii_uppercase() {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x11, 0x0A, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '-' | '−' | '–' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        '±' => [0x04, 0x04, 0x1F, 0x04, 0x04, 0x00, 0x1F],
        '.' | '·' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x0C, 0x04, 0x08],
        ':' => [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '=' => [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00],
        '%' => [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
        '²' => [0x0C, 0x02, 0x04, 0x08, 0x0E, 0x00, 0x00],
        ' ' => [0; 7],
        _ => return None,
    })
}

/// The width of `text` in pixels at `scale` pixels per font cell.
pub fn text_width(text: &str, scale: usize) -> usize {
    text.chars().count() * 6 * scale
}

impl Image {
    /// Writes `text` with its top-left corner at (x, y), `scale` pixels per font cell, on an
    /// optional background box.
    pub fn text(&mut self, x: f64, y: f64, text: &str, color: Rgba, scale: usize, bg: Option<Rgba>) {
        let (x0, y0) = (x.round() as i64, y.round() as i64);
        if let Some(bg) = bg {
            let (w, h) = (text_width(text, scale) as i64 + 2, 7 * scale as i64 + 2);
            for yy in y0 - 1..y0 - 1 + h {
                for xx in x0 - 1..x0 - 1 + w {
                    self.blend(xx, yy, bg, 1.0);
                }
            }
        }
        for (k, ch) in text.chars().enumerate() {
            let Some(g) = glyph(ch) else { continue };
            for (r, row) in g.iter().enumerate() {
                for c in 0..5 {
                    if row & (0x10 >> c) != 0 {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                self.blend(x0 + (k * 6 + c) as i64 * scale as i64 + sx as i64, y0 + r as i64 * scale as i64 + sy as i64, color, 1.0);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Adds a band of `px` rows of background at the bottom (room for a legend).
    pub fn extend_bottom(&mut self, px: usize) {
        let mut rgba = Vec::with_capacity((self.h + px) * self.w * 4);
        rgba.extend_from_slice(&self.rgba);
        for _ in 0..px * self.w {
            rgba.extend_from_slice(&BACKGROUND);
        }
        self.rgba = rgba;
        self.h += px;
    }

    /// A colour-scale legend for a plate diagram in a band at the picture's bottom (added by
    /// `extend_bottom`): the diverging bar from -vmax (blue) through 0 to +vmax (red), the
    /// quantity, the result and the units.
    pub fn legend(&mut self, vmax: f64, label: &str, units: &str) {
        self.extend_bottom(64);
        let (x0, y0, w, h) = (14i64, self.h as i64 - 34, 300i64, 12i64);
        for i in 0..w {
            let t = i as f64 / (w - 1) as f64 * 2.0 - 1.0;
            let c = crate::scene::diverging(t);
            for yy in y0..y0 + h {
                self.blend(x0 + i, yy, [c[0], c[1], c[2], 255], 1.0);
            }
        }
        let ink: Rgba = [222, 222, 224, 255];
        let digits = if vmax >= 100.0 { 0 } else if vmax >= 10.0 { 1 } else { 2 };
        let lo = format!("-{vmax:.digits$}");
        let hi = format!("+{vmax:.digits$}");
        self.text(x0 as f64, (y0 + h + 4) as f64, &lo, ink, 2, None);
        self.text((x0 + w / 2) as f64 - 6.0, (y0 + h + 4) as f64, "0", ink, 2, None);
        self.text((x0 + w) as f64 - text_width(&hi, 2) as f64, (y0 + h + 4) as f64, &hi, ink, 2, None);
        self.text((x0 + w + 24) as f64, (y0 + h + 4) as f64 - 10.0, &format!("{label} ({units})"), ink, 2, None);
    }
}

pub fn write_png(img: &Image, path: &std::path::Path) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.w as u32, img.h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(&img.rgba).map_err(|e| e.to_string())
}
