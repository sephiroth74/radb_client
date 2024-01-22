use std::ffi::OsStr;
use std::process::Output;
use std::sync::Mutex;
use std::time::Duration;

use cached::{Cached, SizedCache};
use crossbeam_channel::Receiver;
use lazy_static::lazy_static;
use regex::Regex;
use rustix::path::Arg;
use simple_cmd::prelude::OutputExt;
use simple_cmd::CommandBuilder;

use crate::types::SELinuxType;
use crate::v2::prelude::*;
use crate::v2::result::Result;
use crate::v2::types::{ActivityManager, Shell};

lazy_static! {
	static ref RE_GET_PROPS: Regex = Regex::new("(?m)^\\[(.*)\\]\\s*:\\s*\\[([^\\]]*)\\]$").unwrap();
	static ref COMMANDS_CACHE: Mutex<SizedCache<String, Option<String>>> = Mutex::new(SizedCache::with_size(10));
}

impl<'a> Shell<'a> {
	/// executes custom command over the shell interface
	pub fn exec<I, S>(&self, args: I, cancel: Option<Receiver<()>>, timeout: Option<Duration>) -> Result<Output>
	where
		I: IntoIterator<Item = S>,
		S: AsRef<OsStr>,
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

	pub fn mount<T: Arg>(&self, dir: T) -> Result<()> {
		self.exec(
			vec![
				"mount -o rw,remount",
				dir.as_str()?,
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn unmount<T: Arg>(&self, dir: T) -> Result<()> {
		self.exec(
			vec![
				"mount -o ro,remount",
				dir.as_str()?,
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn cat<P: Arg>(&self, path: P) -> Result<Vec<u8>> {
		self.exec(
			vec![
				"cat",
				path.as_str()?,
			],
			None,
			None,
		)
		.map(|s| s.stdout)
	}

	/// Check if avbctl is available on the connected device
	fn check_avbctl(&self) -> Result<()> {
		self.get_command_path("avbctl")
			.map(|_| ())
			.ok_or(std::io::ErrorKind::NotFound.into())
	}

	/// Returns if avbctl is available
	#[allow(dead_code)]
	fn has_avbctl(&self) -> Result<bool> {
		self.check_avbctl().map(|_| true)
	}

	pub fn get_command_path<T: Arg>(&self, command: T) -> Option<String> {
		if let Ok(command_string) = command.as_str() {
			let mut binding = COMMANDS_CACHE.lock().unwrap();
			let cache_key = format!("{}{}", self.parent.addr, command_string);

			binding
				.cache_get_or_set_with(cache_key.clone(), || {
					self.exec(vec![format!("command -v {}", command_string).as_str()], None, None)
						.and_then(|result| Ok(Arg::as_str(&result.stdout)?.trim().to_string()))
						.and_then(|result| {
							if result.is_empty() {
								Err(std::io::ErrorKind::NotFound.into())
							} else {
								Ok(result)
							}
						})
						.ok()
				})
				.clone()
		} else {
			None
		}
	}

	pub fn which<T: Arg>(&self, command: T) -> Option<String> {
		if let Ok(command) = command.as_str() {
			let output = self.exec(
				vec![
					"which", command,
				],
				None,
				None,
			);
			if let Ok(output) = output {
				simple_cmd::Vec8ToString::as_str(&output.stdout).map(|ss| String::from(ss.trim_end()))
			} else {
				None
			}
		} else {
			None
		}
	}

	/// Returns the verity status
	pub fn get_verity(&self) -> Result<bool> {
		let _ = self.check_avbctl()?;
		let output = self.exec(
			vec![
				"avbctl",
				"get-verity",
			],
			None,
			None,
		)?;
		let string = Arg::as_str(&output.stdout)?;
		Ok(string.contains("enabled"))
	}

	/// Disable verity using the avbctl service, if available
	pub fn disable_verity(&self) -> Result<()> {
		let _ = self.check_avbctl()?;
		let output = self.exec(
			vec![
				"avbctl",
				"disable-verity",
			],
			None,
			None,
		)?;

		if output.error() {
			Err(output.into())
		} else {
			Ok(())
		}
	}

	/// Enable verity using the avbctl service, if available
	pub fn enable_verity(&self) -> Result<()> {
		let _ = self.check_avbctl()?;
		let output = self.exec(
			vec![
				"avbctl",
				"enable-verity",
			],
			None,
			None,
		)?;
		if output.error() {
			Err(output.into())
		} else {
			Ok(())
		}
	}

	/// Returns the selinux enforce status
	pub fn get_enforce(&self) -> Result<SELinuxType> {
		let result = self.exec(vec!["getenforce"], None, None)?.stdout;
		let enforce: SELinuxType = SELinuxType::try_from(result)?;
		Ok(enforce)
	}

	/// Change the selinux enforce type. root is required
	pub fn set_enforce(&self, enforce: SELinuxType) -> Result<()> {
		let new_value = match enforce {
			SELinuxType::Permissive => "0",
			SELinuxType::Enforcing => "1",
		};

		self.exec(
			vec![
				"setenforce",
				new_value,
			],
			None,
			None,
		)
		.map(|_| ())
	}

	pub fn am(&self) -> ActivityManager {
		ActivityManager { parent: self }
	}
}

#[cfg(test)]
mod test {
	use std::time::Duration;

	use crate::types::SELinuxType;
	use crate::v2::test::test::*;

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

	#[test]
	fn test_mount() {
		init_log();
		let client = connect_client(connection_from_tcpip());
		client.root().expect("failed to root");
		let _ = client.shell().mount("/system").expect("failed to mount");
		let _ = client.shell().unmount("/system").expect("failed to unmount");
	}

	#[test]
	fn test_check_avbctl() {
		init_log();
		let client = connect_emulator();
		client.shell().check_avbctl().expect_err("check_avbctl should fail");

		let client = connect_tcp_ip_client();
		let _result = client.shell().check_avbctl().expect("failed to check_avbctl");
	}

	#[test]
	fn test_get_command_path() {
		init_log();
		let client = connect_emulator();
		let path = client.shell().get_command_path("sh").expect("failed to get sh path");
		println!("path: {path}");
		assert_eq!("/system/bin/sh", path);

		let client = connect_tcp_ip_client();
		let path = client.shell().get_command_path("sh").expect("failed to get sh path");
		println!("path: {path}");
		assert_eq!("/system/bin/sh", path);
	}

	#[test]
	fn test_which() {
		init_log();
		let client = connect_emulator();
		let path = client.shell().which("sh").expect("failed to get sh path");
		println!("path: {path}");
		assert_eq!("/system/bin/sh", path);

		let client = connect_tcp_ip_client();
		let path = client.shell().which("sh").expect("failed to get sh path");
		println!("path: {path}");
		assert_eq!("/system/bin/sh", path);
	}

	#[test]
	fn test_get_verity() {
		init_log();
		let client = connect_tcp_ip_client();
		let has_avbctl = client.shell().has_avbctl().expect("failed to check for avbctl");
		println!("has_avbctl: {has_avbctl}");
		assert!(has_avbctl);

		let verity = client.shell().get_verity().expect("failed to get verity status");
		println!("verity status: {verity}");
	}

	#[test]
	fn test_toggle_verity() {
		init_log();
		let client = connect_tcp_ip_client();
		client.root().expect("failed to root");
		let enabled = client.shell().get_verity().expect("failed to get verity");
		println!("verity is enabled: {enabled}");

		if enabled {
			client.shell().disable_verity().expect("failed to disable verity");
		} else {
			client.shell().enable_verity().expect("failed to disable verity");
		}

		client.reboot(None).expect("failed to reboot device");
		client
			.wait_for_device(Some(Duration::from_secs(120)))
			.expect("failed to wait for device");

		client.root().expect("failed to root");
		let verity_enabled = client.shell().get_verity().expect("failed to get verity");
		println!("verity is now enabled: {verity_enabled}");

		assert_ne!(enabled, verity_enabled);
	}

	#[test]
	fn test_get_enforce() {
		let client = connect_emulator();
		let enforce: SELinuxType = client.shell().get_enforce().expect("failed to get enforce");
		println!("enforce: {enforce}");

		assert_eq!(SELinuxType::Enforcing, enforce);

		let client = connect_tcp_ip_client();
		let enforce = client.shell().get_enforce().expect("failed to get enforce");
		println!("enforce: {enforce}");
	}

	#[test]
	fn test_set_enforce() {
		init_log();

		let client = connect_tcp_ip_client();
		root_client(&client);
		let enforce = client.shell().get_enforce().expect("failed to get enforce");
		println!("enforce: {enforce}");

		if enforce == SELinuxType::Enforcing {
			client
				.shell()
				.set_enforce(SELinuxType::Permissive)
				.expect("failed to change enforce type");
		} else {
			client
				.shell()
				.set_enforce(SELinuxType::Enforcing)
				.expect("failed to change enforce type");
		}

		let now_enforce = client.shell().get_enforce().expect("failed to get enforce");
		println!("now enforce: {now_enforce}");
		assert_ne!(enforce, now_enforce);
	}
}
