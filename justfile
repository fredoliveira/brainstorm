# brainstorm task runner (`brew install just`). `just` lists recipes.
# The toolchain is named explicitly because a global RUSTUP_TOOLCHAIN (mise sets one) overrides
# rust-toolchain.toml.

toolchain := `sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml`

# Show available recipes.
default:
    @just --list

# A development build never uploads to the live 17Lands API (see the README).
# Build and run, following Arena's live log.
run:
    cargo +{{toolchain}} run

# `just demo game` shows the deck overlay instead.
# Show a made-up pack beside Arena's window, whether or not Arena is in front.
demo WHAT="draft":
    BRAINSTORM_DEMO={{WHAT}} BRAINSTORM_ALWAYS=1 cargo +{{toolchain}} run

# Uploads to the live host once it has a token, like an installed app.
# Build dist/Brainstorm.app, signed (see scripts/bundle.sh).
bundle *ARGS:
    scripts/bundle.sh {{ARGS}}

# Pre-commit gate: format check, clippy, tests.
lint:
    cargo +{{toolchain}} fmt --check
    cargo +{{toolchain}} clippy --locked --all-targets -- -D warnings
    cargo +{{toolchain}} test
