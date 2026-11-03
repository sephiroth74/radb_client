//! Host helpers: the Android SDK location, and quoting for the device shell.

use std::path::PathBuf;

use crate::adb::Adb;
use crate::error::{Error, Result};

pub use crate::command::shell_quote;

/// The Android SDK directory: `ANDROID_HOME`, then `ANDROID_SDK_ROOT`, then the
/// SDK that contains the `adb` found in `PATH` (`<sdk>/platform-tools/adb`).
pub fn android_home() -> Result<PathBuf> {
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(path) = std::env::var_os(var).map(PathBuf::from) {
            if path.is_dir() {
                return Ok(path);
            }
        }
    }
    Adb::new()
        .ok()
        .and_then(|adb| {
            Some(
                adb.path()
                    .canonicalize()
                    .ok()?
                    .parent()?
                    .parent()?
                    .to_path_buf(),
            )
        })
        .filter(|path| path.join("platform-tools").is_dir())
        .ok_or_else(|| Error::ToolNotFound("Android SDK (set ANDROID_HOME)".to_string()))
}

/// The path of `apkanalyzer` (`<sdk>/cmdline-tools/latest/bin/apkanalyzer`).
pub fn apk_analyzer() -> Result<PathBuf> {
    let path = android_home()?
        .join("cmdline-tools")
        .join("latest")
        .join("bin")
        .join("apkanalyzer");
    if path.exists() {
        Ok(path)
    } else {
        Err(Error::ToolNotFound(path.display().to_string()))
    }
}
