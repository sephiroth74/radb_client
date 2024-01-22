use std::ffi::OsString;
use std::process::Output;

use cmd_lib::AsOsStr;
use simple_cmd::prelude::OutputExt;

use crate::types::Intent;
use crate::v2::result::Result;
use crate::v2::traits::AsArgs;
use crate::v2::types::{ActivityManager, MemoryStatus, UserOption};

impl<'a> ActivityManager<'a> {
	/// Force stop a package
	pub fn force_stop(&self, package_name: &str) -> Result<()> {
		let result = self.parent.exec(
			vec![
				"am",
				"force-stop",
				package_name,
			],
			None,
			None,
		)?;
		ActivityManager::handle_result(result)
	}

	/// Start a service (using am start-service)
	pub fn start_service(&self, intent: &Intent) -> Result<()> {
		let result = self.parent.exec(
			vec![
				"am",
				"startservice",
				format!("{:}", intent).as_str(),
			],
			None,
			None,
		)?;
		ActivityManager::handle_result(result)
	}

	/// Start a foreground service (using am start-service)
	pub fn start_foreground_service(&self, intent: &Intent) -> Result<()> {
		let result = self.parent.exec(
			vec![
				"am",
				"start-foreground-service",
				format!("{:}", intent).as_str(),
			],
			None,
			None,
		)?;
		ActivityManager::handle_result(result)
	}

	pub fn start(&self, intent: &Intent) -> Result<()> {
		let result = self.parent.exec(
			vec![
				"am",
				"start",
				format!("{:}", intent).as_str(),
			],
			None,
			None,
		)?;
		ActivityManager::handle_result(result)
	}

	pub fn broadcast(&self, intent: &Intent) -> Result<()> {
		let result = self.parent.exec(
			vec![
				"am",
				"broadcast",
				format!("{:}", intent).as_str(),
			],
			None,
			None,
		)?;
		ActivityManager::handle_result(result)
	}

	/// Kill all background processes associated with the given application.
	pub fn kill(&self, user: UserOption, package_name: &str) -> Result<()> {
		let mut args: Vec<OsString> = vec![
			"am".as_os_str(),
			"kill".as_os_str(),
		];
		args.extend(user.as_args());
		args.push(package_name.into());

		let result = self.parent.exec(args, None, None)?;
		ActivityManager::handle_result(result)
	}

	/// Kill all processes that are safe to kill
	pub fn kill_all(&self) -> Result<()> {
		let result = self.parent.exec(
			[
				"am", "kill-all",
			],
			None,
			None,
		)?;
		ActivityManager::handle_result(result)
	}

	/// Send a memory trim event to a <PROCESS>.  May also supply a raw trim int level.
	pub fn trim_memory(&self, process_name: &str, status: MemoryStatus) -> Result<()> {
		let result = self.parent.exec(
			[
				"am",
				"send-trim-memory",
				process_name,
				&status.to_string(),
			],
			None,
			None,
		)?;
		ActivityManager::handle_result(result)
	}

	/// Induce a VM crash in the specified package or process
	pub fn crash(&self, user_id: Option<String>, package_or_pid: &str) -> Result<()> {
		let mut args = vec![
			"am".as_os_str(),
			"crash".as_os_str(),
		];
		if let Some(user_id) = user_id {
			args.push(user_id.as_os_str());
		}
		args.push(package_or_pid.as_os_str());
		let result = self.parent.exec(args, None, None)?;
		ActivityManager::handle_result(result)
	}

	/// Returns id of the current foreground user.
	pub fn get_current_user(&self) -> Result<String> {
		let result = self.parent.exec(
			[
				"am",
				"get-current-user",
			],
			None,
			None,
		)?;
		if result.error() && !result.kill() && !result.interrupt() {
			Err(result.into())
		} else {
			Ok(rustix::path::Arg::as_str(&result.stdout)?.trim().to_string())
		}
	}

	fn handle_result(result: Output) -> Result<()> {
		if result.error() && !result.kill() && !result.interrupt() {
			Err(result.into())
		} else {
			Ok(())
		}
	}
}

#[cfg(test)]
mod test {
	use crate::types::Intent;
	use crate::v2::test::test::{connect_emulator, connect_tcp_ip_client, init_log, root_client};
	use crate::v2::types::{MemoryStatus, UserOption};

	#[test]
	fn test_force_stop() {
		let client = connect_tcp_ip_client();
		client
			.shell()
			.am()
			.force_stop("com.android.bluetooth")
			.expect("failed to stop service");
	}

	#[test]
	fn test_start_service() {
		init_log();
		let client = connect_tcp_ip_client();
		let mut intent = Intent::from_action("swisscom.android.tv.action.FIRMWARE_ACTIVE_CHECK");
		intent.component = Some(format!("{}/.service.SystemService", "com.swisscom.aot.library.standalone"));
		intent.wait = true;

		client.shell().am().start_service(&intent).expect("failed to start service");
	}

	#[test]
	fn test_start_foreground_service() {
		init_log();
		let client = connect_tcp_ip_client();
		let mut intent = Intent::from_action("swisscom.android.tv.action.FIRMWARE_ACTIVE_CHECK");
		intent.component = Some(format!("{}/.service.SystemService", "com.swisscom.aot.library.standalone"));
		intent.wait = true;

		client
			.shell()
			.am()
			.start_foreground_service(&intent)
			.expect("failed to start service");
	}

	#[test]
	fn test_broadcast() {
		init_log();
		let client = connect_tcp_ip_client();

		let mut intent = Intent::from_action("com.google.android.katniss.action.ENABLE_INTENT_LOGGER");
		intent.receiver_foreground = true;
		intent.wait = true;
		intent.package = Some("com.google.android.katniss".to_string());

		client.shell().am().broadcast(&intent).expect("failed to send broadcast");
	}

	#[test]
	fn test_kill() {
		init_log();
		let client = connect_tcp_ip_client();
		client
			.shell()
			.am()
			.kill(UserOption::Current, "com.swisscom.aot.library.standalone")
			.expect("failed to kill");
	}

	#[test]
	fn test_kill_all() {
		init_log();
		let client = connect_tcp_ip_client();
		root_client(&client);
		client.shell().am().kill_all().expect("failed to kill all");
	}

	#[test]
	fn test_trim_memory() {
		init_log();
		let client = connect_tcp_ip_client();
		root_client(&client);

		client
			.shell()
			.am()
			.trim_memory("com.swisscom.aot.ui", MemoryStatus::RunningLow)
			.expect("failed to trim memory");
	}

	#[test]
	fn test_crash() {
		init_log();
		let client = connect_tcp_ip_client();
		client
			.shell()
			.am()
			.crash(None, "com.swisscom.aot.ui")
			.expect("failed to crash process");
	}

	#[test]
	fn test_get_current_user() {
		init_log();
		let client = connect_emulator();
		let user = client.shell().am().get_current_user().expect("failed to get current user");
		println!("current user: {user}");
	}

	#[test]
	fn test_start() {
		init_log();
		let client = connect_tcp_ip_client();

		let mut intent = Intent::from_action("android.intent.action.VIEW");
		intent.data = Some("http://www.google.com".to_string());
		intent.wait = true;
		client.shell().am().start(&intent).expect("failed to send am start");
	}
}
