//! Integration tests against a running emulator.
//!
//! ```sh
//! cargo test --features emulator-tests --test emulator
//! # also the tests that reboot the emulator:
//! cargo test --features emulator-tests --test emulator -- --include-ignored
//! ```
//!
//! See `common::device()` for how the device is selected.

mod common;

mod adb;
mod am;
mod client;
mod pm;
#[cfg(feature = "scanner")]
mod scanner;
mod shell;
