//! Parse the output of `dumpsys package <package>` (`pm dump`).

use crate::error::{Error, Result};
use crate::pm::{InstallPermission, PackageFlags, RuntimePermission};

/// The `Packages:` section of `dumpsys package <package>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimplePackageReader<'a> {
    pub(crate) data: &'a str,
    pub dexopt: DexoptState<'a>,
}

/// The `Dexopt state:` section of `dumpsys package <package>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DexoptState<'a> {
    pub(crate) data: &'a str,
}

/// The lines after `header`, up to the first line that is not more indented
/// than the header (or the end of the text).
fn section<'a>(data: &'a str, header: &str) -> Option<Vec<&'a str>> {
    let indent = |line: &str| line.len() - line.trim_start().len();
    let mut lines = data.lines();
    let header_indent = indent(lines.by_ref().find(|line| line.trim() == header)?);
    Some(
        lines
            .take_while(|line| !line.trim().is_empty() && indent(line) > header_indent)
            .collect(),
    )
}

/// A top-level section (`Packages:`, `Dexopt state:`) of the dump.
fn top_level_section<'a>(data: &'a str, header: &str) -> Option<&'a str> {
    let start = data
        .match_indices(header)
        .find(|(index, _)| {
            let line_start = data[..*index].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let indent = &data[line_start..*index];
            indent.len() <= 2
                && indent.chars().all(|c| c == ' ')
                && data[index + header.len()..].starts_with('\n')
        })?
        .0;
    let body = &data[start + header.len() + 1..];
    let end = body.find("\n\n").map(|i| i + 1).unwrap_or(body.len());
    Some(&body[..end])
}

impl<'a> SimplePackageReader<'a> {
    /// Read the output of `dumpsys package <package>`. Fails if it has no
    /// `Packages:` section (the package is not installed).
    pub fn new(data: &'a str) -> Result<SimplePackageReader<'a>> {
        let packages = top_level_section(data, "Packages:")
            .ok_or_else(|| Error::ParseError("Packages section of dumpsys package".to_string()))?;
        Ok(SimplePackageReader {
            data: packages,
            dexopt: DexoptState {
                data: top_level_section(data, "Dexopt state:").unwrap_or(""),
            },
        })
    }

    pub fn requested_permissions(&self) -> Vec<String> {
        section(self.data, "requested permissions:")
            .unwrap_or_default()
            .iter()
            .filter_map(|line| line.trim().split([':', ' ']).next())
            .map(str::to_string)
            .collect()
    }

    pub fn install_permissions(&self) -> Vec<InstallPermission> {
        section(self.data, "install permissions:")
            .unwrap_or_default()
            .iter()
            .filter_map(|line| {
                let (name, rest) = line.trim().split_once(": ")?;
                Some(InstallPermission {
                    name: name.to_string(),
                    granted: rest.starts_with("granted=true"),
                })
            })
            .collect()
    }

    pub fn runtime_permissions(&self) -> Vec<RuntimePermission> {
        section(self.data, "runtime permissions:")
            .unwrap_or_default()
            .iter()
            .filter_map(|line| {
                let (name, rest) = line.trim().split_once(": ")?;
                let flags = rest
                    .split_once("flags=[")
                    .and_then(|(_, flags)| flags.split_once(']'))
                    .map(|(flags, _)| {
                        flags
                            .split('|')
                            .map(str::trim)
                            .filter(|f| !f.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                Some(RuntimePermission {
                    name: name.to_string(),
                    granted: rest.starts_with("granted=true"),
                    flags,
                })
            })
            .collect()
    }

    /// The known `pkgFlags`.
    pub fn package_flags(&self) -> Vec<PackageFlags> {
        self.data
            .lines()
            .find_map(|line| line.trim().strip_prefix("pkgFlags=["))
            .map(|flags| {
                flags
                    .trim_end_matches(']')
                    .split_whitespace()
                    .filter_map(|flag| PackageFlags::try_from(flag).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn is_system(&self) -> bool {
        self.package_flags().contains(&PackageFlags::System)
    }

    /// The value of a `name=value` line of the package section.
    pub fn get_item(&self, name: &str) -> Result<&'a str> {
        let prefix = format!("{name}=");
        self.data
            .lines()
            .find_map(|line| line.trim_start().strip_prefix(prefix.as_str()))
            .map(str::trim_end)
            .ok_or_else(|| Error::NameNotFoundError(name.to_string()))
    }

    pub fn get_version_name(&self) -> Option<&'a str> {
        self.get_item("versionName").ok()
    }

    /// The version code (`versionCode=1 minSdk=24 targetSdk=24`).
    pub fn get_version_code(&self) -> Option<i64> {
        self.get_item("versionCode")
            .ok()?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    }

    pub fn get_first_install_time(&self) -> Option<&'a str> {
        self.get_item("firstInstallTime").ok()
    }

    pub fn get_last_update_time(&self) -> Option<&'a str> {
        self.get_item("lastUpdateTime").ok()
    }

    pub fn get_timestamp(&self) -> Option<&'a str> {
        self.get_item("timeStamp").ok()
    }

    pub fn get_data_dir(&self) -> Option<&'a str> {
        self.get_item("dataDir").ok()
    }

    /// `appId` (`userId` before Android 10).
    pub fn get_app_id(&self) -> Option<&'a str> {
        self.get_item("appId")
            .or_else(|_| self.get_item("userId"))
            .ok()
    }

    pub fn get_code_path(&self) -> Option<&'a str> {
        self.get_item("codePath").ok()
    }

    pub fn get_resource_path(&self) -> Option<&'a str> {
        self.get_item("resourcePath").ok()
    }
}

impl<'a> DexoptState<'a> {
    /// The APK path of the package, from the dexopt section.
    pub fn get_package_path(&self, package_name: &str) -> Option<&'a str> {
        let header = format!("[{package_name}]");
        section(self.data, &header)?
            .iter()
            .find_map(|line| line.trim().strip_prefix("path:"))
            .map(str::trim)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const APP: &str = include_str!("../../test_files/fixtures/api35/dumpsys_package_app.txt");
    const SETTINGS: &str =
        include_str!("../../test_files/fixtures/api35/dumpsys_package_settings.txt");

    #[test]
    fn test_reader() {
        let reader = SimplePackageReader::new(APP).unwrap();
        assert_eq!(reader.get_version_name(), Some("1.0"));
        assert_eq!(reader.get_version_code(), Some(1));
        assert_eq!(reader.get_app_id(), Some("10207"));
        assert_eq!(
            reader.get_data_dir(),
            Some("/data/user/0/it.sephiroth.android.app.app")
        );
        assert_eq!(reader.get_timestamp(), Some("2026-11-03 09:09:51"));
        assert!(reader.get_code_path().unwrap().starts_with("/data/app/"));
        assert!(reader.get_item("nonexistent").is_err());
    }

    #[test]
    fn test_permissions() {
        let reader = SimplePackageReader::new(APP).unwrap();
        assert_eq!(
            reader.requested_permissions(),
            [
                "android.permission.POST_NOTIFICATIONS",
                "it.sephiroth.android.app.app.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"
            ]
        );
        assert_eq!(
            reader.install_permissions(),
            [InstallPermission {
                name: "it.sephiroth.android.app.app.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"
                    .to_string(),
                granted: true
            }]
        );
        assert_eq!(
            reader.runtime_permissions(),
            [RuntimePermission {
                name: "android.permission.POST_NOTIFICATIONS".to_string(),
                granted: false,
                flags: vec![
                    "USER_SENSITIVE_WHEN_GRANTED".to_string(),
                    "USER_SENSITIVE_WHEN_DENIED".to_string()
                ]
            }]
        );

        let settings = SimplePackageReader::new(SETTINGS).unwrap();
        assert!(settings.requested_permissions().len() > 50);
        assert!(settings
            .install_permissions()
            .iter()
            .all(|p| !p.name.contains(' ')));
        assert!(settings.runtime_permissions().is_empty());
    }

    #[test]
    fn test_flags() {
        let app = SimplePackageReader::new(APP).unwrap();
        assert_eq!(
            app.package_flags(),
            [
                PackageFlags::Debuggable,
                PackageFlags::HasCode,
                PackageFlags::AllowClearUserData,
                PackageFlags::AllowBackup
            ]
        );
        assert!(!app.is_system());
        assert!(SimplePackageReader::new(SETTINGS).unwrap().is_system());
    }

    #[test]
    fn test_dexopt() {
        let app = SimplePackageReader::new(APP).unwrap();
        let path = app
            .dexopt
            .get_package_path("it.sephiroth.android.app.app")
            .unwrap();
        assert!(path.starts_with("/data/app/") && path.ends_with("/base.apk"));
        assert_eq!(app.dexopt.get_package_path("com.other"), None);
        let settings = SimplePackageReader::new(SETTINGS).unwrap();
        assert_eq!(
            settings.dexopt.get_package_path("com.android.settings"),
            Some("/system_ext/priv-app/SettingsGoogle/SettingsGoogle.apk")
        );
    }

    #[test]
    fn test_not_installed() {
        assert!(SimplePackageReader::new("Dexopt state:\n  nothing\n").is_err());
        // pre-Android 8 dumps indent the section
        let old = "  Packages:\n    Package [com.a] (1):\n      versionName=2.0\n\n";
        assert_eq!(
            SimplePackageReader::new(old).unwrap().get_version_name(),
            Some("2.0")
        );
    }
}
