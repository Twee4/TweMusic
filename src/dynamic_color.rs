//! Dynamic colour: a Material 3 colour scheme derived from the current album art.
//!
//! The dominant hue of the artwork seeds a "tonal spot" style scheme, like Material's own
//! dynamic colour. Material builds its palettes in HCT; this module builds them in OKLab and
//! then solves each colour to the exact CIE L* ("tone") Material specifies for that role, so
//! every on-X/X pair keeps the contrast Material guarantees. Hues and chromas are close to, but
//! not identical to, Google's reference output.
//!
//! The approach (seed colour, tonal palettes, spring-animated change) follows the idea in
//! ShinChven/m3-expressive's `@m3-expressive/core`; no code is copied.

use eframe::egui::Color32;

use crate::theme::{self, ColorScheme, DARK, LIGHT, motion};

/// A (light, dark) pair of schemes.
pub type Pair = (ColorScheme, ColorScheme);

// ---- colour maths -------------------------------------------------------------------------

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// Linear sRGB to OKLab (Björn Ottosson's published matrices).
fn linear_to_oklab([r, g, b]: [f32; 3]) -> [f32; 3] {
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn oklab_to_linear([l, a, b]: [f32; 3]) -> [f32; 3] {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

fn luminance([r, g, b]: [f32; 3]) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Relative luminance (0..1) that corresponds to CIE L* `tone` (0..100).
fn y_from_tone(tone: f32) -> f32 {
    if tone > 8.0 { ((tone + 16.0) / 116.0).powi(3) } else { tone / (24389.0 / 27.0) }
}

fn in_gamut(rgb: [f32; 3]) -> bool {
    rgb.iter().all(|channel| (-0.0005..=1.0005).contains(channel))
}

/// Finds the OKLab lightness whose colour at (`hue`, `chroma`) has luminance `y`.
fn solve_lightness(hue: f32, chroma: f32, y: f32) -> [f32; 3] {
    let (a, b) = (chroma * hue.cos(), chroma * hue.sin());
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    let mut rgb = oklab_to_linear([0.5, a, b]);
    for _ in 0..40 {
        let mid = (low + high) / 2.0;
        rgb = oklab_to_linear([mid, a, b]);
        if luminance(rgb) < y { low = mid } else { high = mid }
    }
    rgb
}

/// The colour at CIE L* `tone` with OKLab `hue` (radians). `chroma` is a wish: when it does not
/// fit in sRGB at that tone it is reduced until it does, and the tone is never changed.
fn tone_color(hue: f32, chroma: f32, tone: f32) -> Color32 {
    let y = y_from_tone(tone.clamp(0.0, 100.0));
    let mut rgb = solve_lightness(hue, chroma, y);
    if !in_gamut(rgb) {
        let (mut low, mut high) = (0.0_f32, chroma);
        for _ in 0..24 {
            let mid = (low + high) / 2.0;
            if in_gamut(solve_lightness(hue, mid, y)) { low = mid } else { high = mid }
        }
        rgb = solve_lightness(hue, low, y);
    }
    let [r, g, b] = rgb.map(|channel| (linear_to_srgb(channel) * 255.0).round() as u8);
    Color32::from_rgb(r, g, b)
}

// ---- scheme -------------------------------------------------------------------------------

/// OKLab chroma for Material's HCT chroma values (36, 16, 24, 6 and 8 in "tonal spot").
const PRIMARY_CHROMA: f32 = 0.12;
const SECONDARY_CHROMA: f32 = 0.054;
const TERTIARY_CHROMA: f32 = 0.08;
const NEUTRAL_CHROMA: f32 = 0.012;
const NEUTRAL_VARIANT_CHROMA: f32 = 0.016;
/// Material rotates the tertiary palette 60 degrees from the seed.
const TERTIARY_TURN: f32 = std::f32::consts::FRAC_PI_3;

/// Builds the light and dark scheme for a seed `hue` (OKLab radians, from `dominant_hue`).
pub fn scheme_from_hue(hue: f32) -> Pair {
    let p = |tone: f32| tone_color(hue, PRIMARY_CHROMA, tone);
    let s = |tone: f32| tone_color(hue, SECONDARY_CHROMA, tone);
    let t = |tone: f32| tone_color(hue + TERTIARY_TURN, TERTIARY_CHROMA, tone);
    let n = |tone: f32| tone_color(hue, NEUTRAL_CHROMA, tone);
    let nv = |tone: f32| tone_color(hue, NEUTRAL_VARIANT_CHROMA, tone);

    // The error roles keep Material's fixed red from the baseline schemes.
    let light = ColorScheme {
        primary: p(40.0),
        on_primary: p(100.0),
        primary_container: p(90.0),
        on_primary_container: p(10.0),
        secondary: s(40.0),
        on_secondary: s(100.0),
        secondary_container: s(90.0),
        on_secondary_container: s(10.0),
        tertiary: t(40.0),
        on_tertiary: t(100.0),
        tertiary_container: t(90.0),
        on_tertiary_container: t(10.0),
        surface: n(98.0),
        on_surface: n(10.0),
        surface_variant: nv(90.0),
        on_surface_variant: nv(30.0),
        outline: nv(50.0),
        outline_variant: nv(80.0),
        inverse_surface: n(20.0),
        inverse_on_surface: n(95.0),
        inverse_primary: p(80.0),
        surface_container_lowest: n(100.0),
        surface_container_low: n(96.0),
        surface_container: n(94.0),
        surface_container_high: n(92.0),
        surface_container_highest: n(90.0),
        ..LIGHT
    };
    let dark = ColorScheme {
        primary: p(80.0),
        on_primary: p(20.0),
        primary_container: p(30.0),
        on_primary_container: p(90.0),
        secondary: s(80.0),
        on_secondary: s(20.0),
        secondary_container: s(30.0),
        on_secondary_container: s(90.0),
        tertiary: t(80.0),
        on_tertiary: t(20.0),
        tertiary_container: t(30.0),
        on_tertiary_container: t(90.0),
        surface: n(6.0),
        on_surface: n(90.0),
        surface_variant: nv(30.0),
        on_surface_variant: nv(80.0),
        outline: nv(60.0),
        outline_variant: nv(30.0),
        inverse_surface: n(90.0),
        inverse_on_surface: n(20.0),
        inverse_primary: p(40.0),
        surface_container_lowest: n(4.0),
        surface_container_low: n(10.0),
        surface_container: n(12.0),
        surface_container_high: n(17.0),
        surface_container_highest: n(22.0),
        ..DARK
    };
    (light, dark)
}

// ---- artwork ------------------------------------------------------------------------------

const HUE_BINS: usize = 24;
/// Pixels this grey carry no hue worth following.
const MIN_CHROMA: f32 = 0.03;
/// If less than this share of the artwork is colourful, it is treated as greyscale.
const MIN_COLORFULNESS: f32 = 0.02;
const MAX_ART_BYTES: usize = 8 * 1024 * 1024;

/// The dominant hue (OKLab radians) of pixels, weighted by how colourful they are, or `None` for
/// greyscale artwork. Takes RGB triples.
pub fn dominant_hue(pixels: impl IntoIterator<Item = [u8; 3]>) -> Option<f32> {
    let mut weight = [0.0_f32; HUE_BINS];
    let mut sin_sum = [0.0_f32; HUE_BINS];
    let mut cos_sum = [0.0_f32; HUE_BINS];
    let mut count = 0_u32;
    for [r, g, b] in pixels {
        count += 1;
        let lab = linear_to_oklab([r, g, b].map(|v| srgb_to_linear(f32::from(v) / 255.0)));
        // Near-black and near-white pixels have an unreliable hue.
        if !(0.12..=0.96).contains(&lab[0]) {
            continue;
        }
        let chroma = lab[1].hypot(lab[2]);
        if chroma < MIN_CHROMA {
            continue;
        }
        let hue = lab[2].atan2(lab[1]);
        let bin = (((hue + std::f32::consts::PI) / std::f32::consts::TAU) * HUE_BINS as f32) as usize % HUE_BINS;
        weight[bin] += chroma;
        sin_sum[bin] += hue.sin() * chroma;
        cos_sum[bin] += hue.cos() * chroma;
    }
    if count == 0 {
        return None;
    }
    let (best, best_weight) = weight
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    if best_weight / (count as f32) < MIN_COLORFULNESS {
        return None;
    }
    Some(sin_sum[best].atan2(cos_sum[best]))
}

/// Decodes artwork and returns its dominant hue.
pub fn hue_from_image_bytes(bytes: &[u8]) -> Option<f32> {
    let image = image::load_from_memory(bytes).ok()?;
    let small = image.thumbnail(48, 48).to_rgb8();
    dominant_hue(small.pixels().map(|pixel| pixel.0))
}

/// Downloads the artwork at `url` (YouTube image hosts only) and returns its dominant hue.
/// Blocking; call from a worker thread.
pub fn hue_from_url(url: &str) -> Option<f32> {
    if !crate::ytmusic::is_allowed_thumbnail_url(url) {
        return None;
    }
    let http = twe_innertube::transport::http_client().ok()?;
    let response = http.get(url).send().ok()?.error_for_status().ok()?;
    if response.content_length().is_some_and(|length| length as usize > MAX_ART_BYTES) {
        return None;
    }
    let bytes = response.bytes().ok()?;
    if bytes.len() > MAX_ART_BYTES {
        return None;
    }
    hue_from_image_bytes(&bytes)
}

// ---- animated change ----------------------------------------------------------------------

/// Colours move on a calm, non-overshooting spring: a new song takes about half a second to
/// settle, which reads as the app "taking on" the artwork rather than a flash.
const COLOR_SPRING: motion::Spring = motion::Spring { stiffness: 220.0, damping_ratio: 1.0 };

/// Blends the scheme in use towards a new target.
pub struct Transition {
    from: Pair,
    to: Pair,
    progress: f32,
    velocity: f32,
}

impl Transition {
    pub fn new() -> Self {
        Self { from: (LIGHT, DARK), to: (LIGHT, DARK), progress: 1.0, velocity: 0.0 }
    }

    /// The scheme to draw with right now.
    pub fn current(&self) -> Pair {
        let t = self.progress.clamp(0.0, 1.0);
        (self.from.0.mix(&self.to.0, t), self.from.1.mix(&self.to.1, t))
    }

    /// Starts moving towards the scheme for `hue`, or back to the baseline for `None`.
    pub fn retarget(&mut self, hue: Option<f32>) {
        self.from = self.current();
        self.to = hue.map_or((LIGHT, DARK), scheme_from_hue);
        self.progress = 0.0;
        self.velocity = 0.0;
    }

    /// Advances by `dt` seconds. Returns true if the colours changed (so the caller should
    /// apply `current()` and repaint).
    pub fn step(&mut self, dt: f32) -> bool {
        if self.progress >= 1.0 {
            return false;
        }
        let (value, velocity) = motion::step(COLOR_SPRING, self.progress, self.velocity, 1.0, dt);
        if motion::settled(value, velocity, 1.0) {
            self.progress = 1.0;
            self.velocity = 0.0;
        } else {
            self.progress = value;
            self.velocity = velocity;
        }
        true
    }
}

impl Default for Transition {
    fn default() -> Self {
        Self::new()
    }
}

/// Makes `pair` the scheme every widget reads from `theme::colors`.
pub fn install(pair: Pair) {
    theme::set_dynamic(Some(pair));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lum(color: Color32) -> f32 {
        let [r, g, b] = [color.r(), color.g(), color.b()].map(|v| srgb_to_linear(f32::from(v) / 255.0));
        luminance([r, g, b])
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (hi, lo) = (lum(a).max(lum(b)), lum(a).min(lum(b)));
        (hi + 0.05) / (lo + 0.05)
    }

    /// CIE L* of a colour.
    fn tone_of(color: Color32) -> f32 {
        let y = lum(color);
        if y > 216.0 / 24389.0 { 116.0 * y.cbrt() - 16.0 } else { y * 24389.0 / 27.0 }
    }

    #[test]
    fn tones_are_exact_for_any_hue() {
        for step in 0..24 {
            let hue = step as f32 / 24.0 * std::f32::consts::TAU - std::f32::consts::PI;
            for tone in [10.0, 30.0, 40.0, 60.0, 80.0, 90.0, 98.0] {
                let color = tone_color(hue, PRIMARY_CHROMA, tone);
                assert!((tone_of(color) - tone).abs() < 1.5, "hue {hue} tone {tone} gave {}", tone_of(color));
            }
        }
    }

    #[test]
    fn role_pairs_stay_readable_for_any_hue() {
        for step in 0..24 {
            let hue = step as f32 / 24.0 * std::f32::consts::TAU - std::f32::consts::PI;
            let (light, dark) = scheme_from_hue(hue);
            for (name, c) in [("light", light), ("dark", dark)] {
                let pairs = [
                    ("primary", c.on_primary, c.primary),
                    ("primary container", c.on_primary_container, c.primary_container),
                    ("secondary container", c.on_secondary_container, c.secondary_container),
                    ("tertiary container", c.on_tertiary_container, c.tertiary_container),
                    ("surface", c.on_surface, c.surface),
                    ("surface container", c.on_surface, c.surface_container_highest),
                    ("surface variant", c.on_surface_variant, c.surface_variant),
                    ("inverse", c.inverse_on_surface, c.inverse_surface),
                ];
                for (role, on, background) in pairs {
                    let ratio = contrast(on, background);
                    assert!(ratio >= 4.5, "{name} {role} at hue {hue}: contrast {ratio}");
                }
            }
        }
    }

    #[test]
    fn finds_the_dominant_hue_and_ignores_grey() {
        // Mostly blue pixels with a little red.
        let mut pixels = vec![[30, 80, 220]; 80];
        pixels.extend(vec![[200, 30, 30]; 5]);
        let blue = linear_to_oklab([30_u8, 80, 220].map(|v| srgb_to_linear(f32::from(v) / 255.0)));
        let expected = blue[2].atan2(blue[1]);
        let found = dominant_hue(pixels).expect("blue artwork has a hue");
        assert!((found - expected).abs() < 0.3, "{found} vs {expected}");
        assert_eq!(dominant_hue(vec![[128, 128, 128]; 50]), None);
        assert_eq!(dominant_hue(Vec::<[u8; 3]>::new()), None);
    }

    #[test]
    fn transition_settles_on_the_target() {
        let mut transition = Transition::new();
        transition.retarget(Some(1.0));
        let target = scheme_from_hue(1.0);
        let mut frames = 0;
        while transition.step(1.0 / 60.0) {
            frames += 1;
            assert!(frames < 600, "never settled");
        }
        assert_eq!(transition.current().0.primary, target.0.primary);
        assert_eq!(transition.current().1.surface, target.1.surface);
    }
}
