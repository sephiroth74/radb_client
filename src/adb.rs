use std::ffi::OsStr;
use std::fmt::{Debug, Formatter};
use std::io::BufRead;
use std::path::PathBuf;
use std::process::Output;
use std::time::Duration;

use crossbeam_channel::Receiver;
use lazy_static::lazy_static;
use regex::Regex;
use simple_cmd::prelude::OutputExt;
use simple_cmd::{Cmd, CommandBuilder};
use which::which;

use crate::error::Error;
use crate::prelude::*;
use crate::result::Result;
use crate::types::{Adb, AdbDevice, ConnectionType};

impl Adb {
	/// Create a new adb instance, or error if abd cannot be found in the user PATH.
	///
	/// # Examples:
	/// ```rust
	/// use radb_client::types::Adb;
	/// let adb = Adb::new().expect("failed to find adb");
	/// ```
	pub fn new() -> Result<Adb> {
		let result = which("adb");
		match result {
			Ok(path) => Ok(Adb::from(path)),
			Err(err) => Err(Error::AdbNotFoundError(err)),
		}
	}

	/// Execute a custom `adb` command with an optional cancel signal and timeout.
	/// Use debug true to toggle tracing verbosity.
	///
	/// # Examples:
	/// ```rust
	/// use radb_client::types::{Adb, ConnectionType};
	///
	/// let adb = Adb::new().expect("failed to find adb");
	/// let addr = ConnectionType::try_from("192.168.1.100:5555").unwrap();
	///
	/// // this will execute the command: adb -s 192.168.1.100:5555 get-state
	/// match adb.exec(addr, vec!["get-state"], None, None, true) {
	/// 	Ok(output) => println!("output: {output}"),
	/// 	Err(err) => eprintln!("error: {err}"),
	/// }
	/// ```
	pub fn exec<'a, C, I, S>(
		&self,
		addr: C,
		args: I,
		cancel: Option<Receiver<()>>,
		timeout: Option<Duration>,
		debug: bool,
	) -> Result<Output>
	where
		C: Into<ConnectionType>,
		I: IntoIterator<Item = S>,
		S: AsRef<OsStr>,
	{
		let builder = CommandBuilder::adb(&self)
			.addr(addr)
			.with_debug(debug)
			.args(args)
			.signal(cancel)
			.timeout(timeout);
		Ok(builder.build().output()?)
	}

	/// Check if mdns is available
	/// # Examples
	/// ```rust
	/// 	use radb_client::types::Adb;
	///
	/// 	let adb = Adb::new().expect("failed to find adb");
	///    	let mdns = adb.mdns_check(true);
	///    	println!("mdns available: {mdns}");
	/// ```
	pub fn mdns_check(&self, debug: bool) -> bool {
		CommandBuilder::adb(&self)
			.with_debug(debug)
			.args(&[
				"mdns", "check",
			])
			.build()
			.output()
			.map(|output| output.success())
			.unwrap_or(false)
	}

	/// List connected devices
	///
	/// # Examples:
	/// ```rust
	/// use lazy_static::lazy_static;
	/// use regex::Regex;
	/// use radb_client::types::{Adb, Client};
	///
	/// fn connect_emulator() -> Client {
	///    	lazy_static! {
	///    		static ref RE: Regex = Regex::new(r#"^emulator-*.+$"#).unwrap();
	///    	}
	///    	let devices = Adb::new().unwrap().list_devices(true).expect("failed to list devices");
	///    	let device = devices
	///    		.iter()
	///    		.find(|device| {
	///    			println!("Checking {device}...");
	///    			RE.is_match(&device.name)
	///    		}).expect("no emulator found");
	///    		Client::try_from(device).expect("failed to create client from device")
	/// }
	///
	/// ```
	pub fn list_devices(&self, debug: bool) -> Result<Vec<AdbDevice>> {
		let output = Cmd::builder(self.0.as_path())
			.args([
				"devices", "-l",
			])
			.with_debug(debug)
			.build()
			.output()?;

		lazy_static! {
			static ref RE: Regex = Regex::new(
				"(?P<ip>[^\\s]+)[\\s]+(?P<status>device|offline) product:(?P<device_product>[^\\s]+)\\smodel:(?P<model>[^\\s]+)\\sdevice:(?P<device>[^\\s]+)\\stransport_id:(?P<transport_id>[^\\s]+)"
			)
			.unwrap();
		}

		let mut devices: Vec<AdbDevice> = vec![];
		for line in output.stdout.lines() {
			let line_str = line?;

			if RE.is_match(line_str.as_str()) {
				let captures = RE.captures(line_str.as_str());
				match captures {
					None => {}
					Some(c) => {
						let ip = c.name("ip").unwrap().as_str();
						let product = c.name("device_product").unwrap().as_str();
						let model = c.name("model").unwrap().as_str();
						let device = c.name("device").unwrap().as_str();
						let tr = c.name("transport_id").unwrap().as_str().parse::<u8>()?;
						let connected = c.name("status").unwrap().as_str() == "device";

						if let Ok(d) = match ConnectionType::try_from_ip(ip) {
							Ok(addr) => Ok::<ConnectionType, crate::error::Error>(addr),
							Err(_) => Ok(ConnectionType::Transport(tr)),
						} {
							let device = AdbDevice {
								name: ip.to_string(),
								product: product.to_string(),
								model: model.to_string(),
								device: device.to_string(),
								connected,
								addr: d,
							};
							devices.push(device)
						}
					}
				}
			}
		}
		Ok(devices)
	}

	/// Disconnect all connected devices.
	///
	/// # Arguments
	///
	/// * `debug` - A boolean to toggle tracing verbosity.
	///
	/// # Returns
	///
	/// * `Result<bool>` - A boolean indicating whether the disconnection was successful.
	pub fn disconnect_all(&self, debug: bool) -> Result<bool> {
		match Cmd::builder(self.0.as_path())
			.with_debug(debug)
			.arg("disconnect")
			.build()
			.output()
		{
			Ok(output) => Ok(output.success()),
			Err(err) => Err(Error::CommandError(err)),
		}
	}

	/// Kill the adb server.
	///
	/// # Arguments
	///
	/// * `debug` - A boolean to toggle tracing verbosity.
	///
	/// # Returns
	///
	/// * `Result<bool>` - A boolean indicating whether the server was successfully killed.
	pub fn kill_server(&self, debug: bool) -> Result<bool> {
		let output = Cmd::builder(self.0.as_path())
			.with_debug(debug)
			.arg("kill-server")
			.build()
			.output()?;
		Ok(output.success())
	}

	/// Start the adb server.
	///
	/// # Arguments
	///
	/// * `debug` - A boolean to toggle tracing verbosity.
	///
	/// # Returns
	///
	/// * `Result<bool>` - A boolean indicating whether the server was successfully started.
	///
	pub fn start_server(&self, debug: bool) -> Result<bool> {
		let output = Cmd::builder(self.0.as_path())
			.with_debug(debug)
			.arg("start-server")
			.build()
			.output()?;
		Ok(output.success())
	}

	/// Retrieve the version of the adb tool.
	///
	/// # Arguments
	///
	/// * `debug` - A boolean to toggle tracing verbosity.
	///
	/// # Returns
	///
	/// * `Result<String>` - The version string of the adb tool.
	///
	/// # Examples
	///
	/// ```rust
	/// use radb_client::types::Adb;
	/// let adb = Adb::new().expect("adb not found");
	/// let version = adb.version(true).expect("failed to get adb version");
	/// println!("version: {version}");
	/// ```
	pub fn version(&self, debug: bool) -> Result<String> {
		lazy_static! {
			static ref RE: Regex = Regex::new(r#"^Version\s+(?P<version>[\d+\.-]+)$"#).unwrap();
		}

		let output = CommandBuilder::adb(&self)
			.with_debug(debug)
			.arg("--version")
			.build()
			.output()?;
		let result = rustix::path::Arg::as_str(&output.stdout)?.trim();

		let r: Vec<_> = result
			.lines()
			.filter_map(|line| {
				if let Some(capture) = RE.captures(line) {
					Some(capture.name("version").unwrap().as_str())
				} else {
					None
				}
			})
			.collect();

		Ok(r.get(0)
			.map(|s| s.to_string())
			.ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?)
	}

	pub fn as_os_str(&self) -> &OsStr {
		self.as_ref()
	}
}

impl From<PathBuf> for Adb {
	fn from(value: PathBuf) -> Self {
		Adb(value)
	}
}

impl std::fmt::Display for Adb {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", self.0.to_str())
	}
}

impl Debug for Adb {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		self.0.fmt(f)
	}
}

impl AsRef<OsStr> for Adb {
	fn as_ref(&self) -> &OsStr {
		self.0.as_ref()
	}
}

impl Into<PathBuf> for Adb {
	fn into(self) -> PathBuf {
		self.0.clone()
	}
}

#[cfg(test)]
pub(crate) mod test {
	use std::path::PathBuf;
	use which::which;

	use crate::test::test::init_log;
	use crate::types::{Adb, Client, ConnectionType};

	static DEVICE_IP: &'static str = "192.168.1.101:5555";

	#[test]
	fn test_adb() {
		let _adb = Adb::new().expect("failed to find adb command in you PATH");
	}

	#[test]
	fn test_adb_from() {
		let path = which("adb").expect("failed to find adb");
		let adb = Adb::from(path);
		println!("adb: {}", adb);
	}

	#[test]
	fn test_debug_display() {
		let w = which::which("adb").expect("failed to find adb command in you PATH");
		let adb = Adb::new().expect("failed to find adb command in you PATH");

		assert_eq!(w.to_str(), adb.as_ref().to_str());
		assert_eq!(format!("{:?}", w.to_str()), adb.to_string());
		assert_eq!(format!("{w:#?}"), format!("{adb:#?}"));

		assert_eq!(w, adb.as_ref());
		assert_eq!(w, adb.as_os_str());

		let path: PathBuf = adb.into();
		assert_eq!(w, path);
	}

	#[test]
	fn test_exec() {
		init_log();
		let adb = Adb::new().expect("failed to find adb");
		let addr = ConnectionType::try_from(DEVICE_IP).unwrap();
		let result = adb.exec(addr, vec!["get-state"], None, None, true).unwrap();
		println!("result: {result:?}");

		let addr = ConnectionType::Transport(4);
		let result = adb.exec(addr, vec!["get-state"], None, None, true).unwrap();
		println!("result: {result:?}");
	}

	#[test]
	fn test_mdns_check() {
		init_log();
		let adb = Adb::new().expect("failed to find adb");
		let mdns = adb.mdns_check(true);
		println!("mdns available: {mdns}");
	}

	#[test]
	fn test_list_devices() {
		init_log();
		let adb = Adb::new().expect("failed to find adb");
		let devices = adb.list_devices(true).expect("failed to list devices");
		let devices_count = devices.len();
		println!("devices attached: {devices:#?}");

		let clients: Vec<Client> = devices
			.into_iter()
			.map(|device| device.try_into().expect("failed to convert AdbDevice into Client"))
			.collect();
		assert_eq!(devices_count, clients.len());
	}

	#[test]
	fn test_disconnect_all() {
		init_log();
		let adb = Adb::new().expect("failed to find adb");
		let disconnected = adb.disconnect_all(true).expect("failed to disconnect al devices");
		println!("disconnected: {disconnected}");
	}

	#[test]
	fn test_restart_server() {
		init_log();
		let adb = Adb::new().expect("adb not found");
		adb.kill_server(true).expect("failed to kill-server");
		adb.start_server(true).expect("failed to start-server");
	}

	#[test]
	fn test_get_version() {
		init_log();
		let adb = Adb::new().expect("adb not found");
		let version = adb.version(true).expect("failed to get adb version");
		println!("version: {version}");
	}
}
