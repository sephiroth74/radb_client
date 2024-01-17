use std::borrow::Cow;
use std::env::temp_dir;
use std::fs::File;
use std::io::ErrorKind;
use std::process::{Output, Stdio};
use std::thread::sleep;
use std::time::Duration;

use arboard::ImageData;
use crossbeam::channel::Receiver;
use mac_address::MacAddress;
use rustix::path::Arg;
use simple_cmd::debug::CommandDebug;
use simple_cmd::prelude::OutputExt;
use simple_cmd::Error::CommandError;
use simple_cmd::{Cmd, CommandBuilder};
use uuid::Uuid;

use crate::cmd_ext::CommandBuilderExt;
use crate::errors::AdbError;
use crate::errors::AdbError::InvalidDeviceAddressError;
use crate::traits::AdbDevice;
use crate::types::{AdbInstallOptions, LogcatOptions, RebootType, Wakefulness};
use crate::{ActivityManager, AdbClient, AdbShell, Client, Device, PackageManager};
use crate::{Adb, Shell};

#[allow(dead_code)]
impl Client {
	pub fn logcat<'d, D>(
		adb: &Adb,
		device: D,
		options: LogcatOptions,
		cancel: Option<Receiver<()>>,
		debug: bool,
	) -> crate::Result<Output>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut command = CommandBuilder::adb(adb).device(device);

		command = command.with_arg("logcat").with_debug(debug);

		if let Some(expr) = options.expr {
			command = command.with_args([
				"-e",
				expr.as_str(),
			]);
		}

		if options.dump {
			command = command.with_arg("-d");
		}

		if let Some(filename) = options.filename {
			command = command.with_args([
				"-f",
				filename.as_str(),
			]);
		}

		if let Some(format) = options.format {
			command = command.with_args([
				"-v",
				format.as_str(),
			]);
		}

		if let Some(pid) = options.pid {
			command = command.with_args([
				"--pid",
				format!("{}", pid).as_str(),
			]);
		}

		if let Some(since) = options.since {
			command = command.with_args([
				"-T",
				since.format("%m-%d %H:%M:%S.%3f").to_string().as_str(),
			]);
		}

		if let Some(tags) = options.tags {
			if !tags.is_empty() {
				for tag in tags {
					command = command.with_arg(format!("{:}", tag).as_str());
				}
				command = command.with_arg("*:S");
			}
		}

		if let Some(timeout) = options.timeout {
			command = command.with_timeout(timeout);
		}

		if let Some(signal) = cancel {
			command = command.with_signal(signal);
		}

		command.build().output().map_err(|e| e.into())
	}

	/// Retrieve the device name
	///
	/// # Arguments
	///
	/// * `adb`: the adb path
	/// * `device`: the target device
	///
	/// # Examples
	///
	/// ```
	/// use radb_client::{Adb, Client};
	/// let adb = Adb::new().unwrap();
	/// let device = adb.device("192.168.1.24:5555");
	/// let name = Client::name(&adb, &device, true).unwrap();
	/// ```
	pub fn name<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<String>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::getprop(adb, device, "ro.build.product", debug)
	}

	pub fn api_level<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<String>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::getprop(adb, device, "ro.build.version.sdk", debug)
	}

	pub fn version<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<u8>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let result = Shell::getprop(adb, device, "ro.build.version.release", debug)?;
		result.parse::<u8>().map_err(From::from)
	}

	pub fn install<'d, D, T: Arg>(
		adb: &Adb,
		device: D,
		path: T,
		install_options: Option<AdbInstallOptions>,
		debug: bool,
	) -> crate::Result<Output>
	where
		D: Into<&'d dyn AdbDevice>,
		T: Arg,
	{
		let mut args = vec!["install".to_string()];
		match install_options {
			None => {}
			Some(options) => args.extend(options),
		}
		args.push(path.as_str()?.to_string());
		adb.exec(device, args, None, None, debug)
	}

	pub fn uninstall<'d, D>(adb: &Adb, device: D, package_name: &str, keep_data: Option<bool>, debug: bool) -> crate::Result<Output>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut args = vec!["uninstall"];
		if let Some(_keep_data) = keep_data {
			args.push("-k");
		}

		args.push(package_name);
		adb.exec(device, args, None, None, debug)
	}

	pub fn pull<'d, 's, D, S, T>(adb: &Adb, device: D, src: S, dst: T, debug: bool) -> crate::Result<Output>
	where
		D: Into<&'d dyn AdbDevice>,
		S: Arg,
		T: Arg,
	{
		let mut command = CommandBuilder::adb(adb).device(device);
		command = command.arg("pull").arg(src.as_str()?).arg(dst.as_str()?).with_debug(debug);
		command.build().output().map_err(|e| e.into())
	}

	pub fn push<'d, 't, D, S, T>(adb: &Adb, device: D, src: S, dst: T, debug: bool) -> crate::Result<Output>
	where
		D: Into<&'d dyn AdbDevice>,
		S: Arg,
		T: Arg,
	{
		let mut command = CommandBuilder::adb(adb).device(device);
		command = command.arg("push").arg(src.as_str()?).arg(dst.as_str()?).with_debug(debug);
		command.build().output().map_err(|e| e.into())
	}

	pub fn save_screencap<'d, D>(adb: &Adb, device: D, output: File, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let args = vec![
			"exec-out",
			"screencap",
			"-p",
		];
		let pipe_out = Stdio::from(output);

		let mut cmd = std::process::Command::new(adb.as_os_str());

		cmd.args(device.into().args())
			.args(args)
			.stdout(pipe_out)
			.stderr(Stdio::piped());

		if debug {
			cmd.debug();
		}

		cmd.output()?;

		Ok(())
	}

	pub fn copy_screencap<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut dir = temp_dir();
		let file_name = format!("{}.png", Uuid::new_v4());
		dir.push(file_name);

		let path = dir.as_path().to_owned();
		let file = File::create(path.as_path())?;
		Client::save_screencap(adb, device, file, debug)?;

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

	pub fn wait_for_device<'d, D>(adb: &Adb, device: D, timeout: Option<Duration>, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		CommandBuilder::adb(adb)
			.device(device)
			.with_debug(debug)
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

	pub fn is_root<'a, T>(adb: &Adb, device: T, debug: bool) -> crate::Result<bool>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		Shell::is_root(adb, device.into(), debug)
	}

	/// Attempt to run adb as root
	pub fn root<'a, T>(adb: &Adb, device: T, debug: bool) -> crate::Result<bool>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let d = device.into();

		if Shell::is_root(adb, d, debug)? {
			return Ok(true);
		}
		CommandBuilder::adb(adb).device(d).arg("root").build().output()?;
		sleep(Duration::from_secs(1));
		Ok(true)
	}

	pub fn unroot<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		CommandBuilder::adb(adb)
			.device(device)
			.with_debug(debug)
			.arg("unroot")
			.build()
			.output()?;
		Ok(true)
	}

	pub fn is_connected<'d, D>(adb: &Adb, device: D, debug: bool) -> bool
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut command = CommandBuilder::adb(adb).device(device).with_debug(debug);
		command = command.arg("get-state").timeout(Some(Duration::from_millis(200)));
		let output = command.build().output();

		return if let Ok(output) = output { output.success() } else { false };
	}

	pub fn get_wakefulness<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<Wakefulness>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let command1 = CommandBuilder::adb(adb)
			.device(device)
			.args(vec![
				"shell", "dumpsys", "power",
			])
			.build();
		let command2 = Cmd::builder("sed")
			.arg("-n")
			.arg("s/mWakefulness=\\(\\S*\\)/\\1/p")
			.with_debug(debug)
			.stdout(Some(Stdio::piped()))
			.build();

		let result = command1.pipe(command2)?;
		let awake = Arg::as_str(&result.stdout)?.trim();
		Ok(awake.try_into()?)
	}

	/// Attempt to connect to the given device, optionally waiting until the given
	/// timeout expires.
	/// If debug is set to true, the executed command will be logged out.
	pub fn connect<'d, D>(adb: &Adb, device: D, timeout: Option<Duration>, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let d = device.into();

		if Client::is_connected(adb, d, debug) {
			return Ok(());
		}

		let serial = d
			.addr()
			.serial()
			.ok_or(InvalidDeviceAddressError("Host[:Port] required".to_string()))?;

		let mut command = CommandBuilder::new(adb.as_os_str());

		if let Some(timeout) = timeout {
			command = command.with_timeout(timeout);
		}

		command = command
			.args([
				"connect",
				serial.as_str(),
			])
			.with_debug(debug);
		let output = command.build().output()?;

		if output.error() {
			return Err(AdbError::ConnectToDeviceError());
		} else {
			match Client::is_connected(adb, d, debug) {
				true => Ok(()),
				false => Err(AdbError::ConnectToDeviceError()),
			}
		}
	}

	pub fn disconnect<'d, D>(adb: &Adb, device: D) -> bool
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let d = device.into();
		let serial = d
			.addr()
			.serial()
			.ok_or(InvalidDeviceAddressError("Host[:Port] required".to_string()));

		if let Err(_err) = serial {
			return false;
		}

		match CommandBuilder::new(adb.as_os_str())
			.args([
				"disconnect",
				serial.unwrap().as_str(),
			])
			.build()
			.output()
		{
			Ok(output) => output.success(),
			Err(_err) => false,
		}
	}

	pub fn try_disconnect<'d, D>(adb: &Adb, device: D) -> crate::Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let d = device.into();
		let serial = d
			.addr()
			.serial()
			.ok_or(InvalidDeviceAddressError("Host[:Port] required".to_string()))?;

		match CommandBuilder::new(adb.as_os_str())
			.args([
				"disconnect",
				serial.as_str(),
			])
			.build()
			.run()
		{
			Ok(status) => Ok(status.map_or(false, |status| status.success())),
			Err(err) => Err(AdbError::CmdError(err)),
		}
	}

	pub fn disconnect_all(adb: &Adb) -> crate::Result<bool> {
		CommandBuilder::new(adb.0.as_path()).args(["disconnect"]).build().output()?;
		Ok(true)
	}

	pub fn reboot<'d, D>(adb: &Adb, device: D, reboot_type: Option<RebootType>, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut args: Vec<&str> = vec!["reboot"];
		let s = reboot_type.map(|f| f.value()).unwrap_or_default();
		args.push(s.as_str());

		CommandBuilder::adb(adb)
			.device(device)
			.with_debug(debug)
			.args(args)
			.build()
			.output()?;
		Ok(())
	}

	pub fn remount<'a, T>(adb: &Adb, device: T, debug: bool) -> crate::Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		CommandBuilder::adb(adb)
			.device(device)
			.with_debug(debug)
			.arg("remount")
			.build()
			.output()?;
		Ok(())
	}

	pub fn mount<'d, D, T: Arg>(adb: &Adb, device: D, dir: T, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::exec(
			adb,
			device,
			vec![
				"mount -o rw,remount",
				dir.as_str()?,
			],
			None,
			None,
			debug,
		)?;
		Ok(())
	}

	pub fn unmount<'d, D, T: Arg>(adb: &Adb, device: D, dir: T, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::exec(
			adb,
			device,
			vec![
				"mount -o ro,remount",
				dir.as_str()?,
			],
			None,
			None,
			debug,
		)?;
		Ok(())
	}

	pub fn bug_report<'d, D, T: Arg>(adb: &Adb, device: D, output: Option<T>, debug: bool) -> crate::Result<Output>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let args = match output.as_ref() {
			Some(s) => vec![
				"bugreport",
				s.as_str()?,
			],
			None => vec!["bugreport"],
		};
		CommandBuilder::adb(adb)
			.device(device)
			.args(args)
			.with_debug(debug)
			.build()
			.output()
			.map_err(|e| e.into())
	}

	pub fn clear_logcat<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		CommandBuilder::adb(adb)
			.device(device)
			.args([
				"logcat", "-b", "all", "-c",
			])
			.with_debug(debug)
			.build()
			.output()?;
		Ok(())
	}

	pub fn get_mac_address<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<MacAddress>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Shell::cat(adb, device, "/sys/class/net/eth0/address", debug)?;
		let mac_address_str = Arg::as_str(&output)?.trim_end();
		let mac_address = MacAddress::try_from(mac_address_str)?;
		Ok(mac_address)
	}

	pub fn get_wlan_address<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<MacAddress>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Shell::cat(adb, device, "/sys/class/net/wlan0/address", debug)?;
		let mac_address_str = Arg::as_str(&output)?.trim_end();
		let mac_address = MacAddress::try_from(mac_address_str)?;
		Ok(mac_address)
	}

	pub fn get_boot_id<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<Uuid>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Shell::cat(adb, device, "/proc/sys/kernel/random/boot_id", debug)?;
		let output_str = Arg::as_str(&output)?.trim();
		let boot_id = output_str.try_into()?;
		Ok(boot_id)
	}

	pub fn get_verity<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let d: &dyn AdbDevice = device.into();
		let _ = Client::check_avbctl(adb, d, debug)?;
		let output = Shell::exec(
			adb,
			d,
			vec![
				"avbctl",
				"get-verity",
			],
			None,
			None,
			debug,
		)?;
		let string = Arg::as_str(&output.stdout)?;
		Ok(string.contains("enabled"))
	}

	pub fn disable_verity<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let d: &dyn AdbDevice = device.into();
		let _ = Client::check_avbctl(adb, d, debug)?;
		let output = Shell::exec(
			adb,
			d,
			vec![
				"avbctl",
				"disable-verity",
			],
			None,
			None,
			debug,
		)?;
		if !output.success() {
			let e: simple_cmd::errors::CmdError = output.into();
			Err(AdbError::CmdError(CommandError(e)))
		} else {
			Ok(())
		}
	}

	pub fn enable_verity<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let d: &dyn AdbDevice = device.into();
		let _ = Client::check_avbctl(adb, d, debug)?;
		let output = Shell::exec(
			adb,
			d,
			vec![
				"avbctl",
				"enable-verity",
			],
			None,
			None,
			debug,
		)?;
		if !output.success() {
			let e: simple_cmd::errors::CmdError = output.into();
			Err(AdbError::CmdError(CommandError(e)))
		} else {
			Ok(())
		}
	}

	fn check_avbctl<'d, D>(adb: &Adb, device: D, debug: bool) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::get_command_path(adb, device, "avbctl", debug)
			.map(|_| ())
			.ok_or(AdbError::IoError(std::io::Error::from(ErrorKind::NotFound)))
	}
}

impl AdbClient {
	pub fn copy(other: &AdbClient) -> AdbClient {
		AdbClient {
			adb: Adb::copy(&other.adb),
			device: Device::copy(&other.device),
			debug: other.debug,
		}
	}

	pub fn try_from_device(device: Device) -> Result<AdbClient, AdbError> {
		match Adb::new() {
			Ok(adb) => Ok(AdbClient {
				adb,
				device,
				debug: true,
			}),
			Err(err) => Err(err),
		}
	}

	pub fn is_connected(&self) -> bool {
		Client::is_connected(&self.adb, &self.device, self.debug)
	}

	pub fn is_awake(&self) -> crate::Result<bool> {
		Ok(Client::get_wakefulness(&self.adb, &self.device, self.debug)? != Wakefulness::Asleep)
	}

	pub fn get_wakefulness(&self) -> crate::Result<Wakefulness> {
		Client::get_wakefulness(&self.adb, &self.device, self.debug)
	}

	/// Try to connect to the inner device.
	///
	/// # Arguments
	///
	/// * `timeout`: optional timeout for connecting
	/// * `debug`: verbose output
	///
	/// returns: Result<(), Error>
	///
	/// # Examples
	///
	/// ```
	/// use radb_client::Device;
	/// use radb_client::AdbClient;
	///
	/// pub fn connect() {
	///  let device: Device = "192.168.1.24:5555".parse().unwrap();
	///  let client: AdbClient = device.try_into().unwrap();
	///  client.connect(None ).unwrap();
	/// }
	/// ```
	pub fn connect(&self, timeout: Option<Duration>) -> Result<(), AdbError> {
		Client::connect(&self.adb, &self.device, timeout, self.debug)
	}

	pub fn disconnect(&self) -> bool {
		Client::disconnect(&self.adb, &self.device)
	}

	pub fn try_disconnect(&self) -> crate::Result<bool> {
		Client::try_disconnect(&self.adb, &self.device)
	}

	pub fn root(&self) -> crate::Result<bool> {
		Client::root(&self.adb, &self.device, self.debug)
	}

	pub fn unroot(&self) -> crate::Result<bool> {
		Client::unroot(&self.adb, &self.device, self.debug)
	}

	pub fn is_root(&self) -> crate::Result<bool> {
		Client::is_root(&self.adb, &self.device, self.debug)
	}

	pub fn remount(&self) -> crate::Result<()> {
		Client::remount(&self.adb, &self.device, self.debug)
	}

	pub fn mount<T: Arg>(&self, dir: T) -> crate::Result<()> {
		Client::mount(&self.adb, &self.device, dir, self.debug)
	}

	pub fn unmount<T: Arg>(&self, dir: T) -> crate::Result<()> {
		Client::unmount(&self.adb, &self.device, dir, self.debug)
	}

	pub fn bug_report<T: Arg>(&self, output: Option<T>) -> crate::Result<Output> {
		Client::bug_report(&self.adb, &self.device, output, self.debug)
	}

	///
	/// Root is required
	///
	pub fn get_mac_address(&self) -> crate::Result<MacAddress> {
		Client::get_mac_address(&self.adb, &self.device, self.debug)
	}

	///
	/// Root is required
	pub fn get_wlan_address(&self) -> crate::Result<MacAddress> {
		Client::get_wlan_address(&self.adb, &self.device, self.debug)
	}

	pub fn pull<'s, S, D>(&self, src: S, dst: D) -> crate::Result<Output>
	where
		S: Arg,
		D: Arg,
	{
		Client::pull(&self.adb, &self.device, src, dst, self.debug)
	}

	pub fn push<'d, S, D>(&self, src: S, dst: D) -> crate::Result<Output>
	where
		D: Arg,
		S: Arg,
	{
		Client::push(&self.adb, &self.device, src, dst, self.debug)
	}

	pub fn clear_logcat(&self) -> crate::Result<()> {
		Client::clear_logcat(&self.adb, &self.device, self.debug)
	}

	pub fn logcat(&self, options: LogcatOptions, cancel: Option<Receiver<()>>) -> crate::Result<Output> {
		Client::logcat(&self.adb, &self.device, options, cancel, self.debug)
	}

	pub fn api_level(&self) -> crate::Result<String> {
		Client::api_level(&self.adb, &self.device, self.debug).map_err(|e| e.into())
	}

	pub fn version(&self) -> crate::Result<u8> {
		Client::version(&self.adb, &self.device, self.debug)
	}

	pub fn name(&self) -> crate::Result<Option<String>> {
		Ok(Client::name(&self.adb, &self.device, self.debug).ok())
	}

	pub fn save_screencap(&self, output: File) -> crate::Result<()> {
		Client::save_screencap(&self.adb, &self.device, output, self.debug)
	}

	pub fn copy_screencap(&self) -> crate::Result<()> {
		Client::copy_screencap(&self.adb, &self.device, self.debug)
	}

	pub fn get_boot_id(&self) -> crate::Result<Uuid> {
		Client::get_boot_id(&self.adb, &self.device, self.debug)
	}

	pub fn reboot(&self, reboot_type: Option<RebootType>) -> crate::Result<()> {
		Client::reboot(&self.adb, &self.device, reboot_type, self.debug)
	}

	/// Wait for device to connect, with an optional timeout
	pub fn wait_for_device(&self, timeout: Option<Duration>) -> crate::Result<()> {
		Client::wait_for_device(&self.adb, &self.device, timeout, self.debug)
	}

	pub fn get_verity(&self) -> crate::Result<bool> {
		Client::get_verity(&self.adb, &self.device, self.debug)
	}

	pub fn disable_verity(&self) -> crate::Result<()> {
		Client::disable_verity(&self.adb, &self.device, self.debug)
	}

	pub fn enable_verity(&self) -> crate::Result<()> {
		Client::enable_verity(&self.adb, &self.device, self.debug)
	}

	pub fn install<T: Arg>(&self, path: T, install_options: Option<AdbInstallOptions>) -> crate::Result<Output> {
		Client::install(&self.adb, &self.device, path, install_options, self.debug)
	}

	pub fn uninstall(&self, package_name: &str, keep_data: Option<bool>) -> crate::Result<Output> {
		Client::uninstall(&self.adb, &self.device, package_name, keep_data, self.debug)
	}

	pub fn shell(&self) -> AdbShell {
		AdbShell { parent: self }
	}

	pub fn pm(&self) -> PackageManager {
		PackageManager {
			parent: AdbShell { parent: self },
		}
	}

	pub fn am(&self) -> ActivityManager {
		ActivityManager {
			parent: AdbShell { parent: self },
		}
	}
}
