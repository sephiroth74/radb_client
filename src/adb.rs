use std::ffi::OsStr;
use std::io::BufRead;
use std::path::Path;

use lazy_static::lazy_static;
use regex::Regex;
use which::which;

use crate::command::CommandBuilder;
use crate::errors::AdbError;
use crate::errors::AdbError::AdbNotFoundError;
use crate::traits::AdbDevice;

use super::Adb;
use super::Device;

impl Adb {
	pub fn new() -> Result<Adb, AdbError> {
		let adb = which("adb")?;
		Ok(Adb(adb))
	}

	pub async fn root(&self) -> Result<(), AdbError> {
		CommandBuilder::new(self.0.as_path()).args(["root"]).output().await.map(|_| ())
	}

	pub async fn unroot(&self) -> Result<(), AdbError> {
		CommandBuilder::new(self.0.as_path()).args(["unroot"]).output().await.map(|_| ())
	}

	pub fn from(path: &Path) -> Result<Adb, AdbError> {
		if !path.exists() {
			return Err(AdbNotFoundError(which::Error::CannotFindBinaryPath));
		}
		Ok(Adb(path.to_path_buf()))
	}

	pub fn as_os_str(&self) -> &OsStr {
		self.as_ref()
	}

	/// List connected devices
	pub async fn devices(&self) -> Result<Vec<Box<dyn AdbDevice>>, AdbError> {
		let output = CommandBuilder::new(self.0.as_path()).args(["devices", "-l"]).output().await?;

		lazy_static! {
			static ref RE: Regex = Regex::new(
				"(?P<ip>[^\\s]+)[\\s]+(device|offline) product:(?P<device_product>[^\\s]+)\\smodel:(?P<model>[^\\s]+)\\sdevice:(?P<device>[^\\s]+)\\stransport_id:(?P<transport_id>[^\\s]+)"
			)
			.unwrap();
		}

		let mut devices: Vec<Box<dyn AdbDevice>> = vec![];
		let stdout = output.stdout();
		for line in stdout.lines() {
			let line_str = line?;

			if RE.is_match(line_str.as_str()) {
				let captures = RE.captures(line_str.as_str());
				match captures {
					None => {}
					Some(c) => {
						let ip = c.name("ip").unwrap().as_str();
						let tr = c.name("transport_id").unwrap().as_str().parse::<u8>().unwrap();
						let device = Device::try_from_ip(ip).or(Device::try_from_transport_id(tr).or(Device::try_from_serial(line_str.as_str())));
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
}
