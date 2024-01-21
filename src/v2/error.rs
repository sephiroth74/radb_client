use std::net::AddrParseError;

use image::ImageError;
use thiserror::Error;

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
