use lazy_static::lazy_static;
use regex::{Regex, RegexBuilder};

use crate::error::Error;
use crate::result::Result;
use crate::types::{DexoptState, InstallPermission, PackageFlags, RuntimePermission, SimplePackageReader};

lazy_static! {
	static ref RE_PACKAGES: &'static str = "(?m)^Packages:\\n";
	static ref RE_PACKAGES_26: &'static str = "(?m)^\\s\\sPackages:\\n";
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
	static ref RE_DEXOPT_STATE: Regex = Regex::new("(?m)^Dexopt state:\\n").unwrap();
	static ref RE_PACKAGE_NAME: Regex = Regex::new(r#"^\s+\[[\w.]+]$"#).unwrap();
	static ref RE_PACKAGE_PATH: Regex = Regex::new(r#"^\s+path:\s*(?<path>[^\n]+)$"#).unwrap();
}

#[allow(dead_code)]
impl<'a> SimplePackageReader<'a> {
	/// Create a new SimplePackageReader from the given data.
	/// The data should be the output of the `dump` command.
	/// The `sdk_version` should be the version of the SDK of the device.
	/// This is used to determine the correct regex to use.
	///
	pub fn new(data: &'a str, sdk_version: u16) -> Result<SimplePackageReader<'a>> {
		let mut packages_data: Option<&str> = None;
		let mut dexopt_data: Option<&str> = None;

		let re_packages = Self::packages_regex(sdk_version);

		if let Some(m) = re_packages.captures(data) {
			if m.len() == 1 {
				let mut new_data = &data[m.get(0).unwrap().end()..];
				if let Some(m) = RE_NEW_EMPTY_LINE.captures(new_data) {
					if m.len() == 1 {
						new_data = &new_data[..m.get(0).unwrap().start()];
						packages_data = Some(new_data);
					}
				}
			}
		}

		if let Some(m) = RE_DEXOPT_STATE.captures(data) {
			if m.len() == 1 {
				let mut new_data = &data[m.get(0).unwrap().end()..];
				if let Some(m) = RE_NEW_EMPTY_LINE.captures(new_data) {
					if m.len() == 1 {
						new_data = &new_data[..m.get(0).unwrap().start()];
						dexopt_data = Some(new_data);
					}
				}
			}
		}

		return match packages_data {
			Some(data) => Ok(SimplePackageReader {
				data,
				dexopt: DexoptState {
					data: dexopt_data.take().unwrap_or(""),
				},
			}),
			None => Err(Error::ParseInputError),
		};
	}

	fn packages_regex(sdk_int: u16) -> Regex {
		if sdk_int > 26 {
			Regex::new(&RE_PACKAGES).unwrap()
		} else {
			Regex::new(&RE_PACKAGES_26).unwrap()
		}
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
				for (
					_,
					[
						name,
						granted,
					],
				) in RE_INSTALL_PERMISSION.captures_iter(new_data).map(|c| c.extract())
				{
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

	pub fn get_item(&self, name: &str) -> Result<&str> {
		let re = Regex::new(format!("(?m)^\\s{{3,}}{:}=(.*)$", name).as_str()).unwrap();

		match self.parse(re) {
			Ok(result) => Ok(result),
			Err(_) => Err(Error::NameNotFoundError(name.to_string())),
		}
	}

	#[inline]
	fn parse(&self, regex: Regex) -> Result<&str> {
		if let Some(m) = regex.captures(self.data) {
			if m.len() == 2 {
				return Ok(m.get(1).unwrap().as_str());
			}
		}
		return Err(Error::ParseInputError);
	}
}

impl<'a> DexoptState<'a> {
	pub fn get_package_path(&self, package_name: &str) -> Option<&str> {
		let re_string = format!("^\\s+\\[{package_name}\\]$");
		let re_package_name: Regex = Regex::new(&re_string).unwrap();

		let mut in_section = false;

		for line in self.data.lines() {
			if let Some(_m) = re_package_name.captures(line) {
				in_section = true;
				continue;
			}

			if in_section {
				if let Some(m) = RE_PACKAGE_PATH.captures(line) {
					let group = m.name("path").map(|it| it.as_str());
					return group;
				}
			}
		}
		return None;
	}
}

pub fn is_system(data: &str) -> Result<bool> {
	Ok(package_flags(data)?.contains(&PackageFlags::System))
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
				for (
					_,
					[
						name,
						granted,
						flag_str,
					],
				) in RE_SINGLE_RUNTIME_PERMISSION.captures_iter(output3).map(|c| c.extract())
				{
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

	return Err(Error::ParseInputError);
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
			Err(Error::ParseInputError)
		}
	} else {
		Err(Error::ParseInputError)
	}
}

#[allow(dead_code)]
pub fn is_installed(data: &str, package_name: &str, sdk_int: u16) -> Option<String> {
	match SimplePackageReader::new(data, sdk_int) {
		Ok(reader) => {
			let dex = reader.dexopt;
			let result = dex.get_package_path(package_name).take().map(|s| s.to_string());
			result
		}
		Err(_) => None,
	}
}

#[cfg(test)]
mod test {
	use crate::test::test::{connect_client, connection_from_tcpip, init_log};

	use super::*;

	#[test]
	fn test_package_flags() {
		init_log();
		let client = connect_client(connection_from_tcpip());

		let package_path = client.shell().pm().path("com.swisscom.swisscomTv", None).unwrap();
		eprintln!("Package path: {}", package_path);
		assert!(package_path.contains("SwisscomTvUI"));

		let dump = client
			.shell()
			.pm()
			.dump("com.swisscom.swisscomTv", Some(std::time::Duration::from_secs(3)))
			.unwrap();

		let sdk_version = client.shell().build_version_sdk().unwrap();
		let reader = SimplePackageReader::new(&dump, sdk_version).unwrap();
		let flags = reader.get_package_flags().unwrap();
		eprintln!("Flags: {:?}", flags);
		let is_system = reader.is_system();
		eprintln!("Is system: {}", is_system.unwrap());
		let code_path = reader.get_code_path();
		eprintln!("Code path: {}", code_path.unwrap());
		let resource_path = reader.get_resource_path();
		eprintln!("Resource path: {}", resource_path.unwrap());
		let data_dir = reader.get_data_dir();
		eprintln!("Data dir: {}", data_dir.unwrap());
		let version_name = reader.get_version_name().unwrap();
		eprintln!("Version name: {}", version_name);
		let version_code = reader.get_version_code();
		eprintln!("Version code: {}", version_code.unwrap());

		let requested_permissions = reader.requested_permissions().unwrap();
		eprintln!("Requested permissions: {:?}", requested_permissions);

		let install_permissions = reader.install_permissions();
		eprintln!("Install permissions: {:?}", install_permissions);

		let value = reader.get_first_install_time().unwrap();
		eprintln!("First install time: {}", value);

		let value = reader.get_last_update_time().unwrap();
		eprintln!("Last update time: {}", value);

		let value = reader.get_timestamp().unwrap();
		eprintln!("Timestamp: {}", value);

		let value = reader.get_user_id().unwrap();
		eprintln!("User id: {}", value);
	}
}
