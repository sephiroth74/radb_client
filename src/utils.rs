use std::path::PathBuf;

use anyhow::anyhow;

use crate::types::Adb;

/// Attempt to find the ANDROID_HOME environment variable, if it is not found, attempt to find it using the adb command location
pub fn android_home() -> anyhow::Result<PathBuf> {
	match std::env::var("ANDROID_HOME") {
		Ok(path) => {
			let pathbuf = PathBuf::from(path);
			if pathbuf.exists() {
				return Ok(pathbuf);
			} else {
				match Adb::new() {
					Ok(adb) => match adb.0.parent() {
						Some(x) => {
							let pathbuf = x.to_path_buf();
							if pathbuf.exists() {
								Ok(pathbuf)
							} else {
								Err(anyhow!("ANDROID_HOME not set or invalid"))
							}
						}
						None => Err(anyhow!("ANDROID_HOME not set or invalid")),
					},
					Err(err) => Err(anyhow::Error::from(err)),
				}
			}
		}
		Err(err) => Err(anyhow::Error::from(err)),
	}
}

/// Attempt to find the apkanalyzer command path
pub fn apk_analyzer() -> anyhow::Result<PathBuf> {
	let path = android_home()?
		.join("cmdline-tools")
		.join("latest")
		.join("bin")
		.join("apkanalyzer");
	if path.exists() {
		Ok(path)
	} else {
		Err(anyhow::Error::msg("apkanalyzer not found"))
	}
}
