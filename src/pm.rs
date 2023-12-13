use std::fmt::{Display, Formatter};

use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};
use rustix::path::Arg;

use crate::command::ProcessResult;
use crate::dump_util::{extract_runtime_permissions, SimplePackageReader};
use crate::errors::AdbError;
use crate::pm::PackageFlags::{AllowBackup, AllowClearUserData, HasCode, System, UpdatedSystemApp};

use crate::types::AdbShell;

#[macro_export]
macro_rules! build_pm_operation {
	($name:tt, $operation_name:tt, $typ:ty, $typ2:ty) => {
		pub async fn $name(&self, package_name: $typ, user: $typ2) -> crate::command::Result<()> {
			self.op($operation_name, package_name, user).await
		}
	};
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManager<'a> {
	pub(crate) parent: AdbShell<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UninstallOptions {
	// -k
	pub keep_data: bool,
	// --user
	pub user: Option<String>,
	// --versionCode
	pub version_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListPackageFilter {
	// -d: filter to only show disabled packages
	pub show_only_disabled: bool,
	// -e: filter to only show enabled packages
	pub show_only_enabed: bool,
	// -s: filter to only show system packages
	pub show_only_system: bool,
	// -3: filter to only show third party packages
	pub show_only3rd_party: bool,
	// --apex-only: only show APEX packages
	pub apex_only: bool,
	// --uid UID: filter to only show packages with the given UID
	pub uid: Option<String>,
	// --user USER_ID: only list packages belonging to the given user
	pub user: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPackageDisplayOptions {
	// -U: also show the package UID
	pub show_uid: bool,
	// --show-versioncode: also show the version code
	pub show_version_code: bool,
	// -u: also include uninstalled packages
	pub include_uninstalled: bool,
	// -f: see their associated file
	pub show_apk_file: bool,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum InstallLocationOption {
	// 0=auto, 1=internal only, 2=prefer external
	Auto,
	InternalOnly,
	PreferExternal,
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct InstallOptions {
	// --user: install under the given user.
	pub user: Option<String>,
	// --dont-kill: installing a new feature split, don't kill running app
	pub dont_kill: bool,
	// --restrict-permissions: don't whitelist restricted permissions at install
	pub restrict_permissions: bool,
	// --pkg: specify expected package name of app being installed
	pub package_name: Option<String>,
	// --install-location: force the install location:
	// 0=auto, 1=internal only, 2=prefer external
	pub install_location: Option<InstallLocationOption>,
	// -g: grant all runtime permissions
	pub grant_permissions: bool,
	// -f: force
	pub force: bool,
	// -r replace existing application
	pub replace_existing_application: bool,
	// -d: allow version code downgrade
	pub allow_version_downgrade: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum PackageFlags {
	System,
	HasCode,
	AllowClearUserData,
	UpdatedSystemApp,
	AllowBackup,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RuntimePermission {
	pub name: String,
	pub granted: bool,
	pub flags: Vec<String>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct InstallPermission {
	pub name: String,
	pub granted: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Package {
	pub package_name: String,
	pub file_name: Option<String>,
	pub version_code: Option<i32>,
	pub uid: Option<i32>,
}

impl TryFrom<&str> for PackageFlags {
	type Error = AdbError;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value {
			"SYSTEM" => Ok(System),
			"HAS_CODE" => Ok(HasCode),
			"ALLOW_CLEAR_USER_DATA" => Ok(AllowClearUserData),
			"UPDATED_SYSTEM_APP" => Ok(UpdatedSystemApp),
			"ALLOW_BACKUP" => Ok(AllowBackup),
			_ => Err(AdbError::NameNotFoundError(value.to_string())),
		}
	}
}

impl Display for InstallLocationOption {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			InstallLocationOption::Auto => write!(f, "0"),
			InstallLocationOption::InternalOnly => write!(f, "1"),
			InstallLocationOption::PreferExternal => write!(f, "2"),
		}
	}
}

impl Default for InstallLocationOption {
	fn default() -> Self {
		InstallLocationOption::Auto
	}
}

impl IntoIterator for InstallOptions {
	type Item = String;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args = vec![];
		match self.user.as_ref() {
			None => {}
			Some(user) => args.push(format!("--user {:}", user)),
		}

		match self.package_name.as_ref() {
			None => {}
			Some(user) => args.push(format!("--pkg {:}", user)),
		}

		match self.install_location.as_ref() {
			None => {}
			Some(s) => args.push(format!("--install-location {:}", s)),
		}

		if self.dont_kill {
			args.push("--dont-kill".to_string());
		}

		if self.restrict_permissions {
			args.push("--restrict-permissions".to_string());
		}

		if self.grant_permissions {
			args.push("-g".to_string());
		}

		if self.force {
			args.push("-f".to_string());
		}

		if self.replace_existing_application {
			args.push("-r".to_string());
		}

		if self.allow_version_downgrade {
			args.push("-d".to_string());
		}

		args.into_iter()
	}
}

impl Display for InstallOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args = self.clone().into_iter().collect::<Vec<_>>();
		write!(f, "{:}", args.join(" "))
	}
}

impl IntoIterator for ListPackageDisplayOptions {
	type Item = String;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args: Vec<String> = vec![];
		if self.show_uid {
			args.push("-U".into());
		}

		if self.show_version_code {
			args.push("--show-versioncode".into());
		}

		if self.include_uninstalled {
			args.push("-u".into());
		}

		if self.show_apk_file {
			args.push("-f".into());
		}
		args.into_iter()
	}
}

impl From<&UninstallOptions> for Vec<String> {
	fn from(value: &UninstallOptions) -> Self {
		let mut args: Vec<String> = vec![];
		if value.keep_data {
			args.push("-k".into());
		}

		match value.user.as_ref() {
			None => {}
			Some(s) => {
				args.push("--user".into());
				args.push(s.into());
			}
		}

		match value.version_code.as_ref() {
			None => {}
			Some(s) => {
				args.push("--versionCode".into());
				args.push(format!("{:}", s));
			}
		}

		args
	}
}

impl Display for UninstallOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args: Vec<String> = From::<&UninstallOptions>::from(self);
		write!(f, "{:}", args.join(" "))
	}
}

impl Display for ListPackageDisplayOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args = self.clone().into_iter().collect::<Vec<_>>();
		write!(f, "{:}", args.join(" "))
	}
}

impl Default for ListPackageDisplayOptions {
	fn default() -> Self {
		ListPackageDisplayOptions {
			show_uid: true,
			show_version_code: true,
			include_uninstalled: false,
			show_apk_file: true,
		}
	}
}

impl IntoIterator for ListPackageFilter {
	type Item = String;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args: Vec<String> = vec![];
		if self.show_only_disabled {
			args.push("-d".into());
		}
		if self.show_only_enabed {
			args.push("-e".into());
		}
		if self.show_only_system {
			args.push("-s".into());
		}
		if self.show_only3rd_party {
			args.push("-3".into());
		}
		if self.apex_only {
			args.push("--apex-only".into());
		}

		match self.uid.as_ref() {
			None => {}
			Some(s) => args.push(format!("--uid {:}", s)),
		}

		match self.user.as_ref() {
			None => {}
			Some(s) => args.push(format!("--user {:}", s)),
		}
		args.into_iter()
	}
}

impl Display for ListPackageFilter {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:}", self.clone().into_iter().collect::<Vec<_>>().join(" "))
	}
}

impl<'a> PackageManager<'a> {
	pub async fn uninstall(&self, package_name: &str, options: Option<UninstallOptions>) -> crate::command::Result<ProcessResult> {
		let mut args = vec!["cmd package uninstall".to_string()];
		match options {
			None => {}
			Some(options) => args.extend::<Vec<String>>((&options).into()),
		}
		args.push(package_name.to_string());
		self.parent.exec(args, None).await
	}

	pub async fn install<T: Arg>(&self, src: T, options: Option<InstallOptions>) -> crate::command::Result<ProcessResult> {
		let mut args = vec!["cmd package install".to_string()];
		match options {
			None => {}
			Some(options) => args.extend(options),
		}
		args.push(src.as_str()?.into());
		self.parent.exec(args, None).await
	}

	pub async fn is_installed(&self, package_name: &str, user: Option<&str>) -> crate::command::Result<bool> {
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

	pub async fn is_system(&self, package_name: &str) -> crate::command::Result<bool> {
		//let result = self.parent.exec(vec![format!("pm dump {: } | egrep '^ { { 1, } }flags = '  | egrep ' { { 1, } }SYSTEM { { 1, } }'", package_name)], None).await?.stdout();
		Ok(self.package_flags(package_name).await?.contains(&PackageFlags::System))
	}

	pub async fn package_flags(&self, package_name: &str) -> crate::command::Result<Vec<PackageFlags>> {
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

	pub async fn requested_permissions(&self, package_name: &str) -> crate::command::Result<Vec<String>> {
		let dump = self.dump(package_name).await?;
		let pr = SimplePackageReader::new(dump.as_str())?;
		pr.requested_permissions().await
	}

	pub async fn install_permissions(&self, package_name: &str) -> crate::command::Result<Vec<InstallPermission>> {
		let dump = self.dump(package_name).await?;
		let pr = SimplePackageReader::new(dump.as_str())?;
		pr.install_permissions().await
	}

	pub async fn dump_runtime_permissions(&self, package_name: &str) -> crate::command::Result<Vec<RuntimePermission>> {
		let dump = self.dump(package_name).await?;
		extract_runtime_permissions(dump.as_str()).await
	}

	pub async fn dump(&self, package_name: &str) -> crate::command::Result<String> {
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

	pub async fn grant(&self, package_name: &str, user: Option<&str>, permission: &str) -> crate::command::Result<()> {
		let mut args = vec!["pm grant"];
		if let Some(u) = user {
			args.extend(vec!["--user", u]);
		}
		args.push(package_name);
		args.push(permission);
		self.parent.exec(args, None).await.map(|_f| ())
	}

	pub async fn revoke(&self, package_name: &str, user: Option<&str>, permission: &str) -> crate::command::Result<()> {
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

	pub async fn reset_permissions(&self) -> crate::command::Result<()> {
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

	async fn op(&self, operation: &str, package_or_component: &str, user: Option<&str>) -> crate::command::Result<()> {
		let mut args = vec!["pm", operation];
		if let Some(u) = user {
			args.extend(vec!["--user", u]);
		}
		args.push(package_or_component);
		self.parent.exec(args, None).await.map(|_f| ())
	}
}
