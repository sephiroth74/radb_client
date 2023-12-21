use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::unbounded;
use itertools::Itertools;
use log::trace;

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
	//
	//pub async fn scan(&self) -> Result<Vec<ClientResult>, AdbError> {
	//	let adb = Arc::new(Adb::new()?);
	//	let pool = crate::future::ThreadPool::default();
	//	let mut tasks = vec![];
	//
	//	for i in 0..256 {
	//		let adb = Arc::clone(&adb);
	//		let task = pool.spawn(connect(adb, format!("192.168.1.{:}:5555", i)));
	//		tasks.push(task);
	//	}
	//
	//	let result = join_all(tasks).await.iter().filter_map(|f| f.to_owned()).collect::<Vec<_>>();
	//	Ok(result)
	//}

	pub fn scan(&self) -> Result<Vec<ClientResult>, AdbError> {
		let (tx, rx) = unbounded();

		let adb = Arc::new(Adb::new()?);
		let cpus = std::thread::available_parallelism().map(|s| s.get()).unwrap_or(num_cpus::get());
		trace!("num of cores: {}", cpus);

		let tp = threadpool::ThreadPool::new(cpus);

		for i in 0..256 {
			let adb = Arc::clone(&adb);
			let tx = tx.clone();

			tp.execute(move || {
				let addr = format!("192.168.1.{:}:5555", i);
				if let Some(result) = connect(adb, addr.as_str()) {
					let _ = tx.send(result);
				}
				drop(tx);
			});
		}

		tp.join();
		drop(tx);

		let r = rx.iter().collect_vec();
		Ok(r)
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
#[allow(dead_code)]
async fn connect_async(adb: Arc<Adb>, host: String) -> Option<ClientResult> {
	if let Ok(response) = tokio::time::timeout(Duration::from_millis(TCP_TIMEOUT_MS), tokio::net::TcpStream::connect(host.as_str())).await {
		if let Ok(stream) = response {
			if let Ok(addr) = stream.peer_addr() {
				//let device = adb.device(host.as_str()).unwrap();
				let client = adb.client(host.as_str()).unwrap();
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
	} else {
		None
	}
}

#[cfg(feature = "scanner")]
fn connect(adb: Arc<Adb>, host: &str) -> Option<ClientResult> {
	let sock_addr = SocketAddr::from_str(host).ok();
	if sock_addr.is_none() {
		return None;
	}

	if let Ok(response) = std::net::TcpStream::connect_timeout(sock_addr.as_ref().unwrap(), Duration::from_millis(TCP_TIMEOUT_MS)) {
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
