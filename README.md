use egui::{Color32, Visuals};

pub struct Colors;

impl Colors {
    pub const primary: Color32 = Color32::from_rgb(120, 94, 255);
    pub const secondary: Color32 = Color32::from_rgb(255, 127, 80);
    pub const tertiary: Color32 = Color32::from_rgb(87, 190, 143);
    pub const accent: Color32 = Color32::from_rgb(46, 133, 255);
    pub const success: Color32 = Color32::from_rgb(48, 169, 120);
    pub const warn: Color32 = Color32::from_rgb(241, 171, 84);
    pub const background: Color32 = Color32::from_rgb(18, 20, 26);
    pub const panel: Color32 = Color32::from_rgb(27, 29, 36);
    pub const panel_alt: Color32 = Color32::from_rgb(38, 40, 50);
}

pub fn dark_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = Colors.panel;
    v.window_fill = Colors.background;
    v.extreme_bg_color = Color32::from_rgb(12, 14, 18);
    v.widgets.noninteractive.bg_fill = Colors.panel_alt;
    v.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(214, 218, 230);
    v.widgets.inactive.bg_fill = Colors.panel_alt;
    v.widgets.hovered.bg_fill = Color32::from_rgb(44, 47, 58);
    v.widgets.active.bg_fill = Colors.primary;
    v.selection.stroke.color = Colors.primary;
    v
}

