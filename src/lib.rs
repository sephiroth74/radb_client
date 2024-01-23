#![doc = include_str!("../README.md")]

#[cfg(feature = "scanner")]
pub mod scanner;

pub mod error;
pub mod result;
pub mod traits;
pub mod types;

pub(crate) mod adb;
pub(crate) mod am;
pub(crate) mod client;
pub(crate) mod connection_type;
pub(crate) mod dump_util;
pub(crate) mod impls;
pub(crate) mod pm;
pub(crate) mod prelude;
pub(crate) mod shell;
pub(crate) mod test;
