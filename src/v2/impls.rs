use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::vec::IntoIter;

use cmd_lib::AsOsStr;

use crate::errors::AdbError;
use crate::types::{InputSource, KeyCode, KeyEventType, MotionEvent};
use crate::v2::traits::AsArgs;
use crate::v2::types::{
	AdbDevice, InstallLocationOption, InstallOptions, InstallPermission, ListPackageDisplayOptions, ListPackageFilter,
	MemoryStatus, Package, PackageFlags, RebootType, Reconnect, RuntimePermission, UninstallOptions, UserOption, Wakefulness,
};

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
		let string: &str = self.into();
		string.into()
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
