use lazy_static::lazy_static;
use regex::Regex;

use crate::errors::AdbError;

lazy_static! {
	static ref RE_PACKAGES: Regex = Regex::new("(?m)^Packages:\\n").unwrap();
	static ref RE_NEW_EMPTY_LINE: Regex = Regex::new("(?m)^$").unwrap();
	static ref RE_REQUESTED_PERMISSIONS: Regex = Regex::new("(?m)^\\s{3,}requested permissions:\\n((\\s{4,}[\\w\\.]+$)+)").unwrap();
	static ref RE_SINGLE_PERMISSION: Regex = Regex::new("(?m)^\\s{4,}([\\w\\.]+)$").unwrap();
}

pub(crate) struct SimplePackageReader<'a> {
	data: &'a str,
}

impl<'a> SimplePackageReader<'a> {
	pub(crate) fn new(data: &'a str) -> crate::command::Result<SimplePackageReader<'a>> {
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

	pub async fn requested_permissions(&self) -> crate::command::Result<Vec<String>> {
		if let Some(m) = RE_REQUESTED_PERMISSIONS.captures(self.data) {
			if m.len() > 0 {
				let new_data = &self.data[m.get(0).unwrap().range()];
				let mut result = vec![];
				for (_, [name]) in RE_SINGLE_PERMISSION.captures_iter(new_data).map(|c| c.extract()) {
					result.push(name.to_string())
				}
				return Ok(result);
			}
		}
		Err(AdbError::ParseInputError())
	}
}
