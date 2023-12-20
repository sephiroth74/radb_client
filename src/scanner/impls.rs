use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;

use crate::errors::AdbError;
use crate::scanner::{ClientResult, Scanner};
use crate::{Adb, AdbClient, Device};

static TCP_TIMEOUT_MS: u64 = 400;

#[cfg(feature = "scanner")]
impl Display for ClientResult {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		//192.168.1.29:5555      device product:SwisscomBox23 model:IP2300 device:IP2300 transport_id:5
		let mut strings = vec![];

		if let Some(n) = self.product.as_ref() {
			strings.push(format!("product:{}", n));
		}

		if let Some(n) = self.model.as_ref() {
			strings.push(format!("model:{}", n));
		}

		if let Some(n) = self.device.as_ref() {
			strings.push(format!("device:{}", n));
		}

		if let Some(n) = self.mac.as_ref() {
			strings.push(format!("mac:{}", n));
		}

		if let Some(n) = self.version.as_ref() {
			strings.push(format!("version:{}", n));
		}

		write!(f, "{:}	device {:}", self.addr, strings.join(" "))
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
			product: None,
			model: None,
			mac: None,
			version: None,
			device: None,
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
	if let Ok(response) = tokio::time::timeout(Duration::from_millis(TCP_TIMEOUT_MS), tokio::net::TcpStream::connect(host.as_str())).await {
		if let Ok(stream) = response {
			if let Ok(addr) = stream.peer_addr() {
				//let device = adb.device(host.as_str()).unwrap();
				let client = adb.client(host.as_str()).unwrap();
				if client.connect(Some(Duration::from_millis(400))).is_ok() {
					let shell = client.shell();

					//let product_name = shell.getprop("ro.product.name").await;
					let model_name = shell.getprop("ro.product.vendor.model");
					let device_name = shell.getprop("ro.product.vendor.device");
					let stb_name = shell.getprop("persist.sys.stb.name");
					let sdk_version = shell.getprop("ro.build.version.sdk");
					let client_mac = client.get_mac_address();
					let _ = client.disconnect();

					//192.168.1.29:5555      device product:SwisscomBox23 model:IP2300 device:IP2300 transport_id:5

					Some(ClientResult {
						addr,
						product: stb_name.ok(),
						model: model_name.ok(),
						device: device_name.ok(),
						mac: client_mac.ok(),
						version: sdk_version.ok(),
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
