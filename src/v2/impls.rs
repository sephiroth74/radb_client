use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::vec::IntoIter;

use cmd_lib::AsOsStr;
use lazy_static::lazy_static;
use regex::Regex;

use crate::errors::AdbError;
use crate::v2::error::Error;
use crate::v2::traits::{AsArg, AsArgs};
use crate::v2::types::{
	AdbDevice, AdbInstallOptions, Extra, FFPlayOptions, InputSource, InstallLocationOption, InstallOptions, InstallPermission,
	Intent, KeyCode, KeyEventType, ListPackageDisplayOptions, ListPackageFilter, LogcatLevel, LogcatTag, MemoryStatus, MotionEvent,
	Package, PackageFlags, PropType, Property, RebootType, Reconnect, RuntimePermission, SELinuxType, ScreenRecordOptions,
	UninstallOptions, UserOption, Wakefulness,
};

lazy_static! {
	static ref RE_PROP_TYPE_ENUM: Regex = Regex::new("^enum\\s((?:[\\w_]+\\s?)+)$").unwrap();
}

// region Wakefulness

impl TryFrom<&str> for Wakefulness {
	type Error = crate::v2::error::Error;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value.to_lowercase().as_str() {
			"awake" => Ok(Wakefulness::Awake),
			"asleep" => Ok(Wakefulness::Asleep),
			"dreaming" => Ok(Wakefulness::Dreaming),
			_ => Err(std::io::Error::from(std::io::ErrorKind::InvalidInput).into()),
		}
	}
}

// endregion Wakefulness

// region AdbDevice

impl Display for AdbDevice {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{} product:{} model:{}, device:{} addr:{}",
			self.name, self.product, self.model, self.device, self.addr
		)
	}
}

// endregion AdbDevice

// region Reconnect

impl Display for Reconnect {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			Reconnect::Device => write!(f, "device"),
			Reconnect::Offline => write!(f, "offline"),
		}
	}
}

// endregion Reconnect

// region UserOption

impl AsArgs<OsString> for UserOption {
	fn as_args(&self) -> Vec<OsString> {
		match self {
			UserOption::UserId(user_id) => vec![
				"--user".as_os_str(),
				user_id.as_os_str(),
			],
			UserOption::All => vec!["all".as_os_str()],
			UserOption::Current => vec!["current".as_os_str()],
			UserOption::None => vec![],
		}
	}
}

// endregion UserOption

// region UninstallOptions

impl IntoIterator for UninstallOptions {
	type Item = OsString;
	type IntoIter = IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		self.as_args().into_iter()
	}
}

impl AsArgs<OsString> for UninstallOptions {
	fn as_args(&self) -> Vec<OsString> {
		let mut args: Vec<OsString> = vec![];
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
				args.push(format!("{:}", s).into());
			}
		}

		args
	}
}

// endregion UninstallOptions

// region MemoryStatus

impl Display for MemoryStatus {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			MemoryStatus::Hidden => write!(f, "HIDDEN"),
			MemoryStatus::RunningModerate => write!(f, "RUNNING_MODERATE"),
			MemoryStatus::Background => write!(f, "BACKGROUND"),
			MemoryStatus::RunningLow => write!(f, "RUNNING_LOW"),
			MemoryStatus::Moderate => write!(f, "MODERATE"),
			MemoryStatus::RunningCritical => write!(f, "RUNNING_CRITICAL"),
			MemoryStatus::Complete => write!(f, "COMPLETE"),
		}
	}
}

// endregion MemoryStatus

// region InputSource

impl Into<OsString> for InputSource {
	fn into(self) -> OsString {
		let string: &str = self.into();
		string.into()
	}
}

// endregion InputSource

// region MotionEvent

impl Into<OsString> for MotionEvent {
	fn into(self) -> OsString {
		let string: &str = self.into();
		string.into()
	}
}

// endregion MotionEvent

// region KeyEventType

impl Into<OsString> for KeyEventType {
	fn into(self) -> OsString {
		self.as_arg()
	}
}

impl AsArg<OsString> for KeyEventType {
	fn as_arg(&self) -> OsString {
		match self {
			KeyEventType::LongPress => "--longpress".into(),
			KeyEventType::DoubleTap => "--doubletap".into(),
		}
	}
}

// endregion KeyEventType

// region KeyCode

impl Into<OsString> for KeyCode {
	fn into(self) -> OsString {
		let string: &str = self.into();
		string.into()
	}
}

// endregion KeyCode

// region Package

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

// endregion Package

// region RuntimePermission

impl Display for RuntimePermission {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{} granted={} flags={}", self.name, self.granted, self.flags.join(","))
	}
}

// endregion RuntimePermission

// region InstallPermission

impl Display for InstallPermission {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{} granted={}", self.name, self.granted)
	}
}

// endregion InstallPermission

// region PackageFlags

impl TryFrom<&str> for PackageFlags {
	type Error = AdbError;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value {
			"SYSTEM" => Ok(PackageFlags::System),
			"HAS_CODE" => Ok(PackageFlags::HasCode),
			"ALLOW_CLEAR_USER_DATA" => Ok(PackageFlags::AllowClearUserData),
			"UPDATED_SYSTEM_APP" => Ok(PackageFlags::UpdatedSystemApp),
			"ALLOW_BACKUP" => Ok(PackageFlags::AllowBackup),
			_ => Err(AdbError::NameNotFoundError(value.to_string())),
		}
	}
}

// endregion PackageFlags

// region InstallOptions

impl IntoIterator for InstallOptions {
	type Item = OsString;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args = vec![];
		match self.user.as_ref() {
			None => {}
			Some(user) => args.push(format!("--user {:}", user).into()),
		}

		match self.package_name.as_ref() {
			None => {}
			Some(user) => args.push(format!("--pkg {:}", user).into()),
		}

		match self.install_location.as_ref() {
			None => {}
			Some(s) => args.push(format!("--install-location {:}", s).into()),
		}

		if self.dont_kill {
			args.push("--dont-kill".into());
		}

		if self.restrict_permissions {
			args.push("--restrict-permissions".into());
		}

		if self.grant_permissions {
			args.push("-g".into());
		}

		if self.force {
			args.push("-f".into());
		}

		if self.replace_existing_application {
			args.push("-r".into());
		}

		if self.allow_version_downgrade {
			args.push("-d".into());
		}

		args.into_iter()
	}
}

impl Display for InstallOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args = self.clone().into_iter().collect::<Vec<_>>();
		write!(f, "{}", args.iter().filter_map(|s| s.to_str()).collect::<Vec<_>>().join(" "))
	}
}

// endregion InstallOptions

// region InstallLocationOption

impl Default for InstallLocationOption {
	fn default() -> Self {
		InstallLocationOption::Auto
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

// endregion InstallLocationOption

// region ListPackageDisplayOptions
impl IntoIterator for ListPackageDisplayOptions {
	type Item = OsString;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args: Vec<OsString> = vec![];
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

impl Display for ListPackageDisplayOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args = self.clone().into_iter().collect::<Vec<_>>();
		write!(f, "{}", args.iter().filter_map(|s| s.to_str()).collect::<Vec<_>>().join(" "))
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

// endregion ListPackageDisplayOptions

// region ListPackageFilter

impl IntoIterator for ListPackageFilter {
	type Item = OsString;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args: Vec<OsString> = vec![];
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
			Some(s) => args.push(format!("--uid {:}", s).into()),
		}

		match self.user.as_ref() {
			None => {}
			Some(s) => args.push(format!("--user {:}", s).into()),
		}
		args.into_iter()
	}
}

impl Display for ListPackageFilter {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args = self.clone().into_iter().collect::<Vec<_>>();
		write!(f, "{}", args.iter().filter_map(|s| s.to_str()).collect::<Vec<_>>().join(" "))
	}
}

// endregion ListPackageFilter

// region RebootType

impl Display for RebootType {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			RebootType::Bootloader => write!(f, "bootloader"),
			RebootType::Recovery => write!(f, "recovery"),
			RebootType::Sideload => write!(f, "sideload"),
			RebootType::SideloadAutoReboot => write!(f, "sideload-auto-reboot"),
		}
	}
}

// endregion RebootType

// region LogcatLevel

impl Display for LogcatLevel {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			LogcatLevel::Verbose => write!(f, "V"),
			LogcatLevel::Debug => write!(f, "D"),
			LogcatLevel::Info => write!(f, "I"),
			LogcatLevel::Warn => write!(f, "W"),
			LogcatLevel::Error => write!(f, "E"),
		}
	}
}

// endregion LogcatLevel

// region LogcatTag

impl Display for LogcatTag {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}:{}", self.name, self.level)
	}
}

// endregion LogcatTag

// region FFPlayOptions

impl Default for FFPlayOptions {
	fn default() -> Self {
		FFPlayOptions {
			framerate: Some(30),
			size: Some((1440, 800)),
			probesize: Some(300),
		}
	}
}

impl IntoIterator for FFPlayOptions {
	type Item = OsString;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args = vec![];
		if let Some(framerate) = self.framerate {
			args.push("-framerate".into());
			args.push(framerate.to_string().into());
		}

		if let Some(probesize) = self.probesize {
			args.push("-probesize".into());
			args.push(probesize.to_string().into());
		}

		if let Some(size) = self.size {
			args.push("-vf".into());
			args.push(format!("scale={:}:{:}", size.0, size.1).into());
		}
		args.into_iter()
	}
}

// endregion FFPlayOptions

// region Property

impl Display for Property {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{} = {}", self.key, self.value)
	}
}

// endregion Property

// region PropType

impl TryFrom<Vec<u8>> for PropType {
	type Error = Error;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		Ok(PropType::from(rustix::path::Arg::as_str(&value)?.trim()))
	}
}

impl From<&str> for PropType {
	fn from(value: &str) -> Self {
		return match value {
			"string" => PropType::String,
			"bool" => PropType::Bool,
			"int" => PropType::Int,
			_ => {
				if let Some(captures) = RE_PROP_TYPE_ENUM.captures(value) {
					return if captures.len() == 2 {
						let strings = captures.get(1).unwrap().as_str();
						let s = strings.split(' ').map(|s| s.to_string()).collect::<Vec<String>>();
						PropType::Enum(s)
					} else {
						PropType::Unknown(value.to_string())
					};
				}
				return PropType::Unknown(value.to_string());
			}
		};
	}
}

impl ToString for PropType {
	fn to_string(&self) -> String {
		match self {
			PropType::String => "String".to_string(),
			PropType::Bool => "Bool".to_string(),
			PropType::Int => "Int".to_string(),
			PropType::Enum(_) => "Enum".to_string(),
			PropType::Unknown(_) => "Unknown".to_string(),
		}
	}
}

// endregion PropType

// region ScreenRecordOptions

impl Default for ScreenRecordOptions {
	fn default() -> Self {
		Self::new()
	}
}

impl Display for ScreenRecordOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args = self.clone().into_iter().collect::<Vec<_>>();
		write!(f, "{}", args.iter().filter_map(|s| s.to_str()).collect::<Vec<_>>().join(" "))
	}
}

impl IntoIterator for ScreenRecordOptions {
	type Item = OsString;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args: Vec<OsString> = vec![];
		if let Some(bitrate) = self.bitrate {
			args.push("--bit-rate".into());
			args.push(format!("{:}", bitrate).into());
		}

		if let Some(timelimit) = self.timelimit {
			args.push("--time-limit".into());
			args.push(format!("{:}", timelimit.as_secs()).into());
		}

		if self.rotate.unwrap_or(false) {
			args.push("--rotate".into())
		}

		if self.bug_report.unwrap_or(false) {
			args.push("--bugreport".into())
		}

		if self.verbose {
			args.push("--verbose".into())
		}

		if let Some(size) = self.size {
			args.push("--size".into());
			args.push(format!("{:}x{:}", size.0, size.1).into());
		}
		args.into_iter()
	}
}

impl ScreenRecordOptions {
	pub fn new() -> Self {
		ScreenRecordOptions {
			bitrate: Some(4_000_000),
			timelimit: Some(core::time::Duration::from_secs(10)),
			rotate: None,
			bug_report: None,
			size: None,
			verbose: false,
		}
	}
}

// endregion ScreenRecordOptions

// region SELinuxType

impl Display for SELinuxType {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			SELinuxType::Enforcing => write!(f, "Enforcing"),
			SELinuxType::Permissive => write!(f, "Permissive"),
		}
	}
}

impl TryFrom<Vec<u8>> for SELinuxType {
	type Error = Error;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		let opt_string = rustix::path::Arg::as_str(&value)?;
		opt_string.try_into()
	}
}

impl TryFrom<&str> for SELinuxType {
	type Error = Error;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value.trim() {
			"Enforcing" => Ok(SELinuxType::Enforcing),
			"Permissive" => Ok(SELinuxType::Permissive),
			_ => Err(Error::ParseInputError),
		}
	}
}

impl AsArg<OsString> for SELinuxType {
	fn as_arg(&self) -> OsString {
		match self {
			SELinuxType::Enforcing => "1".into(),
			SELinuxType::Permissive => "0".into(),
		}
	}
}

// endregion SELinuxType

// region Intent

impl Intent {
	pub fn new() -> Intent {
		Intent::default()
	}
	pub fn from_action(action: &str) -> Intent {
		let mut intent = Intent::new();
		intent.action = Some(action.to_string());
		intent
	}
}

impl Display for Intent {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let mut args: Vec<String> = vec![];

		if let Some(action) = self.action.as_ref() {
			args.push(format!("-a {:}", action));
		}

		if let Some(data) = self.data.as_ref() {
			args.push(format!("-d {:}", data));
		}

		if let Some(mime_type) = self.mime_type.as_ref() {
			args.push(format!("-t {:}", mime_type));
		}

		if let Some(category) = self.category.as_ref() {
			args.push(format!("-c {:}", category));
		}

		if let Some(component) = self.component.as_ref() {
			args.push(format!("-n {:}", component));
		}

		if let Some(package) = self.package.as_ref() {
			args.push(format!("-p {:}", package));
		}

		if let Some(user_id) = self.user_id.as_ref() {
			args.push(format!("--user {:}", user_id));
		}

		if self.receiver_foreground {
			args.push("--receiver-foreground".to_string());
		}

		if self.wait {
			args.push("-W".to_string());
		}

		args.push(format!("{:}", self.extra));

		write!(f, "{:}", args.join(" "))
	}
}

// endregion Intent

// region Extra

impl Extra {
	pub fn put_string_extra(&mut self, name: &str, value: &str) -> &mut Self {
		self.es.insert(name.to_string(), value.to_string());
		self
	}

	pub fn put_bool_extra(&mut self, name: &str, value: bool) -> &mut Self {
		self.ez.insert(name.to_string(), value);
		self
	}

	pub fn put_int_extra(&mut self, name: &str, value: i32) -> &mut Self {
		self.ei.insert(name.to_string(), value);
		self
	}

	pub fn put_long_extra(&mut self, name: &str, value: i64) -> &mut Self {
		self.el.insert(name.to_string(), value);
		self
	}

	pub fn put_string_array_extra(&mut self, name: &str, value: Vec<String>) -> &mut Self {
		self.esa.insert(name.to_string(), value);
		self
	}
}

impl Display for Extra {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let mut output: Vec<String> = Vec::new();

		if !self.es.is_empty() {
			self.es.iter().for_each(|entry| {
				output.push(format!("--es {:} {:}", entry.0, entry.1));
			});
		}

		if !self.ez.is_empty() {
			self.ez.iter().for_each(|entry| {
				output.push(format!("--ez {:} {:}", entry.0, entry.1));
			});
		}

		if !self.ei.is_empty() {
			self.ei.iter().for_each(|entry| {
				output.push(format!("--ei {:} {:}", entry.0, entry.1));
			});
		}

		if !self.el.is_empty() {
			self.el.iter().for_each(|entry| {
				output.push(format!("--el {:} {:}", entry.0, entry.1));
			});
		}

		if !self.ef.is_empty() {
			self.ef.iter().for_each(|entry| {
				output.push(format!("--ef {:} {:}", entry.0, entry.1));
			});
		}

		if !self.eu.is_empty() {
			self.eu.iter().for_each(|entry| {
				output.push(format!("--eu {:} {:}", entry.0, entry.1));
			});
		}

		if !self.ecn.is_empty() {
			self.ecn.iter().for_each(|entry| {
				output.push(format!("--ecn {:} {:}", entry.0, entry.1));
			});
		}

		if !self.eia.is_empty() {
			self.eia.iter().for_each(|entry| {
				output.push(format!(
					"--eia {:} {:}",
					entry.0,
					entry.1.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")
				));
			});
		}

		if !self.ela.is_empty() {
			self.ela.iter().for_each(|entry| {
				output.push(format!(
					"--ela {:} {:}",
					entry.0,
					entry.1.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")
				));
			});
		}

		if !self.efa.is_empty() {
			self.efa.iter().for_each(|entry| {
				output.push(format!(
					"--efa {:} {:}",
					entry.0,
					entry.1.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")
				));
			});
		}

		if !self.esa.is_empty() {
			self.esa.iter().for_each(|entry| {
				output.push(format!("--efa {:} {:}", entry.0, entry.1.join(",")));
			});
		}

		if self.grant_read_uri_permission {
			output.push("--grant-read-uri-permission".to_string());
		}

		if self.grant_write_uri_permission {
			output.push("--grant-write-uri-permission".to_string());
		}

		if self.exclude_stopped_packages {
			output.push("--exclude-stopped-packages".to_string());
		}

		if self.include_stopped_packages {
			output.push("--include-stopped-packages".to_string());
		}
		write!(f, "{:}", output.join(" "))
	}
}

// endregion Extra

// region AdbInstallOptions

impl IntoIterator for AdbInstallOptions {
	type Item = OsString;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args = vec![];

		if self.allow_version_downgrade {
			args.push("-d".into());
		}

		if self.allow_test_package {
			args.push("-t".into());
		}

		if self.replace {
			args.push("-r".into());
		}

		if self.forward_lock {
			args.push("-l".into());
		}

		if self.install_external {
			args.push("-s".into());
		}

		if self.grant_permissions {
			args.push("-g".into());
		}

		if self.instant {
			args.push("--instant".into());
		}

		args.into_iter()
	}
}

impl Display for AdbInstallOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args = self.clone().into_iter().collect::<Vec<_>>();
		write!(f, "{}", args.iter().filter_map(|s| s.to_str()).collect::<Vec<_>>().join(" "))
	}
}

// endregion AdbInstallOptions
