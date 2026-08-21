//! The eight procedural backdrops the new tab page can sit on, and the mark
//! drawn in the middle of an empty one.
//!
//! Pure painting: a painter, a rect, a palette and a clock. Nothing here knows
//! what a tab is.

use egui::epaint::Mesh;
use egui::{Color32, Rect, Shape, Stroke, Ui, pos2};

use crate::settings::NewTabBackground;
use crate::theme::{self, Palette};

use super::*;

pub(crate) fn soft_blob(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    const SEGMENTS: u32 = 48;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    for segment in 0..=SEGMENTS {
        let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(
            pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            ),
            Color32::TRANSPARENT,
        );
    }
    for segment in 0..SEGMENTS {
        mesh.add_triangle(0, 1 + segment, 2 + segment);
    }
    painter.add(Shape::mesh(mesh));
}

/// The Zervo "Z" mark, drawn as a stroked path (same geometry as the app
/// icon: straight bars, curved diagonal).
pub fn draw_zervo_mark(painter: &egui::Painter, center: egui::Pos2, height: f32, color: Color32) {
    let scale = height / 364.0;
    let map = |x: f32, y: f32| {
        pos2(
            center.x + (x - 512.0) * scale,
            center.y + (y - 512.0) * scale,
        )
    };
    let mut points = vec![map(318.0, 330.0), map(706.0, 330.0)];
    // Cubic bezier for the diagonal: (706,330) -> (640,432),(462,570) -> (318,694).
    for step in 1..=16 {
        let t = step as f32 / 16.0;
        let inv = 1.0 - t;
        let x = inv * inv * inv * 706.0
            + 3.0 * inv * inv * t * 640.0
            + 3.0 * inv * t * t * 462.0
            + t * t * t * 318.0;
        let y = inv * inv * inv * 330.0
            + 3.0 * inv * inv * t * 432.0
            + 3.0 * inv * t * t * 570.0
            + t * t * t * 694.0;
        points.push(map(x, y));
    }
    points.push(map(706.0, 694.0));
    painter.add(Shape::line(points, Stroke::new(height * 0.27, color)));
}

/// Deterministic pseudo-random in 0..1 from an integer seed — used to place
/// particles without pulling in an RNG (and so they stay put across frames).
pub(crate) fn hashed_unit(seed: u32) -> f32 {
    let x = (seed as f32 * 12.9898).sin() * 43758.547;
    x - x.floor()
}

/// Paint the selected new tab backdrop. Returns true if it animates.
pub fn paint_newtab_background(
    root: &Ui,
    painter: &egui::Painter,
    content_rect: Rect,
    palette: &Palette,
    background: NewTabBackground,
    base: Color32,
) -> bool {
    let time = root.input(|input| input.time) as f32;
    let span = content_rect.width().min(content_rect.height());
    let alpha = if palette.dark { 0.16 } else { 0.12 };

    match background {
        NewTabBackground::Plain => false,

        // Reached only while a photograph is still being fetched, or when the
        // fetch failed: a fade is a kinder thing to wait on than a flat void.
        NewTabBackground::Photo | NewTabBackground::Gradient => {
            vertical_gradient(
                painter,
                content_rect,
                theme::mix(base, palette.accent, if palette.dark { 0.16 } else { 0.12 }),
                base,
            );
            false
        },

        NewTabBackground::Aurora => {
            let blobs = [
                (0.30, 0.35, 0.9, 0.055, palette.accent, 0.62),
                (0.72, 0.30, 1.4, 0.042, theme::workspace_color(1), 0.5),
                (0.55, 0.75, 2.2, 0.035, theme::workspace_color(4), 0.55),
            ];
            for (fx, fy, phase, speed, color, size) in blobs {
                let drift_x = (time * speed + phase).sin() * span * 0.10;
                let drift_y = (time * speed * 0.8 + phase * 1.7).cos() * span * 0.08;
                soft_blob(
                    painter,
                    pos2(
                        content_rect.min.x + content_rect.width() * fx + drift_x,
                        content_rect.min.y + content_rect.height() * fy + drift_y,
                    ),
                    span * size,
                    color.gamma_multiply(alpha),
                );
            }
            true
        },

        NewTabBackground::Mesh => {
            // Same soft-blob technique, but fixed in place: a static wash.
            let blobs = [
                (0.18, 0.22, palette.accent, 0.55),
                (0.82, 0.18, theme::workspace_color(1), 0.45),
                (0.30, 0.85, theme::workspace_color(2), 0.5),
                (0.88, 0.78, theme::workspace_color(4), 0.45),
            ];
            for (fx, fy, color, size) in blobs {
                soft_blob(
                    painter,
                    pos2(
                        content_rect.min.x + content_rect.width() * fx,
                        content_rect.min.y + content_rect.height() * fy,
                    ),
                    span * size,
                    color.gamma_multiply(alpha),
                );
            }
            false
        },

        NewTabBackground::Waves => {
            // Stacked sine bands, each drifting at its own speed. Every band
            // is one mesh so its top edge is a smooth curve and its body
            // fades downward.
            const SAMPLES: usize = 64;
            for band in 0..3_u32 {
                let color = match band {
                    0 => palette.accent,
                    1 => theme::workspace_color(1),
                    _ => theme::workspace_color(2),
                };
                let color = color.gamma_multiply(alpha * 0.9);
                let clear = Color32::TRANSPARENT;
                let base_y =
                    content_rect.min.y + content_rect.height() * (0.55 + band as f32 * 0.14);
                let amplitude = span * (0.05 + band as f32 * 0.015);
                let speed = 0.25 + band as f32 * 0.12;
                let wavelength = 1.6 + band as f32 * 0.7;

                let mut mesh = Mesh::default();
                for sample in 0..=SAMPLES {
                    let t = sample as f32 / SAMPLES as f32;
                    let x = content_rect.min.x + content_rect.width() * t;
                    let y = base_y
                        + (t * wavelength * std::f32::consts::TAU + time * speed).sin() * amplitude;
                    mesh.colored_vertex(pos2(x, y), color);
                    mesh.colored_vertex(pos2(x, content_rect.max.y), clear);
                }
                for sample in 0..SAMPLES as u32 {
                    let index = sample * 2;
                    mesh.add_triangle(index, index + 1, index + 3);
                    mesh.add_triangle(index, index + 3, index + 2);
                }
                painter.add(Shape::mesh(mesh));
            }
            true
        },

        NewTabBackground::Particles => {
            for index in 0..46_u32 {
                let fx = hashed_unit(index * 3 + 1);
                let fy = hashed_unit(index * 7 + 5);
                let speed = 0.02 + hashed_unit(index * 11 + 3) * 0.05;
                let size = 1.2 + hashed_unit(index * 13 + 9) * 2.4;
                // Drift upward, wrapping around the top edge.
                let y_wrapped = (fy - time * speed).rem_euclid(1.0);
                let sway = (time * speed * 6.0 + fx * 10.0).sin() * span * 0.012;
                let color = if index % 5 == 0 {
                    theme::workspace_color(1)
                } else {
                    palette.accent
                };
                painter.circle_filled(
                    pos2(
                        content_rect.min.x + content_rect.width() * fx + sway,
                        content_rect.min.y + content_rect.height() * y_wrapped,
                    ),
                    size,
                    color.gamma_multiply(0.10 + hashed_unit(index * 17 + 2) * 0.22),
                );
            }
            true
        },

        NewTabBackground::Grid => {
            let step = 42.0;
            let line = palette
                .accent
                .gamma_multiply(if palette.dark { 0.10 } else { 0.13 });
            let stroke = Stroke::new(1.0_f32, line);
            let mut x = content_rect.min.x + step;
            while x < content_rect.max.x {
                painter.line_segment(
                    [pos2(x, content_rect.min.y), pos2(x, content_rect.max.y)],
                    stroke,
                );
                x += step;
            }
            let mut y = content_rect.min.y + step;
            while y < content_rect.max.y {
                painter.line_segment(
                    [pos2(content_rect.min.x, y), pos2(content_rect.max.x, y)],
                    stroke,
                );
                y += step;
            }
            false
        },
    }
}
