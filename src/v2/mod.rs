#![doc = include_str!("../../README.md")]

pub mod error;
pub mod result;
pub mod traits;
pub mod types;

mod adb;
mod am;
mod client;
mod connection_type;
mod dump_util;
mod impls;
mod pm;
mod prelude;
mod shell;

pub(crate) mod test;
