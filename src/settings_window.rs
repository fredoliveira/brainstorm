//! The Settings window (⌘,). Everything is saved the moment it is set.

use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;

use crate::cards::{self, Group};
use crate::feed::Feed;
use crate::settings::Settings;
use crate::state::{AppState, Uploads};
use crate::window::title_bar;
use crate::{Shared, apply_appearance, paths};

// Sizes, in points.
const WIDTH: f32 = 520.;
const HEIGHT: f32 = 640.;
const PADDING: f32 = 20.;
const SECTION_GAP: f32 = 22.;
const GAP: f32 = 8.;
const TEXT: f32 = 13.;
const SMALL_TEXT: f32 = 11.;

/// The open Settings window, so ⌘, brings it forward instead of opening a second one.
struct SettingsWindow(AnyWindowHandle, WeakEntity<SettingsView>);

impl Global for SettingsWindow {}

pub fn open(cx: &mut App) {
    // The view is gone once the window closes, and then ⌘, opens a new one.
    let open = cx.try_global::<SettingsWindow>().filter(|w| w.1.upgrade().is_some()).map(|w| w.0);
    if let Some(handle) = open
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    // The same title bar as the main window: transparent, with the view drawing its own.
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions { title: Some("Settings".into()), ..TitleBar::title_bar_options() }),
        window_bounds: Some(WindowBounds::centered(size(px(WIDTH), px(HEIGHT)), cx)),
        is_resizable: false,
        is_minimizable: false,
        ..TitleBar::window_options()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        apply_appearance(window, cx);
        cx.new(|cx| SettingsView::new(window, cx))
    });
    match opened {
        Ok((handle, view)) => cx.set_global(SettingsWindow(handle, view.downgrade())),
        Err(err) => eprintln!("brainstorm: couldn't open Settings: {err:#}"),
    }
}

/// "Ratings were last fetched 2 hours ago."
fn fetched(when: Option<std::time::SystemTime>) -> String {
    let Some(age) = when.and_then(|when| when.elapsed().ok()) else {
        return "No ratings have been fetched yet.".to_string();
    };
    let ago = match age.as_secs() {
        0..60 => "just now".to_string(),
        secs @ 60..3600 => format!("{} min ago", secs / 60),
        secs @ 3600..86400 => format!("{} h ago", secs / 3600),
        secs => format!("{} days ago", secs / 86400),
    };
    format!("Ratings were last fetched {ago}; they refresh every 3 hours.")
}

/// What the last change did, or why it failed (true for a failure).
type Note = Option<(SharedString, bool)>;

pub struct SettingsView {
    feed: Entity<Feed>,
    state: std::sync::Arc<AppState>,
    token: Entity<InputState>,
    token_note: Note,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let shared = cx.global::<Shared>();
        let (feed, state) = (shared.feed.clone(), shared.state.clone());
        let token =
            cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("00000000-0000-4000-8000-000000000000"));
        let subscriptions = vec![
            cx.subscribe_in(&token, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.save_token(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
            cx.observe(&feed, |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, window, cx| apply_appearance(window, cx)),
        ];
        Self { feed, state, token, token_note: None, _subscriptions: subscriptions }
    }

    fn save_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let token = self.token.read(cx).value().trim().to_string();
        if token.is_empty() {
            return;
        }
        self.token_note = Some(match self.state.save_token(&token) {
            Ok(()) => {
                self.token.update(cx, |state, cx| state.set_value("", window, cx));
                ("Saved.".into(), false)
            }
            Err(err) => (err.into(), true),
        });
        cx.notify();
    }

    fn remove_token(&mut self, cx: &mut Context<Self>) {
        self.token_note = Some(match self.state.update_settings(|settings| settings.token = None) {
            Ok(()) => ("Removed. Nothing is uploaded any more.".into(), false),
            Err(err) => (err.into(), true),
        });
        cx.notify();
    }

    /// A switch bound to one setting.
    fn switch(
        &self,
        id: &'static str,
        label: &'static str,
        on: bool,
        set: fn(&mut Settings, bool),
    ) -> impl IntoElement + use<> {
        let state = self.state.clone();
        Switch::new(id).label(label).checked(on).on_click(move |on, _, _| {
            if let Err(err) = state.update_settings(|settings| set(settings, *on)) {
                log::warn!(target: "brainstorm", "{err}");
            }
        })
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, muted, danger) = (theme.background, theme.foreground, theme.muted_foreground, theme.danger);
        let status = self.feed.read(cx).status.clone();
        let settings = status.settings.clone();
        let token_empty = self.token.read(cx).value().trim().is_empty();

        let about_token = match (settings.token_hint(), &status.uploads) {
            (None, _) => "Optional. With a token from your 17Lands account page, your drafts and games are \
                          uploaded there. If Recall or the 17Lands client is uploading already, quit it first."
                .to_string(),
            (Some(tail), Uploads::Held) => format!(
                "A token ending in {tail} is saved, but this development build uploads only where BRAINSTORM_HOST points."
            ),
            (Some(tail), Uploads::To(host)) => format!("A token ending in {tail} is saved. Uploading to {host}."),
            (Some(tail), Uploads::NoToken) => format!("A token ending in {tail} is saved."),
        };
        let caption = |text: String| div().text_size(px(SMALL_TEXT)).text_color(muted).child(text);
        let token_caption = match &self.token_note {
            // A note about the last change replaces the description until the window closes.
            Some((text, failed)) => {
                div().text_size(px(SMALL_TEXT)).text_color(if *failed { danger } else { muted }).child(text.clone())
            }
            None => caption(about_token),
        };
        let section = |label: &'static str| v_flex().w_full().gap(px(GAP)).child(div().font_medium().child(label));

        let state = self.state.clone();
        let groups = h_flex().gap(px(GAP)).children(Group::EVERY.map(|group| {
            let state = state.clone();
            let button = Button::new(group.label()).small().label(group.label()).on_click(move |_, _, _| {
                if let Err(err) = state.update_settings(|settings| settings.group = group) {
                    log::warn!(target: "brainstorm", "{err}");
                }
            });
            if group == settings.group { button.primary() } else { button }
        }));

        let title_bar = title_bar("Settings", None, cx);
        let content = v_flex()
            .flex_1()
            .w_full()
            .text_size(px(TEXT))
            .px(px(PADDING))
            .pb(px(PADDING))
            // The title bar is the space above the first section.
            .pt(px(GAP))
            .gap(px(SECTION_GAP))
            .child(
                section("17Lands token")
                    .child(
                        h_flex()
                            .w_full()
                            .gap(px(GAP))
                            .child(div().flex_1().child(Input::new(&self.token).mask_toggle()))
                            .child(
                                Button::new("save-token")
                                    .primary()
                                    .label("Save")
                                    .disabled(token_empty)
                                    .on_click(cx.listener(|this, _, window, cx| this.save_token(window, cx))),
                            )
                            .children(settings.token.is_some().then(|| {
                                Button::new("remove-token")
                                    .label("Remove")
                                    .on_click(cx.listener(|this, _, _, cx| this.remove_token(cx)))
                            })),
                    )
                    .child(token_caption),
            )
            .child(
                section("Your data")
                    .child(self.switch("save", "Save drafts and games on this Mac", settings.save, |s, on| s.save = on))
                    .child(caption(format!(
                        "One file per day in {}. Nothing leaves this Mac unless a 17Lands token is set.",
                        paths::archive_dir().display()
                    ))),
            )
            .child(
                section("Overlays")
                    .child(self.switch(
                        "draft-overlay",
                        "Card ratings while drafting",
                        settings.draft_overlay,
                        |s, on| s.draft_overlay = on,
                    ))
                    .child(self.switch(
                        "deck-overlay",
                        "What's left in your library while playing",
                        settings.deck_overlay,
                        |s, on| s.deck_overlay = on,
                    ))
                    .child(self.switch("compact", "Smaller overlays", settings.compact, |s, on| s.compact = on))
                    .child(caption(
                        "They show beside Arena's window, only while Arena is in front. Hover a row to see the card."
                            .to_string(),
                    )),
            )
            .child(section("Ratings from").child(groups).child(caption(format!(
                "Whose games the win rates come from: all of 17Lands' players, or its top, middle or bottom \
                 ones. Clicking the draft overlay's header switches too. {}",
                fetched(cards::store().newest_ratings())
            ))))
            .child(
                div()
                    .mt_auto()
                    .text_size(px(SMALL_TEXT))
                    .text_color(muted)
                    .child(format!("Brainstorm {}", env!("CARGO_PKG_VERSION"))),
            );

        v_flex().size_full().bg(bg).text_color(fg).child(title_bar).child(content)
    }
}
