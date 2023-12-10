use std::fmt::{Display, Formatter};

use lazy_static::lazy_static;
use regex::Regex;
use rustix::path::Arg;

use crate::errors::AdbError;
use crate::types::AdbShell;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManager<'a> {
	pub(crate) parent: AdbShell<'a>,
}

pub enum ListPackageFilter {
	// -d: filter to only show disabled packages
	ShowOnlyDisabled,
	// -e: filter to only show enabled packages
	ShowOnlyEnabed,
	// -s: filter to only show system packages
	ShowOnlySystem,
	// -3: filter to only show third party packages
	ShowOnly3rdParty,
	// --apex-only: only show APEX packages
	ApexOnly,
	// --uid UID: filter to only show packages with the given UID
	Uid(String),
	// --user USER_ID: only list packages belonging to the given user
	User(String),
}

pub enum ListPackageDisplayOptions {
	// -U: also show the package UID
	ShowUUID,
	// --show-versioncode: also show the version code
	ShowVersionCode,
	// -u: also include uninstalled packages
	IncludeUninstalled,
	// -f: see their associated file
	ShowApkFile,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Package {
	pub package_name: String,
	pub file_name: Option<String>,
	pub version_code: Option<i32>,
	pub uid: Option<i32>,
}

impl Display for ListPackageDisplayOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			ListPackageDisplayOptions::ShowUUID => write!(f, "-U"),
			ListPackageDisplayOptions::ShowVersionCode => write!(f, "--show-versioncode"),
			ListPackageDisplayOptions::IncludeUninstalled => write!(f, "-u"),
			ListPackageDisplayOptions::ShowApkFile => write!(f, "-f"),
		}
	}
}

impl Display for ListPackageFilter {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			ListPackageFilter::ShowOnlyDisabled => write!(f, "-d"),
			ListPackageFilter::ShowOnlyEnabed => write!(f, "-e"),
			ListPackageFilter::ShowOnlySystem => write!(f, "-s"),
			ListPackageFilter::ShowOnly3rdParty => write!(f, "-3"),
			ListPackageFilter::ApexOnly => write!(f, "--apex-only"),
			ListPackageFilter::Uid(s) => write!(f, "--uid {:}", s),
			ListPackageFilter::User(s) => write!(f, "--user {:}", s),
		}
	}
}

impl<'a> PackageManager<'a> {
	pub async fn path(&self, package_name: &str, user: Option<&str>) -> Result<Option<String>, AdbError> {
		let mut args = vec!["pm", "path"];
		if let Some(u) = user {
			args.push("--user");
			args.push(u);
		}
		args.push(package_name);
		let result = self.parent.exec(args, None).await?.stdout();
		let output = Arg::as_str(&result)?.trim_end();
		let split = output.split_once("package:").map(|s| s.1.to_string());
		Ok(split)
	}

	pub async fn list_packages(&self, filters: Vec<ListPackageFilter>, display: Vec<ListPackageDisplayOptions>, name_filter: Option<&str>) -> Result<Vec<Package>, AdbError> {
		let mut args = vec!["pm".to_string(), "list".to_string(), "packages".to_string()];

		let f = filters.iter().map(|f| f.to_string()).collect::<Vec<_>>();
		args.extend_from_slice(&f);
		args.extend_from_slice(&display.iter().map(|f| f.to_string()).collect::<Vec<_>>());

		if let Some(name) = name_filter {
			args.push(name.to_string());
		}

		let output = self.parent.exec(args, None).await?.stdout();
		let string = Arg::as_str(&output)?;

		lazy_static! {
			static ref RE: Regex = Regex::new("package:(?P<file>.*\\.apk)=(?P<name>\\S+)(\\s(versionCode|uid):(\\d+))?(\\s(versionCode|uid):(\\d+))?").unwrap();
		}

		let captures = RE.captures_iter(string);
		let result = captures
			.into_iter()
			.filter_map(|m| {
				if m.len() == 9 {
					let name = m.name("name")?.as_str();
					let file_name = m.name("file").map(|s| s.as_str().to_string());

					let (version_code_str, uid_str) = match m.get(4).map(|m| m.as_str()) {
						Some("versionCode") => (m.get(5).map(|m| m.as_str()), m.get(8).map(|m| m.as_str())),

						Some("uid") => (m.get(8).map(|m| m.as_str()), m.get(5).map(|m| m.as_str())),

						_ => (None, None),
					};

					let version_code = if let Some(vcode) = version_code_str { Some(vcode.parse::<i32>().ok()?) } else { None };

					let uid = if let Some(uid) = uid_str { Some(uid.parse::<i32>().ok()?) } else { None };

					Some(Package {
						package_name: name.to_string(),
						file_name,
						version_code,
						uid,
					})
				} else {
					None
				}
			})
			.collect::<Vec<_>>();
		Ok(result)
	}
}
