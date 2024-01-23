use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use cidr_utils::cidr::InetIterator;
use crossbeam_channel::Sender;
use itertools::Either;

use crate::scanner::{ClientResult, Scanner};
use crate::v2::error::Error;
use crate::v2::types::{Adb, AdbDevice, Client, ConnectionType};

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

		write!(f, "{:}    device {:}", self.conn, strings.join(" "))
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

		let tx = tx.clone();
		tp.execute(move || {
			let connected_devices: Vec<ClientResult> = adb
				.list_devices(debug)
				.unwrap_or(vec![])
				.iter()
				.filter_map(|device| if device.connected { Some(device.into()) } else { None })
				.collect::<Vec<_>>();

			for device in connected_devices {
				let _ = tx.send(Either::Right(device));
			}
			drop(tx);
		});
	}
}

impl ClientResult {
	pub fn new(addr: SocketAddr) -> ClientResult {
		ClientResult {
			conn: ConnectionType::TcpIp(addr),
			product: None,
			model: None,
			mac: None,
			version: None,
			device: None,
		}
	}
}

impl TryFrom<&ClientResult> for Client {
	type Error = Error;

	fn try_from(value: &ClientResult) -> Result<Self, Self::Error> {
		Client::try_from(value.conn.clone())
	}
}

impl From<&AdbDevice> for ClientResult {
	fn from(value: &AdbDevice) -> Self {
		ClientResult {
			conn: value.addr.clone(),
			product: Some(value.product.clone()),
			model: Some(value.model.clone()),
			device: Some(value.device.clone()),
			version: None,
			mac: None,
		}
	}
}

impl PartialEq for ClientResult {
	fn eq(&self, other: &Self) -> bool {
		self.conn == other.conn
	}
}

fn connect(adb: Arc<Adb>, host: &str, tcp_timeout: Duration, adb_timeout: Duration, debug: bool) -> Option<ClientResult> {
	let sock_addr = SocketAddr::from_str(host).ok();
	if sock_addr.is_none() {
		return None;
	}

	if let Ok(response) = std::net::TcpStream::connect_timeout(sock_addr.as_ref().unwrap(), tcp_timeout) {
		if let Ok(addr) = response.peer_addr() {
			let client = Client::new((*adb).clone(), ConnectionType::TcpIp(addr.clone()), debug);
			//let mut client = Client::try_from(ConnectionType::TcpIp(addr.clone())).unwrap().with_debug(debug);

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
					conn: ConnectionType::TcpIp(addr.clone()),
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

#[cfg(test)]
pub(crate) mod test {
	use std::str::FromStr;
	use std::time::{Duration, Instant};

	use cidr_utils::cidr::Ipv4Cidr;
	use cidr_utils::Ipv4CidrSize;
	use crossbeam_channel::unbounded;
	use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
	use itertools::Either;

	use crate::scanner::Scanner;
	use crate::v2::test::test::init_log;
	use crate::v2::types::Adb;

	#[test]
	fn test_scan() {
		init_log();

		let cidr = Ipv4Cidr::from_str("192.168.1.0/24").unwrap();
		let progress_style = ProgressStyle::with_template(
			"{prefix:.cyan.bold/blue.bold}: {elapsed_precise} [{bar:40.cyan/blue}] {percent:.bold}%. {msg} ",
		)
		.unwrap()
		.progress_chars("=> ");

		let multi_progress = MultiProgress::new();
		let progress = multi_progress.add(ProgressBar::new(cidr.size()));
		progress.set_style(progress_style.clone());
		progress.set_prefix("Scanning");

		let (tx, rx) = unbounded();
		let adb = Adb::new().expect("failed to find adb");

		let scanner = Scanner::default()
			.with_debug(false)
			.with_tcp_timeout(Duration::from_millis(200))
			.with_adb_timeout(Duration::from_millis(100));

		let start = Instant::now();
		scanner.scan(&adb, cidr.iter(), tx.clone());

		drop(tx);

		let mut result = Vec::new();
		for either in rx {
			match either {
				Either::Left(addr) => {
					progress.inc(1);
					progress.set_message(format!("{addr}..."));
				}
				Either::Right(client) => {
					if !result.contains(&client) {
						result.push(client);
					}
				}
			}
		}

		progress.finish_with_message(format!("Scanned {} IPs", cidr.size()));

		let elapsed = start.elapsed();

		println!("Time elapsed for scanning is: {:?}ms", elapsed.as_millis());
		println!("Found {:} devices", result.len());

		result.sort_by_key(|k| k.conn);

		for device in result.iter() {
			println!("{device}");
		}
	}
}
