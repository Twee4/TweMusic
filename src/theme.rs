use eframe::egui::{Color32, Visuals};

pub const PRIMARY: Color32 = Color32::from_rgb(208, 188, 255);
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
    visuals.panel_fill = Color32::from_rgb(29, 27, 32);
    visuals.window_fill = Color32::from_rgb(20, 18, 23);
    visuals.extreme_bg_color = Color32::from_rgb(20, 18, 23);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(40, 37, 43);
    visuals.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(232, 224, 235);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(49, 45, 54);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(64, 59, 71);
    visuals.widgets.active.bg_fill = PRIMARY;
    visuals.selection.bg_fill = Color32::from_rgb(78, 64, 101);
    visuals.selection.stroke.color = PRIMARY;
    visuals
}
