// region Wakefulness

use std::ffi::OsString;
use std::fmt::{Display, Formatter};

use cmd_lib::AsOsStr;

use crate::v2::traits::AsArgs;
use crate::v2::types::{AdbDevice, MemoryStatus, Reconnect, UserOption, Wakefulness};

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
