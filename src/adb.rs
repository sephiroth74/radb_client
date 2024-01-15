use crossbeam_channel::Receiver;
use std::ffi::OsStr;
use std::io::BufRead;
use std::path::Path;
use std::process::Output;
use std::time::Duration;

use crate::cmd_ext::CommandBuilderExt;
use lazy_static::lazy_static;
use regex::Regex;
use simple_cmd::{Cmd, CommandBuilder};
use which::which;

use crate::errors::AdbError;
use crate::errors::AdbError::WhichError;
use crate::traits::AdbDevice;

use super::Device;
use super::{Adb, AdbClient};

impl Adb {
	pub fn new() -> Result<Adb, AdbError> {
		let adb = which("adb")?;
		Ok(Adb(adb))
	}

	pub fn copy(other: &Adb) -> Adb {
		Adb(other.0.to_path_buf())
	}

	pub async fn root(&self) -> Result<(), AdbError> {
		Cmd::builder(self.0.as_path())
			.args(["root"])
			.build()
			.output()
			.map_err(|e| e.into())
			.map(|_| ())
	}

	pub fn unroot(&self) -> Result<(), AdbError> {
		Cmd::builder(self.0.as_path())
			.args(["unroot"])
			.build()
			.output()
			.map_err(|e| e.into())
			.map(|_| ())
	}

	pub fn exec<'a, D, T>(
		&self,
		device: D,
		args: Vec<T>,
		cancel: Option<Receiver<()>>,
		timeout: Option<Duration>,
		debug: bool,
	) -> crate::Result<Output>
	where
		T: Into<String> + AsRef<OsStr>,
		D: Into<&'a dyn AdbDevice>,
	{
		let builder = CommandBuilder::adb(self)
			.device(device)
			.args(args)
			.signal(cancel)
			.with_debug(debug)
			.timeout(timeout);
		Ok(builder.build().output()?)
	}

	pub fn from(path: &Path) -> Result<Adb, AdbError> {
		if !path.exists() {
			return Err(WhichError(which::Error::CannotFindBinaryPath));
		}
		Ok(Adb(path.to_path_buf()))
	}

	pub fn as_os_str(&self) -> &OsStr {
		self.as_ref()
	}

	/// List connected devices
	pub fn devices(&self) -> Result<Vec<Box<dyn AdbDevice>>, AdbError> {
		let output = Cmd::builder(self.0.as_path())
			.args([
				"devices", "-l",
			])
			.build()
			.output()?;

		lazy_static! {
			static ref RE: Regex = Regex::new(
				"(?P<ip>[^\\s]+)[\\s]+(device|offline) product:(?P<device_product>[^\\s]+)\\smodel:(?P<model>[^\\s]+)\\sdevice:(?P<device>[^\\s]+)\\stransport_id:(?P<transport_id>[^\\s]+)"
			)
			.unwrap();
		}

		let mut devices: Vec<Box<dyn AdbDevice>> = vec![];
		for line in output.stdout.lines() {
			let line_str = line?;

			if RE.is_match(line_str.as_str()) {
				let captures = RE.captures(line_str.as_str());
				match captures {
					None => {}
					Some(c) => {
						let ip = c.name("ip").unwrap().as_str();
						let tr = c.name("transport_id").unwrap().as_str().parse::<u8>().unwrap();
						let device = Device::try_from_ip(ip)
							.or(Device::try_from_transport_id(tr).or(Device::try_from_serial(line_str.as_str())));
						if let Ok(d) = device {
							devices.push(Box::new(d))
						}
					}
				}
			}
		}
		Ok(devices)
	}

	pub fn device(&self, input: &str) -> Result<Box<dyn AdbDevice>, AdbError> {
		let d = Device::try_from_serial(input)?;
		Ok(Box::new(d))
	}

	pub fn client(&self, input: &str) -> Result<AdbClient, AdbError> {
		let d = Device::try_from_serial(input)?;
		Ok(AdbClient::try_from_device(d)?)
	}
}
