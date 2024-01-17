#![doc = include_str!("../README.md")]

use std::fmt::Debug;
use std::path::PathBuf;

use crate::errors::AdbError;
use crate::types::DeviceAddress;

pub type Result<T> = std::result::Result<T, AdbError>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Adb(PathBuf);

pub struct Shell {}

pub struct Client {}

#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Device(DeviceAddress);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageManager<'a> {
	pub(crate) parent: AdbShell<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActivityManager<'a> {
	pub(crate) parent: AdbShell<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdbClient {
	pub adb: Adb,
	pub device: Device,
	pub debug: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdbShell<'a> {
	pub(crate) parent: &'a AdbClient,
}

pub mod adb;

pub mod client;

pub mod macros;

pub mod shell;

pub mod traits;

pub mod types;

pub mod am;

pub mod dump_util;

pub mod errors;

pub mod pm;

#[cfg(feature = "scanner")]
pub mod scanner;

pub mod impls;

mod cmd_ext;
mod debug;
mod tests;
