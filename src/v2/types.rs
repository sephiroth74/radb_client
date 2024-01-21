use std::net::SocketAddr;
use std::path::PathBuf;
use strum_macros::Display;

#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Adb(pub(crate) PathBuf);

#[allow(dead_code)]
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum ConnectionType {
	TcpIp(SocketAddr),
	Transport(u8),
	USB,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Client {
	pub adb: Adb,
	pub addr: ConnectionType,
	pub debug: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Shell<'a> {
	pub(crate) parent: &'a Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdbDevice {
	pub name: String,
	pub product: String,
	pub model: String,
	pub device: String,
	pub addr: ConnectionType,
}

#[derive(Debug, Display, Eq, PartialEq, Hash)]
pub enum Wakefulness {
	Awake,
	Asleep,
	Dreaming,
}
