// region Wakefulness

use std::fmt::{Display, Formatter};

use crate::v2::types::{AdbDevice, Reconnect, Wakefulness};

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
