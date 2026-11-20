//! The main window: the log of what Brainstorm did, and one status line under it.

use std::rc::Rc;

use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::feed::{Feed, LogLine};
use crate::state::{Status, Uploads};
use crate::{OpenSettings, Shared, apply_appearance};

// Sizes, in points. Colours come from the theme (light or dark).
const WIDTH: f32 = 760.;
const HEIGHT: f32 = 520.;
const MIN_WIDTH: f32 = 480.;
const MIN_HEIGHT: f32 = 320.;
/// Between the window's sides and everything in it.
const MARGIN: f32 = 16.;
const TEXT: f32 = 13.;
const SMALL_TEXT: f32 = 11.;
const TITLE_TEXT: f32 = 14.;
/// The log itself, in the monospaced font, one row of this height per line.
const LOG_TEXT: f32 = 10.;
const LOG_ROW: f32 = 16.;

/// The open window, so opening it again brings it forward instead of making a second.
struct MainWindow(AnyWindowHandle, WeakEntity<MainView>);

impl Global for MainWindow {}

pub fn open(cx: &mut App) {
    // The view is gone once the window closes, and then this opens a new one.
    let open = cx.try_global::<MainWindow>().filter(|w| w.1.upgrade().is_some()).map(|w| w.0);
    if let Some(handle) = open
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    // A transparent title bar: the view draws its own, so it can hold the settings button.
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions { title: Some("Brainstorm".into()), ..TitleBar::title_bar_options() }),
        window_bounds: Some(WindowBounds::centered(size(px(WIDTH), px(HEIGHT)), cx)),
        window_min_size: Some(size(px(MIN_WIDTH), px(MIN_HEIGHT))),
        ..TitleBar::window_options()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        apply_appearance(window, cx);
        cx.new(|cx| MainView::new(window, cx))
    });
    match opened {
        Ok((handle, view)) => cx.set_global(MainWindow(handle, view.downgrade())),
        Err(err) => eprintln!("brainstorm: couldn't open the window: {err:#}"),
    }
}

pub struct MainView {
    feed: Entity<Feed>,
    /// The lines on screen: a copy of the feed's, shared with the list.
    rows: Rc<Vec<LogLine>>,
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl MainView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let feed = cx.global::<Shared>().feed.clone();
        let subscriptions = vec![
            cx.observe(&feed, |this, _, cx| this.refresh(cx)),
            cx.observe_window_appearance(window, |_, window, cx| apply_appearance(window, cx)),
        ];
        let mut view = Self {
            feed,
            rows: Rc::new(Vec::new()),
            scroll: UniformListScrollHandle::default(),
            _subscriptions: subscriptions,
        };
        view.refresh(cx);
        view
    }

    /// The feed changed: take its lines, and keep the newest in view if it was.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let at_bottom = {
            let handle = &self.scroll.0.borrow().base_handle;
            handle.offset().y <= -handle.max_offset().y + px(LOG_ROW)
        };
        let rows: Vec<LogLine> = self.feed.read(cx).lines.iter().cloned().collect();
        if at_bottom && !rows.is_empty() {
            self.scroll.scroll_to_item(rows.len() - 1, ScrollStrategy::Bottom);
        }
        self.rows = Rc::new(rows);
        cx.notify();
    }
}

/// The title bar every window draws for itself, over a transparent native one (open the
/// window with [`TitleBar::title_bar_options`] and [`TitleBar::window_options`]): the title
/// centred, and `trailing` at the right edge.
///
/// The title is laid over the whole bar, so it's centred on the window rather than on the
/// space the bar leaves right of the traffic lights. It has no mouse handlers, so clicks and
/// drags pass through to the bar and whatever is in it.
pub fn title_bar(title: &'static str, trailing: Option<AnyElement>, cx: &App) -> Div {
    let theme = cx.theme();
    div().relative().child(TitleBar::new().border_b_0().bg(theme.background).children(trailing)).child(
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(TITLE_TEXT))
            .font_medium()
            .text_color(theme.muted_foreground)
            .child(title),
    )
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The right-hand side of the status line: what is being done with what the log says.
fn doing(status: &Status) -> (String, bool) {
    let saving = if status.settings.save { "Saving on this Mac" } else { "Not saving" };
    let (uploads, trouble) = match &status.uploads {
        Uploads::NoToken => ("17Lands uploads off".to_string(), false),
        Uploads::Held => ("uploads held in this development build".to_string(), false),
        Uploads::To(_) if status.upload_failed > 0 => {
            (format!("{} uploaded, {} failed", status.upload_count, status.upload_failed), true)
        }
        Uploads::To(_) => match status.upload_count {
            0 => ("nothing uploaded yet".to_string(), false),
            1 => ("1 upload".to_string(), false),
            n => (format!("{n} uploads"), false),
        },
    };
    (format!("{saving} · {uploads}"), trouble)
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, muted, border) = (theme.background, theme.foreground, theme.muted_foreground, theme.border);
        let (success, warning, danger) = (theme.success, theme.warning, theme.danger);
        let mono = theme.mono_font_family.clone();
        let status = self.feed.read(cx).status.clone();

        let title_bar = title_bar(
            "Brainstorm",
            Some(
                h_flex()
                    .flex_1()
                    .justify_end()
                    .pr(px(8.))
                    .child(
                        Button::new("settings")
                            .ghost()
                            .small()
                            .icon(IconName::Settings)
                            .tooltip("Settings (⌘,)")
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx)),
                    )
                    .into_any_element(),
            ),
            cx,
        );

        let rows = self.rows.clone();
        let log = if rows.is_empty() {
            div()
                .flex_1()
                .w_full()
                .pt(px(120.))
                .flex()
                .justify_center()
                .text_size(px(TEXT))
                .text_color(muted)
                .child("Nothing yet. Activity shows up here as you play.")
                .into_any_element()
        } else {
            uniform_list("log", rows.len(), move |range, _, _| {
                range
                    .map(|i| {
                        let line = &rows[i];
                        let color = match line.level {
                            log::Level::Error => danger,
                            log::Level::Warn => warning,
                            log::Level::Debug | log::Level::Trace => muted,
                            log::Level::Info => fg,
                        };
                        h_flex()
                            .h(px(LOG_ROW))
                            .w_full()
                            .px(px(MARGIN))
                            .gap(px(10.))
                            .child(div().flex_none().text_color(muted).child(line.ts.clone()))
                            .child(div().flex_1().min_w_0().truncate().text_color(color).child(line.msg.clone()))
                    })
                    .collect()
            })
            .track_scroll(&self.scroll)
            .flex_1()
            .w_full()
            .py(px(6.))
            .font_family(mono)
            .text_size(px(LOG_TEXT))
            .into_any_element()
        };

        // The status: whether the log is being followed on the left, what is done with it on
        // the right.
        let (dot, text, text_color): (Hsla, SharedString, Hsla) = match (&status.log_path, status.following) {
            (None, _) => (muted, "Player.log not found. Has Arena run on this Mac?".into(), fg),
            (Some(_), false) => (muted, "Not following".into(), fg),
            (Some(_), true) if status.upload_failed > 0 => (
                warning,
                format!("Last failure: {}", status.last_failure.as_deref().unwrap_or("unknown")).into(),
                warning,
            ),
            (Some(path), true) => match (&status.last_endpoint, &status.last_time) {
                (Some(endpoint), Some(time)) => (success, format!("Last upload: {endpoint} at {time}").into(), muted),
                _ => (success, format!("Following {}", file_name(path)).into(), muted),
            },
        };
        let (doing, trouble) = doing(&status);
        let footer = h_flex()
            .w_full()
            .px(px(MARGIN))
            .py(px(8.))
            .gap(px(8.))
            .border_t_1()
            .border_color(border)
            .text_size(px(SMALL_TEXT))
            .child(div().flex_none().size(px(7.)).rounded_full().bg(dot))
            .child(div().flex_1().min_w_0().truncate().text_color(text_color).child(text))
            .child(div().flex_none().text_color(muted).when(trouble, |this| this.text_color(warning)).child(doing));

        v_flex().size_full().bg(bg).text_color(fg).child(title_bar).child(log).child(footer)
    }
}

#[cfg(test)]
mod tests {
    use super::{Status, Uploads, doing};

    #[test]
    fn the_status_says_what_happens_to_the_log() {
        let mut status = Status::default();
        assert_eq!(doing(&status), ("Saving on this Mac · 17Lands uploads off".to_string(), false));
        status.settings.save = false;
        status.uploads = Uploads::To("https://example.test".into());
        status.upload_count = 3;
        assert_eq!(doing(&status), ("Not saving · 3 uploads".to_string(), false));
        status.upload_failed = 1;
        assert_eq!(doing(&status), ("Not saving · 3 uploaded, 1 failed".to_string(), true));
    }
}
