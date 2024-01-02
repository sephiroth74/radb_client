use std::fmt::{Display, Formatter};
use std::process::Output;
use std::time::Duration;

use lazy_static::lazy_static;
use regex::Regex;
use rustix::path::Arg;
use simple_cmd::Error::CommandError;

use crate::dump_util::{package_flags, runtime_permissions, SimplePackageReader};
use crate::errors::AdbError;
use crate::types::{
	InstallOptions, InstallPermission, ListPackageDisplayOptions, ListPackageFilter, PackageFlags, RuntimePermission,
	UninstallOptions,
};
use crate::PackageManager;

static DUMP_TIMEOUT: Option<Duration> = Some(Duration::from_secs(1));

#[macro_export]
macro_rules! build_pm_operation {
	($name:tt, $operation_name:tt, $typ:ty, $typ2:ty) => {
		pub fn $name(&self, package_name: $typ, user: $typ2) -> crate::Result<()> {
			self.op($operation_name, package_name, user)
		}
	};
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Package {
	pub package_name: String,
	pub file_name: Option<String>,
	pub version_code: Option<i32>,
	pub uid: Option<i32>,
}

impl Display for Package {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.package_name).unwrap();

		if let Some(version_code) = self.version_code {
			write!(f, " version:{}", version_code).unwrap();
		}

		if let Some(uid) = self.uid {
			write!(f, " uid:{}", uid).unwrap();
		}

		if let Some(file_name) = &self.file_name {
			write!(f, " file_name:{}", file_name).unwrap();
		}

		Ok(())
	}
}

impl<'a> PackageManager<'a> {
	pub fn uninstall(&self, package_name: &str, options: Option<UninstallOptions>) -> crate::Result<Output> {
		let mut args = vec!["cmd package uninstall".to_string()];
		match options {
			None => {}
			Some(options) => args.extend::<Vec<String>>((&options).into()),
		}
		args.push(package_name.to_string());
		self.parent.exec(args, None, None)
	}

	pub fn install<T: Arg>(&self, src: T, options: Option<InstallOptions>) -> crate::Result<Output> {
		let mut args = vec!["cmd package install".to_string()];
		match options {
			None => {}
			Some(options) => args.extend(options),
		}
		args.push(src.as_str()?.into());
		self.parent.exec(args, None, None)
	}

	pub fn is_installed(&self, package_name: &str, user: Option<&str>) -> crate::Result<bool> {
		let r = self.path(package_name, user).map(|f| f.len() > 0);
		match r {
			Ok(r) => Ok(r),
			Err(err) => match err {
				AdbError::CmdError(CommandError(err)) => {
					if err.stderr.is_empty() && err.stdout.is_empty() {
						Ok(false)
					} else {
						Err(AdbError::CmdError(CommandError(err)))
					}
				}
				_ => Err(err),
			},
		}
	}

	pub fn is_system(&self, package_name: &str) -> crate::Result<bool> {
		Ok(self.package_flags(package_name)?.contains(&PackageFlags::System))
	}

	pub fn package_flags(&self, package_name: &str) -> crate::Result<Vec<PackageFlags>> {
		let result = self.dump(package_name, DUMP_TIMEOUT)?;
		package_flags(result.as_str())
	}

	pub fn requested_permissions(&self, package_name: &str) -> crate::Result<Vec<String>> {
		let dump = self.dump(package_name, DUMP_TIMEOUT)?;
		let pr = SimplePackageReader::new(dump.as_str())?;
		pr.requested_permissions()
	}

	pub fn install_permissions(&self, package_name: &str) -> crate::Result<Vec<InstallPermission>> {
		let dump = self.dump(package_name, DUMP_TIMEOUT)?;
		let pr = SimplePackageReader::new(dump.as_str())?;
		pr.install_permissions()
	}

	pub fn dump_runtime_permissions(&self, package_name: &str) -> crate::Result<Vec<RuntimePermission>> {
		let dump = self.dump(package_name, DUMP_TIMEOUT)?;
		runtime_permissions(dump.as_str())
	}

	pub fn dump(&self, package_name: &str, timeout: Option<Duration>) -> crate::Result<String> {
		let args = vec![format!("pm dump {:}", package_name)];
		let result = self.parent.exec(args, None, timeout)?.stdout;
		Ok(Arg::as_str(&result)?.to_string())
	}

	pub fn path(&self, package_name: &str, user: Option<&str>) -> Result<String, AdbError> {
		let mut args = vec![
			"pm", "path",
		];
		if let Some(u) = user {
			args.push("--user");
			args.push(u);
		}
		args.push(package_name);
		let result = self.parent.exec(args, None, None)?.stdout;
		let output = Arg::as_str(&result)?.trim_end();
		let split = output
			.split_once("package:")
			.map(|s| s.1.to_string())
			.ok_or(AdbError::NameNotFoundError(package_name.to_string()));
		split
	}

	pub fn grant(&self, package_name: &str, user: Option<&str>, permission: &str) -> crate::Result<()> {
		let mut args = vec!["pm grant"];
		if let Some(u) = user {
			args.extend(vec![
				"--user", u,
			]);
		}
		args.push(package_name);
		args.push(permission);
		self.parent.exec(args, None, None).map(|_f| ())
	}

	pub fn revoke(&self, package_name: &str, user: Option<&str>, permission: &str) -> crate::Result<()> {
		let mut args = vec!["pm revoke"];
		if let Some(u) = user {
			args.extend(vec![
				"--user", u,
			]);
		}
		args.push(package_name);
		args.push(permission);
		self.parent.exec(args, None, None).map(|_f| ())
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

	pub fn reset_permissions(&self) -> crate::Result<()> {
		self.parent
			.exec(
				vec![
					"pm",
					"reset-permissions",
				],
				None,
				None,
			)
			.map(|_f| ())
	}

	pub fn list_packages(
		&self,
		filters: Option<ListPackageFilter>,
		display: Option<ListPackageDisplayOptions>,
		name_filter: Option<&str>,
	) -> Result<Vec<Package>, AdbError> {
		let mut args = vec![
			"pm".into(),
			"list".into(),
			"packages".into(),
		];

		match filters {
			Some(filters) => args.extend(filters),
			None => {}
		}

		match display {
			None => {
				args.extend(ListPackageDisplayOptions::default());
			}
			Some(d) => args.extend(d),
		}

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

	// private methods

	fn op(&self, operation: &str, package_or_component: &str, user: Option<&str>) -> crate::Result<()> {
		let mut args = vec![
			"pm", operation,
		];
		if let Some(u) = user {
			args.extend(vec![
				"--user", u,
			]);
		}
		args.push(package_or_component);
		self.parent.exec(args, None, None).map(|_f| ())
	}
}
