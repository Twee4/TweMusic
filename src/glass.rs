//! Glass-style building blocks for the Home screen: an aurora backdrop, translucent
//! surfaces, artwork tiles with a dark gradient and bold white titles, circular mix
//! artwork, mood chips, the big shuffle button and carousel dots.
//!
//! egui cannot blur what is behind a widget, so "glass" here means a translucent fill with a
//! soft light edge, drawn over a backdrop of large, soft colour blobs taken from the colour
//! scheme (which itself follows the album art of the song that is playing).

use std::f32::consts::{PI, TAU};

use eframe::egui::{
    self,
    epaint::{Mesh, Shadow, Vertex, WHITE_UV},
    pos2, vec2, Color32, Context, FontId, Id, LayerId, Painter, Pos2, Rect, Response, Rounding, Sense, Shape, Stroke, Ui,
    Vec2,
};
use eframe::egui::text::LayoutJob;

use crate::{
    icons::Icon,
    m3,
    theme::{self, motion, shape, ColorScheme},
};

/// Linear mix of two colours (premultiplied channels, so alpha mixes correctly too).
pub fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| m3::lerp(f32::from(a), f32::from(b), t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_premultiplied(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
        channel(from.a(), to.a()),
    )
}

// ---- Backdrop and glass surfaces ---------------------------------------------------

/// Paints the page background: the surface colour with large, soft colour blobs on it.
/// Drawn on the background layer, so every panel can stay transparent above it.
pub fn aurora(ctx: &Context, c: &ColorScheme, dark: bool) {
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::background());
    painter.rect_filled(rect, 0.0, c.surface);
    let reach = rect.width().max(rect.height());
    let strength = if dark { 1.0 } else { 0.7 };
    blob(&painter, rect.min + vec2(rect.width() * 0.12, rect.height() * 0.04), reach * 0.62, theme::with_alpha(c.primary, 0.30 * strength));
    blob(&painter, rect.min + vec2(rect.width() * 0.95, rect.height() * 0.38), reach * 0.55, theme::with_alpha(c.tertiary, 0.22 * strength));
    blob(&painter, rect.min + vec2(rect.width() * 0.18, rect.height() * 0.95), reach * 0.55, theme::with_alpha(c.secondary, 0.20 * strength));
}

/// A soft radial blob: `colour` in the middle fading to nothing at `radius`.
fn blob(painter: &Painter, centre: Pos2, radius: f32, colour: Color32) {
    const SEGMENTS: u32 = 48;
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex { pos: centre, uv: WHITE_UV, color: colour });
    let rings = [(0.5_f32, 0.42_f32), (1.0, 0.0)];
    for (distance, strength) in rings {
        let ring_colour = mix(Color32::TRANSPARENT, colour, strength);
        for step in 0..SEGMENTS {
            let angle = step as f32 / SEGMENTS as f32 * TAU;
            mesh.vertices.push(Vertex {
                pos: centre + vec2(angle.cos(), angle.sin()) * radius * distance,
                uv: WHITE_UV,
                color: ring_colour,
            });
        }
    }
    let inner = |step: u32| 1 + step % SEGMENTS;
    let outer = |step: u32| 1 + SEGMENTS + step % SEGMENTS;
    for step in 0..SEGMENTS {
        mesh.indices.extend([0, inner(step), inner(step + 1)]);
        mesh.indices.extend([inner(step), outer(step), outer(step + 1)]);
        mesh.indices.extend([inner(step), outer(step + 1), inner(step + 1)]);
    }
    painter.add(Shape::mesh(mesh));
}

/// Translucent fill for glass surfaces.
pub fn glass_fill(dark: bool) -> Color32 {
    theme::with_alpha(Color32::WHITE, if dark { 0.08 } else { 0.55 })
}

/// The soft light edge of a glass surface.
pub fn glass_edge(dark: bool) -> Stroke {
    Stroke::new(1.0_f32, theme::with_alpha(Color32::WHITE, if dark { 0.16 } else { 0.8 }))
}

/// Shadow under floating glass.
pub fn glass_shadow(dark: bool) -> Shadow {
    Shadow {
        offset: vec2(0.0, 8.0),
        blur: 28.0,
        spread: 0.0,
        color: theme::with_alpha(Color32::BLACK, if dark { 0.40 } else { 0.16 }),
    }
}

/// A frame for glass surfaces such as the floating player and sheets.
pub fn glass_frame(dark: bool, radius: f32) -> egui::Frame {
    egui::Frame::none()
        .fill(glass_fill(dark))
        .stroke(glass_edge(dark))
        .rounding(Rounding::same(radius))
        .shadow(glass_shadow(dark))
}

// ---- Artwork -----------------------------------------------------------------------

/// Draws album art into `rect`, centre-cropped to fill it, with rounded corners.
/// While it loads (or if it fails, e.g. offline) nothing is drawn, so the placeholder
/// underneath stays visible and egui's warning glyph never appears.
pub fn paint_album_art(ui: &Ui, url: &str, rect: Rect, radius: f32) {
    let image = egui::Image::from_uri(url).show_loading_spinner(false);
    let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(ui.ctx(), rect.size()) else {
        return;
    };
    let aspect = if texture.size.y > 0.0 { texture.size.x / texture.size.y } else { 1.0 };
    let (inset_x, inset_y) = if aspect > 1.0 {
        ((1.0 - 1.0 / aspect) / 2.0, 0.0)
    } else if aspect > 0.0 {
        (0.0, (1.0 - aspect) / 2.0)
    } else {
        (0.0, 0.0)
    };
    image
        .uv(Rect::from_min_max(pos2(inset_x, inset_y), pos2(1.0 - inset_x, 1.0 - inset_y)))
        .rounding(Rounding::same(radius))
        .paint_at(ui, rect);
}

/// A stand-in for missing artwork: a tonal gradient-ish fill chosen by `seed`, with a note.
pub fn placeholder_art(painter: &Painter, rect: Rect, radius: f32, seed: u64, c: &ColorScheme) {
    let t = (seed % 100) as f32 / 100.0;
    let fill = mix(c.primary_container, c.tertiary_container, t);
    painter.rect_filled(rect, Rounding::same(radius), fill);
    let size = (rect.width().min(rect.height()) * 0.36).clamp(14.0, 40.0);
    Icon::Note.paint(painter, rect.center(), size, theme::with_alpha(c.on_primary_container, 0.8), false);
}

/// A dark gradient over the lower part of `rect`, following its rounded bottom corners.
/// `from` is how far down (0..1) it starts and `strength` its opacity at the very bottom.
pub fn bottom_gradient(painter: &Painter, rect: Rect, radius: f32, from: f32, strength: f32) {
    let top = rect.top() + rect.height() * from.clamp(0.0, 0.95);
    let r = radius.min(rect.width() / 2.0).min((rect.bottom() - top).max(1.0) - 0.5).max(0.0);
    let mut points = vec![pos2(rect.left(), top), pos2(rect.left(), rect.bottom() - r)];
    const ARC: u32 = 8;
    for step in 0..=ARC {
        let angle = PI - (PI / 2.0) * step as f32 / ARC as f32;
        points.push(pos2(rect.left() + r, rect.bottom() - r) + vec2(angle.cos(), angle.sin()) * r);
    }
    for step in 0..=ARC {
        let angle = (PI / 2.0) * (1.0 - step as f32 / ARC as f32);
        points.push(pos2(rect.right() - r, rect.bottom() - r) + vec2(angle.cos(), angle.sin()) * r);
    }
    points.push(pos2(rect.right(), top));

    let span = (rect.bottom() - top).max(1.0);
    let colour_at = |y: f32| {
        let t = ((y - top) / span).clamp(0.0, 1.0);
        Color32::from_rgba_unmultiplied(0, 0, 0, (t.powf(1.4) * strength * 255.0).round() as u8)
    };
    let mut mesh = Mesh::default();
    for point in &points {
        mesh.vertices.push(Vertex { pos: *point, uv: WHITE_UV, color: colour_at(point.y) });
    }
    // The outline is convex, so a fan from its first point covers it exactly.
    for index in 1..points.len().saturating_sub(1) as u32 {
        mesh.indices.extend([0, index, index + 1]);
    }
    painter.add(Shape::mesh(mesh));
}

/// Paints `text` in white, two lines at most, with a faux bold (egui ships one weight).
fn bold_white_title(painter: &Painter, text: &str, size: f32, wrap_width: f32, bottom_left: Pos2) {
    let mut job = LayoutJob::simple(text.to_owned(), FontId::proportional(size), Color32::WHITE, wrap_width);
    job.wrap.max_rows = 2;
    let galley = painter.layout_job(job);
    let origin = pos2(bottom_left.x, bottom_left.y - galley.size().y);
    painter.galley(origin + vec2(0.45, 0.0), galley.clone(), Color32::WHITE);
    painter.galley(origin, galley, Color32::WHITE);
}

/// Three little bars that dance while a song plays.
fn equaliser(painter: &Painter, rect: Rect, time: f64, colour: Color32) {
    let bar = rect.width() / 5.0;
    for index in 0..3 {
        let phase = index as f64 * 1.7;
        let height = rect.height() * (0.35 + 0.65 * ((time * (5.0 + index as f64 * 1.3) + phase).sin().abs() as f32));
        let left = rect.left() + bar * (index as f32 * 2.0);
        let bar_rect = Rect::from_min_max(pos2(left, rect.bottom() - height), pos2(left + bar, rect.bottom()));
        painter.rect_filled(bar_rect, Rounding::same(bar / 2.0), colour);
    }
}

// ---- Components --------------------------------------------------------------------

/// What a Speed dial tile shows.
pub struct Tile<'a> {
    pub title: &'a str,
    pub art: Option<&'a str>,
    pub seed: u64,
    /// The song is playing right now: shows the little equaliser.
    pub playing: bool,
}

/// An artwork tile with a dark gradient and a bold white title. It springs down while
/// pressed and its corners become tighter, then settles back.
pub fn speed_tile(ui: &mut Ui, rect: Rect, id: Id, tile: &Tile, c: &ColorScheme) -> Response {
    let response = m3::pointer_cursor(ui.interact(rect, id, Sense::click()), true);
    let press = m3::spring(ui, id.with("press"), if response.is_pointer_button_down_on() { 1.0 } else { 0.0 }, motion::SPATIAL_FAST);
    let lift = m3::spring(ui, id.with("lift"), if response.hovered() { 1.0 } else { 0.0 }, motion::SPATIAL_DEFAULT);
    let scale = (1.0 - 0.05 * press + 0.015 * lift).max(0.5);
    let shown = Rect::from_center_size(rect.center(), rect.size() * scale);
    let radius = m3::lerp(shape::EXTRA_LARGE, shape::LARGE, press.clamp(0.0, 1.0));

    if ui.is_rect_visible(shown) {
        let painter = ui.painter().clone();
        placeholder_art(&painter, shown, radius, tile.seed, c);
        if let Some(url) = tile.art {
            paint_album_art(ui, url, shown, radius);
        }
        bottom_gradient(&painter, shown, radius, 0.38, 0.82);
        bold_white_title(&painter, tile.title, 15.0, shown.width() - 24.0, pos2(shown.left() + 12.0, shown.bottom() - 12.0));
        if tile.playing {
            let badge = Rect::from_min_size(pos2(shown.right() - 36.0, shown.top() + 10.0), vec2(26.0, 26.0));
            painter.rect_filled(badge, Rounding::same(13.0), theme::with_alpha(Color32::BLACK, 0.45));
            equaliser(&painter, badge.shrink2(vec2(7.0, 7.0)), ui.input(|input| input.time), Color32::WHITE);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
        painter.rect_stroke(shown, Rounding::same(radius), Stroke::new(1.0_f32, theme::with_alpha(Color32::WHITE, 0.14)));
    }
    response
}

/// A circular mix: artwork in a circle that turns into a rounded square while pressed,
/// with its name underneath. Returns the click response of the whole item.
pub fn mix_disc(ui: &mut Ui, diameter: f32, id: Id, title: &str, subtitle: &str, art: Option<&str>, seed: u64, c: &ColorScheme) -> Response {
    let label_height = 44.0;
    let (rect, _) = ui.allocate_exact_size(vec2(diameter, diameter + label_height), Sense::hover());
    let response = m3::pointer_cursor(ui.interact(rect, id, Sense::click()), true);
    let press = m3::spring(ui, id.with("press"), if response.is_pointer_button_down_on() { 1.0 } else { 0.0 }, motion::SPATIAL_FAST);
    let lift = m3::spring(ui, id.with("lift"), if response.hovered() { 1.0 } else { 0.0 }, motion::SPATIAL_DEFAULT);

    let disc = Rect::from_min_size(rect.min, vec2(diameter, diameter));
    let scale = (1.0 - 0.06 * press + 0.02 * lift).max(0.5);
    let shown = Rect::from_center_size(disc.center(), disc.size() * scale);
    let radius = m3::lerp(shown.width() / 2.0, shown.width() * 0.3, press.clamp(0.0, 1.0));

    if ui.is_rect_visible(rect) {
        let painter = ui.painter().clone();
        placeholder_art(&painter, shown, radius, seed, c);
        if let Some(url) = art {
            paint_album_art(ui, url, shown, radius);
        }
        painter.rect_stroke(shown, Rounding::same(radius), Stroke::new(1.0_f32, theme::with_alpha(Color32::WHITE, 0.16)));
        // A small play badge, like a "start this mix" hint.
        let badge_centre = pos2(shown.right() - diameter * 0.14, shown.bottom() - diameter * 0.14);
        painter.circle_filled(badge_centre, 15.0, c.primary);
        Icon::Play.paint(&painter, badge_centre + vec2(0.5, 0.0), 16.0, c.on_primary, true);

        let text_left = rect.left();
        let width = diameter;
        let title_galley = painter.layout(title.to_owned(), FontId::proportional(14.0), c.on_surface, width);
        let title_pos = pos2(text_left + (width - title_galley.size().x.min(width)) / 2.0, disc.bottom() + 8.0);
        let title_clip = Rect::from_min_size(pos2(text_left, disc.bottom() + 6.0), vec2(width, 20.0));
        painter.with_clip_rect(title_clip.intersect(ui.clip_rect())).galley(title_pos, title_galley, c.on_surface);
        let sub_galley = painter.layout(subtitle.to_owned(), FontId::proportional(12.0), c.on_surface_variant, width);
        let sub_pos = pos2(text_left + (width - sub_galley.size().x.min(width)) / 2.0, disc.bottom() + 26.0);
        let sub_clip = Rect::from_min_size(pos2(text_left, disc.bottom() + 24.0), vec2(width, 16.0));
        painter.with_clip_rect(sub_clip.intersect(ui.clip_rect())).galley(sub_pos, sub_galley, c.on_surface_variant);
    }
    response
}

/// A mood chip: glass when idle, primary when selected, and its corners tighten when selected.
pub fn mood_chip(ui: &mut Ui, id: Id, icon: Icon, label: &str, selected: bool, c: &ColorScheme) -> Response {
    let dark = ui.visuals().dark_mode;
    let height = 46.0;
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(15.0), c.on_surface);
    let width = 18.0 + 22.0 + 8.0 + galley.size().x + 20.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let response = m3::pointer_cursor(ui.interact(rect, id, Sense::click()), true);
    let on = m3::spring(ui, id.with("on"), if selected { 1.0 } else { 0.0 }, motion::SPATIAL_DEFAULT);
    let press = m3::spring(ui, id.with("press"), if response.is_pointer_button_down_on() { 1.0 } else { 0.0 }, motion::SPATIAL_FAST);

    if ui.is_rect_visible(rect) {
        let shown = Rect::from_center_size(rect.center(), rect.size() * (1.0 - 0.04 * press).max(0.5));
        let t = on.clamp(0.0, 1.0);
        let radius = m3::lerp(height / 2.0, shape::LARGE, t);
        let fill = mix(glass_fill(dark), c.primary, t);
        let ink = mix(c.on_surface, c.on_primary, t);
        let painter = ui.painter();
        painter.rect_filled(shown, Rounding::same(radius), fill);
        painter.rect_stroke(shown, Rounding::same(radius), Stroke::new(1.0_f32, mix(glass_edge(dark).color, Color32::TRANSPARENT, t)));
        m3::state_layer(ui, shown, Rounding::same(radius), &response, ink, true);
        let painter = ui.painter();
        Icon::paint(icon, painter, pos2(shown.left() + 18.0 + 11.0, shown.center().y), 22.0, ink, selected);
        let text = painter.layout_no_wrap(label.to_owned(), FontId::proportional(15.0), ink);
        let text_pos = pos2(shown.left() + 18.0 + 22.0 + 8.0, shown.center().y - text.size().y / 2.0);
        painter.galley(text_pos, text, ink);
    }
    response
}

/// The large shuffle button: a full-width primary pill that tightens while pressed.
pub fn shuffle_button(ui: &mut Ui, id: Id, label: &str, c: &ColorScheme) -> Response {
    let width = ui.available_width().min(520.0);
    let height = 68.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let response = m3::pointer_cursor(ui.interact(rect, id, Sense::click()), true);
    let press = m3::spring(ui, id.with("press"), if response.is_pointer_button_down_on() { 1.0 } else { 0.0 }, motion::SPATIAL_FAST);
    if ui.is_rect_visible(rect) {
        let shown = Rect::from_center_size(rect.center(), vec2(width * (1.0 - 0.025 * press), height * (1.0 - 0.06 * press)).max(vec2(10.0, 10.0)));
        let radius = m3::lerp(height / 2.0, shape::LARGE_INCREASED, press.clamp(0.0, 1.0));
        ui.painter().rect_filled(shown, Rounding::same(radius), c.primary);
        m3::state_layer(ui, shown, Rounding::same(radius), &response, c.on_primary, true);
        let painter = ui.painter();
        let galley = painter.layout_no_wrap(label.to_owned(), FontId::proportional(20.0), c.on_primary);
        let content = 28.0 + 12.0 + galley.size().x;
        let left = shown.center().x - content / 2.0;
        Icon::Shuffle.paint(painter, pos2(left + 14.0, shown.center().y), 28.0, c.on_primary, false);
        let text_pos = pos2(left + 28.0 + 12.0, shown.center().y - galley.size().y / 2.0);
        painter.galley(text_pos + vec2(0.4, 0.0), galley.clone(), c.on_primary);
        painter.galley(text_pos, galley, c.on_primary);
    }
    response
}

/// Carousel dots; the active one stretches into a pill. Returns the page the user clicked.
pub fn dots(ui: &mut Ui, id: Id, count: usize, active: f32, c: &ColorScheme) -> Option<usize> {
    if count < 2 {
        return None;
    }
    let (small, wide, gap) = (8.0_f32, 26.0_f32, 6.0_f32);
    let widths: Vec<f32> = (0..count)
        .map(|index| {
            let nearness = (1.0 - (index as f32 - active).abs()).clamp(0.0, 1.0);
            m3::lerp(small, wide, nearness)
        })
        .collect();
    let total = widths.iter().sum::<f32>() + gap * (count as f32 - 1.0);
    let mut clicked = None;
    ui.vertical_centered(|ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(total + 16.0, 24.0), Sense::click());
        let mut x = rect.center().x - total / 2.0;
        for (index, width) in widths.iter().enumerate() {
            let nearness = (1.0 - (index as f32 - active).abs()).clamp(0.0, 1.0);
            let dot = Rect::from_min_size(pos2(x, rect.center().y - small / 2.0), vec2(*width, small));
            ui.painter().rect_filled(dot, Rounding::same(small / 2.0), mix(theme::with_alpha(c.on_surface, 0.25), c.primary, nearness));
            if response.clicked() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    if pointer.x >= x - gap / 2.0 && pointer.x < x + width + gap / 2.0 {
                        clicked = Some(index);
                    }
                }
            }
            x += width + gap;
        }
        let _ = id;
    });
    clicked
}

/// Convenience for sheets: a title row size.
pub fn tile_size(content_width: f32, columns: usize, gap: f32) -> Vec2 {
    let columns = columns.max(1) as f32;
    let width = ((content_width - gap * (columns - 1.0)) / columns).max(40.0);
    vec2(width, width * 1.08)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_blends_and_clamps() {
        let a = Color32::from_rgb(0, 0, 0);
        let b = Color32::from_rgb(200, 100, 50);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, 7.0), b);
        assert_eq!(mix(a, b, 0.5), Color32::from_rgb(100, 50, 25));
    }

    #[test]
    fn tiles_fit_the_width() {
        let size = tile_size(330.0, 3, 12.0);
        assert!((size.x * 3.0 + 24.0 - 330.0).abs() < 0.01);
        assert!(size.y > size.x);
    }
}
