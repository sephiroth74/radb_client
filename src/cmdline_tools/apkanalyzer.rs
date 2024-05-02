use std::fmt::{Display, Formatter};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::Error;

use regex::Regex;
use rustix::path::Arg;
use simple_cmd::Cmd;

use crate::cmdline_tools::{ApkAnalyzer, ApkSummary};

impl Display for ApkSummary {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"package_name: {}, version_code: {}, version_name: {}",
			self.package_name, self.version_code, self.version_name
		)
	}
}

impl ApkAnalyzer {
	pub fn new() -> anyhow::Result<Self> {
		crate::utils::apk_analyzer().map(|path| ApkAnalyzer { path, debug: false })
	}

	pub fn from<P: AsRef<Path>>(path: P, debug: bool) -> Self {
		ApkAnalyzer {
			path: path.as_ref().to_path_buf(),
			debug,
		}
	}

	/// Returns the version name of the given APK file
	pub fn version_name<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<String> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("manifest")
			.arg("version-name")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		output.stdout.as_str().map(|s| s.trim().to_string()).map_err(|e| e.into())
	}

	/// Returns the version code of the given APK file
	pub fn version_code<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<String> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("manifest")
			.arg("version-code")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		output.stdout.as_str().map(|s| s.trim().to_string()).map_err(|e| e.into())
	}

	/// Returns the application id of the given APK file
	pub fn application_id<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<String> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("manifest")
			.arg("application-id")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		output.stdout.as_str().map(|s| s.trim().to_string()).map_err(|e| e.into())
	}

	/// Returns the minimum sdk of the given APK file
	pub fn min_sdk<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<i64> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("manifest")
			.arg("min-sdk")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		output.stdout.as_str()?.trim().parse::<i64>().map_err(|e| e.into())
	}

	/// Returns the target sdk of the given APK file
	pub fn target_sdk<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<i64> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("manifest")
			.arg("target-sdk")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		output.stdout.as_str()?.trim().parse::<i64>().map_err(|e| e.into())
	}

	/// Returns the manifest of the given APK file
	pub fn manifest_code<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<String> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("manifest")
			.arg("print")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		output.stdout.as_str().map(|s| s.trim().to_string()).map_err(|e| e.into())
	}

	/// Returns if the given APK file is debuggable
	pub fn debuggable<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<bool> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("manifest")
			.arg("debuggable")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		output.stdout.as_str().map(|s| s.trim() == "true").map_err(|e| e.into())
	}

	/// Returns the permissions of the given APK file
	pub fn manifest_permissions<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<Vec<String>> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("manifest")
			.arg("permissions")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		let result = output
			.stdout
			.lines()
			.into_iter()
			.map(|l| l.map_err(|e| e.into()))
			.collect::<Result<Vec<_>, anyhow::Error>>()?;
		Ok(result)
	}

	/// Returns the files list of the given APK file
	pub fn files_list<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<Vec<PathBuf>> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("files")
			.arg("list")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		let lines = output.stdout.lines();
		let result = lines
			.into_iter()
			.map(|l| l.map(|s| PathBuf::from(s)).map_err(|e| e.into()))
			.collect::<Result<Vec<_>, anyhow::Error>>()?;
		Ok(result)
	}

	/// Returns the content of the given file in the APK file
	pub fn files_cat<A, P>(&self, apk_path: A, path: P) -> anyhow::Result<String>
	where
		A: AsRef<Path>,
		P: AsRef<Path>,
	{
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("files")
			.arg("cat")
			.arg("--file")
			.arg(path.as_ref())
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		output.stdout.as_str().map(|s| s.trim().to_string()).map_err(|e| e.into())
	}

	/// Returns the summary of the given APK file
	pub fn summary<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<ApkSummary> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("apk")
			.arg("summary")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		let string = output.stdout.as_str().map(|s| s.trim().to_string())?;
		let regexp = Regex::new(r#"[\s\t]+"#).expect("Failed to create regex");
		let split: Vec<_> = regexp.splitn(&string, 3).map(|s| s.to_string()).collect();
		assert_eq!(split.len(), 3, "Failed to split string");
		Ok(ApkSummary {
			package_name: split[0].clone(),
			version_code: split[1].clone(),
			version_name: split[2].clone(),
		})
	}

	/// Returns the dex list of the given APK file
	pub fn dex_list<P: AsRef<Path>>(&self, apk_path: P) -> anyhow::Result<Vec<String>> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("dex")
			.arg("list")
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		let lines = output.stdout.lines();
		let result = lines
			.into_iter()
			.map(|l| l.map(|s| s).map_err(|e| e.into()))
			.collect::<Result<Vec<_>, anyhow::Error>>()?;
		Ok(result)
	}

	/// Returns the smali code of the given class in the APK file
	pub fn dex_code<P: AsRef<Path>>(&self, apk_path: P, class_name: &str) -> Result<String, Error> {
		let output = simple_cmd::Cmd::builder(&self.path)
			.arg("dex")
			.arg("code")
			.arg("-class")
			.arg(class_name)
			.arg(apk_path.as_ref())
			.with_debug(self.debug)
			.build()
			.output()?;
		output.stdout.as_str().map(|s| s.to_string()).map_err(|e| e.into())
	}

	/// Returns the smali code of the given class in the APK file
	pub fn classes<P: AsRef<Path>>(
		&self,
		apk_path: P,
		dex_names: Option<Vec<&str>>,
		defined_only: Option<bool>,
	) -> Result<Vec<String>, Error> {
		let classname_reg = Regex::new(r#"^C\s+d\s[0-9\t]+(.*)"#).unwrap();
		let mut builder = simple_cmd::Cmd::builder(&self.path).arg("dex").arg("packages");

		if let Some(true) = defined_only {
			builder = builder.arg("--defined-only");
		}

		if let Some(dex_names) = dex_names {
			for dex_name in dex_names {
				builder = builder.arg("--files").arg(dex_name);
			}
		}

		let command2 = Cmd::builder("grep").arg(r#"^C d"#).stdout(Some(Stdio::piped())).build();

		let output = builder.arg(apk_path.as_ref()).with_debug(self.debug).build().pipe(command2)?;

		let lines = output.stdout.lines();
		let result = lines
			.into_iter()
			.filter_map(|l| {
				if let Ok(line) = l {
					if let Some(result) = classname_reg.captures(&line) {
						Some(result.get(1).unwrap().as_str().to_string())
					} else {
						None
					}
				} else {
					None
				}
			})
			.collect::<Vec<_>>();
		Ok(result)
	}
}

#[cfg(test)]
pub(crate) mod test {

	use std::path::PathBuf;

	use tracing::trace;

	use crate::test::test::init_log;
	use crate::utils::apk_analyzer;

	use super::*;

	static APK_PATH: &str = "/Users/alessandro/Documents/git/swisscom/aot-lib/standalone/build/outputs/apk/ip2300/release/standalone-ip2300-release.apk";

	#[test]
	fn test_new() {
		init_log();
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		assert!(apkanalyzer.path.exists());
		trace!("apkanalyzer path: {:?}", apkanalyzer.path);

		let apkpath = apk_analyzer().expect("Failed to get apkanalyzer path");
		let apkanalyzer = ApkAnalyzer::from(apkpath, true);
		assert!(apkanalyzer.path.exists());
		trace!("apkanalyzer path: {:?}", apkanalyzer.path);
	}

	#[test]
	fn test_version_name() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let version_name = apkanalyzer.version_name(&apk_path).expect("Failed to get version name");
		assert!(version_name.len() > 0);
		trace!("version name: {}", version_name);
	}

	#[test]
	fn test_version_code() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let version_code = apkanalyzer.version_code(&apk_path).expect("Failed to get version code");
		assert!(version_code.len() > 0);
		trace!("version code: {}", version_code);
	}

	#[test]
	fn test_application_id() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer.application_id(&apk_path).expect("Failed to get application id");
		assert!(result.len() > 0);
		trace!("application-id: {}", result);
	}

	#[test]
	fn test_manifest_print() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer.manifest_code(&apk_path).expect("Failed to get manifest");
		assert!(result.len() > 0);
		trace!("manifest: {}", result);
	}

	#[test]
	fn test_apk_summary() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer.summary(&apk_path).expect("Failed to get apk summary");
		assert!(result.package_name.len() > 0);
		assert!(result.version_name.len() > 0);
		assert!(result.version_code.len() > 0);
		trace!("apk summary: {}", result);
	}

	#[test]
	fn test_file_list() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer.files_list(&apk_path).expect("Failed to get files list");
		assert!(result.len() > 0);
		for line in result {
			trace!("line: {:?}", line);
		}
	}

	#[test]
	fn test_min_sdk() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer.min_sdk(&apk_path).expect("Failed to get min sdk");
		assert!(result > 0);
		trace!("min-sdk: {}", result);
	}

	#[test]
	fn test_target_sdk() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer.target_sdk(&apk_path).expect("Failed to get target sdk");
		assert!(result > 0);
		trace!("target-sdk: {}", result);
	}

	#[test]
	fn test_manifest_debuggable() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer.debuggable(&apk_path).expect("Failed to get manifest debuggable");
		trace!("debuggable: {}", result);
	}

	#[test]
	fn test_manifest_permissions() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer
			.manifest_permissions(&apk_path)
			.expect("Failed to get manifest permissions");
		assert!(result.len() > 0);
		for permission in result {
			trace!("permission: {}", permission);
		}
	}

	#[test]
	fn test_files_cat() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer
			.files_cat(&apk_path, "/assets/config/router-models.json")
			.expect("Failed to get files cat");
		assert!(result.len() > 0);
		trace!("file content: {}", result);
	}

	#[test]
	fn test_dex_list() {
		init_log();
		let apk_path = PathBuf::from(APK_PATH);
		assert!(apk_path.exists(), "apk path not found");
		let apkanalyzer = ApkAnalyzer::new().expect("Failed to create ApkAnalyzer");
		let result = apkanalyzer.dex_list(&apk_path).expect("Failed to get dex list");
		assert!(result.len() > 0, "dex list len invalid");
		for dex in result {
			trace!("dex-classes: {}", dex);
		}
	}
}
