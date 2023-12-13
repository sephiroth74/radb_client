use std::borrow::Cow;
use std::env::temp_dir;
use std::ffi::OsStr;
use std::fs::File;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use arboard::ImageData;
use futures::future::IntoFuture;
use log::trace;
use mac_address::MacAddress;
use rustix::path::Arg;
use tokio::process::Command;
use tokio::sync::oneshot::Receiver;
use uuid::Uuid;

use crate::command::{CommandBuilder, ProcessResult, Result};
use crate::debug::CommandDebug;
use crate::errors::AdbError;
use crate::errors::AdbError::InvalidDeviceAddressError;
use crate::traits::AdbDevice;
use crate::Client;
use crate::{Adb, Shell};

pub enum RebootType {
	Bootloader,
	Recovery,
	Sideload,
	SideloadAutoReboot,
}

pub struct LogcatOptions {
	/// -e    Only prints lines where the log message matches <expr>, where <expr> is a regular expression.
	pub expr: Option<String>,

	/// -d    Dumps the log to the screen and exits.
	pub dump: bool,

	/// -f <filename>    Writes log message output to <filename>. The default is stdout.
	pub filename: Option<String>,

	/// -s    Equivalent to the filter expression '*:S', which sets priority for all tags to silent and is used to precede a list of filter expressions that add content.
	pub tags: Option<Vec<LogcatTag>>,

	/// -v <format>    Sets the output format for log messages. The default is the threadtime format
	pub format: Option<String>,

	/// -t '<time>'    Prints the most recent lines since the specified time. This option includes -d functionality.
	/// See the -P option for information about quoting parameters with embedded spaces.
	pub since: Option<chrono::DateTime<chrono::Local>>,

	// --pid=<pid> ...
	pub pid: Option<i32>,

	pub timeout: Option<Duration>,
}

pub enum LogcatLevel {
	Verbose,
	Debug,
	Info,
	Warn,
	Error,
}

pub struct LogcatTag {
	pub name: String,
	pub level: LogcatLevel,
}

#[allow(dead_code)]
impl Client {
	pub async fn logcat<'d, D>(adb: &Adb, device: D, options: LogcatOptions, recv: Option<IntoFuture<Receiver<()>>>) -> Result<ProcessResult>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut command = CommandBuilder::device(adb, device);

		command.arg("logcat");

		if let Some(expr) = options.expr {
			command.args(["-e", expr.as_str()]);
		}

		if options.dump {
			command.arg("-d");
		}

		if let Some(filename) = options.filename {
			command.args(["-f", filename.as_str()]);
		}

		if let Some(format) = options.format {
			command.args(["-v", format.as_str()]);
		}

		if let Some(pid) = options.pid {
			command.args(["--pid", format!("{}", pid).as_str()]);
		}

		if let Some(since) = options.since {
			command.args(["-T", since.format("%m-%d %H:%M:%S.%3f").to_string().as_str()]);
		}

		if let Some(tags) = options.tags {
			if !tags.is_empty() {
				for tag in tags {
					command.arg(format!("{:}", tag).as_str());
				}
				command.arg("*:S");
			}
		}

		command.with_timeout(options.timeout).with_signal(recv).output().await
	}

	/// Retrieve the device name
	///
	/// # Arguments
	///
	/// * `adb`: the adb path
	/// * `device`: the target device
	///
	/// returns: Result<Option<String>, Error>
	///
	/// # Examples
	///
	/// ```
	/// use radb_client::{Adb, Client};
	/// let adb = Adb::new().unwrap();
	/// let device = adb.device("192.168.1.24:5555");
	/// let name = Client::name(&adb, &device).unwrap();
	/// ```
	pub async fn name<'d, D>(adb: &Adb, device: D) -> Result<String>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::getprop(adb, device, "ro.build.product").await
	}

	pub async fn api_level<'d, D>(adb: &Adb, device: D) -> Result<u8>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let result = Shell::getprop(adb, device, "ro.build.version.sdk").await?;
		result.parse::<u8>().map_err(From::from)
	}

	pub async fn version<'d, D>(adb: &Adb, device: D) -> Result<u8>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let result = Shell::getprop(adb, device, "ro.build.version.release").await?;
		result.parse::<u8>().map_err(From::from)
	}

	pub async fn pull<'d, 's, D, S, T>(adb: &Adb, device: D, src: S, dst: T) -> Result<ProcessResult>
	where
		D: Into<&'d dyn AdbDevice>,
		S: Into<&'s str> + AsRef<OsStr> + Arg,
		T: AsRef<Path>,
	{
		CommandBuilder::device(adb, device).arg("pull").arg(src.as_ref()).arg(dst.as_ref()).output().await
	}

	pub async fn push<'d, 't, D, S, T>(adb: &Adb, device: D, src: S, dst: T) -> Result<ProcessResult>
	where
		D: Into<&'d dyn AdbDevice>,
		S: AsRef<Path>,
		T: Into<&'t str> + AsRef<OsStr> + Arg,
	{
		CommandBuilder::device(adb, device).arg("push").arg(src.as_ref()).arg(dst.as_ref()).output().await
	}

	pub async fn save_screencap<'d, D>(adb: &Adb, device: D, output: File) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let args = vec!["exec-out", "screencap", "-p"];
		let pipe_out = Stdio::from(output);
		let output = Command::new(adb.as_os_str())
			.args(device.into().args())
			.args(args)
			.stdout(pipe_out)
			.stderr(Stdio::piped())
			.debug()
			.status()
			.await?;

		trace!("output: {:}", output);
		Ok(())
	}

	pub async fn copy_screencap<'d, D>(adb: &Adb, device: D) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut dir = temp_dir();
		let file_name = format!("{}.png", Uuid::new_v4());
		dir.push(file_name);

		let path = dir.as_path().to_owned();
		let _file = File::create(path.as_path())?;
		Client::save_screencap(adb, device, _file).await?;

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

	pub async fn wait_for_device<'d, D>(adb: &Adb, device: D, timeout: Option<Duration>) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		CommandBuilder::device(adb, device)
			.args(["wait-for-device", "shell", "while [[ -z $(getprop sys.boot_completed) ]]; do sleep 1; done; input keyevent 143"])
			.with_timeout(timeout)
			.output()
			.await?;
		Ok(())
	}

	pub async fn is_root<'a, T>(adb: &Adb, device: T) -> Result<bool>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		Shell::is_root(adb, device.into()).await
	}

	/// Attempt to run adb as root
	pub async fn root<'a, T>(adb: &Adb, device: T) -> Result<bool>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let d = device.into();

		if Shell::is_root(adb, d).await? {
			return Ok(true);
		}
		CommandBuilder::device(adb, d).arg("root").output().await?;
		tokio::time::sleep(Duration::from_secs(1)).await;
		Ok(true)
	}

	pub async fn unroot<'d, D>(adb: &Adb, device: D) -> Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		CommandBuilder::device(adb, device).arg("unroot").output().await?;
		Ok(true)
	}

	pub async fn is_connected<'d, D>(adb: &Adb, device: D) -> bool
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = CommandBuilder::device(adb, device).arg("get-state").output().await;
		output.is_ok()
	}

	pub async fn connect<'d, D>(adb: &Adb, device: D, timeout: Option<Duration>) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let d = device.into();

		if Client::is_connected(adb, d).await {
			return Ok(());
		}

		let serial = d.addr().serial().ok_or(InvalidDeviceAddressError("Host[:Port] required".to_string()))?;

		CommandBuilder::new(adb.as_os_str()).with_timeout(timeout).args(["connect", serial.as_str()]).output().await?;

		match Client::is_connected(adb, d).await {
			true => Ok(()),
			false => Err(AdbError::ConnectToDeviceError()),
		}
	}

	pub async fn disconnect<'d, D>(adb: &Adb, device: D) -> Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let serial = device.into().addr().serial().expect("Host[:Port] required");
		CommandBuilder::new(adb.as_os_str()).args(["disconnect", serial.as_str()]).output().await?;
		Ok(true)
	}

	pub async fn disconnect_all(adb: &Adb) -> Result<bool> {
		CommandBuilder::new(adb.0.as_path()).args(["disconnect"]).output().await?;
		Ok(true)
	}

	pub async fn reboot<'d, D>(adb: &Adb, device: D, reboot_type: Option<RebootType>) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut args: Vec<&str> = vec!["reboot"];
		let s = reboot_type.map(|f| f.value()).unwrap_or_default();
		args.push(s.as_str());
		let _output = CommandBuilder::device(adb, device).args(args).output().await?;
		Ok(())
	}

	pub async fn remount<'a, T>(adb: &Adb, device: T) -> Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		CommandBuilder::device(adb, device.into()).arg("remount").output().await?;
		Ok(())
	}

	pub async fn disable_verity<'a, T>(adb: &Adb, device: T) -> Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		CommandBuilder::device(adb, device.into()).arg("disable-verity").output().await?;
		Ok(())
	}

	pub async fn mount<'d, D, T: Arg>(adb: &Adb, device: D, dir: T) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::exec(adb, device, vec!["mount -o rw,remount", dir.as_str()?], None).await?;
		Ok(())
	}

	pub async fn unmount<'d, D, T: Arg>(adb: &Adb, device: D, dir: T) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::exec(adb, device, vec!["mount -o ro,remount", dir.as_str()?], None).await?;
		Ok(())
	}

	pub async fn bug_report<'d, D, T: Arg>(adb: &Adb, device: D, output: Option<T>) -> Result<ProcessResult>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let args = match output.as_ref() {
			Some(s) => vec!["bugreport", s.as_str()?],
			None => vec!["bugreport"],
		};
		CommandBuilder::device(adb, device).args(args).output().await
	}

	pub async fn clear_logcat<'d, D>(adb: &Adb, device: D) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		CommandBuilder::device(adb, device).args(["logcat", "-b", "all", "-c"]).output().await?;
		Ok(())
	}

	pub async fn get_mac_address<'d, D>(adb: &Adb, device: D) -> Result<MacAddress>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Shell::cat(adb, device, "/sys/class/net/eth0/address").await?;
		let mac_address_str = Arg::as_str(&output)?.trim_end();
		let mac_address = MacAddress::try_from(mac_address_str)?;
		Ok(mac_address)
	}

	pub async fn get_wlan_address<'d, D>(adb: &Adb, device: D) -> Result<MacAddress>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Shell::cat(adb, device, "/sys/class/net/wlan0/address").await?;
		let mac_address_str = Arg::as_str(&output)?.trim_end();
		let mac_address = MacAddress::try_from(mac_address_str)?;
		Ok(mac_address)
	}

	pub async fn get_boot_id<'d, D>(adb: &Adb, device: D) -> Result<Uuid>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Shell::cat(adb, device, "/proc/sys/kernel/random/boot_id").await?;
		let output_str = Arg::as_str(&output)?.trim();
		let boot_id = output_str.try_into()?;
		Ok(boot_id)
	}
}
