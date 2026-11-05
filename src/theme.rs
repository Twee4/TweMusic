use eframe::egui::{Color32, Visuals};

pub const PRIMARY: Color32 = Color32::from_rgb(208, 188, 255);
pub const PRIMARY_CONTAINER: Color32 = Color32::from_rgb(79, 55, 110);
pub const ON_PRIMARY_CONTAINER: Color32 = Color32::from_rgb(239, 222, 255);
pub const BACKGROUND: Color32 = Color32::from_rgb(20, 18, 23);
pub const SURFACE: Color32 = Color32::from_rgb(29, 27, 32);
pub const SURFACE_CONTAINER: Color32 = Color32::from_rgb(40, 37, 43);
pub const ON_SURFACE_VARIANT: Color32 = Color32::from_rgb(202, 196, 208);
pub const COVER_COLORS: [Color32; 6] = [
    Color32::from_rgb(117, 82, 161),
    Color32::from_rgb(183, 91, 67),
    Color32::from_rgb(52, 126, 111),
    Color32::from_rgb(63, 103, 158),
    Color32::from_rgb(174, 133, 63),
    Color32::from_rgb(100, 111, 73),
];

pub fn dark_visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = BACKGROUND;
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(232, 224, 235);
    visuals.widgets.inactive.bg_fill = SURFACE_CONTAINER;
    visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(232, 224, 235);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(58, 53, 64);
    visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
    visuals.widgets.active.bg_fill = PRIMARY;
    visuals.widgets.active.fg_stroke.color = Color32::from_rgb(35, 25, 45);
    visuals.selection.bg_fill = Color32::from_rgb(78, 64, 101);
    visuals.selection.stroke.color = PRIMARY;
    visuals
}
