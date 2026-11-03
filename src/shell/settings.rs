//! `settings`: global / system / secure settings.

use std::io::BufReader;

use strum::{EnumIter, IntoStaticStr};

use crate::error::Result;
use crate::shell::{Property, Shell};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, IntoStaticStr, EnumIter)]
#[allow(non_camel_case_types)]
pub enum SettingsType {
    global,
    system,
    secure,
}

impl Shell<'_> {
    pub fn list_settings(&self, settings_type: SettingsType) -> Result<Vec<Property>> {
        let namespace: &str = settings_type.into();
        let output = self.exec(["settings", "list", namespace])?;
        parse_settings(&output.stdout)
    }

    /// The value of a setting, `None` if it is not set.
    pub fn get_setting(&self, settings_type: SettingsType, key: &str) -> Result<Option<String>> {
        let namespace: &str = settings_type.into();
        let value = self.exec_string(["settings", "get", namespace, key])?;
        Ok(match value.as_str() {
            "" | "null" => None,
            _ => Some(value),
        })
    }

    pub fn put_setting(&self, settings_type: SettingsType, key: &str, value: &str) -> Result<()> {
        let namespace: &str = settings_type.into();
        self.exec(["settings", "put", namespace, key, value])
            .map(|_| ())
    }

    pub fn delete_setting(&self, settings_type: SettingsType, key: &str) -> Result<()> {
        let namespace: &str = settings_type.into();
        self.exec(["settings", "delete", namespace, key])
            .map(|_| ())
    }
}

/// Parse the `key=value` lines of `settings list`.
fn parse_settings(output: &[u8]) -> Result<Vec<Property>> {
    let mut result: Vec<Property> = java_properties::read(BufReader::new(output))?
        .into_iter()
        .map(|(key, value)| Property { key, value })
        .collect();
    result.sort();
    Ok(result)
}

#[cfg(test)]
mod test {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn test_parse_settings() {
        let settings = parse_settings(include_bytes!(
            "../../test_files/fixtures/api35/settings_list_global.txt"
        ))
        .unwrap();
        assert!(settings.len() > 50);
        assert!(settings
            .iter()
            .any(|p| p.key == "adb_enabled" && p.value == "1"));
    }

    #[test]
    fn test_settings_type() {
        let names: Vec<&str> = SettingsType::iter().map(Into::into).collect();
        assert_eq!(names, ["global", "system", "secure"]);
    }
}
