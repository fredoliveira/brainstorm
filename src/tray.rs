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

/// A placeholder until the app has an icon of its own: three cards fanned side by side, for
/// the three that Brainstorm draws. White with the shape in the alpha channel, which is all a
/// template image uses.
fn icon() -> Result<Icon, String> {
    // 22pt at 2x; coordinates are in those pixels, y down.
    const SIZE: u32 = 44;
    // Each card: left edge, top edge. The middle one sits higher.
    const CARDS: [(f32, f32); 3] = [(5., 13.), (17., 9.), (29., 13.)];
    const WIDTH: f32 = 10.;
    const HEIGHT: f32 = 22.;
    const RADIUS: f32 = 2.5;
    // Inside a rounded rectangle: within the radius of the rectangle shrunk by that radius.
    let inside = |x: f32, y: f32| {
        CARDS.iter().any(|&(left, top)| {
            let dx = (x - (left + WIDTH / 2.)).abs() - (WIDTH / 2. - RADIUS);
            let dy = (y - (top + HEIGHT / 2.)).abs() - (HEIGHT / 2. - RADIUS);
            dx.max(0.).hypot(dy.max(0.)) < RADIUS
        })
    };

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
