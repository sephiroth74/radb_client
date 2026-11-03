#!/usr/bin/env -S just --justfile

alias d := dev
alias f := fmt
alias l := lint
alias t := test
alias te := test-emulator

# List available commands.
_default:
    just --list --unsorted

# Lint on every change.
dev:
    cargo watch -x 'clippy --all-targets --features scanner,clipboard'

# Format the codebase.
fmt:
    cargo fmt --all

# Check if the codebase is properly formatted.
fmt-check:
    cargo fmt --all -- --check

# Lint the codebase (warnings are errors).
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Unit tests and doctests: no device needed.
test:
    cargo test --features scanner,clipboard

# Start the radb-test emulator, headless, and wait for the boot (see README to create it).
emulator:
    "${ANDROID_HOME}/emulator/emulator" -avd radb-test -no-window -no-audio -no-boot-anim -no-snapshot -writable-system > /dev/null 2>&1 &
    adb -s emulator-5554 wait-for-device shell 'while [ "$(getprop sys.boot_completed)" != "1" ]; do sleep 1; done'

# Integration tests on a running emulator (e.g. `just test-emulator --include-ignored`).
test-emulator *args:
    cargo test --features emulator-tests,scanner --test emulator -- {{args}}

udeps:
    cargo +nightly udeps --all-targets

fix:
    cargo fix --all-targets --allow-no-vcs

# Tasks to make the code base comply with the rules. Mostly used in git hooks.
comply: fmt lint test

# Check if the repository complies with the rules and is ready to be pushed.
check: fmt-check lint test
