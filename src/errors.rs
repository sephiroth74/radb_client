use std::fmt::{Debug, Display, Formatter};
use std::net::AddrParseError;
use std::num::ParseIntError;

use image::ImageError;
use mac_address::MacParseError;
use nom::error::Error;
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
		ParseSELinuxTypeError { msg: Some(value.to_string()) }
	}
}

/// implementation

impl From<nom::Err<nom::error::Error<&[u8]>>> for AdbError {
	fn from(value: nom::Err<Error<&[u8]>>) -> Self {
		AdbError::PropertyParseError(value.to_string())
	}
}
//
//impl From<cmd::Error> for AdbError {
//    fn from(value: cmd::Error) -> Self {
//        AdbError::CmdError(value)
//    }
//}

//impl From<&which::Error> for CommandError {
//	fn from(value: &which::Error) -> Self {
//		CommandError {
//			status: None,
//			stdout: vec![],
//			stderr: value.to_string().to_bytes(),
//		}
//	}
//}
//
//impl From<std::io::Error> for CommandError {
//	fn from(value: std::io::Error) -> Self {
//		CommandError {
//			status: None,
//			stdout: vec![],
//			stderr: value.to_string().to_bytes(),
//		}
//	}
//}
//
//impl From<ParseIntError> for CommandError {
//	fn from(value: ParseIntError) -> Self {
//		CommandError {
//			status: None,
//			stdout: vec![],
//			stderr: value.to_string().to_bytes(),
//		}
//	}
//}
//
//impl From<Elapsed> for CommandError {
//	fn from(value: Elapsed) -> Self {
//		CommandError {
//			status: None,
//			stdout: vec![],
//			stderr: value.to_string().to_bytes(),
//		}
//	}
//}
//
//impl From<nom::Err<nom::error::Error<&[u8]>>> for CommandError {
//	fn from(value: nom::Err<nom::error::Error<&[u8]>>) -> Self {
//		CommandError {
//			status: None,
//			stdout: vec![],
//			stderr: value.to_string().to_bytes(),
//		}
//	}
//}
