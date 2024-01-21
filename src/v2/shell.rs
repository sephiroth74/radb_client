use std::ffi::OsStr;
use std::process::Output;
use std::time::Duration;

use crossbeam_channel::Receiver;
use rustix::path::Arg;
use simple_cmd::CommandBuilder;

use crate::v2::prelude::*;
use crate::v2::result::Result;
use crate::v2::types::Shell;

impl<'a> Shell<'a> {
	/// executes custom command over the shell interface
	pub fn exec<T>(&self, args: Vec<T>, cancel: Option<Receiver<()>>, timeout: Option<Duration>) -> Result<Output>
	where
		T: Into<String> + AsRef<OsStr>,
	{
		let builder = CommandBuilder::shell(self.parent).args(args).signal(cancel).timeout(timeout);
		Ok(builder.build().output()?)
	}

	/// return if adb is running as root
	pub fn is_root(&self) -> Result<bool> {
		let whoami = self.whoami()?;
		Ok(whoami.eq_ignore_ascii_case("root"))
	}

	/// Returns the current running adb user
	///
	/// # Example
	///
	/// ```rust
	/// use radb_client::v2::types::Client;
	/// use radb_client::v2::types::ConnectionType;
	///
	/// fn get_user() {
	///     let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
	///     client.connect(None).unwrap();
	///     let output = client.shell().whoami().unwrap();
	/// }
	/// ```
	pub fn whoami(&self) -> Result<String> {
		let output = self.exec(vec!["whoami"], None, None)?;
		Ok(Arg::as_str(&output.stdout)?.trim().to_owned())
	}
}

#[cfg(test)]
mod test {
	use crate::v2::test::test::{connect_client, connect_emulator, connection_from_tcpip, init_log};

	#[test]
	fn test_who_am_i() {
		init_log();
		let client = connect_emulator();
		let whoami = client.shell().whoami().expect("failed to get user");
		println!("whoami: {whoami}");
		assert!(!whoami.is_empty());
	}

	#[test]
	fn test_is_root() {
		init_log();
		let client = connect_client(connection_from_tcpip());
		let whoami = client.shell().whoami().expect("failed to get user");
		println!("whoami: {whoami}");

		let result: crate::v2::result::Result<bool> = client.shell().is_root();
		let is_root = result.expect("failed to get root status");
		if whoami.eq_ignore_ascii_case("root") {
			assert!(is_root);
		} else {
			assert!(!is_root);
		}
	}
}
