#![doc = include_str!("../README.md")]

pub mod cmdline_tools;
pub mod error;
#[cfg(feature = "scanner")]
pub mod scanner;
pub mod types;
pub mod utils;

mod adb;
mod am;
mod client;
mod command;
mod connection;
mod pm;
mod shell;

pub use error::{CommandFailure, Error, Result};
pub use types::*;
