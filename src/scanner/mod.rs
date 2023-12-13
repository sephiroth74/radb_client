mod impls;

use mac_address::MacAddress;
use std::net::SocketAddr;

#[allow(dead_code)]
#[cfg(feature = "scanner")]
pub struct Scanner {}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(feature = "scanner")]
pub struct ClientResult {
	pub addr: SocketAddr,
	pub name: Option<String>,
	pub mac: Option<MacAddress>,
	pub version: Option<u8>,
}
