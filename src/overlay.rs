//! The overlays' windows: when they show and where they go.
//!
//! Three GPUI pop-up windows, opened hidden at launch and never closed: the draft panel, the
//! deck panel, and the card preview that appears beside whichever row the pointer is on.
//! [`sync`] decides whether each shows and where. It runs when a panel's contents change
//! (the panels ask for it) and twice a second from `main.rs`, since nothing announces
//! Arena's window moving or another app coming to the front.
//!
//! A panel in the way of the game can be collapsed: its window shrinks to a [`tab`] against
//! Arena's right edge, and a click on that opens it again.

use std::path::PathBuf;

use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName};
use gpui_kit::*;

use crate::arena::{self, Arena};
use crate::deck_panel::DeckPanel;
use crate::draft::Draft;
use crate::draft_panel::DraftPanel;
use crate::game::Game;
use crate::native::Panel;
use crate::settings::Settings;
use crate::style;

/// When the overlays may show.
pub enum Visibility {
    /// While Arena is the app in front (the default).
    WithArena,
    /// Whenever there is something to show, beside Arena's window or else on the primary
    /// display (`BRAINSTORM_ALWAYS`, for working on the panels).
    Always,
    /// As `Always`, beside a made-up Arena window (`BRAINSTORM_WINDOW=WxH`).
    Fake(Arena),
}

impl Visibility {
    pub fn from_env() -> Self {
        let size = std::env::var("BRAINSTORM_WINDOW").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        });
        match size {
            Some((w, h)) => Visibility::Fake(arena::fake(w, h)),
            None if std::env::var_os("BRAINSTORM_ALWAYS").is_some() => Visibility::Always,
            None => Visibility::WithArena,
        }
    }

    /// The rectangle to dock to, or `None` when the overlays should be hidden.
    fn anchor(&self) -> Option<CGRect> {
        match self {
            Visibility::Fake(arena) => Some(arena.bounds),
            Visibility::Always => Some(arena::window().map_or_else(arena::primary_display, |a| a.bounds)),
            Visibility::WithArena => {
                let arena = arena::window()?;
                (arena::frontmost_pid() == Some(arena.pid)).then_some(arena.bounds)
            }
        }
    }
}

/// The card image shown beside a hovered row.
pub struct Preview {
    image: Option<PathBuf>,
}

impl Render for Preview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .children(self.image.clone().map(|path| img(path).size_full().rounded(px(style::PREVIEW_RADIUS))))
    }
}

struct Overlays {
    draft: (Panel, Entity<DraftPanel>),
    deck: (Panel, Entity<DeckPanel>),
    preview: (Panel, Entity<Preview>),
    visibility: Visibility,
}

impl Global for Overlays {}

/// What a panel's window should be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// The panel itself, this wide and tall.
    Open(f32, f32),
    /// Collapsed to a tab.
    Tab,
}

/// A collapsed panel: all of its window. The panel adds the click that opens it again.
pub fn tab() -> Stateful<Div> {
    div()
        .id("tab")
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        // Flush against Arena's edge on the right, so only the left corners are round.
        .rounded_l(px(style::TAB_RADIUS))
        .border_1()
        .border_color(rgba(style::BORDER))
        .bg(rgba(style::PANEL))
        .text_color(rgba(style::MUTED))
        .hover(|style| style.text_color(rgba(style::TEXT)))
        .child(Icon::new(IconName::ChevronLeft).size(px(12.)))
}

/// The control at the end of a panel's header that collapses it. The panel adds the click.
pub fn collapse_button(scale: f32) -> Stateful<Div> {
    div()
        .id("collapse")
        .size(px(18. * scale))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4. * scale))
        .text_color(rgba(style::MUTED))
        .hover(|style| style.bg(rgba(style::PILL_HOVER)).text_color(rgba(style::TEXT)))
        .child(Icon::new(IconName::ChevronRight).size(px(12. * scale)))
}

/// All there is in an overlay's window besides the panel. `gpui_kit::open_window` would put
/// its own root there, which paints the theme's background over the whole window: behind a
/// panel's round corners, and where a see-through panel should show the game.
struct Bare<V>(Entity<V>);

impl<V: Render> Render for Bare<V> {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().font_family(cx.theme().font_family.clone()).child(self.0.clone())
    }
}

/// One hidden pop-up window with `view` in it, and the panel behind it.
fn open_panel<V: Render>(
    cx: &mut App,
    build: impl FnOnce(&mut Context<V>) -> V + 'static,
) -> Result<(Panel, Entity<V>), String> {
    let options = WindowOptions {
        // Sized and placed by `sync` before it is ever shown.
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(300.), px(100.))))),
        titlebar: None,
        show: false,
        // Never the key window: Arena keeps the keyboard.
        focus: false,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let (mut panel, mut view) = (None, None);
    cx.open_window(options, |window, cx| {
        panel = Panel::of(window);
        let built = cx.new(build);
        view = Some(built.clone());
        cx.new(|_| Bare(built))
    })
    .map_err(|e| format!("{e:#}"))?;
    panel.zip(view).ok_or_else(|| "an overlay window has no NSPanel".to_string())
}

pub fn open(
    draft: Entity<Draft>,
    game: Entity<Game>,
    prefs: Entity<Settings>,
    visibility: Visibility,
    cx: &mut App,
) -> Result<(), String> {
    let draft = open_panel(cx, {
        let prefs = prefs.clone();
        move |cx| DraftPanel::new(draft, prefs, cx)
    })?;
    let deck = open_panel(cx, move |cx| DeckPanel::new(game, prefs, cx))?;
    let preview = open_panel(cx, |_| Preview { image: None })?;
    // The pointer passes through the preview, or it would end the hover that shows it.
    preview.0.ignore_mouse();
    cx.set_global(Overlays { draft, deck, preview, visibility });
    sync(cx);
    Ok(())
}

/// Show each overlay beside Arena, or hide it.
pub fn sync(cx: &mut App) {
    let Some(overlays) = cx.try_global::<Overlays>() else { return };
    let (draft, deck) = (overlays.draft.1.read(cx), overlays.deck.1.read(cx));
    let deck_shape = deck.shape(cx);
    // A game is on: a pack left over from the draft has no business showing.
    let draft_shape = if deck_shape.is_some() { None } else { draft.shape(cx) };
    let panels = [(overlays.draft.0, draft_shape, draft.preview(cx)), (overlays.deck.0, deck_shape, deck.preview(cx))];
    let (preview_panel, preview_view) = (overlays.preview.0, overlays.preview.1.clone());
    // Nothing to show: Arena's window isn't even looked for.
    let anchor = if panels.iter().any(|(_, shape, _)| shape.is_some()) { overlays.visibility.anchor() } else { None };

    // Where each window goes, or `None` to hide it.
    let mut moves: Vec<(Panel, Option<CGRect>)> = Vec::new();
    let mut preview = None;
    for (panel, shape, hovered) in panels {
        let frame = anchor.zip(shape).map(|(anchor, shape)| frame(&anchor, &arena::display_for(&anchor), shape));
        if let Some((frame, (row, image))) = frame.zip(hovered) {
            preview = Some((frame, row, image));
        }
        moves.push((panel, frame));
    }
    let preview_frame = preview.map(|(beside, row, image)| {
        preview_view.update(cx, |view, cx| {
            if view.image.as_ref() != Some(&image) {
                view.image = Some(image);
                cx.notify();
            }
        });
        preview_frame(&beside, f64::from(row))
    });
    moves.push((preview_panel, preview_frame));

    // Not from in here: resizing a window calls back into GPUI, which is busy running this
    // very update and would drop the call, leaving the window drawn at its old size. A task
    // on the main thread runs once the update is over.
    cx.spawn(async move |_| {
        for (panel, frame) in moves {
            match frame {
                Some(frame) => {
                    panel.place(frame);
                    panel.show();
                }
                None => panel.hide(),
            }
        }
    })
    .detach();
}

/// Where a panel goes in Arena's window (`anchor`): an open one hangs by its top-right corner
/// from a point just inside the window, a tab sits right against the window's edge at the
/// same height. Either is kept on `display`.
fn frame(anchor: &CGRect, display: &CGRect, shape: Shape) -> CGRect {
    let (width, height, inset) = match shape {
        Shape::Open(width, height) => (f64::from(width), f64::from(height), style::ANCHOR_INSET),
        Shape::Tab => (f64::from(style::TAB_WIDTH), f64::from(style::TAB_HEIGHT), 0.),
    };
    let right = anchor.origin.x + anchor.size.width - inset;
    let top = anchor.origin.y + anchor.size.height * style::ANCHOR_DROP;
    let x = fit(right - width, display.origin.x, display.origin.x + display.size.width - width);
    let y = fit(top, display.origin.y, display.origin.y + display.size.height - height);
    CGRect::new(&CGPoint::new(x, y), &CGSize::new(width, height))
}

/// Where the card preview goes: beside the panel, on whichever side has room, level with
/// the hovered row (`row` is the row's middle, measured down from the panel's top).
fn preview_frame(panel: &CGRect, row: f64) -> CGRect {
    let (width, height, gap) = (style::PREVIEW_WIDTH, style::PREVIEW_HEIGHT, style::PREVIEW_GAP);
    let display = arena::display_for(panel);
    let mut x = panel.origin.x - width - gap;
    if x < display.origin.x {
        x = panel.origin.x + panel.size.width + gap;
    }
    let y = fit(
        panel.origin.y + row - height / 2.,
        display.origin.y + gap,
        display.origin.y + display.size.height - height - gap,
    );
    CGRect::new(&CGPoint::new(x, y), &CGSize::new(width, height))
}

/// `at`, moved into `min..=max` (`min` wins when the range is empty).
fn fit(at: f64, min: f64, max: f64) -> f64 {
    at.min(max).max(min)
}

#[cfg(test)]
mod tests {
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};

    use super::{Shape, fit, frame, style};

    fn rect(x: f64, y: f64, width: f64, height: f64) -> CGRect {
        CGRect::new(&CGPoint::new(x, y), &CGSize::new(width, height))
    }

    #[test]
    fn an_open_panel_hangs_inside_arena_and_a_tab_sits_on_its_edge() {
        let (arena, display) = (rect(100., 50., 1000., 800.), rect(0., 0., 1440., 900.));
        let top = 50. + 800. * style::ANCHOR_DROP;

        let open = frame(&arena, &display, Shape::Open(300., 400.));
        assert_eq!((open.origin.x + open.size.width, open.origin.y), (1100. - style::ANCHOR_INSET, top));
        assert_eq!((open.size.width, open.size.height), (300., 400.));

        let tab = frame(&arena, &display, Shape::Tab);
        assert_eq!((tab.origin.x + tab.size.width, tab.origin.y), (1100., top));
        assert_eq!(tab.size.width, f64::from(style::TAB_WIDTH));
    }

    #[test]
    fn a_tab_stays_on_the_display_when_arena_runs_off_it() {
        let (arena, display) = (rect(1000., 0., 1000., 800.), rect(0., 0., 1440., 900.));
        let tab = frame(&arena, &display, Shape::Tab);
        assert_eq!(tab.origin.x + tab.size.width, 1440.);
    }

    #[test]
    fn fit_keeps_a_value_in_range() {
        assert_eq!(fit(5., 0., 10.), 5.);
        assert_eq!(fit(-5., 0., 10.), 0.);
        assert_eq!(fit(15., 0., 10.), 10.);
        // A panel taller than the display starts at its top.
        assert_eq!(fit(5., 0., -10.), 0.);
    }
}
