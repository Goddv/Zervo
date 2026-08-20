//! Offscreen probe for the card corners.
//!
//! Tessellates the real `glass::shapes` output with epaint and rasterises it
//! the way the GPU does — no multisampling, source-over on premultiplied
//! colours — so a hard mesh silhouette shows up as a staircase and a feathered
//! one does not. Prints a luminance map of one corner and dumps the raw
//! framebuffer for a PNG.
//!
//!     cargo run --example corner_probe -- [clip]
//!
//! `clip` scissors to the card's own bounding box, which is what `hover_card`
//! used to do.

#[path = "../src/glass.rs"]
mod glass;
#[path = "../src/theme.rs"]
mod theme;

use egui::epaint::{ClippedShape, Mesh, Primitive, TessellationOptions, Tessellator};
use egui::{Color32, Rect, pos2, vec2};

use glass::Glass;

const PPP: f32 = 2.0;

struct Canvas {
    width: usize,
    height: usize,
    /// Straight (non-premultiplied is never needed here) RGBA, 0..1, premultiplied.
    pixels: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(width: usize, height: usize, fill: Color32) -> Self {
        let c = [
            fill.r() as f32 / 255.0,
            fill.g() as f32 / 255.0,
            fill.b() as f32 / 255.0,
            fill.a() as f32 / 255.0,
        ];
        Self {
            width,
            height,
            pixels: vec![c; width * height],
        }
    }

    fn blend(&mut self, x: usize, y: usize, src: [f32; 4]) {
        let dst = &mut self.pixels[y * self.width + x];
        let inv = 1.0 - src[3];
        for i in 0..4 {
            dst[i] = src[i] + dst[i] * inv;
        }
    }

    /// Rasterise one mesh, scissored to `clip` (in physical pixels).
    fn draw(&mut self, mesh: &Mesh, clip: Rect) {
        for triangle in mesh.indices.chunks_exact(3) {
            let v: Vec<_> = triangle
                .iter()
                .map(|&i| mesh.vertices[i as usize])
                .collect();
            let p: Vec<[f32; 2]> = v
                .iter()
                .map(|vert| [vert.pos.x * PPP, vert.pos.y * PPP])
                .collect();
            let min_x = p
                .iter()
                .map(|q| q[0])
                .fold(f32::MAX, f32::min)
                .max(clip.min.x)
                .floor()
                .max(0.0) as usize;
            let max_x = (p
                .iter()
                .map(|q| q[0])
                .fold(f32::MIN, f32::max)
                .min(clip.max.x)
                .ceil() as usize)
                .min(self.width);
            let min_y = p
                .iter()
                .map(|q| q[1])
                .fold(f32::MAX, f32::min)
                .max(clip.min.y)
                .floor()
                .max(0.0) as usize;
            let max_y = (p
                .iter()
                .map(|q| q[1])
                .fold(f32::MIN, f32::max)
                .min(clip.max.y)
                .ceil() as usize)
                .min(self.height);

            let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
            if area.abs() < 1e-9 {
                continue;
            }
            for y in min_y..max_y {
                for x in min_x..max_x {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 =
                        ((p[1][0] - px) * (p[2][1] - py) - (p[2][0] - px) * (p[1][1] - py)) / area;
                    let w1 =
                        ((p[2][0] - px) * (p[0][1] - py) - (p[0][0] - px) * (p[2][1] - py)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    // Color32 is already premultiplied, so the components
                    // interpolate directly — which is what the GPU does.
                    let mut src = [0.0_f32; 4];
                    for (weight, vert) in [(w0, v[0]), (w1, v[1]), (w2, v[2])] {
                        src[0] += weight * vert.color.r() as f32 / 255.0;
                        src[1] += weight * vert.color.g() as f32 / 255.0;
                        src[2] += weight * vert.color.b() as f32 / 255.0;
                        src[3] += weight * vert.color.a() as f32 / 255.0;
                    }
                    self.blend(x, y, src);
                }
            }
        }
    }

    fn luminance(&self, x: usize, y: usize) -> f32 {
        let p = self.pixels[y * self.width + x];
        0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
    }
}

fn main() {
    let clip_to_card = std::env::args().any(|a| a == "clip");
    let palette = theme::resolve(theme::ThemeMode::Light, false, theme::AccentColor::Lavender);

    // A card the size of the favourites one, with room around it for the shadow.
    let card = Rect::from_min_size(pos2(30.0, 30.0), vec2(200.0, 120.0));
    let canvas_points = vec2(card.max.x + 30.0, card.max.y + 30.0);
    let (w, h) = (
        (canvas_points.x * PPP) as usize,
        (canvas_points.y * PPP) as usize,
    );

    // The chrome behind the card, so the shadow has something to fall on.
    let mut canvas = Canvas::new(w, h, palette.bg);

    let shapes = glass::shapes(
        card,
        &palette,
        Glass::new(12).opaque(palette.bg).border(palette.border),
    );

    let clip = if clip_to_card { card } else { Rect::EVERYTHING };
    let mut tess = Tessellator::new(PPP, TessellationOptions::default(), [1, 1], vec![]);
    let clipped: Vec<ClippedShape> = shapes
        .into_iter()
        .map(|shape| ClippedShape {
            clip_rect: clip,
            shape,
        })
        .collect();
    for primitive in tess.tessellate_shapes(clipped) {
        let scissor = Rect::from_min_max(
            pos2(
                primitive.clip_rect.min.x * PPP,
                primitive.clip_rect.min.y * PPP,
            ),
            pos2(
                primitive.clip_rect.max.x * PPP,
                primitive.clip_rect.max.y * PPP,
            ),
        );
        if let Primitive::Mesh(mesh) = primitive.primitive {
            canvas.draw(&mesh, scissor);
        }
    }

    // ── The bottom-right corner, in physical pixels.
    let corner_x = (card.max.x * PPP) as usize;
    let corner_y = (card.max.y * PPP) as usize;
    let (x0, y0) = (corner_x.saturating_sub(34), corner_y.saturating_sub(34));
    println!(
        "bottom-right corner, {}x{} physical pixels, ppp {PPP}{}",
        44,
        44,
        if clip_to_card {
            ", scissored to the card"
        } else {
            ""
        }
    );
    println!("darkest .. lightest:  # % * + - . (space)");
    for y in y0..(y0 + 44).min(h) {
        let mut row = String::new();
        for x in x0..(x0 + 44).min(w) {
            let l = canvas.luminance(x, y);
            row.push(match l {
                l if l < 0.55 => '#',
                l if l < 0.70 => '%',
                l if l < 0.80 => '*',
                l if l < 0.875 => '+',
                l if l < 0.925 => '-',
                l if l < 0.965 => '.',
                _ => ' ',
            });
        }
        println!("{row}");
    }

    // ── A radial slice straight out from the corner, to see the falloff.
    println!("\nluminance along the diagonal out of the corner:");
    let centre = ((card.max.x - 12.0) * PPP, (card.max.y - 12.0) * PPP);
    for step in 0..30 {
        let d = 12.0 * PPP + step as f32;
        let x = (centre.0 + d * std::f32::consts::FRAC_1_SQRT_2) as usize;
        let y = (centre.1 + d * std::f32::consts::FRAC_1_SQRT_2) as usize;
        if x < w && y < h {
            println!(
                "  +{:>4.1}px  {:.4}",
                d - 12.0 * PPP,
                canvas.luminance(x, y)
            );
        }
    }

    let mut raw = Vec::with_capacity(w * h * 4);
    for p in &canvas.pixels {
        for c in p {
            raw.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    let name = if clip_to_card {
        "corner-clipped.raw"
    } else {
        "corner.raw"
    };
    std::fs::write(name, &raw).unwrap();
    println!("\n{w}x{h} RGBA written to {name}");
}
