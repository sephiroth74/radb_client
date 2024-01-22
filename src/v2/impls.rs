// region Wakefulness

use std::ffi::OsString;
use std::fmt::{Display, Formatter};

use cmd_lib::AsOsStr;

use crate::errors::AdbError;
use crate::types::{InputSource, KeyCode, KeyEventType, MotionEvent};
use crate::v2::traits::AsArgs;
use crate::v2::types::{AdbDevice, MemoryStatus, Package, PackageFlags, Reconnect, RuntimePermission, UserOption, Wakefulness};

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
