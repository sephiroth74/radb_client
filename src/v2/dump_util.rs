use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};

use crate::v2::error::Error;
use crate::v2::result::Result;
use crate::v2::types::{PackageFlags, RuntimePermission};

lazy_static! {
	static ref RE_PACKAGES: Regex = Regex::new("(?m)^Packages:\\n").unwrap();
	static ref RE_NEW_EMPTY_LINE: Regex = Regex::new("(?m)^$").unwrap();
	static ref RE_REQUESTED_PERMISSIONS: Regex = Regex::new("(?m)^\\s{3,}requested permissions:\\n((\\s{4,}[\\w\\.]+$)+)").unwrap();
	static ref RE_SINGLE_PERMISSION: Regex = Regex::new("(?m)^\\s{4,}([\\w\\.]+)$").unwrap();
	static ref RE_RUNTIME_PERMISSIONS: Regex = RegexBuilder::new("(?m)^\\s{3,}runtime permissions:\\s+")
		.multi_line(true)
		.build()
		.unwrap();
	static ref RE_SINGLE_RUNTIME_PERMISSION: Regex =
		RegexBuilder::new("^\\s*([^:]+):\\s+granted=(false|true),\\s+flags=\\[\\s*([^\\]]+)\\]$")
			.multi_line(true)
			.build()
			.unwrap();
	static ref RE_INSTALL_PERMISSIONS: Regex =
		Regex::new("(?m)^\\s{3,}install permissions:\n(?P<permissions>(\\s{4,}[^\\:]+:\\s+granted=(true|false)\n)+)").unwrap();
	static ref RE_INSTALL_PERMISSION: Regex =
		Regex::new("(?m)^\\s{4,}(?P<name>[^\\:]+):\\s+granted=(?P<granted>true|false)$").unwrap();
}

pub(crate) fn runtime_permissions(data: &str) -> Result<Vec<RuntimePermission>> {
	if let Some(captures) = RE_RUNTIME_PERMISSIONS.captures(data) {
		let mut result: Vec<RuntimePermission> = vec![];
		if captures.len() == 1 {
			let m = captures.get(0).unwrap();
			let start = m.end();
			let output2 = &data[start..];

			if let Some(m2) = RE_NEW_EMPTY_LINE.find(output2) {
				let output3 = &output2[..m2.end()];
				for (_, [name, granted, flag_str]) in RE_SINGLE_RUNTIME_PERMISSION.captures_iter(output3).map(|c| c.extract()) {
					let flags = flag_str.split("|").map(|f| f.to_string()).collect::<Vec<_>>();
					result.push(RuntimePermission {
						name: name.to_string(),
						granted: granted == "true",
						flags,
					});
				}
			}
			return Ok(result);
		}
	}

	return Err(Error::ParseInputError());
}

pub(crate) fn package_flags(dump: &str) -> Result<Vec<PackageFlags>> {
	lazy_static! {
		static ref RE: Regex = RegexBuilder::new("^\\s*pkgFlags=\\[\\s(.*)\\s]")
			.multi_line(true)
			.build()
			.unwrap();
	}

	if let Some(captures) = RE.captures(dump) {
		if captures.len() == 2 {
			let flags = captures.get(1).unwrap().as_str().split(" ").collect::<Vec<_>>();
			let package_flags = flags
				.iter()
				.filter_map(|line| if let Ok(flag) = (*line).try_into() { Some(flag) } else { None })
				.collect::<Vec<PackageFlags>>();
			Ok(package_flags)
		} else {
			Err(Error::ParseInputError())
		}
	} else {
		Err(Error::ParseInputError())
	}
}
