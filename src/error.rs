//! The error type of the crate. Every command that exits with a non-zero status fails
//! with [`Error::CommandFailed`], with the command line and its output.

use std::fmt::{Display, Formatter};
use std::num::ParseIntError;
use std::process::Output;

use java_properties::PropertiesError;
use mac_address::MacParseError;
use thiserror::Error;

/// Result type used by every fallible function in this crate.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[non_exhaustive]
/// The errors of this crate.
pub enum Error {
    #[error("failed to parse address: {0}")]
    /// An invalid ip address or `ip:port`.
    AddressParseError(String),

    #[error("failed to find adb command in your PATH")]
    /// `adb` is not in `PATH`.
    AdbNotFoundError(#[from] which::Error),

    /// The command ran, but exited with a non-zero status.
    #[error(transparent)]
    CommandFailed(Box<CommandFailure>),

    /// The command could not be spawned or awaited.
    #[error(transparent)]
    CommandError(#[from] simple_cmd::Error),

    #[error(transparent)]
    /// An I/O error on the host.
    IoError(#[from] std::io::Error),

    #[error("invalid connection type for this operation: {0}")]
    /// The operation does not apply to this [`crate::ConnectionType`] (e.g. `disconnect`
    /// of a USB device).
    InvalidConnectionTypeError(String),

    #[cfg(feature = "clipboard")]
    #[error(transparent)]
    /// The screenshot could not be decoded (feature `clipboard`).
    ImageError(#[from] image::ImageError),

    #[cfg(feature = "clipboard")]
    #[error(transparent)]
    /// The host clipboard is not available (feature `clipboard`).
    ClipboardError(#[from] arboard::Error),

    #[error(transparent)]
    /// Not a MAC address.
    MacParseError(#[from] MacParseError),

    #[error(transparent)]
    /// Not a UUID (e.g. the boot id).
    UuidParseError(#[from] uuid::Error),

    /// The command output could not be understood. The message says what was being parsed.
    #[error("failed to parse {0}")]
    ParseError(String),

    #[error(transparent)]
    /// Not a number.
    ParseIntError(#[from] ParseIntError),

    #[error(transparent)]
    /// The output of a command is not valid UTF-8.
    Utf8Error(#[from] std::str::Utf8Error),

    #[error(transparent)]
    /// The output of `settings list` could not be read.
    PropertiesError(#[from] PropertiesError),

    #[error("package not found: {0}")]
    /// The package is not installed.
    PackageNotFoundError(String),

    #[error("name not found: {0}")]
    /// An unknown `dumpsys package` field or package flag.
    NameNotFoundError(String),

    #[error("avbctl not installed")]
    /// `avbctl` is not on the device.
    AvbctlNotInstalled,

    #[error("timed out: {0}")]
    /// The device did not answer in time. The message says what was expected.
    Timeout(String),

    #[error("{0} not found")]
    /// A host tool (Android SDK, `apkanalyzer`) was not found.
    ToolNotFound(String),

    /// Installing or removing a system app failed (the message says why).
    #[error("{0}")]
    SystemAppError(String),

    /// The APK requests privileged permissions of the platform that the privapp-permissions
    /// allowlist does not grant: with `ro.control_privapp_permissions=enforce`, the device
    /// would not boot.
    #[error("{package} requests privileged permissions that are not in the privapp-permissions allowlist: {}", permissions.join(", "))]
    PrivappPermissionsError {
        /// The package.
        package: String,
        /// The privileged permissions that the allowlist does not grant.
        permissions: Vec<String>,
    },
}

/// Details of a command that exited with a non-zero status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandFailure {
    /// The command line, as it was passed to the host.
    pub command: String,
    /// Exit code, `None` if the process was terminated by a signal.
    pub exit_code: Option<i32>,
    /// The standard output, trimmed.
    pub stdout: String,
    /// The standard error, trimmed.
    pub stderr: String,
}

impl CommandFailure {
    pub(crate) fn new(command: String, output: &Output) -> Self {
        CommandFailure {
            command,
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }
    }

    /// The most useful error text: stderr if present, otherwise stdout
    /// (some Android tools, e.g. `am`, report errors on stdout).
    pub fn message(&self) -> &str {
        if self.stderr.is_empty() {
            &self.stdout
        } else {
            &self.stderr
        }
    }
}

impl Display for CommandFailure {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}` failed", self.command)?;
        match self.exit_code {
            Some(code) => write!(f, " with exit code {code}")?,
            None => write!(f, " (terminated by signal)")?,
        }
        let message = self.message();
        if !message.is_empty() {
            write!(f, ": {message}")?;
        }
        Ok(())
    }
}

impl std::error::Error for CommandFailure {}

impl From<CommandFailure> for Error {
    fn from(value: CommandFailure) -> Self {
        Error::CommandFailed(Box::new(value))
    }
}

impl From<std::net::AddrParseError> for Error {
    fn from(value: std::net::AddrParseError) -> Self {
        Error::AddressParseError(value.to_string())
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(value: std::string::FromUtf8Error) -> Self {
        Error::Utf8Error(value.utf8_error())
    }
}

impl From<std::io::ErrorKind> for Error {
    fn from(value: std::io::ErrorKind) -> Self {
        Error::IoError(std::io::Error::from(value))
    }
}

impl Error {
    /// Exit code of a failed command, if this error is [`Error::CommandFailed`].
    pub fn exit_code(&self) -> Option<i32> {
        match self {
            Error::CommandFailed(failure) => failure.exit_code,
            _ => None,
        }
    }
}
