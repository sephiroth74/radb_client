use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;

use crate::errors::AdbError;
use crate::scanner::{ClientResult, Scanner};
use crate::{Adb, AdbClient, Client, Device};

#[cfg(feature = "scanner")]
impl Display for ClientResult {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let mut strings = vec![];

		if let Some(n) = self.name.as_ref() {
			strings.push(format!("name:{}", n));
		}

		if let Some(n) = self.mac.as_ref() {
			strings.push(format!("mac:{}", n));
		}

		if let Some(n) = self.version.as_ref() {
			strings.push(format!("version:{}", n));
		}

		write!(f, "{:}	{:}", self.addr, strings.join(" "))
	}
}

#[allow(dead_code)]
#[cfg(feature = "scanner")]
impl Scanner {
	pub fn new() -> Scanner {
		Scanner {}
	}

	pub async fn scan(&self) -> Result<Vec<ClientResult>, AdbError> {
		let adb = Arc::new(Adb::new()?);
		let pool = crate::future::ThreadPool::default();
		let mut tasks = vec![];

		for i in 0..256 {
			let adb = Arc::clone(&adb);
			let task = pool.spawn(connect(adb, format!("192.168.1.{:}:5555", i)));
			tasks.push(task);
		}

		let result = join_all(tasks).await.iter().filter_map(|f| f.to_owned()).collect::<Vec<_>>();
		Ok(result)
	}
}

#[cfg(feature = "scanner")]
impl ClientResult {
	pub fn new(addr: SocketAddr) -> ClientResult {
		ClientResult {
			addr,
			name: None,
			mac: None,
			version: None,
		}
	}
}

impl TryInto<AdbClient> for ClientResult {
	type Error = AdbError;

	fn try_into(self) -> Result<AdbClient, Self::Error> {
		match Device::try_from_sock_addr(&self.addr) {
			Ok(device) => AdbClient::try_from_device(device),
			Err(err) => Err(AdbError::AddrParseError(err)),
		}
	}
}

impl TryFrom<&ClientResult> for AdbClient {
	type Error = AdbError;

	fn try_from(value: &ClientResult) -> Result<Self, Self::Error> {
		match Device::try_from_sock_addr(&value.addr) {
			Ok(device) => AdbClient::try_from_device(device),
			Err(err) => Err(AdbError::AddrParseError(err)),
		}
	}
}

#[cfg(feature = "scanner")]
async fn connect(adb: Arc<Adb>, host: String) -> Option<ClientResult> {
	if let Ok(response) = tokio::time::timeout(Duration::from_millis(200), tokio::net::TcpStream::connect(host.as_str())).await {
		if let Ok(stream) = response {
			if let Ok(addr) = stream.peer_addr() {
				let device = adb.device(host.as_str()).unwrap();
				if Client::connect(&adb, device.as_ref(), Some(Duration::from_millis(400))).await.is_ok() {
					let client_name = Client::name(&adb, device.as_ref()).await;
					let client_mac = Client::get_mac_address(&adb, device.as_ref()).await;
					let version = Client::api_level(&adb, device.as_ref()).await;
					let _ = Client::disconnect(&adb, device.as_ref()).await;

					Some(ClientResult {
						addr,
						name: client_name.ok(),
						mac: client_mac.map_or(None, |m| Some(m)),
						version: version.map_or(None, |m| Some(m)),
					})
				} else {
					Some(ClientResult::new(addr))
				}
			} else {
				None
			}
		} else {
			None
		}
	} else {
		None
	}
}
