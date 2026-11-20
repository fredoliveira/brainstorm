# Brainstorm

An MTG Arena companion for macOS. One app in the menu bar that follows Arena's log and:

- **shows card ratings while you draft**: a panel beside Arena lists the pack you're picking from,
  ranked by [17Lands](https://www.17lands.com) GIH WR;
- **shows what's left in your library while you play**: every card of your deck with how many
  copies you haven't seen yet, and the chance your next draw is one;
- **saves your drafts and games** on your Mac, one file per day;
- **uploads them to 17Lands**, if you give it your token. Without one, nothing leaves your Mac.

It brings [Recall](https://github.com/fredoliveira/recall)'s desktop app and
[Ponder](https://github.com/fredoliveira/ponder) under one roof, on the same log parser
(`recall-core`). The name is the third of the blue card-drawing spells, after Ancestral Recall and
Ponder: draw three, then put two back.

## Using it

1. In Arena, turn on **Detailed Logs**: gear icon → Account → "Detailed Logs". Without it, Arena
   doesn't write draft picks or games to its log, and Brainstorm has nothing to work with.
2. Open Brainstorm. Three cards appear in the menu bar; the overlays appear by themselves the
   next time you're picking a card or playing a game.

The overlays hang off the top-right of Arena's window and show only while Arena is the app in
front. They follow Arena into fullscreen and never take focus from the game. Hover a row to see
the whole card.

Brainstorm reads the log from the start each time it opens, so a draft or a game already under
way is picked up where it stands.

### While drafting

- **The big number** is the card's GIH WR: how often decks won games in which the card was drawn
  or in the opening hand. A dash means 17Lands doesn't have enough games yet.
- **Its colour** shows how the card compares with the set's average: bright green for standouts,
  light green above average, white for average, then orange and red below. The bands are in
  standard deviations from the set mean, so they mean the same thing in every set.
- **The bar on the left edge** marks rarity; **the dots** are the card's colours.
- **Click the header** to switch whose games the win rates come from: all of 17Lands' players, or
  its top, middle or bottom ones. The choice is remembered.

Ratings are for the format you're drafting. A format with no numbers yet (Quick Draft early in a
set) falls back to Premier Draft, and the header says so in orange.

### While playing

One row per card of your deck: the copies still unseen, the name, the mana cost, and the chance
that the next card off the top is that one. Spells come first, by mana value, then lands. A card
you've seen every copy of stays listed, faded.

"Unseen" is your deck minus every card of yours that is in your hand, on the battlefield or the
stack, in your graveyard or in exile. A card shuffled back into your library counts as unseen
again.

### Settings

The menu-bar icon opens the window (a log of what Brainstorm did, and its status) and Settings:

| Setting       | What it does                                                                          |
| ------------- | ------------------------------------------------------------------------------------- |
| 17Lands token | Optional. With it, drafts and games are uploaded to 17Lands                           |
| Your data     | Save drafts and games on this Mac (on by default)                                     |
| Overlays      | Turn either overlay off, or draw them at 80%                                          |
| Ratings from  | All, top, middle or bottom players (also switchable from the draft overlay's header) |

**Coming from Recall or the 17Lands client?** Quit it before you give Brainstorm a token: two
uploaders send everything twice. Brainstorm carries on from where Recall left off, so the games
already uploaded today are not sent again.

### Where things are kept

| What                    | Where                                                          |
| ----------------------- | -------------------------------------------------------------- |
| Settings, the token     | `~/Library/Application Support/brainstorm/settings.toml`       |
| Saved drafts and games  | `~/Library/Application Support/brainstorm/archive/<date>.jsonl` |
| 17Lands data and images | `~/Library/Caches/brainstorm`                                  |

The saved files are `recall-core`'s
[archive format](https://github.com/fredoliveira/recall#keeping-your-own-data-the-archive): one
JSON object per line, every event the log held.

## How it works

Brainstorm is one Rust program on [GPUI](https://gpui.rs). Two followers read `Player.log`, each
on its own thread:

- The **local** follower always runs. It feeds the overlays and writes the archive.
- The **upload** follower runs only with a token. It is `recall-core`'s 17Lands uploader, byte
  for byte what Recall sends.

There are two because an upload can wait minutes on a network that's down, and the overlays must
not wait with it. Each keeps its own checkpoint, so a restart neither saves nor uploads anything
twice.

Win rates, card names and images come from 17Lands' public endpoints (ratings are refreshed
every 3 hours); mana costs come from [Scryfall](https://scryfall.com). All of it is cached, so
the overlays work offline with what they have.

## Develop

You'll need Rust via [rustup](https://rustup.rs) (the toolchain is pinned in
`rust-toolchain.toml`) and [just](https://github.com/casey/just).

The Recall parser is pinned to a release tag in [`Cargo.toml`](Cargo.toml). To try unreleased
parser changes, point it at a local checkout with `path = "../recall/crates/core"` while you
work, and don't commit that.

```sh
just run          # follow Arena's live log
just demo         # a made-up pack beside Arena's window, whether or not Arena is in front
just demo game    # ...or a made-up game, for the deck overlay
just lint         # format check, clippy, tests
just bundle       # dist/Brainstorm.app for this Mac
```

**A development build never uploads to the live 17Lands API.** With a token saved it uploads
only where `BRAINSTORM_HOST` points, and holds uploads otherwise. To exercise uploading, run
the mock server from a checkout of Recall and point at it:

```sh
python3 ../recall/tools/mock_server.py 8732 /tmp/brainstorm-out.jsonl &
BRAINSTORM_HOST=http://127.0.0.1:8732 just run
```

| Variable                | Effect                                                               |
| ----------------------- | -------------------------------------------------------------------- |
| `BRAINSTORM_HOST`       | Upload host. The only way a development build uploads at all         |
| `BRAINSTORM_HOME`       | Keep settings, data and caches under this directory instead          |
| `BRAINSTORM_LOG`        | Follow this file instead of the auto-detected `Player.log`           |
| `BRAINSTORM_DEMO`       | Made-up cards and no followers; `game` for the deck overlay          |
| `BRAINSTORM_ALWAYS`     | Show the overlays even when Arena isn't the app in front             |
| `BRAINSTORM_WINDOW=WxH` | Dock to a made-up Arena window of this size at the screen's top-left |
| `BRAINSTORM_SHOW`       | `window` or `settings`: open that window at launch                   |
| `BRAINSTORM_APPEARANCE` | `light` or `dark` windows instead of following the system            |

A log cut off mid-draft or mid-game makes a good fixture: `BRAINSTORM_LOG=cut.log
BRAINSTORM_WINDOW=1280x900 BRAINSTORM_HOME=/tmp/bs just run` shows the overlay for wherever the
file ends, without touching your real settings and data.

### Layout

| File                     | Role                                                                           |
| ------------------------ | ------------------------------------------------------------------------------ |
| `src/main.rs`            | App setup, the task that drains the feed, the twice-a-second overlay check     |
| `src/state.rs`           | `AppState`: the settings, the two followers, the status the windows show       |
| `src/feed.rs`            | The `Signal` channel from the followers to the main thread, and the `log` sink |
| `src/settings.rs`        | The settings file                                                              |
| `src/cards.rs`           | 17Lands ratings and card data, Scryfall costs, card images, all cached         |
| `src/draft.rs`, `game.rs` | What the overlays show: the pack waiting for a pick, the game in progress      |
| `src/overlay.rs`         | The overlays' windows: when they show and where they go                        |
| `src/draft_panel.rs`     | The draft overlay                                                              |
| `src/deck_panel.rs`      | The deck overlay                                                               |
| `src/window.rs`          | The main window: the log and the status line                                   |
| `src/settings_window.rs` | The Settings window                                                            |
| `src/tray.rs`            | The menu-bar icon (`tray-icon`; GPUI has no status-item API)                   |
| `src/arena.rs`           | Arena's window and whether Arena is in front (macOS)                           |
| `src/native.rs`          | The overlays' `NSPanel`s: moving, showing and hiding, which GPUI has no API for |
| `src/style.rs`           | Every size and colour in the overlays                                          |
| `src/paths.rs`           | Where the files above live                                                     |

`arena.rs` and `native.rs` are the only platform-specific files.

Two things worth knowing before changing the overlays:

- A window is moved or resized from a task, never from inside a GPUI update (see `overlay::sync`).
  Resizing calls back into GPUI, which drops the call while it is busy, and the window is then
  drawn at its old size.
- A panel asks for `overlay::sync` itself (`cx.defer`) whenever its size or hovered row changes.
  Observers run after the update that notified them, so a sync called from that update is too
  early.

### Not done yet

- A release workflow (notarization) and a Homebrew cask: both wait for a repository.
- The icon is a placeholder, drawn by `scripts/icon.swift`.

## Credits

Win rates and card images come from [17Lands](https://www.17lands.com), built on data that
players contribute by running its client; mana costs from [Scryfall](https://scryfall.com).

Brainstorm is unofficial Fan Content permitted under the
[Wizards of the Coast Fan Content Policy](https://company.wizards.com/en/legal/fancontentpolicy).
It is not approved or endorsed by Wizards, or affiliated with 17Lands or Scryfall. Portions of
the materials used are property of Wizards of the Coast. © Wizards of the Coast LLC.

## License

[MIT](LICENSE)
