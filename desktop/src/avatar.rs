//! The round device pictures in the tray menu: a coral circle with a white
//! phone, tablet or computer, and a green dot when the device is online.
//! Drawn in code, so names alone decide the picture and no assets ship.

/// Pixels on a side. The menu shows them at 18 points on macOS, so this is
/// sharp on Retina screens, and Windows scales them down to 16 pixels.
pub const SIZE: u32 = 36;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Phone,
    Tablet,
    Computer,
    /// The grey "Add a Device" picture.
    Add,
}

impl Kind {
    /// Guesses from the device's name, as the window does.
    pub fn of(name: &str) -> Self {
        let name = name.to_lowercase();
        if name.contains("ipad") {
            Kind::Tablet
        } else if name.contains("iphone") || name.contains("ipod") {
            Kind::Phone
        } else {
            Kind::Computer
        }
    }
}

type Rgba = [f32; 4];

/// #FF9158 to #F0533C, as in the iPhone app.
const CORAL_TOP: Rgba = [1.0, 0.569, 0.345, 1.0];
const CORAL_BOTTOM: Rgba = [0.941, 0.325, 0.235, 1.0];
const SCREEN: Rgba = [1.0, 0.76, 0.65, 1.0];
const GREY: Rgba = [0.88, 0.88, 0.89, 1.0];
const GREY_GLYPH: Rgba = [0.42, 0.42, 0.45, 1.0];
const WHITE: Rgba = [1.0, 1.0, 1.0, 1.0];
const GREEN: Rgba = [0.20, 0.78, 0.35, 1.0];

/// The picture as RGBA bytes, `SIZE` by `SIZE`.
pub fn draw(kind: Kind, online: bool) -> Vec<u8> {
    // Each pixel averages a 4×4 grid of samples, for smooth edges.
    const SAMPLES: u32 = 4;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut sum = [0.0f32; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    // In units of the picture: 0 to 1 across.
                    let u = (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / SIZE as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / SIZE as f32;
                    let color = sample(kind, online, u, v);
                    // Premultiplied, so transparent samples don't darken edges.
                    for channel in 0..3 {
                        sum[channel] += color[channel] * color[3];
                    }
                    sum[3] += color[3];
                }
            }
            let count = (SAMPLES * SAMPLES) as f32;
            let alpha = sum[3] / count;
            for channel in 0..3 {
                let value = if sum[3] > 0.0 {
                    sum[channel] / sum[3]
                } else {
                    0.0
                };
                pixels.push((value * 255.0).round() as u8);
            }
            pixels.push((alpha * 255.0).round() as u8);
        }
    }
    pixels
}

/// The color at one point, painting back to front.
fn sample(kind: Kind, online: bool, u: f32, v: f32) -> Rgba {
    let mut color = [0.0; 4];
    // The circle leaves room for the dot to overhang it slightly.
    let (cx, cy, r) = (0.47, 0.47, 0.46);
    if circle(u, v, cx, cy, r) {
        color = match kind {
            Kind::Add => GREY,
            _ => mix(CORAL_TOP, CORAL_BOTTOM, v),
        };
        if let Some(glyph) = glyph(kind, u - cx, v - cy) {
            color = glyph;
        }
    }
    if online {
        let (dx, dy, dr) = (0.80, 0.80, 0.17);
        if circle(u, v, dx, dy, dr) {
            color = WHITE;
        }
        if circle(u, v, dx, dy, dr - 0.05) {
            color = GREEN;
        }
    }
    color
}

/// The white device shape, relative to the circle's center.
fn glyph(kind: Kind, x: f32, y: f32) -> Option<Rgba> {
    match kind {
        Kind::Phone => device(x, y, 0.12, 0.22, 0.05, 0.03),
        Kind::Tablet => device(x, y, 0.17, 0.22, 0.04, 0.03),
        Kind::Computer => {
            // A monitor on a stand.
            let screen = device(x, y + 0.05, 0.25, 0.16, 0.03, 0.035);
            if screen.is_some() {
                return screen;
            }
            let neck = rounded_rect(x, y - 0.155, 0.035, 0.04, 0.0);
            let base = rounded_rect(x, y - 0.20, 0.12, 0.025, 0.02);
            (neck || base).then_some(WHITE)
        }
        Kind::Add => {
            let bar = 0.025;
            let arm = 0.17;
            let plus = rounded_rect(x, y, arm, bar, bar) || rounded_rect(x, y, bar, arm, bar);
            plus.then_some(GREY_GLYPH)
        }
    }
}

/// A white body with a lighter coral screen inset by `border`.
fn device(x: f32, y: f32, half_w: f32, half_h: f32, radius: f32, border: f32) -> Option<Rgba> {
    if !rounded_rect(x, y, half_w, half_h, radius) {
        return None;
    }
    let inner = rounded_rect(
        x,
        y,
        half_w - border,
        half_h - border,
        (radius - border).max(0.0),
    );
    Some(if inner { SCREEN } else { WHITE })
}

fn circle(u: f32, v: f32, cx: f32, cy: f32, r: f32) -> bool {
    (u - cx).powi(2) + (v - cy).powi(2) <= r * r
}

/// Whether a point is inside a rectangle centered on the origin.
fn rounded_rect(x: f32, y: f32, half_w: f32, half_h: f32, radius: f32) -> bool {
    let qx = (x.abs() - (half_w - radius)).max(0.0);
    let qy = (y.abs() - (half_h - radius)).max(0.0);
    x.abs() <= half_w && y.abs() <= half_h && qx * qx + qy * qy <= radius * radius
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], u: f32, v: f32) -> [u8; 4] {
        let (x, y) = ((u * SIZE as f32) as usize, (v * SIZE as f32) as usize);
        let at = (y * SIZE as usize + x) * 4;
        pixels[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn names_pick_pictures() {
        assert_eq!(Kind::of("Cole's iPhone 17 Pro"), Kind::Phone);
        assert_eq!(Kind::of("iPad"), Kind::Tablet);
        assert_eq!(Kind::of("MacBook Pro"), Kind::Computer);
        assert_eq!(Kind::of("Windows PC"), Kind::Computer);
    }

    #[test]
    fn online_devices_get_a_green_dot() {
        let online = draw(Kind::Phone, true);
        let away = draw(Kind::Phone, false);
        assert_eq!(online.len(), (SIZE * SIZE * 4) as usize);
        let green = pixel(&online, 0.80, 0.80);
        assert!(green[1] > 150 && green[0] < 100, "{green:?}");
        assert_ne!(pixel(&away, 0.80, 0.80), green);
        // Corners stay transparent, and the circle's edge is coral.
        assert_eq!(pixel(&away, 0.01, 0.01)[3], 0);
        let edge = pixel(&away, 0.47, 0.06);
        assert!(edge[0] > 200 && edge[2] < 150 && edge[3] == 255, "{edge:?}");
    }
}
