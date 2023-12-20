use std::net::SocketAddr;

use mac_address::MacAddress;

mod impls;

#[allow(dead_code)]
#[cfg(feature = "scanner")]
pub struct Scanner {}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(feature = "scanner")]
pub struct ClientResult {
	pub addr: SocketAddr,
	pub product: Option<String>,
	pub model: Option<String>,
	pub device: Option<String>,
	pub version: Option<String>,
	pub mac: Option<MacAddress>,
}
