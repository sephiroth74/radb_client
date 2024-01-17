use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use cidr_utils::cidr::InetIterator;
use crossbeam_channel::Sender;
use itertools::Either;

use crate::errors::AdbError;
use crate::scanner::{ClientResult, Scanner};
use crate::{Adb, AdbClient, Device};

static TCP_TIMEOUT_MS: u64 = 200;
static ADB_TIMEOUT_MS: u64 = 100;

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

impl Default for Scanner {
	fn default() -> Self {
		Scanner::new(
			Duration::from_millis(TCP_TIMEOUT_MS),
			Duration::from_millis(ADB_TIMEOUT_MS),
			true,
		)
	}
}

#[allow(dead_code)]
impl Scanner {
	pub fn new(tcp_timeout: Duration, adb_timeout: Duration, debug: bool) -> Scanner {
		Scanner {
			tcp_timeout,
			adb_timeout,
			debug,
		}
	}

	pub fn with_debug(mut self, debug: bool) -> Self {
		self.debug = debug;
		self
	}

	pub fn with_tcp_timeout(mut self, timeout: Duration) -> Self {
		self.tcp_timeout = timeout;
		self
	}

	pub fn with_adb_timeout(mut self, timeout: Duration) -> Self {
		self.adb_timeout = timeout;
		self
	}

	pub fn scan<I>(&self, adb: &Adb, iterator: I, tx: Sender<Either<String, ClientResult>>)
	where
		I: Into<InetIterator<Ipv4Addr>>,
	{
		let adb = Arc::new(adb.clone());
		let cpus = std::thread::available_parallelism()
			.map(|s| s.get())
			.unwrap_or(num_cpus::get());
		let tp = threadpool::ThreadPool::new(cpus);

		let tcp_timeout = self.tcp_timeout.clone();
		let adb_timeout = self.adb_timeout.clone();
		let debug = self.debug;

		for ip in iterator.into() {
			let adb = Arc::clone(&adb);
			let tx = tx.clone();

			tp.execute(move || {
				let addr = format!("{}:5555", ip.address());
				let _ = tx.send(Either::Left(addr.clone()));
				if let Some(result) = connect(adb, &addr, tcp_timeout, adb_timeout, debug) {
					let _ = tx.send(Either::Right(result));
				}
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

fn connect(adb: Arc<Adb>, host: &str, tcp_timeout: Duration, adb_timeout: Duration, debug: bool) -> Option<ClientResult> {
	let sock_addr = SocketAddr::from_str(host).ok();
	if sock_addr.is_none() {
		return None;
	}

	if let Ok(response) = std::net::TcpStream::connect_timeout(sock_addr.as_ref().unwrap(), tcp_timeout) {
		if let Ok(addr) = response.peer_addr() {
			//let device = adb.device(host.as_str()).unwrap();
			let mut client = adb.client(host).unwrap();
			client.debug = debug;

			if client.connect(Some(adb_timeout)).is_ok() {
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
