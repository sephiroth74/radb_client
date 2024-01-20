use std::ffi::OsStr;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::process::Output;
use std::time::Duration;

use crossbeam_channel::Receiver;
use simple_cmd::CommandBuilder;
use which::which;

use crate::v2::prelude::*;
use crate::v2::types::{Adb, AddressType};

impl Adb {
	pub fn new() -> crate::v2::result::Result<Adb> {
		let adb = which("adb")?;
		Ok(Adb(adb))
	}

	pub fn exec<'a, C, T>(
		&self,
		addr: C,
		args: Vec<T>,
		cancel: Option<Receiver<()>>,
		timeout: Option<Duration>,
		debug: bool,
	) -> crate::Result<Output>
	where
		T: Into<String> + AsRef<OsStr>,
		C: Into<AddressType>,
	{
		let builder = CommandBuilder::adb(&self)
			.addr(addr)
			.with_debug(debug)
			.args(args)
			.signal(cancel)
			.timeout(timeout);
		Ok(builder.build().output()?)
	}

	pub fn as_os_str(&self) -> &OsStr {
		self.as_ref()
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
mod test {
	use std::path::PathBuf;

	use crate::v2::types::{Adb, AddressType};

	static DEVICE_IP: &'static str = "192.168.1.34:5555";

	#[test]
	fn test_adb() {
		let _adb = Adb::new().expect("failed to find adb command in you PATH");
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
		crate::v2::test::test::init_log();
		let adb = Adb::new().expect("failed to find adb");
		let addr = AddressType::try_from(DEVICE_IP).unwrap();
		let result = adb.exec(addr, vec!["get-state"], None, None, true).unwrap();
		println!("result: {result:?}");

		let addr = AddressType::Transport(4);
		let result = adb.exec(addr, vec!["get-state"], None, None, true).unwrap();
		println!("result: {result:?}");
	}
}
