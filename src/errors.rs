use std::fmt::{Debug, Display, Formatter};
use std::net::AddrParseError;
use std::num::ParseIntError;
use std::process::Output;

use image::ImageError;
use java_properties::PropertiesError;
use mac_address::MacParseError;
use rustix::io::Errno;
use thiserror::Error;

#[derive(Error, Clone, PartialEq, Eq, Debug)]
pub struct ParseSELinuxTypeError {
	pub msg: Option<String>,
}

#[derive(Error, Debug)]
pub enum AdbError {
	#[error(transparent)]
	RegExpError(#[from] regex::Error),

	#[error(transparent)]
	WhichError(#[from] which::Error),

	#[error("invalid device address `{0}`")]
	InvalidDeviceAddressError(String),

	#[error("failed to connect to device")]
	ConnectToDeviceError(),

	#[error("name not found: `{0}`")]
	NameNotFoundError(String),

	#[error("failed to parse properties: {0}")]
	PropertyParseError(String),

	#[error(transparent)]
	PropertiesError(#[from] PropertiesError),

	#[error("parse int error")]
	ParseIntError(#[from] ParseIntError),

	#[error(transparent)]
	CmdError(#[from] simple_cmd::Error),

	#[error("errno")]
	Errno(#[from] Errno),

	#[error(transparent)]
	ImageError(#[from] ImageError),

	#[error(transparent)]
	IoError(#[from] std::io::Error),

	#[error(transparent)]
	ClipboardError(#[from] arboard::Error),

	#[error(transparent)]
	UuidError(#[from] uuid::Error),

	#[error(transparent)]
	ParseSELinuxTypeError(#[from] ParseSELinuxTypeError),

	#[error(transparent)]
	MacParseError(#[from] MacParseError),

	#[error(transparent)]
	AddrParseError(#[from] AddrParseError),

	#[error("invalid device: {0}")]
	InvalidDeviceError(String),

	#[error("failed to parse input")]
	ParseInputError(),

	#[error("parse error: {0}")]
	ParseError(String),

	#[error("Package {0} not found or not installed")]
	PackageNotFoundError(String),

	#[error("unknown error: {0}")]
	Unknown(String),
}

impl Display for ParseSELinuxTypeError {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self.msg.as_ref() {
			Some(m) => write!(f, "{:}", m),
			None => write!(f, "ParseSELinuxTypeError"),
		}
	}
}

impl From<Errno> for ParseSELinuxTypeError {
	fn from(value: Errno) -> Self {
		ParseSELinuxTypeError {
			msg: Some(value.to_string()),
		}
	}
}

/// implementation

impl From<Output> for AdbError {
	fn from(value: Output) -> Self {
		AdbError::CmdError(value.into())
	}
}
