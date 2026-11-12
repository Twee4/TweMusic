//! Material 3 components built on egui: buttons, icon buttons, filter chips, segmented
//! buttons, cards, search bar, navigation rail/bar destinations, seek slider, snackbar.
//!
//! Each component is painted by hand so it can follow the Material spec (container
//! shapes, colour roles, 8% / 10% state layers) instead of egui's default widget look.

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CursorIcon, Frame, InnerResponse, Layout, Margin,
    Pos2, Rect, Response, Rounding, Sense, Stroke, Ui, UiBuilder, Vec2,
};

use crate::{
    icons::{self, Icon},
    theme::{self, motion, shape, state, with_alpha, Role},
};

/// An animated value that follows `target` on a spring (Material 3 Expressive motion).
/// The value is remembered per `id`; it starts at the target, so nothing animates on the
/// first frame. Keeps the UI repainting until it has settled.
pub fn spring(ui: &Ui, id: egui::Id, target: f32, spring: motion::Spring) -> f32 {
    let ctx = ui.ctx();
    let (value, velocity) = ctx.data_mut(|data| data.get_temp::<(f32, f32)>(id)).unwrap_or((target, 0.0));
    let (next, next_velocity) = motion::step(spring, value, velocity, target, ctx.input(|input| input.stable_dt));
    let (next, next_velocity) = if motion::settled(next, next_velocity, target) {
        (target, 0.0)
    } else {
        ctx.request_repaint();
        (next, next_velocity)
    };
    ctx.data_mut(|data| data.insert_temp(id, (next, next_velocity)));
    next
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Paints the hover / focus / pressed overlay for a component. `content` is the colour the
/// component draws its label or icon in, which is also the state layer colour.
fn state_layer(ui: &Ui, rect: Rect, rounding: Rounding, response: &Response, content: Color32, enabled: bool) {
    if !enabled {
        return;
    }
    let alpha = if response.is_pointer_button_down_on() {
        state::PRESSED
    } else if response.has_focus() {
        state::FOCUS
    } else if response.hovered() {
        state::HOVER
    } else {
        return;
    };
    ui.painter().rect_filled(rect, rounding, with_alpha(content, alpha));
}

fn pointer_cursor(response: Response, enabled: bool) -> Response {
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

// ---- Buttons ----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// High emphasis: the main action on a screen.
    Filled,
    /// Medium emphasis: important but secondary actions.
    Tonal,
    /// Medium emphasis with an outline.
    Outlined,
    /// Low emphasis.
    Text,
    /// A text button for destructive actions, in the error colour.
    Destructive,
}

pub fn button(ui: &mut Ui, kind: ButtonKind, label: &str) -> Response {
    button_enabled(ui, kind, label, true)
}

pub fn button_enabled(ui: &mut Ui, kind: ButtonKind, label: &str, enabled: bool) -> Response {
    let c = theme::colors(ui);
    let (container, content, outline) = match kind {
        ButtonKind::Filled => (Some(c.primary), c.on_primary, None),
        ButtonKind::Tonal => (Some(c.secondary_container), c.on_secondary_container, None),
        ButtonKind::Outlined => (None, c.primary, Some(c.outline)),
        ButtonKind::Text => (None, c.primary, None),
        ButtonKind::Destructive => (None, c.error, None),
    };
    let (container, content, outline) = if enabled {
        (container, content, outline)
    } else {
        (
            container.map(|_| with_alpha(c.on_surface, state::DISABLED_CONTAINER)),
            with_alpha(c.on_surface, state::DISABLED_CONTENT),
            outline.map(|_| with_alpha(c.on_surface, state::DISABLED_CONTAINER)),
        )
    };

    let galley = ui.painter().layout_no_wrap(label.to_owned(), Role::LabelLarge.font(), content);
    let padding = if matches!(kind, ButtonKind::Text | ButtonKind::Destructive) { 12.0 } else { 24.0 };
    let width = (galley.size().x + 2.0 * padding).max(if padding == 12.0 { 48.0 } else { 64.0 });
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 40.0), sense);

    if ui.is_rect_visible(rect) {
        // Expressive: a fully round button squares off while it is pressed.
        let pressed = spring(ui, response.id.with("press"), f32::from(u8::from(response.is_pointer_button_down_on() && enabled)), motion::SPATIAL_FAST);
        let rounding = Rounding::same(lerp(20.0, 12.0, pressed).max(0.0));
        let painter = ui.painter();
        if let Some(fill) = container {
            painter.rect_filled(rect, rounding, fill);
        }
        if let Some(line) = outline {
            painter.rect_stroke(rect, rounding, Stroke::new(1.0_f32, line));
        }
        state_layer(ui, rect, rounding, &response, content, enabled);
        painter.galley(rect.center() - galley.size() / 2.0, galley, content);
    }
    pointer_cursor(response, enabled)
}

// ---- Icon buttons -----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconButtonKind {
    /// No container. `selected` tints the icon with the primary colour.
    Standard,
    /// Primary container, for the single most important icon action (play / pause).
    Filled,
    /// Secondary container.
    Tonal,
}

pub fn icon_button(ui: &mut Ui, kind: IconButtonKind, icon: Icon, selected: bool, tooltip: &str) -> Response {
    icon_button_ex(ui, kind, icon, selected, true, 40.0, tooltip)
}

pub fn icon_button_ex(
    ui: &mut Ui,
    kind: IconButtonKind,
    icon: Icon,
    selected: bool,
    enabled: bool,
    size: f32,
    tooltip: &str,
) -> Response {
    let c = theme::colors(ui);
    let (container, content) = match (kind, selected) {
        (IconButtonKind::Standard, false) => (None, c.on_surface_variant),
        (IconButtonKind::Standard, true) => (None, c.primary),
        (IconButtonKind::Filled, _) => (Some(c.primary), c.on_primary),
        (IconButtonKind::Tonal, _) => (Some(c.secondary_container), c.on_secondary_container),
    };
    let (container, content) = if enabled {
        (container, content)
    } else {
        (
            container.map(|_| with_alpha(c.on_surface, state::DISABLED_CONTAINER)),
            with_alpha(c.on_surface, state::DISABLED_CONTENT),
        )
    };

    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), sense);
    if ui.is_rect_visible(rect) {
        // Expressive shape morphing: round when off, a rounded square when selected, and
        // squarer again while pressed. Standard icon buttons only morph on press.
        let toggled = if kind == IconButtonKind::Standard { 0.0 } else { f32::from(u8::from(selected)) };
        let on = spring(ui, response.id.with("on"), toggled, motion::SPATIAL_DEFAULT);
        let pressed = spring(ui, response.id.with("press"), f32::from(u8::from(response.is_pointer_button_down_on() && enabled)), motion::SPATIAL_FAST);
        let radius = lerp(lerp(size / 2.0, size * 0.32, on), size * 0.22, pressed).max(0.0);
        let rounding = Rounding::same(radius);
        let painter = ui.painter();
        if let Some(fill) = container {
            painter.rect_filled(rect, rounding, fill);
        }
        state_layer(ui, rect, rounding, &response, content, enabled);
        let icon_size = if size >= 40.0 { 24.0 } else { size * 0.6 };
        icon.paint(painter, rect.center(), icon_size, content, selected);
    }
    let response = pointer_cursor(response, enabled);
    if tooltip.is_empty() { response } else { response.on_hover_text(tooltip) }
}

// ---- Chips and segmented buttons --------------------------------------------------

/// A filter chip: outlined when off, secondary container with a check mark when on.
pub fn filter_chip(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let c = theme::colors(ui);
    let (fill, content, outline) = if selected {
        (Some(c.secondary_container), c.on_secondary_container, None)
    } else {
        (None, c.on_surface_variant, Some(c.outline))
    };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), Role::LabelLarge.font(), content);
    let check_space = if selected { 26.0 } else { 0.0 };
    let width = 12.0 + check_space + galley.size().x + 12.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());

    if ui.is_rect_visible(rect) {
        let rounding = Rounding::same(shape::SMALL);
        let painter = ui.painter();
        if let Some(fill) = fill {
            painter.rect_filled(rect, rounding, fill);
        }
        if let Some(line) = outline {
            painter.rect_stroke(rect, rounding, Stroke::new(1.0_f32, line));
        }
        state_layer(ui, rect, rounding, &response, content, true);
        if selected {
            icons::paint_check(painter, pos2(rect.left() + 12.0 + 9.0, rect.center().y), 18.0, content);
        }
        let text_pos = pos2(rect.left() + 12.0 + check_space, rect.center().y - galley.size().y / 2.0);
        painter.galley(text_pos, galley, content);
    }
    pointer_cursor(response, true)
}

/// A single-select connected button group (Material 3 Expressive): segments sit 2dp apart,
/// the outer ends are round, and the selected segment morphs into a full pill.
/// Returns the index that was clicked, if any.
pub fn segmented(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let c = theme::colors(ui);
    let widest = labels
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), Role::LabelLarge.font(), c.on_surface)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let segment_width = widest + 40.0;
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (index, label) in labels.iter().enumerate() {
            let is_selected = index == selected;
            let (rect, response) = ui.allocate_exact_size(vec2(segment_width, 40.0), Sense::click());
            if ui.is_rect_visible(rect) {
                let on = spring(ui, response.id.with("on"), f32::from(u8::from(is_selected)), motion::SPATIAL_DEFAULT);
                let pressed = spring(ui, response.id.with("press"), f32::from(u8::from(response.is_pointer_button_down_on())), motion::SPATIAL_FAST);
                let (content, fill) = if is_selected {
                    (c.on_primary, c.primary)
                } else {
                    (c.on_secondary_container, c.secondary_container)
                };
                let inner = lerp(lerp(6.0, 20.0, on), 10.0, pressed).max(0.0);
                let first = index == 0;
                let last = index + 1 == labels.len();
                let rounding = Rounding {
                    nw: if first { 20.0 } else { inner },
                    sw: if first { 20.0 } else { inner },
                    ne: if last { 20.0 } else { inner },
                    se: if last { 20.0 } else { inner },
                };
                let painter = ui.painter();
                painter.rect_filled(rect, rounding, fill);
                state_layer(ui, rect, rounding, &response, content, true);
                let galley = painter.layout_no_wrap((*label).to_owned(), Role::LabelLarge.font(), content);
                painter.galley(rect.center() - galley.size() / 2.0, galley, content);
            }
            if pointer_cursor(response, true).clicked() {
                clicked = Some(index);
            }
        }
    });
    clicked
}

// ---- Cards ------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    Filled,
    Elevated,
    Outlined,
}

/// A card that spans the available width. Elevation is expressed with tonal surface colours.
pub fn card<R>(ui: &mut Ui, kind: CardKind, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let c = theme::colors(ui);
    let (fill, stroke, radius) = match kind {
        CardKind::Filled => (c.surface_container_high, Stroke::NONE, shape::EXTRA_LARGE),
        CardKind::Elevated => (c.surface_container_low, Stroke::NONE, shape::LARGE_INCREASED),
        CardKind::Outlined => (c.surface, Stroke::new(1.0_f32, c.outline_variant), shape::LARGE_INCREASED),
    };
    Frame::none()
        .fill(fill)
        .stroke(stroke)
        .rounding(Rounding::same(radius))
        .inner_margin(Margin::same(20.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
}

// ---- Text fields and search bar ---------------------------------------------------

/// A filled text field.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> Response {
    let c = theme::colors(ui);
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(egui::RichText::new(hint).color(c.on_surface_variant))
            .desired_width(width)
            .margin(Margin::symmetric(16.0, 12.0)),
    )
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SearchBarResult {
    /// Enter was pressed in the field.
    pub submitted: bool,
}

/// A Material 3 search bar: 56dp tall, fully rounded, with a leading search icon button
/// (which submits) and a clear button while there is text.
pub fn search_bar(ui: &mut Ui, text: &mut String, hint: &str) -> SearchBarResult {
    let c = theme::colors(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
    ui.painter().rect_filled(rect, shape::full(), c.surface_container_high);

    let mut result = SearchBarResult::default();
    let mut focused = false;
    let inner = rect.shrink2(vec2(8.0, 0.0));
    ui.allocate_new_ui(
        UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)),
        |ui| {
            // The leading icon doubles as the submit button.
            if icon_button(ui, IconButtonKind::Standard, Icon::Search, false, "Search").clicked() {
                result.submitted = true;
            }

            let clear_space = if text.is_empty() { 0.0 } else { 48.0 };
            let edit_width = (ui.available_width() - clear_space).max(40.0);
            let edit = ui.add(
                egui::TextEdit::singleline(text)
                    .frame(false)
                    .font(Role::BodyLarge.font())
                    .hint_text(egui::RichText::new(hint).color(c.on_surface_variant))
                    .desired_width(edit_width)
                    .margin(Margin::symmetric(8.0, 0.0)),
            );
            focused = edit.has_focus();
            if edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                result.submitted = true;
            }
            if !text.is_empty()
                && icon_button(ui, IconButtonKind::Standard, Icon::Close, false, "Clear").clicked()
            {
                text.clear();
            }
        },
    );
    if focused {
        ui.painter().rect_stroke(rect, shape::full(), Stroke::new(2.0_f32, c.primary));
    }
    result
}

// ---- Navigation -------------------------------------------------------------------

fn nav_item(
    ui: &mut Ui,
    size: Vec2,
    indicator_width: f32,
    indicator_top: f32,
    icon: Icon,
    label: &str,
    selected: bool,
) -> Response {
    let c = theme::colors(ui);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let indicator = Rect::from_center_size(
        pos2(rect.center().x, rect.top() + indicator_top + 16.0),
        vec2(indicator_width, 32.0),
    );
    let rounding = shape::full();
    // Expressive: the selection indicator springs open from the centre of the icon.
    let open = spring(ui, response.id.with("indicator"), f32::from(u8::from(selected)), motion::SPATIAL_DEFAULT);
    let painter = ui.painter();
    if open > 0.01 {
        let width = (indicator_width * open).max(0.0);
        painter.rect_filled(Rect::from_center_size(indicator.center(), vec2(width, 32.0)), rounding, c.secondary_container);
    }
    state_layer(ui, indicator, rounding, &response, c.on_surface, true);

    let icon_color = if selected { c.on_secondary_container } else { c.on_surface_variant };
    icon.paint(painter, indicator.center(), 24.0, icon_color, selected);

    let label_color = if selected { c.on_surface } else { c.on_surface_variant };
    let galley = painter.layout_no_wrap(label.to_owned(), Role::LabelMedium.font(), label_color);
    painter.galley(
        pos2(rect.center().x - galley.size().x / 2.0, indicator.bottom() + 4.0),
        galley,
        label_color,
    );
    pointer_cursor(response, true)
}

/// A navigation rail destination: 80dp wide, icon in a 56x32 indicator with the label below.
pub fn rail_destination(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> Response {
    nav_item(ui, vec2(80.0, 64.0), 56.0, 4.0, icon, label, selected)
}

/// A navigation bar destination: `width` wide, 80dp tall, 64x32 indicator.
pub fn bar_destination(ui: &mut Ui, width: f32, icon: Icon, label: &str, selected: bool) -> Response {
    nav_item(ui, vec2(width, 80.0), 64.0, 12.0, icon, label, selected)
}

// ---- Seek slider ------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeekEvent {
    Idle,
    /// The handle is being dragged; the value is the position it points at.
    Dragging(f64),
    /// The user released the handle (or clicked the track) at this position.
    Committed(f64),
}

/// Maps a pointer x position to a value in `0..=max`.
pub fn seek_value(x: f32, left: f32, right: f32, max: f64) -> f64 {
    if right <= left {
        return 0.0;
    }
    f64::from(((x - left) / (right - left)).clamp(0.0, 1.0)) * max
}

/// A Material 3 slider for seeking. Dragging only previews; the caller seeks on `Committed`.
pub fn seek_bar(ui: &mut Ui, value: f64, max: f64, width: f32, enabled: bool) -> SeekEvent {
    seek_bar_impl(ui, value, max, width, enabled, None)
}

/// The Material 3 Expressive seek slider: the played part is a wave that moves while
/// `playing` and flattens when paused. Dragging only previews; seek on `Committed`.
pub fn wavy_seek_bar(ui: &mut Ui, value: f64, max: f64, width: f32, enabled: bool, playing: bool) -> SeekEvent {
    seek_bar_impl(ui, value, max, width, enabled, Some(playing))
}

/// Points of a horizontal sine wave from `left` to `right`, for the wavy progress track.
pub fn wave_points(left: f32, right: f32, y: f32, amplitude: f32, wavelength: f32, phase: f32) -> Vec<Pos2> {
    if right <= left {
        return vec![pos2(left, y)];
    }
    let steps = (((right - left) / 2.0).ceil() as usize).max(1);
    (0..=steps)
        .map(|step| {
            let x = left + (right - left) * step as f32 / steps as f32;
            let angle = (x - left) / wavelength * std::f32::consts::TAU - phase;
            pos2(x, y + amplitude * angle.sin())
        })
        .collect()
}

fn seek_bar_impl(ui: &mut Ui, value: f64, max: f64, width: f32, enabled: bool, wave: Option<bool>) -> SeekEvent {
    let c = theme::colors(ui);
    let usable = enabled && max > 0.0;
    let sense = if usable { Sense::click_and_drag() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), sense);
    let left = rect.left() + 10.0;
    let right = rect.right() - 10.0;

    let mut event = SeekEvent::Idle;
    if usable {
        let pointer_x = response
            .interact_pointer_pos()
            .or_else(|| ui.input(|input| input.pointer.latest_pos()))
            .map(|pos| pos.x);
        if let Some(x) = pointer_x {
            let target = seek_value(x, left, right, max);
            if response.drag_stopped() || response.clicked() {
                event = SeekEvent::Committed(target);
            } else if response.dragged() {
                event = SeekEvent::Dragging(target);
            }
        }
    }

    if ui.is_rect_visible(rect) {
        let shown = match event {
            SeekEvent::Dragging(v) | SeekEvent::Committed(v) => v,
            SeekEvent::Idle => value,
        };
        let fraction = if max > 0.0 { (shown / max).clamp(0.0, 1.0) as f32 } else { 0.0 };
        let handle_x = left + fraction * (right - left);
        let y = rect.center().y;
        let painter = ui.painter();

        let inactive = if usable { c.secondary_container } else { with_alpha(c.on_surface, state::DISABLED_CONTAINER) };
        match wave {
            None => {
                painter.rect_filled(Rect::from_min_max(pos2(left, y - 2.0), pos2(right, y + 2.0)), Rounding::same(2.0), inactive);
                if usable {
                    painter.rect_filled(
                        Rect::from_min_max(pos2(left, y - 2.0), pos2(handle_x.max(left), y + 2.0)),
                        Rounding::same(2.0),
                        c.primary,
                    );
                    if response.hovered() || response.dragged() {
                        painter.circle_filled(pos2(handle_x, y), 16.0, with_alpha(c.primary, 0.12));
                    }
                    painter.circle_filled(pos2(handle_x, y), 10.0, c.primary);
                }
            }
            Some(playing) => {
                // Handle: a 4dp bar with a 6dp gap either side (2dp wide while dragged).
                let grabbed = spring(ui, response.id.with("grab"), f32::from(u8::from(response.dragged())), motion::SPATIAL_FAST);
                let gap = 6.0;
                let painter = ui.painter();
                if usable {
                    let flat_from = (handle_x + gap + 2.0).min(right);
                    painter.rect_filled(
                        Rect::from_min_max(pos2(flat_from, y - 2.0), pos2(right, y + 2.0)),
                        Rounding::same(2.0),
                        inactive,
                    );
                    let amplitude = spring(ui, response.id.with("wave"), if playing { 3.0 } else { 0.0 }, motion::SPATIAL_SLOW);
                    let painter = ui.painter();
                    let wave_end = (handle_x - gap - 2.0).max(left);
                    let phase = (ui.input(|input| input.time) % 1.6) as f32 / 1.6 * std::f32::consts::TAU;
                    if wave_end > left {
                        let points = wave_points(left, wave_end, y, amplitude, 40.0, phase);
                        painter.add(egui::Shape::line(points, Stroke::new(4.0_f32, c.primary)));
                        painter.circle_filled(pos2(left, y + amplitude * (-phase).sin()), 2.0, c.primary);
                    }
                    if playing && amplitude > 0.01 {
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
                    }
                    let bar_width = lerp(4.0, 2.0, grabbed);
                    painter.rect_filled(
                        Rect::from_center_size(pos2(handle_x, y), vec2(bar_width, 20.0)),
                        Rounding::same(2.0),
                        c.primary,
                    );
                } else {
                    painter.rect_filled(Rect::from_min_max(pos2(left, y - 2.0), pos2(right, y + 2.0)), Rounding::same(2.0), inactive);
                }
            }
        }
    }
    if usable { response.on_hover_cursor(CursorIcon::PointingHand); }
    event
}

// ---- Misc -------------------------------------------------------------------------

/// A centred empty state: tonal icon badge, title and supporting text.
pub fn empty_state(ui: &mut Ui, icon: Icon, title: &str, body: &str) {
    let c = theme::colors(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        let (rect, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 32.0, c.secondary_container);
        icon.paint(ui.painter(), rect.center(), 32.0, c.on_secondary_container, false);
        ui.add_space(16.0);
        ui.label(theme::text(Role::TitleLarge, title).color(c.on_surface));
        ui.add_space(4.0);
        ui.label(theme::text(Role::BodyMedium, body).color(c.on_surface_variant));
    });
}

/// A snackbar anchored to the bottom centre of `anchor` (the free content area).
pub fn snackbar(ctx: &egui::Context, anchor: Rect, message: &str) {
    let c = theme::colors_ctx(ctx);
    egui::Area::new(egui::Id::new("m3-snackbar"))
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(pos2(anchor.center().x, anchor.bottom() - 16.0))
        .interactable(false)
        .show(ctx, |ui| {
            Frame::none()
                .fill(c.inverse_surface)
                .rounding(Rounding::same(shape::EXTRA_SMALL))
                .inner_margin(Margin::symmetric(16.0, 14.0))
                .show(ui, |ui| {
                    ui.set_max_width((anchor.width() - 32.0).clamp(120.0, 560.0));
                    ui.label(theme::text(Role::BodyMedium, message).color(c.inverse_on_surface));
                });
        });
}

/// Corner radii for an item in a grouped list (Material 3 Expressive): the first and last
/// items have large outer corners, everything between is nearly square, and a lone item is
/// fully rounded.
pub fn grouped_rounding(index: usize, count: usize) -> Rounding {
    let (outer, inner) = (24.0, 6.0);
    let top = if index == 0 { outer } else { inner };
    let bottom = if index + 1 >= count { outer } else { inner };
    Rounding { nw: top, ne: top, sw: bottom, se: bottom }
}

/// The large play / pause button (Material 3 Expressive "medium" FAB, 64dp). It is a circle
/// while paused, morphs into a rounded square while playing, and squares off when pressed.
pub fn play_button(ui: &mut Ui, playing: bool, tooltip: &str) -> Response {
    let c = theme::colors(ui);
    let size = 64.0;
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    if ui.is_rect_visible(rect) {
        let on = spring(ui, response.id.with("playing"), f32::from(u8::from(playing)), motion::SPATIAL_DEFAULT);
        let pressed = spring(ui, response.id.with("press"), f32::from(u8::from(response.is_pointer_button_down_on())), motion::SPATIAL_FAST);
        let radius = lerp(lerp(size / 2.0, 22.0, on), 14.0, pressed).max(0.0);
        let rounding = Rounding::same(radius);
        let painter = ui.painter();
        painter.rect_filled(rect, rounding, c.primary_container);
        state_layer(ui, rect, rounding, &response, c.on_primary_container, true);
        let icon = if playing { Icon::Pause } else { Icon::Play };
        icon.paint(painter, rect.center(), 32.0, c.on_primary_container, true);
    }
    pointer_cursor(response, true).on_hover_text(tooltip)
}

/// The indeterminate loading indicator: a soft star that rotates and breathes between a
/// round blob and a pointier shape, in a round container.
pub fn loading_indicator(ui: &mut Ui, size: f32) {
    let c = theme::colors(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let time = ui.input(|input| input.time) as f32;
    let painter = ui.painter();
    painter.circle_filled(rect.center(), size / 2.0, c.secondary_container);
    let radius = size * 0.30;
    let depth = 0.10 + 0.10 * (0.5 + 0.5 * (time * 2.4).sin());
    let rotation = time * 3.0;
    let points: Vec<Pos2> = (0..60)
        .map(|step| {
            let angle = step as f32 / 60.0 * std::f32::consts::TAU;
            let reach = radius * (1.0 + depth * (6.0 * (angle + rotation)).cos());
            rect.center() + vec2(angle.cos(), angle.sin()) * reach
        })
        .collect();
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), c.on_secondary_container);
    for point in &points {
        mesh.colored_vertex(*point, c.on_secondary_container);
    }
    let count = points.len() as u32;
    for index in 0..count {
        mesh.add_triangle(0, 1 + index, 1 + (index + 1) % count);
    }
    painter.add(egui::Shape::mesh(mesh));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_points_span_the_range_and_flatten_at_zero_amplitude() {
        let wave = wave_points(10.0, 110.0, 20.0, 3.0, 40.0, 0.0);
        assert_eq!(wave.first().unwrap().x, 10.0);
        assert_eq!(wave.last().unwrap().x, 110.0);
        assert!(wave.iter().all(|point| (point.y - 20.0).abs() <= 3.0 + 1e-4));
        assert!(wave.iter().any(|point| (point.y - 20.0).abs() > 2.0));
        let flat = wave_points(10.0, 110.0, 20.0, 0.0, 40.0, 1.0);
        assert!(flat.iter().all(|point| point.y == 20.0));
        assert_eq!(wave_points(50.0, 50.0, 1.0, 3.0, 40.0, 0.0).len(), 1);
    }

    #[test]
    fn grouped_rounding_rounds_only_the_ends() {
        let only = grouped_rounding(0, 1);
        assert_eq!((only.nw, only.sw), (24.0, 24.0));
        let first = grouped_rounding(0, 3);
        assert_eq!((first.nw, first.sw), (24.0, 6.0));
        let middle = grouped_rounding(1, 3);
        assert_eq!((middle.nw, middle.se), (6.0, 6.0));
        let last = grouped_rounding(2, 3);
        assert_eq!((last.nw, last.se), (6.0, 24.0));
    }

    #[test]
    fn seek_value_maps_and_clamps() {
        assert_eq!(seek_value(10.0, 10.0, 110.0, 200.0), 0.0);
        assert_eq!(seek_value(60.0, 10.0, 110.0, 200.0), 100.0);
        assert_eq!(seek_value(110.0, 10.0, 110.0, 200.0), 200.0);
        assert_eq!(seek_value(-50.0, 10.0, 110.0, 200.0), 0.0);
        assert_eq!(seek_value(500.0, 10.0, 110.0, 200.0), 200.0);
        assert_eq!(seek_value(50.0, 10.0, 10.0, 200.0), 0.0);
    }
}

/// A Material 3 switch (52x32 track, thumb grows when pressed). Returns the response;
/// `*on` is flipped when it is clicked.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let c = theme::colors(ui);
    let (rect, mut response) = ui.allocate_exact_size(vec2(52.0, 32.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool(response.id, *on);
        let lerp = |a: Color32, b: Color32| theme::blend(a, b, t);
        let track = lerp(c.surface_container_highest, c.primary);
        let handle = lerp(c.outline, c.on_primary);
        let radius = if response.is_pointer_button_down_on() { 14.0 } else { 8.0 + 4.0 * t };
        let center = pos2(rect.left() + 16.0 + 20.0 * t, rect.center().y);
        let painter = ui.painter();
        painter.rect_filled(rect, Rounding::same(16.0), track);
        if t < 1.0 {
            painter.rect_stroke(rect.shrink(1.0), Rounding::same(15.0), Stroke::new(2.0 * (1.0 - t), c.outline));
        }
        if response.hovered() || response.has_focus() {
            painter.circle_filled(center, 20.0, theme::with_alpha(handle, theme::state::HOVER));
        }
        painter.circle_filled(center, radius, handle);
    }
    pointer_cursor(response, true)
}
