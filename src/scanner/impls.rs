use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender;

use crate::errors::AdbError;
use crate::scanner::{ClientResult, Scanner};
use crate::{Adb, AdbClient, Device};

static TCP_TIMEOUT_MS: u64 = 400;

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

		write!(f, "{:}    device {:}", self.addr, strings.join(" "))
	}
}

#[allow(dead_code)]
impl Scanner {
	pub fn new() -> Scanner {
		Scanner {}
	}

	pub fn scan(&self, adb: &Adb, connection_timeout: Option<Duration>, tx: Sender<Option<ClientResult>>) {
		let adb = Arc::new(adb.clone());
		let cpus = std::thread::available_parallelism()
			.map(|s| s.get())
			.unwrap_or(num_cpus::get());
		let tp = threadpool::ThreadPool::new(cpus);

		for i in 0..256 {
			let adb = Arc::clone(&adb);
			let tx = tx.clone();

			tp.execute(move || {
				let addr = format!("192.168.1.{:}:5555", i);
				let _ = tx.send(connect(
					adb,
					addr.as_str(),
					connection_timeout.unwrap_or(Duration::from_millis(TCP_TIMEOUT_MS)),
				));
				drop(tx);
			});
		}
	}
}

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

fn connect(adb: Arc<Adb>, host: &str, timeout: Duration) -> Option<ClientResult> {
	let sock_addr = SocketAddr::from_str(host).ok();
	if sock_addr.is_none() {
		return None;
	}

	if let Ok(response) = std::net::TcpStream::connect_timeout(sock_addr.as_ref().unwrap(), timeout) {
		if let Ok(addr) = response.peer_addr() {
			//let device = adb.device(host.as_str()).unwrap();
			let client = adb.client(host).unwrap();
			if client.connect(Some(Duration::from_millis(400))).is_ok() {
				let shell = client.shell();
				let root = client.root().unwrap_or(false);

				//let product_name = shell.getprop("ro.product.name").await;
				let sdk_version = shell.getprop("ro.build.version.sdk");
				let model_name = shell.getprop("ro.product.model");
				let device_name = shell.getprop("ro.product.device");
				let stb_name = shell.getprop("persist.sys.stb.name");
				let client_mac = if root { client.get_mac_address().ok() } else { None };
				let _ = client.try_disconnect();

				//192.168.1.29:5555      device product:SwisscomBox23 model:IP2300 device:IP2300 transport_id:5

				Some(ClientResult {
					addr,
					product: stb_name.ok(),
					model: model_name.ok(),
					device: device_name.ok(),
					mac: client_mac,
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
}
