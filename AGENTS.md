# Brainstorm

A macOS menu-bar app (Rust, GPUI via `gpui-kit`) that follows MTG Arena's `Player.log` through
`recall-core`, saves drafts and games, optionally uploads them to 17Lands, and draws overlays on
Arena. Each module's doc comment says what it is for; this file holds what the code doesn't show.

## Rules

- **Never upload to the live 17Lands API while developing.** A debug build uploads only where
  `BRAINSTORM_HOST` points and holds uploads otherwise. A release build (`just bundle`) with a token
  uploads for real, so never give one a token to test with. Exercise uploads against Recall's mock:
  `python3 ../recall/tools/mock_server.py 8732 out.jsonl`, then
  `BRAINSTORM_HOST=http://127.0.0.1:8732 just run`.
- **Never commit a raw `Player.log`** or a cut of one: it holds the account id, screen name and
  token.
- **Don't draw test overlays over a game in progress, and don't take focus.** Run with
  `BRAINSTORM_WINDOW=WxH` (a made-up Arena window at the screen's top-left) and `BRAINSTORM_HOME`
  set to a scratch directory that already has `config/settings.toml` in it: an empty home is a first
  run, which opens a window and activates the app. Check the result with `screencapture -x` and a
  `CGWindowListCopyWindowInfo` listing.
- **Apple Silicon only.** No Intel or universal builds, in the bundle script, the workflow or the
  cask.
- The 17Lands payloads are `recall-core`'s, byte for byte. Change them there, never here.

## Commands

Use `just` (or `cargo +<channel in rust-toolchain.toml>`): a global `RUSTUP_TOOLCHAIN` overrides the
pinned toolchain, and GPUI needs the pinned one. `just lint` is the pre-commit gate and mirrors CI,
including Prettier on every Markdown file (100 columns; `just fmt-md` fixes it).

`recall-core` is pinned to a Recall release tag in `Cargo.toml`. To try unreleased parser changes,
point it at `path = "../recall/crates/core"` while you work, and don't commit that.

| Variable                | Effect                                                         |
| ----------------------- | -------------------------------------------------------------- |
| `BRAINSTORM_HOST`       | Upload host. The only way a debug build uploads at all         |
| `BRAINSTORM_HOME`       | Keep settings, data and caches under this directory            |
| `BRAINSTORM_LOG`        | Follow this file instead of the auto-detected `Player.log`     |
| `BRAINSTORM_DEMO`       | Made-up cards and no followers; `game` for the deck overlay    |
| `BRAINSTORM_ALWAYS`     | Show the overlays even when Arena isn't the app in front       |
| `BRAINSTORM_WINDOW=WxH` | Dock to a made-up Arena window instead of the real one         |
| `BRAINSTORM_SHOW`       | `window` or `settings`: open that window at launch             |
| `BRAINSTORM_APPEARANCE` | `light` or `dark` windows instead of following the system mode |

## Things that will bite

- **Move or resize an overlay window from a task, never inside a GPUI update** (see
  `overlay::sync`). Resizing calls back into GPUI, which drops the call while it is busy, and the
  window is drawn at its old size.
- **A panel asks for `overlay::sync` itself** (`cx.defer`) when its shape or hovered row changes.
  Observers run after the update that notified them, so a sync called from that update is too early.
- **Overlay windows are borderless and have no `gpui_kit` root** (`native.rs`,
  `overlay::open_panel`). macOS rounds a titled window's corners, and the kit's root paints the
  theme's background over the whole window. The main and Settings windows do use
  `gpui_kit::open_window`.
- **Settings change through `AppState::update_settings` only.** It saves the file and sends
  `Signal::Status`; `main.rs` then refreshes the `prefs` entity the panels observe. Writing to
  `prefs` directly changes nothing that lasts.
- **There are two followers on purpose.** An upload can block for minutes on a dead network, and the
  overlays must not wait with it. Each has its own checkpoint (`local-state.json`,
  `upload-state.json`).
- **In a test module beside a view, import items by name.** `use super::*` brings in GPUI's `test`
  macro, which shadows `#[test]` and fails with "recursion limit reached".
- **Scryfall doesn't know new sets by Arena id**, so costs are asked for by id, then by exact name.
  Cards it doesn't know are not cached, so they are asked for again later.
- **Clicks and hover can't be scripted**: synthetic mouse events need Accessibility permission. Say
  so instead of claiming a click path was tested.

## Releases

Bump the version in `Cargo.toml`, move the changelog's Unreleased entries under it, commit, and push
a tag `vX.Y.Z` that matches (the workflow refuses a mismatch). `.github/workflows/release.yml` then
tests, builds, signs, notarizes, publishes a GitHub release and bumps the cask in
`fredoliveira/homebrew-tap`. Run by hand, it stops at a `.dmg` build artifact. The seven secrets it
needs are listed in `packaging/homebrew/README.md`.

The app icon is `assets/icon-art.png` (made with an image model, in homage to DiTerlizzi's art for
the card) cut to the macOS tile by `scripts/icon.swift`, which writes `icon.png` and `icon.icns`.
Rerun it after changing the art. The menu-bar icon in `tray.rs` is separate: a template image, still
the three-card placeholder.
