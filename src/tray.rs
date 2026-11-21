//! The menu-bar icon. GPUI has no status-item API, so this uses the `tray-icon` crate on
//! GPUI's own `NSApplication`. Menu choices come back as [`Signal::Menu`] on the same channel
//! as everything else, so they are handled on the main thread with the app in hand.

use smol::channel::Sender;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::feed::Signal;

pub const OPEN: &str = "open";
pub const SETTINGS: &str = "settings";
pub const QUIT: &str = "quit";

/// Must be called on the main thread, once the app has launched. Keep the returned icon
/// alive: dropping it removes it from the menu bar.
pub fn install(signals: Sender<Signal>) -> Result<TrayIcon, String> {
    let menu = Menu::new();
    menu.append_items(&[
        &MenuItem::with_id(OPEN, "Open Brainstorm", true, None),
        &MenuItem::with_id(SETTINGS, "Settings…", true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(QUIT, "Quit Brainstorm", true, None),
    ])
    .map_err(|e| e.to_string())?;

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = signals.try_send(Signal::Menu(event.id.0));
    }));

    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(concat!("Brainstorm v", env!("CARGO_PKG_VERSION")))
        // A template image: macOS draws it in the menu bar's own colour.
        .with_icon_templated(icon()?)
        .build()
        .map_err(|e| e.to_string())
}

/// A thinker in a peaked hood, eyes low on her face, with three lights above her: the app
/// icon's figure, and the three cards that Brainstorm draws. White with the shape in the alpha
/// channel, which is all a template image uses.
fn icon() -> Result<Icon, String> {
    // 22pt at 2x. The shapes are drawn on a 22-unit grid, y down, so one unit is two pixels.
    const SIZE: u32 = 44;
    const UNIT: f32 = SIZE as f32 / 22.;
    let inside = |x: f32, y: f32| shape(x / UNIT, y / UNIT);

    // Coverage from a 4×4 grid of samples per pixel, for antialiased corners.
    const SAMPLES: u32 = 4;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let at = |p: u32, s: u32| p as f32 + (s as f32 + 0.5) / SAMPLES as f32;
                    hits += u32::from(inside(at(x, sx), at(y, sy)));
                }
            }
            let alpha = (hits * 255 + SAMPLES * SAMPLES / 2) / (SAMPLES * SAMPLES);
            rgba.extend_from_slice(&[255, 255, 255, alpha as u8]);
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).map_err(|e| e.to_string())
}

/// The hood's left edge, one cubic Bézier from the bottom-left corner up to the peak. The right
/// edge is its mirror image across x = 11.
const HOOD: [(f32, f32); 4] = [(3.5, 21.5), (3.5, 13.5), (6.3, 9.), (11., 6.4)];
/// The face cut out of the hood: centre and radii of an ellipse.
const FACE: (f32, f32, f32, f32) = (11., 15.2, 3.3, 4.3);
/// Two small upright eyes, low on the face.
const EYES: [(f32, f32, f32, f32); 2] = [(9.7, 16.6, 0.65, 0.9), (12.3, 16.6, 0.65, 0.9)];
/// The three lights: centre and radius.
const LIGHTS: [(f32, f32, f32); 3] = [(3.6, 7.4, 1.8), (11., 2.4, 1.8), (18.4, 7.4, 1.8)];

/// Whether a point on the 22-unit grid is inside the icon's shape.
fn shape(x: f32, y: f32) -> bool {
    let ellipse = |(cx, cy, rx, ry): (f32, f32, f32, f32)| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) < 1.;
    let hood = y <= HOOD[0].1 && y >= HOOD[3].1 && (x - 11.).abs() <= 11. - hood_edge(y);
    (hood && !ellipse(FACE))
        || EYES.iter().any(|&eye| ellipse(eye))
        || LIGHTS.iter().any(|&(cx, cy, r)| (x - cx).hypot(y - cy) < r)
}

/// The x of the hood's left edge at height `y`. The curve only ever rises, so its y falls as t
/// grows and a bisection finds the t for `y`.
fn hood_edge(y: f32) -> f32 {
    let at = |t: f32, axis: fn((f32, f32)) -> f32| {
        let [p0, p1, p2, p3] = HOOD.map(axis);
        let u = 1. - t;
        u * u * u * p0 + 3. * u * u * t * p1 + 3. * u * t * t * p2 + t * t * t * p3
    };
    let (mut low, mut high) = (0f32, 1f32);
    for _ in 0..24 {
        let mid = (low + high) / 2.;
        if at(mid, |p| p.1) > y {
            low = mid;
        } else {
            high = mid;
        }
    }
    at((low + high) / 2., |p| p.0)
}

#[cfg(test)]
mod tests {
    use super::{HOOD, hood_edge, shape};

    #[test]
    fn hood_edge_runs_from_corner_to_peak() {
        assert!((hood_edge(HOOD[0].1) - HOOD[0].0).abs() < 0.01);
        assert!((hood_edge(HOOD[3].1) - HOOD[3].0).abs() < 0.01);
    }

    #[test]
    fn eyes_sit_in_the_face_cut_out_of_the_hood() {
        assert!(shape(11., 9.)); // the hood above the face
        assert!(!shape(11., 13.)); // the forehead, cut out
        assert!(shape(9.7, 16.6) && shape(12.3, 16.6)); // the eyes
        assert!(!shape(11., 16.6)); // between them
        assert!(shape(11., 2.4)); // the middle light
        assert!(!shape(11., 4.6)); // the gap between it and the peak
    }
}
