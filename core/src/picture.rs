// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Sompote Youwai. Part of Pondus, https://github.com/Sompote/pondus

//! The agent's pictures: the scene of `scene.rs` rasterised on the CPU (anti-aliased lines,
//! filled polygons drawn far to near) into a PNG, on the app's dark canvas.

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
                Prim::Dot { p, .. } | Prim::Text { p, .. } => to(*p).1,
            };
            (-d, i)
        })
        .collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, i) in order {
        match &prims[i] {
            Prim::Line { a, b, color, width, .. } => img.line(to(*a).0, to(*b).0, *color, *width * 1.3),
            Prim::Poly { pts, color } => img.poly(&pts.iter().map(|q| to(*q).0).collect::<Vec<_>>(), *color),
            Prim::Dot { p, color, r } => {
                let c = to(*p).0;
                img.line(c, c, *color, *r * 2.0);
            }
            Prim::Text { .. } => {}
        }
    }
    img
}

pub fn write_png(img: &Image, path: &std::path::Path) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.w as u32, img.h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(&img.rgba).map_err(|e| e.to_string())
}
