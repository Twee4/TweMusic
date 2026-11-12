//! Material 3 design tokens: colour roles (light and dark), shape scale, type scale,
//! state layers, window size classes, and the egui style that applies them.
//!
//! Colours are the Material 3 baseline scheme (seed #6750A4). Every "on-X" role is meant
//! to be drawn on its "X" role; the tests below check that these pairs stay readable.

use std::path::PathBuf;

use eframe::egui::{
    self, Color32, Context, FontId, RichText, Rounding, Shadow, Stroke, Style, TextStyle, Theme,
    ThemePreference, Visuals, vec2,
};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

#[derive(Debug, Clone, Copy)]
pub struct ColorScheme {
    pub primary: Color32,
    pub on_primary: Color32,
    pub primary_container: Color32,
    pub on_primary_container: Color32,
    pub secondary: Color32,
    pub on_secondary: Color32,
    pub secondary_container: Color32,
    pub on_secondary_container: Color32,
    pub tertiary: Color32,
    pub on_tertiary: Color32,
    pub tertiary_container: Color32,
    pub on_tertiary_container: Color32,
    pub error: Color32,
    pub on_error: Color32,
    pub error_container: Color32,
    pub on_error_container: Color32,
    pub surface: Color32,
    pub on_surface: Color32,
    pub surface_variant: Color32,
    pub on_surface_variant: Color32,
    pub outline: Color32,
    pub outline_variant: Color32,
    pub inverse_surface: Color32,
    pub inverse_on_surface: Color32,
    pub inverse_primary: Color32,
    pub surface_container_lowest: Color32,
    pub surface_container_low: Color32,
    pub surface_container: Color32,
    pub surface_container_high: Color32,
    pub surface_container_highest: Color32,
}

pub const DARK: ColorScheme = ColorScheme {
    primary: hex(0xD0BCFF),
    on_primary: hex(0x381E72),
    primary_container: hex(0x4F378B),
    on_primary_container: hex(0xEADDFF),
    secondary: hex(0xCCC2DC),
    on_secondary: hex(0x332D41),
    secondary_container: hex(0x4A4458),
    on_secondary_container: hex(0xE8DEF8),
    tertiary: hex(0xEFB8C8),
    on_tertiary: hex(0x492532),
    tertiary_container: hex(0x633B48),
    on_tertiary_container: hex(0xFFD8E4),
    error: hex(0xF2B8B5),
    on_error: hex(0x601410),
    error_container: hex(0x8C1D18),
    on_error_container: hex(0xF9DEDC),
    surface: hex(0x141218),
    on_surface: hex(0xE6E0E9),
    surface_variant: hex(0x49454F),
    on_surface_variant: hex(0xCAC4D0),
    outline: hex(0x938F99),
    outline_variant: hex(0x49454F),
    inverse_surface: hex(0xE6E0E9),
    inverse_on_surface: hex(0x322F35),
    inverse_primary: hex(0x6750A4),
    surface_container_lowest: hex(0x0F0D13),
    surface_container_low: hex(0x1D1B20),
    surface_container: hex(0x211F26),
    surface_container_high: hex(0x2B2930),
    surface_container_highest: hex(0x36343B),
};

pub const LIGHT: ColorScheme = ColorScheme {
    primary: hex(0x6750A4),
    on_primary: hex(0xFFFFFF),
    primary_container: hex(0xEADDFF),
    on_primary_container: hex(0x21005D),
    secondary: hex(0x625B71),
    on_secondary: hex(0xFFFFFF),
    secondary_container: hex(0xE8DEF8),
    on_secondary_container: hex(0x1D192B),
    tertiary: hex(0x7D5260),
    on_tertiary: hex(0xFFFFFF),
    tertiary_container: hex(0xFFD8E4),
    on_tertiary_container: hex(0x31111D),
    error: hex(0xB3261E),
    on_error: hex(0xFFFFFF),
    error_container: hex(0xF9DEDC),
    on_error_container: hex(0x410E0B),
    surface: hex(0xFEF7FF),
    on_surface: hex(0x1D1B20),
    surface_variant: hex(0xE7E0EC),
    on_surface_variant: hex(0x49454F),
    outline: hex(0x79747E),
    outline_variant: hex(0xCAC4D0),
    inverse_surface: hex(0x322F35),
    inverse_on_surface: hex(0xF5EFF7),
    inverse_primary: hex(0xD0BCFF),
    surface_container_lowest: hex(0xFFFFFF),
    surface_container_low: hex(0xF7F2FA),
    surface_container: hex(0xF3EDF7),
    surface_container_high: hex(0xECE6F0),
    surface_container_highest: hex(0xE6E0E9),
};

pub fn colors_for(dark: bool) -> &'static ColorScheme {
    if dark { &DARK } else { &LIGHT }
}

/// The colour scheme for the theme `ui` is currently drawn in.
pub fn colors(ui: &egui::Ui) -> &'static ColorScheme {
    colors_for(ui.visuals().dark_mode)
}

/// Same, for code that only has the context.
pub fn colors_ctx(ctx: &Context) -> &'static ColorScheme {
    colors_for(ctx.style().visuals.dark_mode)
}

/// A translucent copy of `color`, used for state layers drawn on top of a container.
pub fn with_alpha(color: Color32, alpha: f32) -> Color32 {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

/// `over` painted at `alpha` on top of an opaque `base`.
pub fn blend(base: Color32, over: Color32, alpha: f32) -> Color32 {
    let mix = |b: u8, o: u8| (f32::from(b) * (1.0 - alpha) + f32::from(o) * alpha).round() as u8;
    Color32::from_rgb(mix(base.r(), over.r()), mix(base.g(), over.g()), mix(base.b(), over.b()))
}

/// Material 3 state layer opacities.
pub mod state {
    pub const HOVER: f32 = 0.08;
    pub const FOCUS: f32 = 0.10;
    pub const PRESSED: f32 = 0.10;
    pub const DISABLED_CONTAINER: f32 = 0.12;
    pub const DISABLED_CONTENT: f32 = 0.38;
}

/// Material 3 corner radius scale (in dp).
pub mod shape {
    use eframe::egui::Rounding;

    pub const EXTRA_SMALL: f32 = 4.0;
    pub const SMALL: f32 = 8.0;
    pub const MEDIUM: f32 = 12.0;
    pub const LARGE: f32 = 16.0;
    pub const EXTRA_LARGE: f32 = 28.0;
    // Material 3 Expressive additions.
    pub const LARGE_INCREASED: f32 = 20.0;
    pub const EXTRA_LARGE_INCREASED: f32 = 32.0;
    pub const EXTRA_EXTRA_LARGE: f32 = 48.0;

    /// Fully rounded ("stadium") corners. egui clamps this to half the shorter side.
    pub fn full() -> Rounding {
        Rounding::same(1000.0)
    }
}

/// Material 3 Expressive motion: components move on springs instead of fixed-time curves,
/// so shapes and positions can overshoot slightly and settle.
pub mod motion {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Spring {
        pub stiffness: f32,
        pub damping_ratio: f32,
    }

    /// Spatial springs move things (position, size, shape); they overshoot a little.
    pub const SPATIAL_FAST: Spring = Spring { stiffness: 1400.0, damping_ratio: 0.9 };
    pub const SPATIAL_DEFAULT: Spring = Spring { stiffness: 700.0, damping_ratio: 0.9 };
    pub const SPATIAL_SLOW: Spring = Spring { stiffness: 300.0, damping_ratio: 0.9 };
    /// Effects springs change colour and opacity; they never overshoot.
    pub const EFFECTS_FAST: Spring = Spring { stiffness: 3800.0, damping_ratio: 1.0 };
    pub const EFFECTS_DEFAULT: Spring = Spring { stiffness: 1600.0, damping_ratio: 1.0 };

    /// Advances a unit-mass spring by `dt` seconds towards `target`; returns the new
    /// `(value, velocity)`. Integrated in small steps so it stays stable at any frame rate.
    pub fn step(spring: Spring, value: f32, velocity: f32, target: f32, dt: f32) -> (f32, f32) {
        let dt = dt.clamp(0.0, 0.05);
        let damping = 2.0 * spring.damping_ratio * spring.stiffness.sqrt();
        let steps = ((dt / 0.004).ceil() as u32).max(1);
        let h = dt / steps as f32;
        let (mut x, mut v) = (value, velocity);
        for _ in 0..steps {
            let acceleration = -spring.stiffness * (x - target) - damping * v;
            v += acceleration * h;
            x += v * h;
        }
        (x, v)
    }

    pub fn settled(value: f32, velocity: f32, target: f32) -> bool {
        (value - target).abs() < 0.001 && velocity.abs() < 0.01
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn run(spring: Spring, target: f32, seconds: f32) -> (f32, f32, f32) {
            let (mut value, mut velocity, mut peak) = (0.0_f32, 0.0_f32, 0.0_f32);
            let frames = (seconds * 60.0) as u32;
            for _ in 0..frames {
                (value, velocity) = step(spring, value, velocity, target, 1.0 / 60.0);
                peak = peak.max(value);
            }
            (value, velocity, peak)
        }

        #[test]
        fn springs_settle_on_the_target() {
            for spring in [SPATIAL_FAST, SPATIAL_DEFAULT, SPATIAL_SLOW, EFFECTS_FAST, EFFECTS_DEFAULT] {
                let (value, velocity, _) = run(spring, 1.0, 4.0);
                assert!(settled(value, velocity, 1.0), "{spring:?} ended at {value}");
            }
        }

        #[test]
        fn spatial_springs_overshoot_and_effects_springs_do_not() {
            assert!(run(SPATIAL_DEFAULT, 1.0, 2.0).2 > 1.0);
            assert!(run(EFFECTS_DEFAULT, 1.0, 2.0).2 <= 1.001);
        }

        #[test]
        fn a_huge_frame_time_does_not_blow_up() {
            let (value, velocity) = step(SPATIAL_FAST, 0.0, 0.0, 1.0, 10.0);
            assert!(value.is_finite() && velocity.is_finite() && value.abs() < 5.0);
        }
    }
}

/// Material 3 type scale roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    DisplayLarge,
    DisplayMedium,
    DisplaySmall,
    HeadlineLarge,
    HeadlineMedium,
    HeadlineSmall,
    TitleLarge,
    TitleMedium,
    TitleSmall,
    BodyLarge,
    BodyMedium,
    BodySmall,
    LabelLarge,
    LabelMedium,
    LabelSmall,
}

impl Role {
    pub const fn size(self) -> f32 {
        match self {
            Self::DisplayLarge => 57.0,
            Self::DisplayMedium => 45.0,
            Self::DisplaySmall => 36.0,
            Self::HeadlineLarge => 32.0,
            Self::HeadlineMedium => 28.0,
            Self::HeadlineSmall => 24.0,
            Self::TitleLarge => 22.0,
            Self::TitleMedium => 16.0,
            Self::TitleSmall => 14.0,
            Self::BodyLarge => 16.0,
            Self::BodyMedium => 14.0,
            Self::LabelLarge => 14.0,
            Self::BodySmall | Self::LabelMedium => 12.0,
            Self::LabelSmall => 11.0,
        }
    }

    pub fn font(self) -> FontId {
        FontId::proportional(self.size())
    }
}

/// Text in a type scale role. Colour defaults to the theme's text colour.
pub fn text(role: Role, content: impl Into<String>) -> RichText {
    RichText::new(content).size(role.size())
}

/// Material 3 window size classes by width in dp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSize {
    Compact,
    Medium,
    Expanded,
}

impl WindowSize {
    pub fn from_width(width: f32) -> Self {
        if width < 600.0 {
            Self::Compact
        } else if width < 840.0 {
            Self::Medium
        } else {
            Self::Expanded
        }
    }

    /// Page side margin: 16dp on compact windows, 24dp otherwise.
    pub fn margin(self) -> f32 {
        if self == Self::Compact { 16.0 } else { 24.0 }
    }
}

/// Installs the Material 3 styles for both the light and the dark theme.
/// Which one is shown follows the theme preference (system by default).
pub fn apply(ctx: &Context) {
    for (theme, dark) in [(Theme::Dark, true), (Theme::Light, false)] {
        ctx.style_mut_of(theme, |style| configure(style, dark));
    }
}

fn configure(style: &mut Style, dark: bool) {
    let c = colors_for(dark);
    style.visuals = visuals(c, dark);
    style.spacing.item_spacing = vec2(8.0, 8.0);
    style.spacing.button_padding = vec2(16.0, 8.0);
    style.spacing.interact_size = vec2(40.0, 40.0);
    style.animation_time = 0.2;
    style.text_styles = [
        (TextStyle::Small, Role::BodySmall.font()),
        (TextStyle::Body, Role::BodyMedium.font()),
        (TextStyle::Button, Role::LabelLarge.font()),
        (TextStyle::Heading, Role::HeadlineSmall.font()),
        (TextStyle::Monospace, FontId::monospace(14.0)),
    ]
    .into();
}

fn visuals(c: &ColorScheme, dark: bool) -> Visuals {
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    let corner = Rounding::same(shape::SMALL);

    v.panel_fill = c.surface;
    v.window_fill = c.surface_container_high;
    v.window_stroke = Stroke::NONE;
    v.window_rounding = Rounding::same(shape::EXTRA_LARGE);
    v.menu_rounding = Rounding::same(shape::EXTRA_SMALL);
    let shadow = Shadow {
        offset: vec2(0.0, 2.0),
        blur: 8.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if dark { 110 } else { 50 }),
    };
    v.window_shadow = shadow;
    v.popup_shadow = shadow;
    v.extreme_bg_color = c.surface_container_highest;
    v.faint_bg_color = c.surface_container_low;
    v.code_bg_color = c.surface_container_high;
    v.hyperlink_color = c.primary;
    v.error_fg_color = c.error;
    v.slider_trailing_fill = true;
    v.selection.bg_fill = c.primary_container;
    v.selection.stroke = Stroke::new(1.0_f32, c.primary);
    v.text_cursor.stroke = Stroke::new(2.0_f32, c.primary);

    // Text and frames.
    v.widgets.noninteractive.bg_fill = c.surface;
    v.widgets.noninteractive.weak_bg_fill = c.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, c.outline_variant);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, c.on_surface);
    v.widgets.noninteractive.rounding = corner;

    // Buttons and fields at rest: transparent, with the Material state layers on top.
    v.widgets.inactive.bg_fill = c.surface_container_highest;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, c.on_surface);
    v.widgets.inactive.rounding = corner;
    v.widgets.inactive.expansion = 0.0;

    let hover_weak = blend(c.surface, c.on_surface, state::HOVER);
    v.widgets.hovered.bg_fill = blend(c.surface_container_highest, c.on_surface, state::HOVER);
    v.widgets.hovered.weak_bg_fill = hover_weak;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, c.on_surface);
    v.widgets.hovered.rounding = corner;
    v.widgets.hovered.expansion = 0.0;

    let pressed_weak = blend(c.surface, c.on_surface, state::PRESSED);
    v.widgets.active.bg_fill = blend(c.surface_container_highest, c.on_surface, state::PRESSED);
    v.widgets.active.weak_bg_fill = pressed_weak;
    v.widgets.active.bg_stroke = Stroke::NONE;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, c.on_surface);
    v.widgets.active.rounding = corner;
    v.widgets.active.expansion = 0.0;

    v.widgets.open = v.widgets.active;
    v
}

// ---- Theme preference persistence -------------------------------------------------

pub fn preference_to_str(preference: ThemePreference) -> &'static str {
    match preference {
        ThemePreference::System => "system",
        ThemePreference::Light => "light",
        ThemePreference::Dark => "dark",
    }
}

pub fn preference_from_str(value: &str) -> ThemePreference {
    match value.trim() {
        "light" => ThemePreference::Light,
        "dark" => ThemePreference::Dark,
        _ => ThemePreference::System,
    }
}

/// The next preference in the System -> Light -> Dark cycle.
pub fn next_preference(preference: ThemePreference) -> ThemePreference {
    match preference {
        ThemePreference::System => ThemePreference::Light,
        ThemePreference::Light => ThemePreference::Dark,
        ThemePreference::Dark => ThemePreference::System,
    }
}

fn preference_path() -> Option<PathBuf> {
    Some(crate::library::default_path()?.with_file_name("theme.txt"))
}

pub fn load_preference() -> ThemePreference {
    preference_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| preference_from_str(&text))
        .unwrap_or(ThemePreference::System)
}

pub fn save_preference(preference: ThemePreference) {
    if let Some(path) = preference_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, preference_to_str(preference));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color32) -> f64 {
        let channel = |value: u8| {
            let c = f64::from(value) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn text_roles_are_readable_on_their_containers() {
        for (name, c) in [("dark", &DARK), ("light", &LIGHT)] {
            let pairs = [
                ("on_primary/primary", c.on_primary, c.primary),
                ("on_primary_container", c.on_primary_container, c.primary_container),
                ("on_secondary_container", c.on_secondary_container, c.secondary_container),
                ("on_tertiary_container", c.on_tertiary_container, c.tertiary_container),
                ("on_error/error", c.on_error, c.error),
                ("on_error_container", c.on_error_container, c.error_container),
                ("on_surface/surface", c.on_surface, c.surface),
                ("on_surface/container_low", c.on_surface, c.surface_container_low),
                ("on_surface/container", c.on_surface, c.surface_container),
                ("on_surface/container_high", c.on_surface, c.surface_container_high),
                ("on_surface/container_highest", c.on_surface, c.surface_container_highest),
                ("on_surface_variant/surface", c.on_surface_variant, c.surface),
                ("on_surface_variant/container_high", c.on_surface_variant, c.surface_container_high),
                ("on_surface_variant/container_highest", c.on_surface_variant, c.surface_container_highest),
                ("inverse_on_surface/inverse_surface", c.inverse_on_surface, c.inverse_surface),
            ];
            for (label, foreground, background) in pairs {
                let ratio = contrast(foreground, background);
                assert!(ratio >= 4.5, "{name} {label}: contrast {ratio:.2} is below 4.5");
            }
        }
    }

    #[test]
    fn outline_is_visible_against_the_surface() {
        for c in [&DARK, &LIGHT] {
            assert!(contrast(c.outline, c.surface) >= 3.0);
        }
    }

    #[test]
    fn window_size_classes_use_material_breakpoints() {
        assert_eq!(WindowSize::from_width(360.0), WindowSize::Compact);
        assert_eq!(WindowSize::from_width(599.9), WindowSize::Compact);
        assert_eq!(WindowSize::from_width(600.0), WindowSize::Medium);
        assert_eq!(WindowSize::from_width(839.9), WindowSize::Medium);
        assert_eq!(WindowSize::from_width(840.0), WindowSize::Expanded);
        assert_eq!(WindowSize::Compact.margin(), 16.0);
        assert_eq!(WindowSize::Expanded.margin(), 24.0);
    }

    #[test]
    fn blend_and_alpha_behave() {
        let base = Color32::from_rgb(0, 0, 0);
        let over = Color32::from_rgb(200, 100, 50);
        assert_eq!(blend(base, over, 0.0), base);
        assert_eq!(blend(base, over, 1.0), over);
        assert_eq!(blend(base, over, 0.5), Color32::from_rgb(100, 50, 25));
        assert_eq!(with_alpha(over, 0.0).a(), 0);
        assert_eq!(with_alpha(over, 1.0).a(), 255);
    }

    #[test]
    fn theme_preference_round_trips_and_cycles() {
        for preference in [ThemePreference::System, ThemePreference::Light, ThemePreference::Dark] {
            assert_eq!(preference_from_str(preference_to_str(preference)), preference);
        }
        assert_eq!(preference_from_str("garbage"), ThemePreference::System);
        assert_eq!(next_preference(ThemePreference::System), ThemePreference::Light);
        assert_eq!(next_preference(ThemePreference::Light), ThemePreference::Dark);
        assert_eq!(next_preference(ThemePreference::Dark), ThemePreference::System);
    }

    #[test]
    fn type_scale_matches_material_sizes() {
        assert_eq!(Role::DisplayLarge.size(), 57.0);
        assert_eq!(Role::HeadlineMedium.size(), 28.0);
        assert_eq!(Role::TitleLarge.size(), 22.0);
        assert_eq!(Role::BodyLarge.size(), 16.0);
        assert_eq!(Role::BodyMedium.size(), 14.0);
        assert_eq!(Role::LabelSmall.size(), 11.0);
    }
}
