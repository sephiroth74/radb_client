use mac_address::MacAddress;

use crate::types::ConnectionType;

#[cfg(feature = "scanner")]
mod impls;

#[allow(dead_code)]
#[cfg(feature = "scanner")]
pub struct Scanner {
	tcp_timeout: core::time::Duration,
	adb_timeout: core::time::Duration,
	debug: bool,
}

#[derive(Debug, Clone)]
#[cfg(feature = "scanner")]
pub struct ClientResult {
	pub conn: ConnectionType,
	pub product: Option<String>,
	pub model: Option<String>,
	pub device: Option<String>,
	pub version: Option<String>,
	pub mac: Option<MacAddress>,
}
