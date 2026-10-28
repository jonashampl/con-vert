//! The floating con/vert icon as liquid glass: a squircle that blurs and bends whatever is
//! behind it, split along the diagonal into a blue pane (a video screen with a play mark)
//! and a clear pane (sound bars). Pure pixels; the widget passes in the screen patch.
//!
//! Drawn from signed distance fields in a 100x100 icon space, so it stays crisp at every
//! size the widget uses (32 to 96 logical px at any display scale).

use image::{Rgba, RgbaImage};

const SQRT2: f32 = std::f32::consts::SQRT_2;
const BLUE: [f32; 3] = [10.0, 132.0, 255.0];
const WHITE: [f32; 3] = [255.0, 255.0, 255.0];

/// Distance to a rounded box centred at (cx, cy) with half-size (hw, hh); negative inside.
fn rounded_box(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (x - cx).abs() - (hw - r);
    let qy = (y - cy).abs() - (hh - r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

/// Distance to a triangle (any winding); negative inside.
fn triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    let sub = |u: (f32, f32), v: (f32, f32)| (u.0 - v.0, u.1 - v.1);
    let dot = |u: (f32, f32), v: (f32, f32)| u.0 * v.0 + u.1 * v.1;
    let cross = |u: (f32, f32), v: (f32, f32)| u.0 * v.1 - u.1 * v.0;
    let s = cross(sub(b, a), sub(a, c)).signum();
    let edge = |from: (f32, f32), to: (f32, f32)| {
        let (e, v) = (sub(to, from), sub(p, from));
        let t = (dot(v, e) / dot(e, e)).clamp(0.0, 1.0);
        let q = (v.0 - e.0 * t, v.1 - e.1 * t);
        (dot(q, q), s * cross(v, e))
    };
    let (d, inside) = [edge(a, b), edge(b, c), edge(c, a)].into_iter().fold((f32::MAX, f32::MAX), |m, e| (m.0.min(e.0), m.1.min(e.1)));
    -d.sqrt() * inside.signum()
}

/// The icon outline.
fn squircle(x: f32, y: f32) -> f32 {
    rounded_box(x, y, 50.0, 50.0, 50.0, 50.0, 22.5)
}

/// The white mark: screen + play left of the diagonal, sound bars right of it.
fn glyph(x: f32, y: f32) -> f32 {
    let screen = rounded_box(x, y, 36.5, 50.0, 20.0, 19.0, 10.0).abs() - 2.5;
    let play = triangle((x, y), (33.0, 43.0), (33.0, 56.4), (43.8, 49.7)) - 1.2;
    let left = screen.min(play).max((x + y - 95.0) / SQRT2);
    let bars = [(60.7, 16.0), (69.2, 23.0), (77.7, 13.0), (86.2, 7.0)]
        .iter()
        .map(|&(cx, hh)| rounded_box(x, y, cx, 50.0, 2.7, hh, 2.7))
        .fold(f32::MAX, f32::min);
    let right = bars.max((105.0 - (x + y)) / SQRT2);
    left.min(right)
}

fn over(c: &mut [f32; 3], col: [f32; 3], a: f32) {
    for i in 0..3 {
        c[i] += (col[i] - c[i]) * a.clamp(0.0, 1.0);
    }
}

fn bilinear(img: &RgbaImage, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (img.width() as f32 - 1.0, img.height() as f32 - 1.0);
    let (x, y) = (x.clamp(0.0, w), y.clamp(0.0, h));
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let px = |dx: f32, dy: f32| img.get_pixel((x0 + dx).min(w) as u32, (y0 + dy).min(h) as u32);
    let mut out = [0.0; 3];
    for (i, o) in out.iter_mut().enumerate() {
        let top = px(0.0, 0.0)[i] as f32 * (1.0 - fx) + px(1.0, 0.0)[i] as f32 * fx;
        let bottom = px(0.0, 1.0)[i] as f32 * (1.0 - fx) + px(1.0, 1.0)[i] as f32 * fx;
        *o = top * (1.0 - fy) + bottom * fy;
    }
    out
}

/// Renders the icon over `backdrop` (the screen patch it sits on), at the backdrop's size.
/// Straight (not premultiplied) RGBA; transparent outside the squircle.
pub fn glass(backdrop: &RgbaImage) -> RgbaImage {
    let size = backdrop.width().min(backdrop.height()).max(1);
    let blurred = image::imageops::blur(backdrop, (size as f32 * 0.035).max(1.0));
    let ppu = size as f32 / 100.0; // pixels per icon unit
    let cov = |d: f32| (0.5 - d * ppu).clamp(0.0, 1.0);
    let mut out = RgbaImage::new(size, size);
    for (px, py, o) in out.enumerate_pixels_mut() {
        let (x, y) = ((px as f32 + 0.5) / ppu, (py as f32 + 0.5) / ppu);
        let sq = squircle(x, y);
        let alpha = cov(sq);
        if alpha <= 0.0 {
            continue;
        }

        // Lens: near the rim, sample further in, so the edge bends what's behind it.
        let (mut sx, mut sy) = (px as f32 + 0.5, py as f32 + 0.5);
        let depth = -sq;
        if depth < 20.0 {
            let m = (1.0 - depth.max(0.0) / 20.0).powi(2);
            let (nx, ny) = (squircle(x + 0.5, y) - squircle(x - 0.5, y), squircle(x, y + 0.5) - squircle(x, y - 0.5));
            let len = nx.hypot(ny).max(1e-6);
            sx -= nx / len * m * size as f32 * 0.12;
            sy -= ny / len * m * size as f32 * 0.12;
        }
        let mut c = bilinear(&blurred, sx - 0.5, sy - 0.5);
        let luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        for v in c.iter_mut() {
            *v = ((luma + (*v - luma) * 1.6) * 1.08).clamp(0.0, 255.0);
        }

        // Two panes split along the diagonal, then a sheen across the top.
        let left_pane = cov((x + y - 100.0) / SQRT2);
        over(&mut c, BLUE, 0.34 * left_pane);
        over(&mut c, WHITE, 0.07 * (1.0 - left_pane));
        if y < 45.0 {
            over(&mut c, WHITE, 0.38 * (1.0 - y / 45.0));
        }
        // The seam: a shadow line with a bright edge beside it.
        over(&mut c, [0.0; 3], 0.16 * cov((x + y - 100.0).abs() / SQRT2 - 1.1));
        over(&mut c, WHITE, 0.75 * cov((x + y - 99.0).abs() / SQRT2 - 0.45));
        // The mark, with a soft blue-black shadow under it.
        let shadow = (0.5 - glyph(x, y - 1.2) / 2.8).clamp(0.0, 1.0);
        over(&mut c, [0.0, 42.0, 92.0], 0.45 * shadow);
        over(&mut c, WHITE, cov(glyph(x, y)));
        // Rim light: bright top-left and bottom-right, faint in between.
        let t = (x + y) / 200.0;
        let rim = if t < 0.35 { 0.95 - t / 0.35 * 0.77 } else if t < 0.7 { 0.18 - (t - 0.35) / 0.35 * 0.08 } else { 0.1 + (t - 0.7) / 0.3 * 0.5 };
        over(&mut c, WHITE, rim * cov((sq + 0.65).abs() - 0.65));

        *o = Rgba([c[0] as u8, c[1] as u8, c[2] as u8, (alpha * 255.0).round() as u8]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_icon_shape_and_mark() {
        let bg = RgbaImage::from_pixel(100, 100, Rgba([40, 120, 60, 255]));
        let g = glass(&bg);
        assert_eq!(g.dimensions(), (100, 100));
        // Rounded corners are clear, the middle is solid.
        assert_eq!(g.get_pixel(1, 1)[3], 0);
        assert_eq!(g.get_pixel(50, 50)[3], 255);
        // The tallest sound bar is white.
        let bar = g.get_pixel(69, 50);
        assert!(bar[0] > 230 && bar[1] > 230 && bar[2] > 230, "{bar:?}");
        // The upper-left pane is tinted blue over the green backdrop.
        let pane = g.get_pixel(22, 20);
        assert!(pane[2] > pane[0] + 40, "{pane:?}");
        // The play mark sits inside the screen and is white.
        let play = g.get_pixel(36, 50);
        assert!(play[0] > 230, "{play:?}");
    }

    #[test]
    fn glass_icon_is_symmetric_enough() {
        // The mark's ink is balanced left to right around the centre (within 8 units).
        let (mut sum, mut n) = (0.0, 0.0);
        for y in 0..100 {
            for x in 0..100 {
                if glyph(x as f32 + 0.5, y as f32 + 0.5) < 0.0 {
                    sum += x as f32 + 0.5;
                    n += 1.0;
                }
            }
        }
        assert!((sum / n - 50.0).abs() < 8.0, "ink centre {}", sum / n);
    }
}
