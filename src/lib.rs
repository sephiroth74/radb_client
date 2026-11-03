#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub mod cmdline_tools;
/// The error type and [`Result`].
pub mod error;
#[cfg(feature = "scanner")]
pub mod scanner;
pub mod types;
/// Host helpers: Android SDK paths, shell quoting.
pub mod utils;

mod adb;
mod am;
mod apk;
mod client;
mod command;
mod connection;
mod pm;
mod shell;
mod system_app;

pub use error::{CommandFailure, Error, Result};
pub use types::*;
