use std::net::AddrParseError;
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
}

impl From<AddrParseError> for Error {
	fn from(_value: AddrParseError) -> Self {
		Error::AddressParseError
	}
}
