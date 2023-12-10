use std::fmt::Debug;
use std::net::SocketAddr;
use std::path::PathBuf;

pub mod adb;
pub mod client;
pub mod command;
pub mod debug;
pub mod future;
pub mod impls;
pub mod input;
pub mod intent;
pub mod macros;
pub mod scanner;
pub mod shell;
pub mod traits;
pub mod types;
pub mod util;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Adb(PathBuf);

pub struct Shell {}

pub struct Client {}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum AddressType {
	Sock(SocketAddr),
	Name(String),
	Transport(u8),
}

#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct DeviceAddress(AddressType);

#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Device(DeviceAddress);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SELinuxType {
	Enforcing,
	Permissive,
}

mod errors;
mod tests;
