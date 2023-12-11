use std::fmt::{Display, Formatter};

use lazy_static::lazy_static;
use nom::Slice;
use regex::{Regex, RegexBuilder};
use rustix::path::Arg;

use crate::command::ProcessResult;
use crate::errors::AdbError;
use crate::pm::PackageFlags::{AllowBackup, AllowClearUserData, HasCode, System, UpdatedSystemApp};
use crate::traits::AsArgs;
use crate::types::AdbShell;

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
pub struct PackageRuntimePermission {
	pub name: String,
	pub granted: bool,
	pub flags: Vec<String>,
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

impl AsArgs for InstallOptions {
	fn as_args(&self) -> Vec<String> {
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

		args
	}
}

impl Display for InstallOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:}", self.as_args().join(" "))
	}
}

impl AsArgs for ListPackageDisplayOptions {
	fn as_args(&self) -> Vec<String> {
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
		args
	}
}

impl AsArgs for UninstallOptions {
	fn as_args(&self) -> Vec<String> {
		let mut args: Vec<String> = vec![];
		if self.keep_data {
			args.push("-k".into());
		}

		match self.user.as_ref() {
			None => {}
			Some(s) => {
				args.push("--user".into());
				args.push(s.into());
			}
		}

		match self.version_code.as_ref() {
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
		write!(f, "{:}", self.as_args().join(" "))
	}
}

impl Display for ListPackageDisplayOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:}", self.as_args().join(" "))
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

impl AsArgs for ListPackageFilter {
	fn as_args(&self) -> Vec<String> {
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

		args
	}
}

impl Display for ListPackageFilter {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:}", self.as_args().join(" "))
	}
}

impl<'a> PackageManager<'a> {
	pub async fn uninstall<T: Arg>(&self, package_name: T, options: Option<UninstallOptions>) -> crate::command::Result<ProcessResult> {
		let mut args = vec!["cmd package uninstall".to_string()];
		match options {
			None => {}
			Some(options) => args.extend(options.as_args()),
		}
		args.push(package_name.as_str()?.into());
		self.parent.exec(args, None).await
	}

	pub async fn install<T: Arg>(&self, src: T, options: Option<InstallOptions>) -> crate::command::Result<ProcessResult> {
		let mut args = vec!["cmd package install".to_string()];
		match options {
			None => {}
			Some(options) => args.extend(options.as_args()),
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
		Ok(self.dump_package_flags(package_name).await?.contains(&PackageFlags::System))
	}

	pub async fn dump_package_flags(&self, package_name: &str) -> crate::command::Result<Vec<PackageFlags>> {
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

	pub async fn dump_runtime_permissions(&self, package_name: &str) -> crate::command::Result<Vec<PackageRuntimePermission>> {
		lazy_static! {
			static ref RE1: Regex = RegexBuilder::new("(?m)^\\s{3,}runtime permissions:\\s+").multi_line(true).build().unwrap();
			static ref RE2: Regex = RegexBuilder::new("(?m)^$").multi_line(true).build().unwrap();
			static ref RE3: Regex = RegexBuilder::new("^\\s*([^:]+):\\s+granted=(false|true),\\s+flags=\\[\\s*([^\\]]+)\\]$")
				.multi_line(true)
				.build()
				.unwrap();
		}

		let dump = self.dump(package_name).await?;
		let output = dump.as_str();

		let mut result: Vec<PackageRuntimePermission> = vec![];

		if let Some(captures) = RE1.captures(output) {
			if captures.len() == 1 {
				let m = captures.get(0).unwrap();
				let start = m.end();
				let output2 = output.slice(start..);

				if let Some(m2) = RE2.find(output2) {
					let output3 = output2.slice(..m2.end());

					result = RE3
						.captures_iter(output3)
						.filter_map(|m3| {
							if m3.len() == 4 {
								let name = m3.get(1).unwrap().as_str().to_string();
								let granted = m3.get(2).unwrap().as_str() == "true";
								let flags = m3.get(3).unwrap().as_str().split("|").map(|f| f.to_string()).collect::<Vec<_>>();

								Some(PackageRuntimePermission { name, granted, flags })
							} else {
								None
							}
						})
						.collect::<Vec<_>>();
				} else {
				}
			}
		}

		Ok(result)
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

	pub async fn list_packages(&self, filters: Option<ListPackageFilter>, display: Option<ListPackageDisplayOptions>, name_filter: Option<&str>) -> Result<Vec<Package>, AdbError> {
		let mut args = vec!["pm".into(), "list".into(), "packages".into()];

		match filters {
			Some(filters) => args.extend(filters.as_args()),
			None => {}
		}

		match display {
			None => args.extend(ListPackageDisplayOptions::default().as_args()),
			Some(d) => args.extend(d.as_args()),
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
}
