//! Brainstorm: an MTG Arena companion on GPUI and `recall-core`. It follows Arena's log from
//! the menu bar, saves your drafts and games, uploads them to 17Lands if you give it a
//! token, and draws overlays on top of the game: card ratings while you draft, what is left
//! in your library while you play.

mod arena;
mod cards;
mod deck_panel;
mod draft;
mod draft_panel;
mod feed;
mod game;
mod native;
mod overlay;
mod paths;
mod settings;
mod settings_window;
mod state;
mod style;
mod tray;
mod window;

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::*;

use draft::Draft;
use feed::{Feed, Signal};
use game::Game;
use overlay::Visibility;
use settings::Settings;
use state::AppState;

actions!(brainstorm, [Quit, OpenWindow, OpenSettings]);

/// How often the overlays check where Arena's window is and whether Arena is in front.
const TICK: Duration = Duration::from_millis(500);
/// How many ticks between tries at finding a log to follow, while there is none.
const RETRY_TICKS: u32 = 20;

/// What every window needs: the followers and settings, and the feed of what they report.
pub struct Shared {
    pub state: Arc<AppState>,
    pub feed: Entity<Feed>,
}

impl Global for Shared {}

/// What the follower's signals update.
struct Models {
    draft: Entity<Draft>,
    game: Entity<Game>,
    /// The settings as the overlays read them: a copy of the state's, refreshed on change.
    prefs: Entity<Settings>,
}

/// Follow the system's light or dark mode, unless `BRAINSTORM_APPEARANCE` is `light` or
/// `dark` (for screenshots and checking both themes without changing the system setting).
pub fn apply_appearance(window: &mut Window, cx: &mut App) {
    use gpui_kit::component::{Theme, ThemeMode};
    match std::env::var("BRAINSTORM_APPEARANCE").as_deref() {
        Ok("light") => Theme::change(ThemeMode::Light, Some(window), cx),
        Ok("dark") => Theme::change(ThemeMode::Dark, Some(window), cx),
        _ => Theme::sync_system_appearance(Some(window), cx),
    }
}

fn main() {
    let (signals, incoming) = smol::channel::unbounded::<Signal>();
    // Before anything logs.
    feed::install_logger(signals.clone());

    let (state, first_run) = AppState::new(signals.clone());
    let state = Arc::new(state);

    // A menu-bar app: no Dock icon. The policy is set on the platform before the app
    // launches: set any later, the Dock icon shows for a moment first.
    let platform = gpui_kit::platform::current_platform(false);
    platform.set_activation_policy(ActivationPolicy::Accessory);
    let app = gpui_kit::Application::with_platform(platform).with_assets(gpui_kit::assets::Assets);
    app.run(move |cx| {
        gpui_kit::init(cx);

        let feed = cx.new(|_| Feed { status: state.status(), ..Default::default() });
        cx.set_global(Shared { state: state.clone(), feed: feed.clone() });

        // BRAINSTORM_DEMO: made-up cards and no followers, for working on the panels.
        // `BRAINSTORM_DEMO=game` shows the deck panel, anything else the draft panel.
        let demo = std::env::var("BRAINSTORM_DEMO").ok();
        let models = Models {
            draft: cx
                .new(|_| if demo.as_deref().is_some_and(|d| d != "game") { Draft::demo() } else { Draft::default() }),
            game: cx.new(|_| if demo.as_deref() == Some("game") { Game::demo() } else { Game::default() }),
            prefs: cx.new(|_| state.settings()),
        };
        let opened =
            overlay::open(models.draft.clone(), models.game.clone(), models.prefs.clone(), Visibility::from_env(), cx);
        if let Err(err) = opened {
            log::error!(target: "brainstorm", "Couldn't open the overlays: {err}");
        }

        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-,", OpenSettings, None),
            KeyBinding::new("cmd-0", OpenWindow, None),
        ]);
        cx.on_action(|_: &Quit, cx: &mut App| quit(cx));
        cx.on_action(|_: &OpenWindow, cx: &mut App| window::open(cx));
        cx.on_action(|_: &OpenSettings, cx: &mut App| settings_window::open(cx));
        cx.set_menus(vec![
            Menu {
                name: "Brainstorm".into(),
                disabled: false,
                items: vec![
                    MenuItem::action("Settings…", OpenSettings),
                    MenuItem::separator(),
                    MenuItem::action("Quit Brainstorm", Quit),
                ],
            },
            Menu { name: "Window".into(), disabled: false, items: vec![MenuItem::action("Brainstorm", OpenWindow)] },
        ]);

        let tray = tray::install(signals.clone())
            .inspect_err(|err| log::warn!(target: "brainstorm", "No menu-bar icon: {err}"))
            .ok();
        // Without the icon there is no way back to the windows, so be an ordinary Dock app.
        let lost = tray.is_none();
        if lost {
            cx.set_activation_policy(ActivationPolicy::Regular);
        }

        if demo.is_none() {
            state.start();
        }

        // Everything the followers report arrives here, on the main thread.
        cx.spawn(async move |cx| {
            let _tray = tray; // lives as long as the app
            while let Ok(first) = incoming.recv().await {
                // Take what has queued up in one go: the whole log is replayed at launch, and
                // that should be one redraw, not hundreds.
                let mut batch = vec![first];
                while let Ok(more) = incoming.try_recv() {
                    batch.push(more);
                }
                cx.update(|cx| receive(batch, &models, cx));
            }
        })
        .detach();

        cx.spawn({
            let state = state.clone();
            async move |cx| {
                let mut ticks = 0u32;
                loop {
                    cx.background_executor().timer(TICK).await;
                    cx.update(overlay::sync);
                    // Arena writes its first log only once it has run: keep looking.
                    ticks = ticks.wrapping_add(1);
                    if demo.is_none() && ticks.is_multiple_of(RETRY_TICKS) && !state.is_following() {
                        state.start();
                    }
                }
            }
        })
        .detach();

        // A first run says hello, and so does a run with no menu-bar icon to find it by.
        // Otherwise the app starts where it lives: in the menu bar.
        // BRAINSTORM_SHOW=window|settings opens one regardless, for working on the windows.
        let show = std::env::var("BRAINSTORM_SHOW").ok();
        if first_run || lost || show.as_deref() == Some("window") {
            window::open(cx);
            cx.activate(true);
        }
        if show.as_deref() == Some("settings") {
            settings_window::open(cx);
            cx.activate(true);
        }
    });
}

fn receive(batch: Vec<Signal>, models: &Models, cx: &mut App) {
    let shared = cx.global::<Shared>();
    let (state, feed) = (shared.state.clone(), shared.feed.clone());
    let (mut drafts, mut game, mut lines, mut menu) = (Vec::new(), None, Vec::new(), Vec::new());
    let mut status_changed = false;
    for signal in batch {
        match signal {
            Signal::Draft(event) => drafts.push(event),
            // Only where the game stands now matters.
            Signal::Game(view) => game = Some(view),
            Signal::Log(line) => lines.push(line),
            Signal::Status => status_changed = true,
            Signal::Menu(id) => menu.push(id),
        }
    }

    if !drafts.is_empty() {
        models.draft.update(cx, |draft, cx| {
            drafts.into_iter().for_each(|event| draft.apply(event));
            cx.notify();
        });
    }
    if let Some(view) = game {
        models.game.update(cx, |game, cx| {
            game.view = view;
            cx.notify();
        });
    }
    if status_changed || !lines.is_empty() {
        feed.update(cx, |feed, cx| {
            lines.into_iter().for_each(|line| feed.push(line));
            if status_changed {
                feed.status = state.status();
            }
            cx.notify();
        });
    }
    if status_changed {
        let settings = state.settings();
        models.prefs.update(cx, |prefs, cx| {
            if *prefs != settings {
                *prefs = settings;
                cx.notify();
            }
        });
    }
    for id in menu {
        match id.as_str() {
            tray::OPEN => {
                window::open(cx);
                cx.activate(true);
            }
            tray::SETTINGS => {
                settings_window::open(cx);
                cx.activate(true);
            }
            tray::QUIT => quit(cx),
            _ => {}
        }
    }
}

fn quit(cx: &mut App) {
    cx.global::<Shared>().state.stop();
    cx.quit();
}
