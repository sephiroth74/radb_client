use std::time::Duration;

use lazy_static::lazy_static;
use regex::Regex;
use rustix::path::Arg;

use crate::types::{ListPackageDisplayOptions, ListPackageFilter};
use crate::v2::dump_util::{package_flags, runtime_permissions};
use crate::v2::error::Error;
use crate::v2::result::Result;
use crate::v2::types::{Package, PackageFlags, PackageManager, RuntimePermission};

static DUMP_TIMEOUT: Option<Duration> = Some(Duration::from_secs(1));

macro_rules! build_pm_operation {
	($name:tt, $operation_name:tt, $typ:ty, $typ2:ty) => {
		pub fn $name(&self, package_name: $typ, user: $typ2) -> Result<()> {
			self.operation($operation_name, package_name, user)
		}
	};
}

impl<'a> PackageManager<'a> {
	/// Return the path of a given package name
	pub fn path(&self, package_name: &str, user: Option<&str>) -> Result<String> {
		let mut args = vec![
			"pm", "path",
		];
		if let Some(u) = user {
			args.push("--user");
			args.push(u);
		}
		args.push(package_name);
		let result = self.parent.exec(args, None, None)?.stdout;
		let output = Arg::as_str(&result)?.trim();
		let split = output
			.split_once("package:")
			.map(|s| s.1.to_string())
			.ok_or(Error::PackageNotFoundError(package_name.to_string()));
		split
	}

	// Grant permission to given package
	pub fn grant(&self, package_name: &str, permission: &str, user: Option<&str>) -> Result<()> {
		let mut args = vec![
			"pm", "grant",
		];
		if let Some(u) = user {
			args.extend(vec![
				"--user", u,
			]);
		}
		args.push(package_name);
		args.push(permission);
		super::shell::handle_result(self.parent.exec(args, None, None)?)
	}

	// Revoke permission to given package
	pub fn revoke(&self, package_name: &str, permission: &str, user: Option<&str>) -> Result<()> {
		let mut args = vec![
			"pm", "revoke",
		];
		if let Some(u) = user {
			args.extend(vec![
				"--user", u,
			]);
		}
		args.push(package_name);
		args.push(permission);
		super::shell::handle_result(self.parent.exec(args, None, None)?)
	}

	/// Revert all runtime permissions to their default state
	pub fn reset_permissions(&self) -> Result<()> {
		super::shell::handle_result(self.parent.exec(
			vec![
				"pm",
				"reset-permissions",
			],
			None,
			None,
		)?)
	}

	pub fn list_packages(
		&self,
		filters: ListPackageFilter,
		display: ListPackageDisplayOptions,
		name_filter: Option<&str>,
	) -> Result<Vec<Package>> {
		let mut args = vec![
			"pm".into(),
			"list".into(),
			"packages".into(),
		];

		args.extend(filters);
		args.extend(display);

		if let Some(name) = name_filter {
			args.push(name.to_string());
		}

		let output = self.parent.exec(args, None, None)?.stdout;
		let string = Arg::as_str(&output)?;

		lazy_static! {
			static ref RE: Regex = Regex::new(
				"package:((?P<file>.*\\.apk)=)?(?P<name>\\S+)(\\s(versionCode|uid):(\\d+))?(\\s(versionCode|uid):(\\d+))?"
			)
			.unwrap();
		}

		let captures = RE.captures_iter(string);
		let result = captures
			.into_iter()
			.filter_map(|m| {
				if m.len() == 10 {
					let name = m.name("name")?.as_str();
					let file_name = m.name("file").map(|s| s.as_str().to_string());

					let (version_code_str, uid_str) = match m.get(5).map(|m| m.as_str()) {
						Some("versionCode") => (m.get(6).map(|m| m.as_str()), m.get(9).map(|m| m.as_str())),
						Some("uid") => (m.get(9).map(|m| m.as_str()), m.get(6).map(|m| m.as_str())),
						_ => (None, None),
					};

					let version_code = if let Some(vcode) = version_code_str {
						Some(vcode.parse::<i32>().ok()?)
					} else {
						None
					};

					let uid = if let Some(uid) = uid_str {
						Some(uid.parse::<i32>().ok()?)
					} else {
						None
					};

					Some(Package {
						package_name: name.to_string(),
						file_name,
						version_code,
						uid,
					})
				} else {
					None
				}
			})
			.collect::<Vec<_>>();
		Ok(result)
	}

	pub fn dump(&self, package_name: &str, timeout: Option<Duration>) -> Result<String> {
		let args = vec![
			"pm",
			"dump",
			package_name.into(),
		];
		let result = self.parent.exec(args, None, timeout)?.stdout;
		Ok(Arg::as_str(&result)?.to_string())
	}

	pub fn dump_runtime_permissions(&self, package_name: &str) -> Result<Vec<RuntimePermission>> {
		let dump = self.dump(package_name, DUMP_TIMEOUT)?;
		runtime_permissions(dump.as_str())
	}

	pub fn package_flags(&self, package_name: &str) -> Result<Vec<PackageFlags>> {
		let result = self.dump(package_name, DUMP_TIMEOUT)?;
		package_flags(result.as_str())
	}

	pub fn is_system(&self, package_name: &str) -> Result<bool> {
		Ok(self.package_flags(package_name)?.contains(&PackageFlags::System))
	}

	pub fn is_installed(&self, package_name: &str, user: Option<&str>) -> Result<bool> {
		let r = self.path(package_name, user).map(|f| f.len() > 0);
		println!("r = {r:?}");
		match r {
			Ok(r) => Ok(r),
			Err(err) => match err {
				Error::PackageNotFoundError(_) => Ok(false),
				Error::CommandError(simple_cmd::Error::CommandError(err)) => {
					if err.stderr.is_empty() && err.stdout.is_empty() {
						Ok(false)
					} else {
						Err(Error::CommandError(err.into()))
					}
				}
				_ => Err(err),
			},
		}
	}

	build_pm_operation!(clear, "clear", &str, Option<&str>);

	build_pm_operation!(suspend, "suspend", &str, Option<&str>);

	build_pm_operation!(unsuspend, "unsuspend", &str, Option<&str>);

	build_pm_operation!(hide, "hide", &str, Option<&str>);

	build_pm_operation!(unhide, "unhide", &str, Option<&str>);

	build_pm_operation!(default_state, "default-state", &str, Option<&str>);

	build_pm_operation!(disable_until_used, "disable-until-used", &str, Option<&str>);

	build_pm_operation!(disable_user, "disable-user", &str, Option<&str>);

	build_pm_operation!(disable, "disable", &str, Option<&str>);

	build_pm_operation!(enable, "enable", &str, Option<&str>);

	pub(crate) fn operation(&self, operation: &str, package_or_component: &str, user: Option<&str>) -> Result<()> {
		let mut args = vec![
			"pm", operation,
		];
		if let Some(u) = user {
			args.extend(vec![
				"--user", u,
			]);
		}
		args.push(package_or_component);
		super::shell::handle_result(self.parent.exec(args, None, None)?)
	}
}

#[cfg(test)]
mod test {
	use itertools::Itertools;

	use crate::types::{ListPackageDisplayOptions, ListPackageFilter};
	use crate::v2::test::test::*;

	#[test]
	fn test_path() {
		init_log();
		let client = connect_emulator();
		let path = client
			.shell()
			.pm()
			.path("com.android.bluetooth", None)
			.expect("failed to get package path");
		println!("path: {path}");
		assert!(!path.is_empty());
	}

	#[test]
	fn test_revoke() {
		init_log();
		let client = connect_tcp_ip_client();
		client
			.shell()
			.pm()
			.revoke(
				"com.swisscom.aot.library.standalone",
				"android.permission.BLUETOOTH_SCAN",
				None,
			)
			.expect("failed to revoke permission");
	}

	#[test]
	fn test_grant() {
		init_log();
		let client = connect_tcp_ip_client();
		client
			.shell()
			.pm()
			.grant(
				"com.swisscom.aot.library.standalone",
				"android.permission.BLUETOOTH_SCAN",
				None,
			)
			.expect("failed to grant permission");
	}

	#[test]
	fn test_enable_package() {
		init_log();
		let client = connect_tcp_ip_client();
		root_client(&client);

		let package_name = "com.android.bluetooth";
		let user_id = Some("0");

		client
			.shell()
			.pm()
			.clear(package_name, user_id)
			.expect("failed to clear package");
		client
			.shell()
			.pm()
			.suspend(package_name, user_id)
			.expect("failed to suspend package");
		client
			.shell()
			.pm()
			.unsuspend(package_name, user_id)
			.expect("failed to unsuspend package");
		client
			.shell()
			.pm()
			.hide(package_name, user_id)
			.expect("failed to hide package");
		client
			.shell()
			.pm()
			.unhide(package_name, user_id)
			.expect("failed to unhide package");
		client
			.shell()
			.pm()
			.default_state(package_name, user_id)
			.expect("failed to set default-state for package");
		client
			.shell()
			.pm()
			.disable_until_used(package_name, user_id)
			.expect("failed to set default-state for package");
		client
			.shell()
			.pm()
			.disable_user(package_name, user_id)
			.expect("failed to disable user for package");
		client
			.shell()
			.pm()
			.enable(package_name, user_id)
			.expect("failed to enable package");
		client
			.shell()
			.pm()
			.disable(package_name, user_id)
			.expect("failed to disable package");
		client
			.shell()
			.pm()
			.enable(package_name, user_id)
			.expect("failed to enable package");
	}

	#[test]
	fn test_reset_permissions() {
		init_log();
		let client = connect_emulator();
		client.shell().pm().reset_permissions().expect("failed to reset permissions");
	}

	#[test]
	fn test_list_packages() {
		init_log();
		let client = connect_emulator();
		let packages = client
			.shell()
			.pm()
			.list_packages(
				ListPackageFilter {
					show_only_disabled: false,
					show_only_enabed: true,
					show_only_system: false,
					show_only3rd_party: true,
					apex_only: false,
					uid: None,
					user: None,
				},
				ListPackageDisplayOptions {
					show_uid: true,
					show_version_code: true,
					include_uninstalled: true,
					show_apk_file: true,
				},
				None,
			)
			.expect("failed to list packages");

		assert!(!packages.is_empty());

		for package in packages {
			println!("package: {package}");
		}
	}

	#[test]
	fn test_dump() {
		init_log();
		let client = connect_emulator();
		let dump = client
			.shell()
			.pm()
			.dump("com.android.bluetooth", None)
			.expect("failed to dump package");
		assert!(!dump.is_empty());
		println!("dump: {dump}");
	}

	#[test]
	fn test_dump_runtime_permissions() {
		init_log();
		let client = connect_tcp_ip_client();
		let permissions = client
			.shell()
			.pm()
			.dump_runtime_permissions("com.swisscom.aot.library.standalone")
			.expect("failed to get runtime permissions");

		assert!(!permissions.is_empty());

		for permission in permissions {
			println!("permission: {permission}");
		}
	}

	#[test]
	fn test_package_flags() {
		init_log();
		let client = connect_tcp_ip_client();
		let flags = client
			.shell()
			.pm()
			.package_flags("com.swisscom.aot.library.standalone")
			.expect("failed to get package flags");

		assert!(!flags.is_empty());
		println!("package flags: {}", flags.iter().map(|p| format!("{}", p)).join(","));
	}

	#[test]
	fn test_is_system() {
		init_log();
		let client = connect_emulator();
		let result = client
			.shell()
			.pm()
			.is_system("com.android.bluetooth")
			.expect("failed to call is_system");
		println!("is_system: {result}");
		assert!(result);
	}

	#[test]
	fn test_is_installed() {
		init_log();
		let client = connect_emulator();
		let result = client
			.shell()
			.pm()
			.is_installed("com.android.bluetooth", None)
			.expect("failed to call is_installed");
		assert!(result);

		let result = client
			.shell()
			.pm()
			.is_installed("com.android.bluetooth", Some("0"))
			.expect("failed to call is_installed");
		assert!(result);

		let result = client
			.shell()
			.pm()
			.is_installed("com.android.xxx", None)
			.expect("failed to call is_installed");
		assert!(!result);
	}
}
