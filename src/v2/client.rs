use std::fmt::{Display, Formatter};
use std::process::Stdio;
use std::time::Duration;

use rustix::path::Arg;
use simple_cmd::prelude::OutputExt;
use simple_cmd::{Cmd, CommandBuilder};

use crate::v2::error::Error;
use crate::v2::prelude::*;
use crate::v2::types::{Adb, AdbDevice, Client, ConnectionType, Shell, Wakefulness};

static GET_STATE_TIMEOUT: u64 = 200u64;

impl Client {
	pub fn new(adb: Adb, addr: ConnectionType, debug: bool) -> Self {
		Client { adb, addr, debug }
	}

	/// Attempt to connect to a tcp/ip client, optionally waiting until the given
	/// timeout expires.
	/// If debug is set to true, the executed command will be logged out.
	pub fn connect(&self, timeout: Option<Duration>) -> crate::v2::result::Result<()> {
		if self.is_connected() {
			return Ok(());
		}

		let addr = match self.addr {
			ConnectionType::TcpIp(ip) => ip.ip(),
			_ => return Err(Error::InvalidConnectionTypeError),
		};

		let mut command = CommandBuilder::adb(&self.adb).with_debug(self.debug);
		command = command.arg("connect").arg(addr.to_string()).timeout(timeout);

		let output = command.build().output()?;

		if output.error() {
			return Err(Error::IoError(std::io::Error::from(std::io::ErrorKind::NotConnected)));
		} else {
			match self.is_connected() {
				true => Ok(()),
				false => Err(Error::IoError(std::io::Error::from(std::io::ErrorKind::NotConnected))),
			}
		}
	}

	/// Disconnect a device.
	/// Note that if the connection type is not tcp/ip, all devices
	/// will be disconnected
	pub fn disconnect(&self) -> crate::v2::result::Result<bool> {
		let mut command = CommandBuilder::adb(&self.adb).with_debug(self.debug);
		command = command.arg("disconnect");
		command = match self.addr {
			ConnectionType::TcpIp(ip) => command.arg(ip.to_string()),
			_ => command,
		};

		match command.build().output() {
			Ok(output) => Ok(output.success()),
			Err(err) => Err(Error::CommandError(err)),
		}
	}

	/// Checks if the client is already connected
	pub fn is_connected(&self) -> bool {
		let mut command = CommandBuilder::from(self);
		command = command
			.arg("get-state")
			.timeout(Some(Duration::from_millis(GET_STATE_TIMEOUT)));
		let output = command.build().output();
		return if let Ok(output) = output { output.success() } else { false };
	}

	/// Wait for device to be available with an optional timeout
	pub fn wait_for_device(&self, timeout: Option<Duration>) -> crate::v2::result::Result<()> {
		CommandBuilder::from(self)
			.args([
				"wait-for-device",
				"shell",
				"while [[ -z $(getprop sys.boot_completed) ]]; do sleep 1; done; input keyevent 143",
			])
			.timeout(timeout)
			.build()
			.output()?;
		Ok(())
	}

	/// Get the current awake status
	pub fn get_wakefulness(&self) -> crate::v2::result::Result<Wakefulness> {
		let command1 = CommandBuilder::from(self)
			.args(vec![
				"shell", "dumpsys", "power",
			])
			.build();
		let command2 = Cmd::builder("sed")
			.arg("-n")
			.arg("s/mWakefulness=\\(\\S*\\)/\\1/p")
			.with_debug(self.debug)
			.stdout(Some(Stdio::piped()))
			.build();

		let result = command1.pipe(command2)?;
		let awake = Arg::as_str(&result.stdout)?.trim();
		Ok(awake.try_into()?)
	}

	/// return the adb root status for the current connection
	pub fn is_root(&self) -> crate::v2::result::Result<bool> {
		self.shell().is_root()
	}

	/// return the client shell interface
	pub fn shell(&self) -> Shell {
		Shell { parent: self }
	}

	/// Add debug tracing to connection
	pub fn with_debug(mut self, debug: bool) -> Self {
		self.debug = debug;
		self
	}
}

impl TryFrom<ConnectionType> for Client {
	type Error = crate::v2::error::Error;

	fn try_from(value: ConnectionType) -> Result<Self, Self::Error> {
		let adb = Adb::new()?;
		Ok(Client::new(adb, value, false))
	}
}

impl TryFrom<AdbDevice> for Client {
	type Error = crate::v2::error::Error;

	fn try_from(value: AdbDevice) -> Result<Self, Self::Error> {
		value.addr.try_into()
	}
}

impl TryFrom<&AdbDevice> for Client {
	type Error = crate::v2::error::Error;

	fn try_from(value: &AdbDevice) -> Result<Self, Self::Error> {
		value.addr.try_into()
	}
}

impl From<&Client> for CommandBuilder {
	fn from(value: &Client) -> Self {
		CommandBuilder::adb(&value.adb).addr(value.addr).with_debug(value.debug)
	}
}

impl Display for Client {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		self.addr.fmt(f)
	}
}

#[cfg(test)]
mod test {
	use std::time::Duration;

	use crate::v2::test::test::{
		client_from, connect_client, connect_emulator, connection_from_tcpip, connection_from_transport_id, init_log,
	};
	use crate::v2::types::ConnectionType;

	#[test]
	fn test_new_client() {
		let address: ConnectionType = connection_from_tcpip();
		let mut client = client_from(address);
		client = client.with_debug(true);
		let connected = client.is_connected();
		println!("connected: {}", connected);

		let mut client = client_from(connection_from_transport_id());
		client = client.with_debug(true);
		let connected = client.is_connected();
		println!("connected: {}", connected);
	}

	#[test]
	fn test_connect() {
		init_log();
		let client = client_from(connection_from_tcpip());
		let _ = client.connect(Some(Duration::from_secs(1))).expect("failed to connect");
	}

	#[test]
	fn test_disconnect() {
		init_log();
		let client = client_from(connection_from_transport_id());
		let disconnected = client.disconnect().expect("failed to disconnect");
		println!("disconnected: {disconnected}");
	}

	#[test]
	fn test_wait_for_device() {
		init_log();
		let client = connect_client(connection_from_tcpip());
		client
			.wait_for_device(Some(Duration::from_secs(1)))
			.expect("failed to wait for device");

		let client = connect_emulator();
		client.wait_for_device(None).expect("failed to wait for emulator");
	}

	#[test]
	fn test_get_wakefulness() {
		init_log();
		let client = connect_client(connection_from_tcpip());
		let awake = client.get_wakefulness().expect("failed to get awake status");
		println!("awake status: {awake}");

		let client = connect_emulator();
		let awake = client.get_wakefulness().expect("failed to get awake status");
		println!("awake status: {awake}");
	}

	#[test]
	fn test_is_root() {
		init_log();
		let client = connect_emulator();
		let is_root = client.is_root().expect("failed to get root status");
		println!("client {client} is root: {is_root}");
	}
}
