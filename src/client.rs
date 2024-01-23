use std::borrow::Cow;
use std::env::temp_dir;

use std::fs::File;
use std::process::{Output, Stdio};
use std::thread::sleep;
use std::time::Duration;

use arboard::ImageData;
use crossbeam_channel::Receiver;
use mac_address::MacAddress;
use rustix::path::Arg;
use simple_cmd::debug::CommandDebug;
use simple_cmd::prelude::OutputExt;
use simple_cmd::{Cmd, CommandBuilder};
use uuid::Uuid;

use crate::error::Error;
use crate::prelude::*;
use crate::result::Result;
use crate::traits::AsArgs;
use crate::types::{Adb, AdbInstallOptions, Client, ConnectionType, LogcatOptions, RebootType, Reconnect, Shell, Wakefulness};

static GET_STATE_TIMEOUT: u64 = 200;
static SLEEP_AFTER_ROOT: u64 = 1_000;

impl Client {
	pub fn new(adb: Adb, addr: ConnectionType, debug: bool) -> Self {
		Client { adb, addr, debug }
	}

	/// Attempt to connect to a tcp/ip client, optionally waiting until the given
	/// timeout expires.
	/// If debug is set to true, the executed command will be logged out.
	pub fn connect(&self, timeout: Option<Duration>) -> Result<()> {
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
	pub fn disconnect(&self) -> Result<bool> {
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

	pub fn try_disconnect(&self) -> Result<bool> {
		let mut command = CommandBuilder::adb(&self.adb).with_debug(self.debug);
		command = command.arg("disconnect");
		command = match self.addr {
			ConnectionType::TcpIp(ip) => command.arg(ip.to_string()),
			_ => command,
		};
		match command.build().run() {
			Ok(status) => Ok(status.map_or(false, |status| status.success())),
			Err(err) => Err(Error::CommandError(err)),
		}
	}

	/// disconnect all connected devices
	pub fn disconnect_all(&self) -> Result<()> {
		super::shell::handle_result(
			CommandBuilder::adb(&self.adb)
				.with_debug(self.debug)
				.arg("disconnect")
				.build()
				.output()?,
		)
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
	pub fn wait_for_device(&self, timeout: Option<Duration>) -> Result<()> {
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
	pub fn get_wakefulness(&self) -> Result<Wakefulness> {
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

	pub fn is_awake(&self) -> Result<bool> {
		Ok(self.get_wakefulness()? != Wakefulness::Asleep)
	}

	/// return the adb root status for the current connection
	pub fn is_root(&self) -> Result<bool> {
		self.shell().is_root()
	}

	/// Attempt to run adb as root
	pub fn root(&self) -> Result<bool> {
		if self.shell().is_root()? {
			return Ok(true);
		}

		let output = CommandBuilder::from(self).arg("root").build().output()?;

		if output.success() {
			sleep(Duration::from_millis(SLEEP_AFTER_ROOT));
			Ok(self.is_root()?)
		} else {
			Err(Error::CommandError(simple_cmd::Error::from(output)))
		}
	}

	pub fn unroot(&self) -> Result<()> {
		super::shell::handle_result(CommandBuilder::from(self).arg("unroot").build().output()?)
	}

	/// Save screencap to local file
	pub fn save_screencap(&self, output: File) -> Result<()> {
		let args = vec![
			"exec-out",
			"screencap",
			"-p",
		];
		let pipe_out = Stdio::from(output);
		let mut cmd = std::process::Command::new(self.adb.as_os_str());

		cmd.args(self.addr.as_args())
			.args(args)
			.stdout(pipe_out)
			.stderr(Stdio::piped());

		if self.debug {
			cmd.debug();
		}

		cmd.output()?;

		Ok(())
	}

	/// copy screencap to clipboard
	pub fn copy_screencap(&self) -> Result<()> {
		let mut dir = temp_dir();
		let file_name = format!("{}.png", Uuid::new_v4());
		dir.push(file_name);

		let path = dir.as_path().to_owned();
		let file = File::create(path.as_path())?;
		self.save_screencap(file)?;

		let img = image::open(path.as_path())?;
		let width = img.width();
		let height = img.height();

		let image_data = ImageData {
			width: width as usize,
			height: height as usize,
			bytes: Cow::from(img.as_bytes()),
		};

		let mut clipboard = arboard::Clipboard::new()?;
		clipboard.set_image(image_data)?;
		Ok(())
	}

	/// reboot the device; defaults to booting system image but
	/// supports bootloader and recovery too. sideload reboots
	/// into recovery and automatically starts sideload mode,
	/// sideload-auto-reboot is the same but reboots after sideloading.
	pub fn reboot(&self, reboot_type: Option<RebootType>) -> Result<()> {
		let mut args = vec!["reboot".to_string()];

		if let Some(reboot_type) = reboot_type {
			let s = format!("{}", reboot_type);
			args.push(s.to_owned());
		}

		CommandBuilder::from(self).args(args).build().output()?;
		Ok(())
	}

	/// remount partitions read-write. if a reboot is required, `reboot_if_required` will
	/// will automatically reboot the device.
	pub fn remount(&self, reboot_if_required: bool) -> Result<()> {
		let mut cmd = CommandBuilder::from(self).arg("remount");
		if reboot_if_required {
			cmd = cmd.arg("-R");
		}

		let result = cmd.build().output()?;

		if result.success() {
			Ok(())
		} else {
			Err(simple_cmd::Error::CommandError(simple_cmd::errors::CmdError::from(result)).into())
		}
	}

	/// print serial-number
	pub fn get_seriano(&self) -> Result<String> {
		let output = CommandBuilder::from(self).arg("get-serialno").build().output()?;
		Ok(Arg::as_str(&output.stdout)?.trim().to_string())
	}

	/// reconnect                kick connection from host side to force reconnect
	/// reconnect device         kick connection from device side to force reconnect
	/// reconnect offline        reset offline/unauthorized devices to force reconnect
	pub fn reconnect(&self, r#type: Option<Reconnect>) -> Result<String> {
		let mut cmd = CommandBuilder::from(self).arg("reconnect".to_string());
		if let Some(reconnect_type) = r#type {
			cmd = cmd.arg(reconnect_type.to_string());
		}
		let output = cmd.build().output()?;
		Ok(Arg::as_str(&output.stdout)?.trim().to_owned())
	}

	///  bugreport PATH
	///     write bugreport to given PATH (default=bugreport.zip);
	///     if PATH is a directory, the bug report is saved in that directory.
	///     devices that don't support zipped bug reports output to stdout.
	pub fn bug_report<T: Arg>(&self, output: Option<T>) -> Result<Output> {
		let args = match output.as_ref() {
			Some(s) => vec![
				"bugreport",
				s.as_str()?,
			],
			None => vec!["bugreport"],
		};
		CommandBuilder::from(self).args(args).build().output().map_err(|e| e.into())
	}

	pub fn clear_logcat(&self) -> Result<()> {
		let output = CommandBuilder::from(self)
			.args([
				"logcat", "-b", "all", "-c",
			])
			.build()
			.output()?;

		if output.error() {
			Err(output.into())
		} else {
			Ok(())
		}
	}

	pub fn logcat(&self, options: LogcatOptions, cancel: Option<Receiver<()>>) -> Result<Output> {
		let mut command = CommandBuilder::from(self);
		let mut args = vec!["logcat".into()];
		args.extend(options.clone());

		if let Some(timeout) = options.timeout {
			command = command.with_timeout(timeout);
		}

		if let Some(signal) = cancel {
			command = command.with_signal(signal);
		}

		command.with_args(args).build().output().map_err(|e| e.into())
	}

	/// Returns the device mac-address
	pub fn get_mac_address(&self) -> Result<MacAddress> {
		let output = self.shell().cat("/sys/class/net/eth0/address")?;
		let mac_address_str = Arg::as_str(&output)?.trim_end();
		let mac_address = MacAddress::try_from(mac_address_str)?;
		Ok(mac_address)
	}

	/// Returns the wlan mac-address
	pub fn get_wlan_address(&self) -> Result<MacAddress> {
		let output = self.shell().cat("/sys/class/net/wlan0/address")?;
		let mac_address_str = Arg::as_str(&output)?.trim_end();
		let mac_address = MacAddress::try_from(mac_address_str)?;
		Ok(mac_address)
	}

	/// Returns the boot id
	pub fn get_boot_id(&self) -> Result<Uuid> {
		let output = self.shell().cat("/proc/sys/kernel/random/boot_id")?;
		let output_str = Arg::as_str(&output)?.trim();
		let boot_id = output_str.try_into()?;
		Ok(boot_id)
	}

	/// Disable verity
	pub fn disable_verity(&self) -> Result<()> {
		let output = CommandBuilder::from(self).arg("disable-verity").build().output()?;

		if !output.success() {
			Err(output.into())
		} else {
			Ok(())
		}
	}

	/// Enable verity
	pub fn enable_verity(&self) -> Result<()> {
		let output = CommandBuilder::from(self).arg("enable-verity").build().output()?;
		println!("output: {output:?}");

		if !output.success() {
			Err(output.into())
		} else {
			Ok(())
		}
	}

	pub fn pull<S, T>(&self, src: S, dst: T) -> Result<Output>
	where
		S: Arg,
		T: Arg,
	{
		let mut command = CommandBuilder::from(self);
		command = command.arg("pull").arg(src.as_str()?).arg(dst.as_str()?);
		command.build().output().map_err(|e| e.into())
	}

	pub fn push<S, T>(&self, src: S, dst: T) -> Result<Output>
	where
		S: Arg,
		T: Arg,
	{
		let mut command = CommandBuilder::from(self);
		command = command.arg("push").arg(src.as_str()?).arg(dst.as_str()?);
		command.build().output().map_err(|e| e.into())
	}

	pub fn install<T>(&self, path: T, install_options: Option<AdbInstallOptions>) -> Result<()>
	where
		T: Arg,
	{
		let mut args = vec!["install".into()];
		match install_options {
			None => {}
			Some(options) => args.extend(options),
		}
		args.push(path.as_str()?.into());
		super::shell::handle_result(self.adb.exec(self.addr, args, None, None, self.debug)?)
	}

	pub fn uninstall(&self, package_name: &str, keep_data: bool) -> Result<()> {
		let mut args = vec!["uninstall"];
		if keep_data {
			args.push("-k");
		}

		args.push(package_name);
		super::shell::handle_result(self.adb.exec(self.addr, args, None, None, self.debug)?)
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

#[cfg(test)]
mod test {
	use std::fs::{remove_file, File};
	use std::io::BufRead;
	use std::net::SocketAddr;
	use std::time::Duration;

	use chrono::Local;
	use simple_cmd::prelude::OutputExt;

	use crate::error::Error;
	use crate::test::test::{
		client_from, connect_client, connect_emulator, connect_tcp_ip_client, connection_from_tcpip, init_log, test_files_dir,
	};
	use crate::types::{AdbInstallOptions, Client, ConnectionType, LogcatLevel, LogcatOptions, LogcatTag, Reconnect};

	#[test]
	fn test_new_client() {
		let address: ConnectionType = connection_from_tcpip();
		let mut client = client_from(address);
		client = client.with_debug(true);
		let connected = client.is_connected();
		println!("connected: {}", connected);

		let mut client = connect_emulator();
		client = client.with_debug(true);
		let connected = client.is_connected();
		println!("connected: {}", connected);
	}

	#[test]
	fn test_connect() {
		init_log();
		let client = connect_tcp_ip_client();
		let _ = client.connect(Some(Duration::from_secs(1))).expect("failed to connect");
	}

	#[test]
	fn test_disconnect() {
		init_log();
		let client = connect_tcp_ip_client();
		let disconnected = client.disconnect().expect("failed to disconnect");
		println!("disconnected: {disconnected}");
	}

	#[test]
	fn test_try_disconnect() {
		init_log();
		let client = connect_emulator();
		let disconnected = client.try_disconnect().expect("failed to disconnect");
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

	#[test]
	fn test_root() {
		init_log();
		let client = connect_client(connection_from_tcpip());

		if client.is_root().expect("failed to get user") {
			client.unroot().expect("failed to unroot");
		}

		let is_root = client.is_root().expect("failed to get user");
		assert!(!is_root);

		let success = client.root().expect("failed to root client");
		assert!(success);

		let is_root = client.is_root().expect("failed to get user status");
		assert!(is_root);

		client.unroot().expect("failed to unroot");
		let is_root = client.is_root().expect("failed to get user status");
		assert!(!is_root);

		let client = connect_emulator();
		let success = client.root();

		if let Err(Error::CommandError(simple_cmd::Error::CommandError(err))) = success {
			println!("expected error: {}", err);
			return;
		} else if let Ok(false) = success {
			// ok
		} else {
			println!("err = {:?}", success);
			assert!(false, "incorrect error received");
		}
	}

	#[test]
	fn test_save_screencap_locally() {
		init_log();
		let client = connect_client(connection_from_tcpip());

		let output = dirs::desktop_dir().unwrap().join("screencap.png");
		let output_path = output.as_path();

		println!("target local file: {:?}", output_path.to_str());

		if output.exists() {
			remove_file(output_path).expect("Error deleting file");
		}

		let file = File::create(output_path).expect("failed to create file");
		let _result = client.save_screencap(file).expect("failed to save screencap");
		println!("ok. done => {:?}", output);

		remove_file(output_path).unwrap();
	}

	#[test]
	pub fn test_copy_screencap() {
		init_log();
		let client = connect_emulator();
		let _result = client.copy_screencap().expect("failed to copy screencap");
	}

	#[test]
	pub fn test_reboot() {
		init_log();
		let client = connect_emulator();
		let _result = client.reboot(None);
	}

	#[test]
	fn test_remount() {
		init_log();
		let client = connect_emulator();
		client.remount(true).expect_err("remount should have returned an error");

		let client = connect_tcp_ip_client();
		client.root().expect("failed to root client");
		client.remount(true).expect("failed to remount");
	}

	#[test]
	fn test_get_serialno() {
		init_log();
		let client = connect_emulator();
		let serial_no = client.get_seriano().expect("failed to get serial number");
		assert!(serial_no.starts_with("emulator-"));
		println!("serial: {serial_no}");

		let client = connect_tcp_ip_client();
		let serial_no = client.get_seriano().expect("failed to get serial number");
		let ip_addr = serial_no.parse::<SocketAddr>().expect("failed to parse serial no");
		println!("serial: {ip_addr}");
	}

	#[test]
	fn test_reconnect() {
		init_log();
		let client = connect_emulator();
		client.reconnect(None).expect("failed to reconnect");
		client.reconnect(Some(Reconnect::Device)).expect("failed to reconnect device");
		client
			.reconnect(Some(Reconnect::Offline))
			.expect("failed to reconnect offline");

		let client = Client::try_from(ConnectionType::try_from_ip("192.168.1.99:5555").expect("failed to parse ip address"))
			.expect("failed to create client");
		client.reconnect(None).expect("failed to reconnect");
		client.reconnect(Some(Reconnect::Device)).expect("failed to reconnect");
		client.reconnect(Some(Reconnect::Offline)).expect("failed to reconnect");
	}

	#[test]
	fn test_bugreport() {
		let client = connect_emulator();
		let output = dirs::desktop_dir().unwrap().join("bugreport.zip");

		if output.exists() {
			remove_file(output.as_path()).expect("failed to delete file");
		}

		let _ = client.bug_report(Some(output.clone())).expect("failed to generate bugreport");
		assert!(output.exists());

		remove_file(output.as_path()).expect("failed to delete file");
	}

	#[test]
	fn test_clear_logcat() {
		let client = connect_emulator();
		let _ = client.clear_logcat().expect("failed to clear logcat");
	}

	#[test]
	fn test_get_mac_address() {
		let client = connect_tcp_ip_client();
		client.root().expect("failed to root");
		let mac_address = client.get_mac_address().expect("failed to read mac address");
		println!("mac address: {}", mac_address);
	}

	#[test]
	fn test_get_wlan_address() {
		let client = connect_tcp_ip_client();
		client.root().expect("failed to root");
		match client.get_wlan_address() {
			Ok(mac_address) => {
				println!("wlan mac address: {}", mac_address);
			}
			Err(err) => {
				eprintln!("unable to fetch wlan address: {err}");
			}
		}
	}

	#[test]
	fn test_get_boot_id() {
		let client = connect_tcp_ip_client();
		client.root().expect("failed to root");
		let boot_id = client.get_boot_id().expect("failed to read boot_id");
		println!("boot_id: {boot_id}");
	}

	#[test]
	fn test_disable_verity() {
		let client = connect_tcp_ip_client();
		client.root().expect("failed to root");
		let _ = client.disable_verity().expect("failed to disable verity");
	}

	#[test]
	fn test_enable_verity() {
		let client = connect_tcp_ip_client();
		client.root().expect("failed to root");
		let _ = client.enable_verity().expect("failed to enable verity");
	}

	#[test]
	fn test_logcat() {
		init_log();
		let client = connect_tcp_ip_client();

		let timeout = Some(Duration::from_secs(3));
		let since = Some(Local::now() - chrono::Duration::seconds(600));

		let options = LogcatOptions {
			expr: None,
			dump: false,
			filename: None,
			tags: Some(vec![
				LogcatTag {
					name: "tl.RestClient".to_string(),
					level: LogcatLevel::Debug,
				},
			]),
			format: None,
			since,
			pid: None,
			timeout,
		};

		let output = client.logcat(options, None);

		match output {
			Ok(o) => {
				if o.status.success() || o.kill() || o.interrupt() {
					let mut index = 0;
					let stdout = o.stdout;
					let lines = stdout.lines().map(|l| l.unwrap());
					for line in lines {
						println!("{}", line);
						index = index + 1;
						if index > 10 {
							break;
						}
					}
				} else if o.error() {
					panic!("{:?}", o);
				} else {
					panic!("{:?}", o);
				}
			}
			Err(err) => {
				panic!("{}", err);
			}
		}
	}

	#[test]
	fn test_install() {
		init_log();
		let client = connect_emulator();
		let test_files_dir = test_files_dir();
		println!("test_files_dir: {:?}", test_files_dir);

		let path = test_files_dir.join("app-debug.apk");
		let package_name = "it.sephiroth.android.app.app";

		let is_installed = client
			.shell()
			.pm()
			.is_installed(package_name, None)
			.expect("failed to check if package is installed");
		if is_installed {
			client.uninstall(package_name, false).expect("failed to uninstall package");
			assert!(!client.shell().pm().is_installed(package_name, None).unwrap());
		}

		client
			.install(
				path,
				Some(AdbInstallOptions {
					allow_version_downgrade: false,
					allow_test_package: false,
					replace: false,
					forward_lock: false,
					install_external: false,
					grant_permissions: false,
					instant: false,
				}),
			)
			.expect("failed to install apk");

		assert!(client
			.shell()
			.pm()
			.is_installed(package_name, None)
			.expect("failed to check if package is installed"));
	}
}
