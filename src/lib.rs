use std::fmt::Debug;
use std::path::PathBuf;

use crate::types::DeviceAddress;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Adb(PathBuf);

pub struct Shell {}

pub struct Client {}

#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Device(DeviceAddress);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManager<'a> {
	pub(crate) parent: AdbShell<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityManager<'a> {
	pub(crate) parent: AdbShell<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbClient {
	pub(crate) adb: Adb,
	pub device: Device,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbShell<'a> {
	pub(crate) parent: &'a AdbClient,
}

pub mod adb;

pub mod client;

pub mod debug;

pub mod future;

pub mod macros;

pub mod shell;

pub mod traits;

pub mod types;

pub mod am;

pub mod dump_util;

pub mod errors;

pub mod pm;

mod process;

#[cfg(feature = "scanner")]
pub mod scanner;

pub mod impls;

mod tests;
