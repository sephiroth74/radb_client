use nom::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::num::ParseIntError;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

use rustix::io::Errno;
use string_builder::ToBytes;
use thiserror::Error;
use tokio::time::error::Elapsed;

use crate::util::Vec8ToString;

#[allow(dead_code)]
pub struct CommandError {
	pub status: Option<ExitStatus>,
	pub msg: Vec<u8>,
}

#[derive(Error, Debug)]
pub enum AdbError {
	#[error(transparent)]
	AdbNotFoundError(#[from] which::Error),

	#[error("invalid device address `{0}`")]
	InvalidDeviceAddressError(String),

	#[error("failed to connect to device")]
	ConnectToDeviceError(),

	#[error("failed to parse properties: {0}")]
	PropertyParseError(String),

	#[error("parse int error")]
	ParseIntError(#[from] ParseIntError),

	#[error("command error")]
	CmdError(#[from] CommandError),

	#[error("errno")]
	Errno(#[from] Errno),

	#[error("data store disconnected")]
	Disconnect(#[from] std::io::Error),
	#[error("the data for key `{0}` is not available")]
	Redaction(String),
	#[error("invalid header (expected {expected:?}, found {found:?})")]
	InvalidHeader { expected: String, found: String },
	#[error("unknown error: {0}")]
	Unknown(String),
}

/// implementation

impl From<nom::Err<nom::error::Error<&[u8]>>> for AdbError {
	fn from(value: nom::Err<Error<&[u8]>>) -> Self {
		AdbError::PropertyParseError(value.to_string())
	}
}

impl std::error::Error for CommandError {}

impl CommandError {
	pub fn from(msg: &str) -> Self {
		CommandError {
			status: None,
			msg: msg.to_owned().to_bytes(),
		}
	}

	pub fn from_err(status: ExitStatus, msg: Vec<u8>) -> Self {
		CommandError { status: Some(status), msg }
	}

	pub fn exit_code(&self) -> Option<i32> {
		match self.status {
			Some(s) => s.code(),
			None => None,
		}
	}

	pub fn exit_signal(&self) -> Option<i32> {
		match self.status {
			None => None,
			Some(s) => s.signal(),
		}
	}
}

impl Display for CommandError {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self.msg.as_str() {
			None => {
				write!(f, "code: {:?}, msg: unknown error", self.status)
			}
			Some(s) => {
				write!(f, "code: {:?}, msg: {:?}", self.status, s)
			}
		}
	}
}

impl Debug for CommandError {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self.msg.as_str() {
			None => {
				write!(f, "code: {:?}, msg: unknown error", self.status)
			}
			Some(s) => {
				write!(f, "code: {:?}, msg: {:?}", self.status, s)
			}
		}
	}
}

impl From<&which::Error> for CommandError {
	fn from(value: &which::Error) -> Self {
		CommandError {
			status: None,
			msg: value.to_string().to_bytes(),
		}
	}
}

impl From<std::io::Error> for CommandError {
	fn from(value: std::io::Error) -> Self {
		CommandError {
			status: None,
			msg: value.to_string().to_bytes(),
		}
	}
}

impl From<ParseIntError> for CommandError {
	fn from(value: ParseIntError) -> Self {
		CommandError {
			status: None,
			msg: value.to_string().to_bytes(),
		}
	}
}

impl From<Elapsed> for CommandError {
	fn from(value: Elapsed) -> Self {
		CommandError {
			status: None,
			msg: value.to_string().to_bytes(),
		}
	}
}

impl From<nom::Err<nom::error::Error<&[u8]>>> for CommandError {
	fn from(value: nom::Err<nom::error::Error<&[u8]>>) -> Self {
		CommandError {
			status: None,
			msg: value.to_string().to_bytes(),
		}
	}
}
