# Brainstorm

An MTG Arena companion for macOS. It lives in the menu bar, follows Arena's log, and:

- **shows card ratings while you draft**: the pack you're picking from, ranked by
  [17Lands](https://www.17lands.com) GIH WR;
- **shows what's left in your library while you play**, with the chance your next draw is each card;
- **saves your drafts and games** on your Mac;
- **uploads them to 17Lands**, if you give it your token. Without one, nothing leaves your Mac.

It brings [Recall](https://github.com/fredoliveira/recall)'s desktop app and
[Ponder](https://github.com/fredoliveira/ponder) under one roof, on the same log parser.

## Install

Apple Silicon Macs only, macOS 11 or later. With [Homebrew](https://brew.sh):

```sh
brew install --cask fredoliveira/tap/brainstorm
```

Or download `Brainstorm.dmg` from the
[latest release](https://github.com/fredoliveira/brainstorm/releases/latest). To build it from
source instead, you'll need Rust (via [rustup](https://rustup.rs)) and
[just](https://github.com/casey/just): `just bundle && open dist/Brainstorm.app`.

## Using it

1. In Arena, turn on **Detailed Logs** (gear icon → Account). Without it there is nothing to read.
2. Open Brainstorm. The overlays appear by themselves when you're picking a card or playing a game.

The overlays hang off the top-right of Arena's window, show only while Arena is in front, and never
take focus. Hover a row to see the whole card. Click the arrow at the end of an overlay's header to
fold it into a tab on Arena's right edge, and the tab to open it again. In the draft overlay, click
the header to switch whose games the win rates come from.

The menu-bar icon opens Settings: your 17Lands token, whether to save your data, and which overlays
to show. Coming from Recall or the 17Lands client? Quit it before you give Brainstorm a token, or
everything is uploaded twice.

Settings and saved games are in `~/Library/Application Support/brainstorm`; cached 17Lands data and
card images are in `~/Library/Caches/brainstorm`.

## Develop

```sh
just run          # follow Arena's live log
just demo         # a made-up pack beside Arena's window (`just demo game` for the deck overlay)
just lint         # format checks, clippy, tests
```

A development build never uploads to the live 17Lands API. [`AGENTS.md`](AGENTS.md) has the rest:
the rules, the environment variables, the things that will bite, and how to release.

## Credits

Win rates and card images come from [17Lands](https://www.17lands.com); mana costs from
[Scryfall](https://scryfall.com).

Brainstorm is unofficial Fan Content permitted under the
[Wizards of the Coast Fan Content Policy](https://company.wizards.com/en/legal/fancontentpolicy). It
is not approved or endorsed by Wizards, or affiliated with 17Lands or Scryfall. Portions of the
materials used are property of Wizards of the Coast. © Wizards of the Coast LLC.

## License

[MIT](LICENSE)
