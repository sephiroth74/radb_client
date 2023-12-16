use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};
use rustix::path::Arg;
use std::fmt::{Display, Formatter};

use crate::dump_util::{extract_runtime_permissions, SimplePackageReader};
use crate::errors::AdbError;

use crate::process::ProcessResult;
use crate::types::{InstallOptions, InstallPermission, ListPackageDisplayOptions, ListPackageFilter, PackageFlags, RuntimePermission, UninstallOptions};
use crate::PackageManager;

#[macro_export]
macro_rules! build_pm_operation {
	($name:tt, $operation_name:tt, $typ:ty, $typ2:ty) => {
		pub async fn $name(&self, package_name: $typ, user: $typ2) -> crate::process::Result<()> {
			self.op($operation_name, package_name, user).await
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
	pub async fn uninstall(&self, package_name: &str, options: Option<UninstallOptions>) -> crate::process::Result<ProcessResult> {
		let mut args = vec!["cmd package uninstall".to_string()];
		match options {
			None => {}
			Some(options) => args.extend::<Vec<String>>((&options).into()),
		}
		args.push(package_name.to_string());
		self.parent.exec(args, None).await
	}

	pub async fn install<T: Arg>(&self, src: T, options: Option<InstallOptions>) -> crate::process::Result<ProcessResult> {
		let mut args = vec!["cmd package install".to_string()];
		match options {
			None => {}
			Some(options) => args.extend(options),
		}
		args.push(src.as_str()?.into());
		self.parent.exec(args, None).await
	}

	pub async fn is_installed(&self, package_name: &str, user: Option<&str>) -> crate::process::Result<bool> {
		let r = self.path(package_name, user).await.map(|f| f.len() > 0);
		match r {
			Ok(r) => Ok(r),
			Err(err) => match err {
				AdbError::CmdError(err) => {
					if err.stderr.is_empty() && err.stdout.is_empty() {
						Ok(false)
					} else {
						Err(AdbError::CmdError(err))
					}
				}
				_ => Err(err),
			},
		}
	}

	pub async fn is_system(&self, package_name: &str) -> crate::process::Result<bool> {
		//let result = self.parent.exec(vec![format!("pm dump {: } | egrep '^ { { 1, } }flags = '  | egrep ' { { 1, } }SYSTEM { { 1, } }'", package_name)], None).await?.stdout();
		Ok(self.package_flags(package_name).await?.contains(&PackageFlags::System))
	}

	pub async fn package_flags(&self, package_name: &str) -> crate::process::Result<Vec<PackageFlags>> {
		let result = self.dump(package_name).await?;
		lazy_static! {
			static ref RE: Regex = RegexBuilder::new("^\\s*pkgFlags=\\[\\s(.*)\\s]").multi_line(true).build().unwrap();
		}

		if let Some(captures) = RE.captures(result.as_str()) {
			if captures.len() == 2 {
				let flags = captures.get(1).unwrap().as_str().split(" ").collect::<Vec<_>>();
				let package_flags = flags
					.iter()
					.filter_map(|line| if let Ok(flag) = (*line).try_into() { Some(flag) } else { None })
					.collect::<Vec<PackageFlags>>();
				Ok(package_flags)
			} else {
				Err(AdbError::ParseInputError())
			}
		} else {
			Err(AdbError::ParseInputError())
		}
	}

	pub async fn requested_permissions(&self, package_name: &str) -> crate::process::Result<Vec<String>> {
		let dump = self.dump(package_name).await?;
		let pr = SimplePackageReader::new(dump.as_str())?;
		pr.requested_permissions().await
	}

	pub async fn install_permissions(&self, package_name: &str) -> crate::process::Result<Vec<InstallPermission>> {
		let dump = self.dump(package_name).await?;
		let pr = SimplePackageReader::new(dump.as_str())?;
		pr.install_permissions().await
	}

	pub async fn dump_runtime_permissions(&self, package_name: &str) -> crate::process::Result<Vec<RuntimePermission>> {
		let dump = self.dump(package_name).await?;
		extract_runtime_permissions(dump.as_str()).await
	}

	pub async fn dump(&self, package_name: &str) -> crate::process::Result<String> {
		let args = vec!["pm", "dump", package_name];
		let result = self.parent.exec(args, None).await?.stdout();
		Ok(Arg::as_str(&result)?.to_string())
	}

	pub async fn path(&self, package_name: &str, user: Option<&str>) -> Result<String, AdbError> {
		let mut args = vec!["pm", "path"];
		if let Some(u) = user {
			args.push("--user");
			args.push(u);
		}
		args.push(package_name);
		let result = self.parent.exec(args, None).await?.stdout();
		let output = Arg::as_str(&result)?.trim_end();
		let split = output.split_once("package:").map(|s| s.1.to_string()).ok_or(AdbError::NameNotFoundError(package_name.to_string()));
		split
	}

	pub async fn grant(&self, package_name: &str, user: Option<&str>, permission: &str) -> crate::process::Result<()> {
		let mut args = vec!["pm grant"];
		if let Some(u) = user {
			args.extend(vec!["--user", u]);
		}
		args.push(package_name);
		args.push(permission);
		self.parent.exec(args, None).await.map(|_f| ())
	}

	pub async fn revoke(&self, package_name: &str, user: Option<&str>, permission: &str) -> crate::process::Result<()> {
		let mut args = vec!["pm revoke"];
		if let Some(u) = user {
			args.extend(vec!["--user", u]);
		}
		args.push(package_name);
		args.push(permission);
		self.parent.exec(args, None).await.map(|_f| ())
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

	pub async fn reset_permissions(&self) -> crate::process::Result<()> {
		self.parent.exec(vec!["pm", "reset-permissions"], None).await.map(|_f| ())
	}

	pub async fn list_packages(&self, filters: Option<ListPackageFilter>, display: Option<ListPackageDisplayOptions>, name_filter: Option<&str>) -> Result<Vec<Package>, AdbError> {
		let mut args = vec!["pm".into(), "list".into(), "packages".into()];

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

		let output = self.parent.exec(args, None).await?.stdout();
		let string = Arg::as_str(&output)?;

		lazy_static! {
			static ref RE: Regex = Regex::new("package:((?P<file>.*\\.apk)=)?(?P<name>\\S+)(\\s(versionCode|uid):(\\d+))?(\\s(versionCode|uid):(\\d+))?").unwrap();
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

					let version_code = if let Some(vcode) = version_code_str { Some(vcode.parse::<i32>().ok()?) } else { None };

					let uid = if let Some(uid) = uid_str { Some(uid.parse::<i32>().ok()?) } else { None };

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

	async fn op(&self, operation: &str, package_or_component: &str, user: Option<&str>) -> crate::process::Result<()> {
		let mut args = vec!["pm", operation];
		if let Some(u) = user {
			args.extend(vec!["--user", u]);
		}
		args.push(package_or_component);
		self.parent.exec(args, None).await.map(|_f| ())
	}
}
