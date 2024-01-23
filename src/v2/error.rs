use std::io::ErrorKind;
use std::net::AddrParseError;
use std::num::ParseIntError;
use std::process::Output;

use image::ImageError;
use java_properties::PropertiesError;
use mac_address::MacParseError;
use thiserror::Error;

use crate::errors::ParseSELinuxTypeError;

#[derive(Error, Debug)]
pub enum Error {
	// parse device address error
	#[error("Failed to parse address")]
	AddressParseError,

	#[error(transparent)]
	WhichError(#[from] which::Error),

	#[error(transparent)]
	CommandError(#[from] simple_cmd::Error),

	#[error(transparent)]
	IoError(#[from] std::io::Error),

	#[error("Invalid connection type")]
	InvalidConnectionTypeError,

	#[error(transparent)]
	ImageError(#[from] ImageError),

	#[error(transparent)]
	ClipboardError(#[from] arboard::Error),

	#[error(transparent)]
	MacParseError(#[from] MacParseError),

	#[error(transparent)]
	UuidParseError(#[from] uuid::Error),

	#[error(transparent)]
	ParseSELinuxTypeError(#[from] ParseSELinuxTypeError),

	#[error("failed to parse input")]
	ParseInputError,

	#[error(transparent)]
	ParseIntError(#[from] ParseIntError),

	#[error(transparent)]
	PropertiesError(#[from] PropertiesError),

	#[error("package not found {0}")]
	PackageNotFoundError(String),

	#[error("name not found {0}")]
	NameNotFoundError(String),
}

impl From<AddrParseError> for Error {
	fn from(_value: AddrParseError) -> Self {
		Error::AddressParseError
	}
}

impl From<rustix::io::Errno> for Error {
	fn from(value: rustix::io::Errno) -> Self {
		Error::IoError(std::io::Error::from(value))
	}
}

impl From<Output> for Error {
	fn from(value: Output) -> Self {
		Error::CommandError(simple_cmd::Error::from(value))
	}
}

impl From<std::io::ErrorKind> for Error {
	fn from(value: ErrorKind) -> Self {
		Error::IoError(std::io::Error::from(value))
	}
}
