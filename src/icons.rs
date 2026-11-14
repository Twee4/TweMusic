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
    Power,
    History,
    Activity,
    User,
    Shuffle,
    Bolt,
    Moon,
    Smile,
    Dumbbell,
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
            Self::Power => {
                // An arc open at the top, with a vertical bar through the gap.
                let arc: Vec<Pos2> = (0..=22)
                    .map(|step| {
                        let angle = -PI / 2.0 + PI / 5.0 + step as f32 / 22.0 * (2.0 * PI - 2.0 * PI / 5.0);
                        at(12.0 + 8.0 * angle.cos(), 13.0 + 8.0 * angle.sin())
                    })
                    .collect();
                painter.add(Shape::line(arc, stroke));
                painter.line_segment([at(12.0, 3.0), at(12.0, 11.5)], stroke);
            }
            Self::History => {
                // A clock face with a gap at the upper left, and hands.
                let arc: Vec<Pos2> = (0..=26)
                    .map(|step| {
                        let angle = -PI * 0.5 + 0.55 + step as f32 / 26.0 * (2.0 * PI - 1.1);
                        at(12.0 + 8.5 * angle.cos(), 12.0 + 8.5 * angle.sin())
                    })
                    .collect();
                painter.add(Shape::line(arc, stroke));
                painter.add(Shape::line(vec![at(12.0, 7.0), at(12.0, 12.0), at(15.5, 14.0)], stroke));
            }
            Self::Activity => {
                painter.add(Shape::line(
                    vec![at(3.0, 12.0), at(7.0, 12.0), at(10.0, 4.5), at(14.0, 19.5), at(17.0, 12.0), at(21.0, 12.0)],
                    stroke,
                ));
            }
            Self::User => {
                painter.circle_stroke(at(12.0, 8.0), 4.0 * unit, stroke);
                let shoulders: Vec<Pos2> = (0..=20)
                    .map(|step| {
                        let angle = PI + step as f32 / 20.0 * PI;
                        at(12.0 + 8.0 * angle.cos(), 21.0 + 8.0 * angle.sin())
                    })
                    .collect();
                painter.add(Shape::line(shoulders, stroke));
            }
            Self::Shuffle => {
                painter.add(Shape::line(vec![at(3.0, 7.0), at(7.0, 7.0), at(16.0, 17.0), at(20.5, 17.0)], stroke));
                painter.add(Shape::line(vec![at(3.0, 17.0), at(7.0, 17.0), at(10.0, 13.6)], stroke));
                painter.add(Shape::line(vec![at(14.0, 9.4), at(16.0, 7.0), at(20.5, 7.0)], stroke));
                painter.add(Shape::line(vec![at(18.0, 4.5), at(20.5, 7.0), at(18.0, 9.5)], stroke));
                painter.add(Shape::line(vec![at(18.0, 14.5), at(20.5, 17.0), at(18.0, 19.5)], stroke));
            }
            Self::Bolt => {
                painter.add(Shape::closed_line(
                    vec![at(13.0, 2.5), at(4.5, 14.0), at(11.0, 14.0), at(10.0, 21.5), at(19.5, 10.0), at(13.0, 10.0)],
                    stroke,
                ));
            }
            Self::Moon => {
                // A crescent: the part of a circle that lies outside a second, offset circle.
                let (a_center, a_radius) = ((12.0_f32, 12.0_f32), 8.5_f32);
                let (b_center, b_radius) = ((17.0_f32, 8.0_f32), 7.0_f32);
                let start = (b_center.1 - a_center.1).atan2(b_center.0 - a_center.0);
                let outer: Vec<(f32, f32)> = (0..=60)
                    .map(|step| start + step as f32 / 60.0 * 2.0 * PI)
                    .map(|angle| (a_center.0 + a_radius * angle.cos(), a_center.1 + a_radius * angle.sin()))
                    .filter(|(x, y)| (x - b_center.0).hypot(y - b_center.1) > b_radius)
                    .collect();
                let back = (a_center.1 - b_center.1).atan2(a_center.0 - b_center.0);
                let mut inner: Vec<(f32, f32)> = (0..=60)
                    .map(|step| back - PI + step as f32 / 60.0 * 2.0 * PI)
                    .map(|angle| (b_center.0 + b_radius * angle.cos(), b_center.1 + b_radius * angle.sin()))
                    .filter(|(x, y)| (x - a_center.0).hypot(y - a_center.1) < a_radius)
                    .collect();
                if let (Some(last), Some(first), Some(end)) = (outer.last(), inner.first(), inner.last()) {
                    let gap = |p: &(f32, f32), q: &(f32, f32)| (p.0 - q.0).hypot(p.1 - q.1);
                    if gap(last, end) < gap(last, first) {
                        inner.reverse();
                    }
                }
                let outline: Vec<Pos2> = outer.iter().chain(inner.iter()).map(|(x, y)| at(*x, *y)).collect();
                if outline.len() > 2 {
                    painter.add(Shape::closed_line(outline, stroke));
                }
            }
            Self::Smile => {
                painter.circle_stroke(at(12.0, 12.0), 9.0 * unit, stroke);
                painter.circle_filled(at(9.0, 9.6), 1.3 * unit, color);
                painter.circle_filled(at(15.0, 9.6), 1.3 * unit, color);
                let mouth: Vec<Pos2> = (0..=14)
                    .map(|step| {
                        let angle = 0.16 * PI + step as f32 / 14.0 * 0.68 * PI;
                        at(12.0 + 5.6 * angle.cos(), 11.5 + 5.6 * angle.sin())
                    })
                    .collect();
                painter.add(Shape::line(mouth, stroke));
            }
            Self::Dumbbell => {
                for (from, to) in [
                    ((6.5, 6.5), (6.5, 17.5)),
                    ((17.5, 6.5), (17.5, 17.5)),
                    ((3.5, 9.0), (3.5, 15.0)),
                    ((20.5, 9.0), (20.5, 15.0)),
                    ((6.5, 12.0), (17.5, 12.0)),
                ] {
                    painter.line_segment([at(from.0, from.1), at(to.0, to.1)], stroke);
                }
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
