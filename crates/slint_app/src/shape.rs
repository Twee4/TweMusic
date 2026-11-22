//! Shapes for the play button, morphed by springing a ring of radii towards the next shape
//! (the Material 3 Expressive shape library: circle, cookies, burst, sunny, ...). Slint cannot
//! blend one path into another, so the blend is done here and the result is handed over as a path.

use std::{f32::consts::TAU, fmt::Write};

/// How many points make up the outline.
pub const POINTS: usize = 96;

/// A radial shape: `lobes` bumps of height `amplitude`, `sharpness` below 1 makes them pointier.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub lobes: u32,
    pub amplitude: f32,
    pub sharpness: f32,
}

pub const CIRCLE: Shape = Shape { lobes: 0, amplitude: 0.0, sharpness: 1.0 };
pub const COOKIE_9: Shape = Shape { lobes: 9, amplitude: 0.06, sharpness: 1.0 };
pub const COOKIE_6: Shape = Shape { lobes: 6, amplitude: 0.08, sharpness: 1.0 };
pub const COOKIE_4: Shape = Shape { lobes: 4, amplitude: 0.07, sharpness: 1.0 };
pub const SOFT_BURST: Shape = Shape { lobes: 12, amplitude: 0.10, sharpness: 0.7 };
pub const SUNNY: Shape = Shape { lobes: 8, amplitude: 0.12, sharpness: 0.6 };

/// The shapes the loader cycles through while a song loads.
pub const LOADER: [Shape; 5] = [COOKIE_9, SOFT_BURST, COOKIE_6, SUNNY, COOKIE_4];

/// A rounded square (a superellipse), the shape of the album art while it is paused.
pub fn squircle() -> [f32; POINTS] {
    let mut radii = [1.0; POINTS];
    for (i, radius) in radii.iter_mut().enumerate() {
        let angle = i as f32 / POINTS as f32 * TAU;
        *radius = 1.0 / (angle.cos().abs().powf(5.0) + angle.sin().abs().powf(5.0)).powf(0.2);
    }
    let widest = radii.iter().cloned().fold(0.0_f32, f32::max);
    for radius in &mut radii {
        *radius /= widest;
    }
    radii
}

impl Shape {
    /// The outline as distances from the centre, 1.0 at the widest point.
    pub fn radii(self) -> [f32; POINTS] {
        let mut radii = [1.0; POINTS];
        if self.lobes == 0 || self.amplitude == 0.0 {
            return radii;
        }
        for (i, radius) in radii.iter_mut().enumerate() {
            let angle = i as f32 / POINTS as f32 * TAU;
            let wave = (self.lobes as f32 * angle).cos();
            let bump = wave.signum() * wave.abs().powf(self.sharpness);
            *radius = (1.0 + self.amplitude * bump) / (1.0 + self.amplitude);
        }
        radii
    }
}

/// The shape on screen: it springs towards the target (damping 0.6, stiffness 380) and spins.
pub struct Morph {
    radii: [f32; POINTS],
    velocity: [f32; POINTS],
    rotation: f32,
}

impl Morph {
    pub fn new() -> Self {
        Self { radii: [1.0; POINTS], velocity: [0.0; POINTS], rotation: 0.0 }
    }

    pub fn step(&mut self, target: &[f32; POINTS], dt: f32, spin: f32) {
        const STIFFNESS: f32 = 380.0;
        const DAMPING: f32 = 2.0 * 0.6 * 19.493; // 2 * ratio * sqrt(stiffness)
        let dt = dt.clamp(0.0, 0.03);
        for i in 0..POINTS {
            self.velocity[i] += (STIFFNESS * (target[i] - self.radii[i]) - DAMPING * self.velocity[i]) * dt;
            self.radii[i] += self.velocity[i] * dt;
        }
        self.rotation = (self.rotation + spin * dt) % TAU;
    }

    /// True once the outline has come to rest on `target`.
    pub fn settled(&self, target: &[f32; POINTS]) -> bool {
        (0..POINTS).all(|i| (target[i] - self.radii[i]).abs() < 0.002 && self.velocity[i].abs() < 0.01)
    }

    /// Cuts the outline out of a square picture (`size` x `size` RGBA pixels): what lies outside
    /// becomes transparent, with a one pixel soft edge.
    pub fn mask(&self, rgba: &[u8], size: u32) -> Vec<u8> {
        let mut out = rgba.to_vec();
        let centre = size as f32 / 2.0;
        for y in 0..size {
            for x in 0..size {
                let (dx, dy) = (x as f32 + 0.5 - centre, y as f32 + 0.5 - centre);
                let angle = (dy.atan2(dx) - self.rotation).rem_euclid(TAU);
                let at = angle / TAU * POINTS as f32;
                let (low, fraction) = (at.floor() as usize % POINTS, at.fract());
                let radius = self.radii[low] + (self.radii[(low + 1) % POINTS] - self.radii[low]) * fraction;
                let coverage = (centre * radius - dx.hypot(dy) + 0.5).clamp(0.0, 1.0);
                let alpha = ((y * size + x) * 4 + 3) as usize;
                out[alpha] = (f32::from(out[alpha]) * coverage) as u8;
            }
        }
        out
    }

    /// The outline as path commands in a 100 x 100 box.
    pub fn path(&self) -> String {
        let mut path = String::with_capacity(POINTS * 14);
        for (i, radius) in self.radii.iter().enumerate() {
            let angle = i as f32 / POINTS as f32 * TAU + self.rotation;
            let reach = 46.0 * radius;
            let _ = write!(path, "{} {:.2} {:.2} ", if i == 0 { "M" } else { "L" }, 50.0 + reach * angle.cos(), 50.0 + reach * angle.sin());
        }
        path.push('Z');
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_is_round_and_cookies_stay_inside_the_box() {
        assert!(CIRCLE.radii().iter().all(|r| (*r - 1.0).abs() < 1e-6));
        for shape in LOADER {
            let radii = shape.radii();
            let widest = radii.iter().cloned().fold(0.0_f32, f32::max);
            let narrowest = radii.iter().cloned().fold(f32::MAX, f32::min);
            assert!(widest <= 1.0001 && widest > 0.99, "{shape:?}");
            assert!(narrowest > 0.7, "{shape:?}");
        }
    }

    #[test]
    fn morph_settles_on_the_target() {
        let mut morph = Morph::new();
        let target = COOKIE_9.radii();
        assert!(!morph.settled(&target));
        for _ in 0..200 {
            morph.step(&target, 0.016, 0.0);
        }
        assert!(morph.settled(&target));
    }

    #[test]
    fn mask_clears_the_corners_of_a_circle_and_keeps_the_middle() {
        let size = 32;
        let picture = vec![255_u8; (size * size * 4) as usize];
        let masked = Morph::new().mask(&picture, size);
        let alpha = |x: u32, y: u32| masked[((y * size + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(16, 16), 255);
    }

    #[test]
    fn squircle_stays_inside_the_box_and_reaches_the_corners() {
        let radii = squircle();
        let widest = radii.iter().cloned().fold(0.0_f32, f32::max);
        let narrowest = radii.iter().cloned().fold(f32::MAX, f32::min);
        assert!((widest - 1.0).abs() < 1e-5);
        assert!(narrowest > 0.75 && narrowest < 0.85);
    }

    #[test]
    fn path_is_closed() {
        let path = Morph::new().path();
        assert!(path.starts_with("M "));
        assert!(path.ends_with('Z'));
        assert_eq!(path.matches('L').count(), POINTS - 1);
    }
}
