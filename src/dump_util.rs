use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};

use crate::errors::AdbError;
use crate::types::{InstallPermission, PackageFlags, RuntimePermission};

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

pub struct SimplePackageReader<'a> {
	data: &'a str,
}

#[allow(dead_code)]
impl<'a> SimplePackageReader<'a> {
	pub fn new(data: &'a str) -> crate::Result<SimplePackageReader<'a>> {
		if let Some(m) = RE_PACKAGES.captures(data) {
			if m.len() == 1 {
				let mut new_data = &data[m.get(0).unwrap().end()..];
				if let Some(m) = RE_NEW_EMPTY_LINE.captures(new_data) {
					if m.len() == 1 {
						new_data = &new_data[..m.get(0).unwrap().start()];
						return Ok(SimplePackageReader { data: new_data });
					}
				}
			}
		}
		return Err(AdbError::ParseInputError());
	}

	pub fn requested_permissions(&self) -> Option<Vec<String>> {
		if let Some(m) = RE_REQUESTED_PERMISSIONS.captures(self.data) {
			if m.len() > 0 {
				let new_data = &self.data[m.get(0).unwrap().range()];
				let mut result = vec![];
				for (_, [name]) in RE_SINGLE_PERMISSION.captures_iter(new_data).map(|c| c.extract()) {
					result.push(name.to_string())
				}
				return Some(result);
			}
		}
		None
	}

	pub fn install_permissions(&self) -> Option<Vec<InstallPermission>> {
		if let Some(m) = RE_INSTALL_PERMISSIONS.captures(self.data) {
			if m.len() > 0 {
				let mut result = vec![];
				let new_data = &self.data[m.get(0).unwrap().range()];
				for (_, [name, granted]) in RE_INSTALL_PERMISSION.captures_iter(new_data).map(|c| c.extract()) {
					result.push(InstallPermission {
						name: name.to_string(),
						granted: granted == "true",
					})
				}
				return Some(result);
			}
		}
		return None;
	}

	pub fn get_version_name(&self) -> Option<&str> {
		self.get_item("versionName").ok()
	}

	pub fn get_first_install_time(&self) -> Option<&str> {
		self.get_item("firstInstallTime").ok()
	}

	pub fn get_last_update_time(&self) -> Option<&str> {
		self.get_item("lastUpdateTime").ok()
	}

	pub fn get_timestamp(&self) -> Option<&str> {
		self.get_item("timeStamp").ok()
	}

	pub fn get_data_dir(&self) -> Option<&str> {
		self.get_item("dataDir").ok()
	}

	pub fn get_user_id(&self) -> Option<&str> {
		self.get_item("userId").ok()
	}

	pub fn get_code_path(&self) -> Option<&str> {
		self.get_item("codePath").ok()
	}

	pub fn get_resource_path(&self) -> Option<&str> {
		self.get_item("resourcePath").ok()
	}

	pub fn get_version_code(&self) -> Option<i32> {
		if let Ok(string) = self.get_item("versionCode") {
			let re = Regex::new("^(?P<versionCode>\\d+)").unwrap();
			if let Some(m) = re.captures(string) {
				if m.len() == 2 {
					return m.get(1).unwrap().as_str().parse::<i32>().ok();
				}
			}
		}
		return None;
	}

	pub fn get_package_flags(&self) -> Option<Vec<PackageFlags>> {
		package_flags(&self.data).ok()
	}

	pub fn is_system(&self) -> Option<bool> {
		is_system(self.data).ok()
	}

	pub fn get_item(&self, name: &str) -> crate::Result<&str> {
		let re = Regex::new(format!("(?m)^\\s{{3,}}{:}=(.*)$", name).as_str()).unwrap();

		match self.parse(re) {
			Ok(result) => Ok(result),
			Err(_) => Err(AdbError::NameNotFoundError(name.to_string())),
		}
	}

	#[inline]
	fn parse(&self, regex: Regex) -> crate::Result<&str> {
		if let Some(m) = regex.captures(self.data) {
			if m.len() == 2 {
				return Ok(m.get(1).unwrap().as_str());
			}
		}
		return Err(AdbError::ParseInputError());
	}
}

pub fn is_system(data: &str) -> crate::Result<bool> {
	Ok(package_flags(data)?.contains(&PackageFlags::System))
}

pub fn runtime_permissions(data: &str) -> crate::Result<Vec<RuntimePermission>> {
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

	return Err(AdbError::ParseInputError());
}

pub fn package_flags(dump: &str) -> crate::Result<Vec<PackageFlags>> {
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
			Err(AdbError::ParseInputError())
		}
	} else {
		Err(AdbError::ParseInputError())
	}
}
