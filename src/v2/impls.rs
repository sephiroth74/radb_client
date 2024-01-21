// region Wakefulness

use crate::v2::types::Wakefulness;

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
