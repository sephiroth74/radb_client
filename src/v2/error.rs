use std::net::AddrParseError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	// parse device address error
	#[error("Failed to parse address")]
	AddressParseError,

	#[error(transparent)]
	WhichError(#[from] which::Error),
}

impl From<AddrParseError> for Error {
	fn from(_value: AddrParseError) -> Self {
		Error::AddressParseError
	}
}
