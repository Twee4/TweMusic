//! Colour roles for the interface (Material 3 baseline scheme, seed #6750A4, light and dark),
//! the album-art schemes that replace them, and the spring maths used to fade between them.
//! Every "on-X" role is meant to be drawn on its "X" role; the tests check that these pairs stay
//! readable.

use std::sync::RwLock;

/// An opaque sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color32([u8; 3]);

impl Color32 {
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b])
    }
    pub const fn r(self) -> u8 {
        self.0[0]
    }
    pub const fn g(self) -> u8 {
        self.0[1]
    }
    pub const fn b(self) -> u8 {
        self.0[2]
    }
}

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

/// Schemes derived from the album art (light, dark), when dynamic colour is active.
static DYNAMIC: RwLock<Option<(ColorScheme, ColorScheme)>> = RwLock::new(None);

/// Replaces the baseline schemes with `pair` (light, dark), or restores them with `None`.
pub fn set_dynamic(pair: Option<(ColorScheme, ColorScheme)>) {
    if let Ok(mut slot) = DYNAMIC.write() {
        *slot = pair;
    }
}

/// The colour scheme in use for light or dark mode.
pub fn colors_for(dark: bool) -> ColorScheme {
    let dynamic = DYNAMIC.read().ok().and_then(|slot| *slot);
    match (dynamic, dark) {
        (Some((_, dark_scheme)), true) => dark_scheme,
        (Some((light_scheme, _)), false) => light_scheme,
        (None, true) => DARK,
        (None, false) => LIGHT,
    }
}

impl ColorScheme {
    /// This scheme blended towards `other` by `t` (0 = this, 1 = other), role by role.
    pub fn mix(&self, other: &ColorScheme, t: f32) -> ColorScheme {
        let t = t.clamp(0.0, 1.0);
        macro_rules! mixed {
            ($($field:ident),* $(,)?) => {
                ColorScheme { $($field: blend(self.$field, other.$field, t)),* }
            };
        }
        mixed!(
            primary, on_primary, primary_container, on_primary_container,
            secondary, on_secondary, secondary_container, on_secondary_container,
            tertiary, on_tertiary, tertiary_container, on_tertiary_container,
            error, on_error, error_container, on_error_container,
            surface, on_surface, surface_variant, on_surface_variant,
            outline, outline_variant, inverse_surface, inverse_on_surface, inverse_primary,
            surface_container_lowest, surface_container_low, surface_container,
            surface_container_high, surface_container_highest,
        )
    }
}

/// `over` painted at `alpha` on top of an opaque `base`.
pub fn blend(base: Color32, over: Color32, alpha: f32) -> Color32 {
    let mix = |b: u8, o: u8| (f32::from(b) * (1.0 - alpha) + f32::from(o) * alpha).round() as u8;
    Color32::from_rgb(mix(base.r(), over.r()), mix(base.g(), over.g()), mix(base.b(), over.b()))
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
    fn blend_mixes_colours() {
        let base = Color32::from_rgb(0, 0, 0);
        let over = Color32::from_rgb(200, 100, 50);
        assert_eq!(blend(base, over, 0.0), base);
        assert_eq!(blend(base, over, 1.0), over);
        assert_eq!(blend(base, over, 0.5), Color32::from_rgb(100, 50, 25));
    }


}
