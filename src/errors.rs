use std::fmt::{Debug, Display, Formatter};
use std::net::AddrParseError;
use std::num::ParseIntError;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

use image::ImageError;
use mac_address::MacParseError;
use nom::error::Error;
use rustix::io::Errno;
use string_builder::ToBytes;
use thiserror::Error;
use tokio::time::error::Elapsed;

use crate::util::Vec8ToString;

#[allow(dead_code)]
pub struct CommandError {
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Error, Clone, PartialEq, Eq, Debug)]
pub struct ParseSELinuxTypeError {
    pub msg: Option<String>,
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

#[derive(Error, Debug)]
pub enum AdbError {
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

    #[error("command error")]
    CmdError(#[from] CommandError),

    #[error("errno")]
    Errno(#[from] Errno),

    #[error(transparent)]
    ImageError(#[from] ImageError),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    ClipbardError(#[from] arboard::Error),

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
            stdout: vec![],
            stderr: msg.to_owned().to_bytes(),
        }
    }

    pub fn from_err(status: ExitStatus, stdout: Vec<u8>, stderr: Vec<u8>) -> Self {
        CommandError { status: Some(status), stdout, stderr }
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

impl Debug for CommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:}", self)
    }
}

impl Display for CommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "code:{:?}, stdout:{:?}, stderr:{:?}", self.status, self.stdout.as_str(), self.stderr.as_str())

        //match self.stdout.as_str() {
        //	None => {
        //		match self.stderr.as_str() {
        //			None => {}
        //
        //		}
        //		write!(f, "code: {:?}, msg: unknown error", self.status)
        //	}
        //	Some(s) => {
        //		write!(f, "code: {:?}, msg: {:?}", self.status, s)
        //	}
        //}
    }
}

impl From<&which::Error> for CommandError {
    fn from(value: &which::Error) -> Self {
        CommandError {
            status: None,
            stdout: vec![],
            stderr: value.to_string().to_bytes(),
        }
    }
}

impl From<std::io::Error> for CommandError {
    fn from(value: std::io::Error) -> Self {
        CommandError {
            status: None,
            stdout: vec![],
            stderr: value.to_string().to_bytes(),
        }
    }
}

impl From<ParseIntError> for CommandError {
    fn from(value: ParseIntError) -> Self {
        CommandError {
            status: None,
            stdout: vec![],
            stderr: value.to_string().to_bytes(),
        }
    }
}

impl From<Elapsed> for CommandError {
    fn from(value: Elapsed) -> Self {
        CommandError {
            status: None,
            stdout: vec![],
            stderr: value.to_string().to_bytes(),
        }
    }
}

impl From<nom::Err<nom::error::Error<&[u8]>>> for CommandError {
    fn from(value: nom::Err<nom::error::Error<&[u8]>>) -> Self {
        CommandError {
            status: None,
            stdout: vec![],
            stderr: value.to_string().to_bytes(),
        }
    }
}
