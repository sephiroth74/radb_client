use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use mac_address::MacAddress;

use crate::{Adb, Client};

#[allow(dead_code)]
pub struct Scanner {}

#[allow(dead_code)]
impl Scanner {
	pub fn new() -> Scanner {
		Scanner {}
	}

	pub async fn scan(&self) -> Vec<ClientResult> {
		let adb = Arc::new(Adb::new().unwrap());
		let pool = crate::future::ThreadPool::default();
		let mut tasks = vec![];

		for i in 0..256 {
			let adb = Arc::clone(&adb);
			let task = pool.spawn(connect(adb, format!("192.168.1.{:}:5555", i)));
			tasks.push(task);
		}

		join_all(tasks).await.iter().filter_map(|f| f.to_owned()).collect::<Vec<_>>()
	}
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientResult {
	pub addr: SocketAddr,
	pub name: Option<String>,
	pub mac: Option<MacAddress>,
	pub version: Option<u8>,
}

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

impl Display for ClientResult {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let mut strings = vec![format!("addr={:}", self.addr)];
		if let Some(n) = self.name.as_ref() {
			strings.push(format!("name={}", n));
		}
		if let Some(n) = self.mac.as_ref() {
			strings.push(format!("mac={}", n));
		}
		if let Some(n) = self.version.as_ref() {
			strings.push(format!("version={}", n));
		}
		write!(f, "ClientResult({:})", strings.join(", "))
	}
}
