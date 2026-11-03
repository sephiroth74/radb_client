//! `pm`: the package manager.

use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::shell::Shell;

mod dump;

pub use dump::{DexoptState, SimplePackageReader};

const DUMP_TIMEOUT: Option<Duration> = Some(Duration::from_secs(10));

/// The `pm` interface of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageManager<'a> {
    pub(crate) shell: Shell<'a>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Package {
    pub package_name: String,
    pub file_name: Option<String>,
    pub version_code: Option<i64>,
    pub uid: Option<i32>,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct RuntimePermission {
    pub name: String,
    pub granted: bool,
    pub flags: Vec<String>,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct InstallPermission {
    pub name: String,
    pub granted: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash, strum::IntoStaticStr, strum::Display)]
pub enum PackageFlags {
    System,
    HasCode,
    AllowClearUserData,
    UpdatedSystemApp,
    AllowBackup,
    Debuggable,
}

/// Options of `pm uninstall` / `adb uninstall`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct UninstallOptions {
    /// `-k`: keep the data and cache directories.
    pub keep_data: bool,
    /// `--user`
    pub user: Option<String>,
    /// `--versionCode`
    pub version_code: Option<i64>,
}

/// Options of `pm install`.
#[derive(Debug, Default, Eq, PartialEq, Clone, Hash)]
pub struct InstallOptions {
    /// `--user`: install for the given user.
    pub user: Option<String>,
    /// `--dont-kill`: installing a new feature split, don't kill the running app.
    pub dont_kill: bool,
    /// `--restrict-permissions`: don't allowlist restricted permissions at install.
    pub restrict_permissions: bool,
    /// `--pkg`: the expected package name of the app being installed.
    pub package_name: Option<String>,
    /// `--install-location`
    pub install_location: Option<InstallLocationOption>,
    /// `-g`: grant all runtime permissions.
    pub grant_permissions: bool,
    /// `-f`: install on internal flash.
    pub force: bool,
    /// `-r`: replace the existing application.
    pub replace_existing_application: bool,
    /// `-d`: allow version code downgrade.
    pub allow_version_downgrade: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash, Default)]
pub enum InstallLocationOption {
    #[default]
    Auto,
    InternalOnly,
    PreferExternal,
}

/// Filters of `pm list packages`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ListPackageFilter {
    /// `-d`: only disabled packages.
    pub show_only_disabled: bool,
    /// `-e`: only enabled packages.
    pub show_only_enabled: bool,
    /// `-s`: only system packages.
    pub show_only_system: bool,
    /// `-3`: only third party packages.
    pub show_only_3rd_party: bool,
    /// `--apex-only`: only APEX packages.
    pub apex_only: bool,
    /// `--uid`: only the packages with this UID.
    pub uid: Option<String>,
    /// `--user`: only the packages of this user.
    pub user: Option<String>,
}

/// What `pm list packages` prints for each package.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ListPackageDisplayOptions {
    /// `-U`: the package UID.
    pub show_uid: bool,
    /// `--show-versioncode`: the version code.
    pub show_version_code: bool,
    /// `-u`: include uninstalled packages.
    pub include_uninstalled: bool,
    /// `-f`: the APK file.
    pub show_apk_file: bool,
}

macro_rules! pm_operation {
    ($name:ident, $operation:literal) => {
        #[doc = concat!("`pm ", $operation, " [--user <user>] <package or component>`")]
        pub fn $name(&self, package_or_component: &str, user: Option<&str>) -> Result<()> {
            self.operation($operation, package_or_component, user)
        }
    };
}

impl PackageManager<'_> {
    /// The path of the APK of a package.
    pub fn path(&self, package_name: &str, user: Option<&str>) -> Result<String> {
        let mut args = vec!["pm", "path"];
        if let Some(user) = user {
            args.extend(["--user", user]);
        }
        args.push(package_name);
        let not_found = || Error::PackageNotFoundError(package_name.to_string());
        match self.shell.exec_string(args) {
            // split APKs have several `package:` lines: the first one is base.apk
            Ok(output) => output
                .lines()
                .find_map(|line| line.trim().strip_prefix("package:"))
                .map(str::to_string)
                .ok_or_else(not_found),
            Err(err) if err.exit_code() == Some(1) => Err(not_found()),
            Err(err) => Err(err),
        }
    }

    pub fn is_installed(&self, package_name: &str, user: Option<&str>) -> Result<bool> {
        match self.path(package_name, user) {
            Ok(_) => Ok(true),
            Err(Error::PackageNotFoundError(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn grant(&self, package_name: &str, permission: &str, user: Option<&str>) -> Result<()> {
        self.permission("grant", package_name, permission, user)
    }

    pub fn revoke(&self, package_name: &str, permission: &str, user: Option<&str>) -> Result<()> {
        self.permission("revoke", package_name, permission, user)
    }

    fn permission(
        &self,
        command: &str,
        package_name: &str,
        permission: &str,
        user: Option<&str>,
    ) -> Result<()> {
        let mut args = vec!["pm", command];
        if let Some(user) = user {
            args.extend(["--user", user]);
        }
        args.extend([package_name, permission]);
        self.shell.exec(args).map(|_| ())
    }

    /// Revert all the runtime permissions to their default state.
    pub fn reset_permissions(&self) -> Result<()> {
        self.shell.exec(["pm", "reset-permissions"]).map(|_| ())
    }

    pub fn list_packages(
        &self,
        filters: ListPackageFilter,
        display: ListPackageDisplayOptions,
        name_filter: Option<&str>,
    ) -> Result<Vec<Package>> {
        let mut args: Vec<OsString> = vec!["pm".into(), "list".into(), "packages".into()];
        args.extend(filters);
        args.extend(display);
        if let Some(name) = name_filter {
            args.push(name.into());
        }
        Ok(parse_package_list(&self.shell.exec_string(args)?))
    }

    /// The `dumpsys package` output of a package.
    pub fn dump(&self, package_name: &str, timeout: Option<Duration>) -> Result<String> {
        let output = self
            .shell
            .exec_with(["pm", "dump", package_name], None, timeout)?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn runtime_permissions(&self, package_name: &str) -> Result<Vec<RuntimePermission>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        Ok(SimplePackageReader::new(&dump)?.runtime_permissions())
    }

    pub fn install_permissions(&self, package_name: &str) -> Result<Vec<InstallPermission>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        Ok(SimplePackageReader::new(&dump)?.install_permissions())
    }

    pub fn requested_permissions(&self, package_name: &str) -> Result<Vec<String>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        Ok(SimplePackageReader::new(&dump)?.requested_permissions())
    }

    pub fn package_flags(&self, package_name: &str) -> Result<Vec<PackageFlags>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        Ok(SimplePackageReader::new(&dump)?.package_flags())
    }

    pub fn is_system(&self, package_name: &str) -> Result<bool> {
        Ok(self
            .package_flags(package_name)?
            .contains(&PackageFlags::System))
    }

    /// Install an APK that is already on the device.
    pub fn install<P: AsRef<str>>(&self, path: P, options: Option<InstallOptions>) -> Result<()> {
        let mut args: Vec<OsString> = vec!["cmd".into(), "package".into(), "install".into()];
        if let Some(options) = options {
            args.extend(options);
        }
        args.push(path.as_ref().into());
        self.shell.exec(args).map(|_| ())
    }

    pub fn uninstall(&self, package_name: &str, options: Option<UninstallOptions>) -> Result<()> {
        let mut args: Vec<OsString> = vec!["cmd".into(), "package".into(), "uninstall".into()];
        if let Some(options) = options {
            args.extend(options);
        }
        args.push(package_name.into());
        self.shell.exec(args).map(|_| ())
    }

    pm_operation!(clear, "clear");
    pm_operation!(suspend, "suspend");
    pm_operation!(unsuspend, "unsuspend");
    pm_operation!(hide, "hide");
    pm_operation!(unhide, "unhide");
    pm_operation!(default_state, "default-state");
    pm_operation!(disable_until_used, "disable-until-used");
    pm_operation!(disable_user, "disable-user");
    pm_operation!(disable, "disable");
    pm_operation!(enable, "enable");

    fn operation(
        &self,
        operation: &str,
        package_or_component: &str,
        user: Option<&str>,
    ) -> Result<()> {
        let mut args = vec!["pm", operation];
        if let Some(user) = user {
            args.extend(["--user", user]);
        }
        args.push(package_or_component);
        self.shell.exec(args).map(|_| ())
    }
}

/// Parse the output of `pm list packages [-f] [-U] [--show-versioncode]`:
/// `package:<file>=<name> versionCode:<code> uid:<uid>`.
pub(crate) fn parse_package_list(output: &str) -> Vec<Package> {
    output
        .lines()
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            let spec = tokens.next()?.strip_prefix("package:")?;
            // the file name may contain '=': the package name is after the last one
            let (file_name, package_name) = match spec.rsplit_once('=') {
                Some((file, name)) => (Some(file.to_string()), name.to_string()),
                None => (None, spec.to_string()),
            };
            let mut package = Package {
                package_name,
                file_name,
                version_code: None,
                uid: None,
            };
            for token in tokens {
                match token.split_once(':') {
                    Some(("versionCode", code)) => package.version_code = code.parse().ok(),
                    Some(("uid", uid)) => package.uid = uid.parse().ok(),
                    _ => {}
                }
            }
            Some(package)
        })
        .collect()
}

impl IntoIterator for UninstallOptions {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let mut args: Vec<OsString> = vec![];
        if self.keep_data {
            args.push("-k".into());
        }
        if let Some(user) = self.user {
            args.extend(["--user".into(), user.into()]);
        }
        if let Some(version_code) = self.version_code {
            args.extend(["--versionCode".into(), version_code.to_string().into()]);
        }
        args.into_iter()
    }
}

impl IntoIterator for InstallOptions {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let mut args: Vec<OsString> = vec![];
        if let Some(user) = self.user {
            args.extend(["--user".into(), user.into()]);
        }
        if let Some(package_name) = self.package_name {
            args.extend(["--pkg".into(), package_name.into()]);
        }
        if let Some(location) = self.install_location {
            args.extend(["--install-location".into(), location.to_string().into()]);
        }
        let flags = [
            (self.dont_kill, "--dont-kill"),
            (self.restrict_permissions, "--restrict-permissions"),
            (self.grant_permissions, "-g"),
            (self.force, "-f"),
            (self.replace_existing_application, "-r"),
            (self.allow_version_downgrade, "-d"),
        ];
        args.extend(
            flags
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, flag)| flag.into()),
        );
        args.into_iter()
    }
}

impl IntoIterator for ListPackageFilter {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let flags = [
            (self.show_only_disabled, "-d"),
            (self.show_only_enabled, "-e"),
            (self.show_only_system, "-s"),
            (self.show_only_3rd_party, "-3"),
            (self.apex_only, "--apex-only"),
        ];
        let mut args: Vec<OsString> = flags
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, flag)| flag.into())
            .collect();
        if let Some(uid) = self.uid {
            args.extend(["--uid".into(), uid.into()]);
        }
        if let Some(user) = self.user {
            args.extend(["--user".into(), user.into()]);
        }
        args.into_iter()
    }
}

impl IntoIterator for ListPackageDisplayOptions {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let flags = [
            (self.show_uid, "-U"),
            (self.show_version_code, "--show-versioncode"),
            (self.include_uninstalled, "-u"),
            (self.show_apk_file, "-f"),
        ];
        flags
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, flag)| flag.into())
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl Default for ListPackageDisplayOptions {
    fn default() -> Self {
        ListPackageDisplayOptions {
            show_uid: true,
            show_version_code: true,
            include_uninstalled: false,
            show_apk_file: true,
        }
    }
}

impl Display for InstallLocationOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallLocationOption::Auto => write!(f, "0"),
            InstallLocationOption::InternalOnly => write!(f, "1"),
            InstallLocationOption::PreferExternal => write!(f, "2"),
        }
    }
}

impl Display for Package {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.package_name)?;
        if let Some(version_code) = self.version_code {
            write!(f, " version:{version_code}")?;
        }
        if let Some(uid) = self.uid {
            write!(f, " uid:{uid}")?;
        }
        if let Some(file_name) = &self.file_name {
            write!(f, " file_name:{file_name}")?;
        }
        Ok(())
    }
}

impl Display for RuntimePermission {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} granted={} flags={}",
            self.name,
            self.granted,
            self.flags.join(",")
        )
    }
}

impl Display for InstallPermission {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} granted={}", self.name, self.granted)
    }
}

impl TryFrom<&str> for PackageFlags {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "SYSTEM" => Ok(PackageFlags::System),
            "HAS_CODE" => Ok(PackageFlags::HasCode),
            "ALLOW_CLEAR_USER_DATA" => Ok(PackageFlags::AllowClearUserData),
            "UPDATED_SYSTEM_APP" => Ok(PackageFlags::UpdatedSystemApp),
            "ALLOW_BACKUP" => Ok(PackageFlags::AllowBackup),
            "DEBUGGABLE" => Ok(PackageFlags::Debuggable),
            _ => Err(Error::NameNotFoundError(value.to_string())),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn strings<I: IntoIterator<Item = OsString>>(args: I) -> Vec<String> {
        args.into_iter().map(|s| s.into_string().unwrap()).collect()
    }

    #[test]
    fn test_parse_package_list() {
        let packages = parse_package_list(include_str!(
            "../../test_files/fixtures/api35/pm_list_packages.txt"
        ));
        assert!(packages.len() > 200);
        let app = packages
            .iter()
            .find(|p| p.package_name == "it.sephiroth.android.app.app")
            .expect("test app not found");
        assert_eq!(app.version_code, Some(1));
        assert!(app.uid.is_some_and(|uid| uid >= 10000));
        assert!(app
            .file_name
            .as_ref()
            .is_some_and(|f| f.ends_with("/base.apk")));

        let packages = parse_package_list(
            "package:com.a\npackage:/data/app/x==/base.apk=com.b uid:1000 versionCode:7\n",
        );
        assert_eq!(packages[0].package_name, "com.a");
        assert_eq!(packages[0].file_name, None);
        assert_eq!(packages[1].package_name, "com.b");
        assert_eq!(
            packages[1].file_name.as_deref(),
            Some("/data/app/x==/base.apk")
        );
        assert_eq!(
            (packages[1].uid, packages[1].version_code),
            (Some(1000), Some(7))
        );
    }

    #[test]
    fn test_install_options() {
        assert!(strings(InstallOptions::default()).is_empty());
        let options = InstallOptions {
            user: Some("10".to_string()),
            package_name: Some("com.example".to_string()),
            install_location: Some(InstallLocationOption::PreferExternal),
            grant_permissions: true,
            replace_existing_application: true,
            ..Default::default()
        };
        assert_eq!(
            strings(options),
            [
                "--user",
                "10",
                "--pkg",
                "com.example",
                "--install-location",
                "2",
                "-g",
                "-r"
            ]
        );
    }

    #[test]
    fn test_uninstall_options() {
        let options = UninstallOptions {
            keep_data: true,
            user: Some("0".to_string()),
            version_code: Some(42),
        };
        assert_eq!(
            strings(options),
            ["-k", "--user", "0", "--versionCode", "42"]
        );
    }

    #[test]
    fn test_list_package_options() {
        let filter = ListPackageFilter {
            show_only_3rd_party: true,
            user: Some("0".to_string()),
            ..Default::default()
        };
        assert_eq!(strings(filter), ["-3", "--user", "0"]);
        assert_eq!(
            strings(ListPackageDisplayOptions::default()),
            ["-U", "--show-versioncode", "-f"]
        );
    }
}
