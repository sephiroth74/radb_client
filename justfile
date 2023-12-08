#!/usr/bin/env -S just --justfile

alias d := dev
alias r := run
alias f := fmt
alias l := lint
alias t := test

# List available commands.
_default:
    just --list --unsorted

# Develop the app.
dev:
    cargo watch -x 'clippy --locked --all-targets --all-features'

# Develop the app.
run:
    touch qml/qml.qrc && cargo run

# Format the codebase.
fmt:
    cargo fmt --all -- --config-path ~/.config/rustfmt/rustfmt.toml

# Check if the codebase is properly formatted.
fmt-check:
    cargo fmt --all -- --check

# Lint the codebase.
lint:
    cargo clippy --locked --all-targets --all-features

# Test the codebase.
test:
    cargo test run --all-targets

udeps:
    cargo +nightly udeps --all-targets

fix:
    cargo fix --all-targets --allow-no-vcs

# Tasks to make the code base comply with the rules. Mostly used in git hooks.
comply: fmt lint test

# Check if the repository complies with the rules and is ready to be pushed.
check: fmt-check lint test