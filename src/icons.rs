//! Small vector icons drawn with egui's painter, so they don't depend on which glyphs
//! the default font contains. Shapes are laid out on Material's 24x24 icon grid and
//! scaled to the requested size.

use std::f32::consts::PI;

use eframe::egui::{vec2, Color32, Mesh, Painter, Pos2, Rect, Rounding, Shape, Stroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Home,
    Search,
    Library,
    Settings,
    Play,
    Pause,
    Next,
    Previous,
    Heart,
    More,
    Close,
    Contrast,
    Note,
}

impl Icon {
    /// Draws the icon centred on `center`. `size` is the icon's width and height in dp
    /// (Material uses 24). `filled` selects the filled variant where one exists
    /// (used for selected navigation items and for liked songs).
    pub fn paint(self, painter: &Painter, center: Pos2, size: f32, color: Color32, filled: bool) {
        let unit = size / 24.0;
        let at = |x: f32, y: f32| center + vec2(x - 12.0, y - 12.0) * unit;
        let stroke_width = (if filled { 2.6 } else { 2.0 }) * unit;
        let stroke = Stroke::new(stroke_width, color);

        match self {
            Self::Home => {
                let outline = vec![at(12.0, 3.0), at(21.0, 11.0), at(19.0, 21.0), at(5.0, 21.0), at(3.0, 11.0)];
                if filled {
                    painter.add(Shape::convex_polygon(outline.clone(), color, Stroke::NONE));
                }
                painter.add(Shape::closed_line(outline, stroke));
            }
            Self::Search => {
                painter.circle_stroke(at(10.0, 10.0), 6.0 * unit, stroke);
                painter.line_segment([at(14.5, 14.5), at(20.5, 20.5)], stroke);
            }
            Self::Library | Self::Note => {
                painter.circle_filled(at(8.0, 18.0), 3.6 * unit, color);
                painter.line_segment([at(11.4, 18.0), at(11.4, 4.5)], Stroke::new(2.4 * unit, color));
                painter.line_segment([at(11.4, 4.5), at(19.0, 8.0)], Stroke::new(2.8 * unit, color));
            }
            Self::Settings => {
                painter.circle_stroke(at(12.0, 12.0), 6.5 * unit, stroke);
                painter.circle_stroke(at(12.0, 12.0), 2.4 * unit, stroke);
                for tooth in 0..8 {
                    let angle = tooth as f32 * PI / 4.0;
                    let (sin, cos) = angle.sin_cos();
                    painter.line_segment(
                        [at(12.0 + 6.5 * cos, 12.0 + 6.5 * sin), at(12.0 + 9.5 * cos, 12.0 + 9.5 * sin)],
                        Stroke::new(2.8 * unit, color),
                    );
                }
            }
            Self::Play => {
                painter.add(Shape::convex_polygon(vec![at(7.0, 4.5), at(7.0, 19.5), at(20.0, 12.0)], color, Stroke::NONE));
            }
            Self::Pause => {
                let rounding = Rounding::same(1.0 * unit);
                painter.rect_filled(Rect::from_two_pos(at(6.0, 5.0), at(10.0, 19.0)), rounding, color);
                painter.rect_filled(Rect::from_two_pos(at(14.0, 5.0), at(18.0, 19.0)), rounding, color);
            }
            Self::Next => {
                painter.add(Shape::convex_polygon(vec![at(5.5, 5.5), at(5.5, 18.5), at(16.0, 12.0)], color, Stroke::NONE));
                painter.rect_filled(Rect::from_two_pos(at(17.0, 5.5), at(19.5, 18.5)), Rounding::same(1.0 * unit), color);
            }
            Self::Previous => {
                painter.add(Shape::convex_polygon(vec![at(18.5, 5.5), at(18.5, 18.5), at(8.0, 12.0)], color, Stroke::NONE));
                painter.rect_filled(Rect::from_two_pos(at(4.5, 5.5), at(7.0, 18.5)), Rounding::same(1.0 * unit), color);
            }
            Self::Heart => {
                // The classic heart curve, mapped onto the 24x24 grid.
                let outline: Vec<Pos2> = (0..48)
                    .map(|step| {
                        let t = step as f32 / 48.0 * 2.0 * PI;
                        let x = 16.0 * t.sin().powi(3);
                        let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                        at(12.0 + x * 0.6, 11.0 - y * 0.6)
                    })
                    .collect();
                if filled {
                    // Fan of triangles from the middle; a mesh has no anti-aliasing seams.
                    let mut mesh = Mesh::default();
                    mesh.colored_vertex(at(12.0, 12.0), color);
                    for point in &outline {
                        mesh.colored_vertex(*point, color);
                    }
                    let count = outline.len() as u32;
                    for index in 0..count {
                        mesh.add_triangle(0, 1 + index, 1 + (index + 1) % count);
                    }
                    painter.add(Shape::mesh(mesh));
                }
                painter.add(Shape::closed_line(outline, Stroke::new(1.8 * unit, color)));
            }
            Self::More => {
                for y in [5.0, 12.0, 19.0] {
                    painter.circle_filled(at(12.0, y), 2.0 * unit, color);
                }
            }
            Self::Close => {
                painter.line_segment([at(6.0, 6.0), at(18.0, 18.0)], stroke);
                painter.line_segment([at(18.0, 6.0), at(6.0, 18.0)], stroke);
            }
            Self::Contrast => {
                painter.circle_stroke(at(12.0, 12.0), 8.0 * unit, stroke);
                // Left half filled.
                let half: Vec<Pos2> = (0..=16)
                    .map(|step| {
                        let angle = PI / 2.0 + step as f32 / 16.0 * PI;
                        at(12.0 + 8.0 * angle.cos(), 12.0 + 8.0 * angle.sin())
                    })
                    .collect();
                painter.add(Shape::convex_polygon(half, color, Stroke::NONE));
            }
        }
    }
}

/// Paints a checkmark (used on selected chips) centred on `center`.
pub fn paint_check(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let unit = size / 18.0;
    let at = |x: f32, y: f32| center + vec2(x - 9.0, y - 9.0) * unit;
    painter.add(Shape::line(
        vec![at(3.5, 9.5), at(7.5, 13.5), at(14.5, 5.5)],
        Stroke::new(2.0 * unit, color),
    ));
}
