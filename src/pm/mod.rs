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
/// A package listed by [`PackageManager::list_packages`].
pub struct Package {
    /// The package name.
    pub package_name: String,
    /// The APK path (`-f`).
    pub file_name: Option<String>,
    /// The version code (`--show-versioncode`).
    pub version_code: Option<i64>,
    /// The Linux uid of the app (`-U`).
    pub uid: Option<i32>,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
/// A runtime (dangerous) permission of a package, for user 0.
pub struct RuntimePermission {
    /// The permission name.
    pub name: String,
    /// True if the user granted it.
    pub granted: bool,
    /// The permission flags, e.g. `USER_SET`, `USER_SENSITIVE_WHEN_GRANTED`.
    pub flags: Vec<String>,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
/// An install-time (normal or signature) permission of a package.
pub struct InstallPermission {
    /// The permission name.
    pub name: String,
    /// True if it was granted at install time.
    pub granted: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash, strum::IntoStaticStr, strum::Display)]
/// The known `pkgFlags` of `dumpsys package`.
pub enum PackageFlags {
    /// `SYSTEM`: the package has a copy on a system partition.
    System,
    /// `HAS_CODE`
    HasCode,
    /// `ALLOW_CLEAR_USER_DATA`
    AllowClearUserData,
    /// `UPDATED_SYSTEM_APP`: a system app whose active copy is an update in `/data/app`.
    UpdatedSystemApp,
    /// `ALLOW_BACKUP`
    AllowBackup,
    /// `DEBUGGABLE`
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
/// `pm install --install-location`.
pub enum InstallLocationOption {
    #[default]
    /// `0`: let the system decide.
    Auto,
    /// `1`: internal storage only.
    InternalOnly,
    /// `2`: prefer the external storage.
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

    /// True if the package is installed (for `user`, or the current user). A system app
    /// hidden with `pm uninstall --user` is not installed for that user.
    pub fn is_installed(&self, package_name: &str, user: Option<&str>) -> Result<bool> {
        match self.path(package_name, user) {
            Ok(_) => Ok(true),
            Err(Error::PackageNotFoundError(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Grant a runtime permission (`pm grant`). Fails if the package is not installed.
    pub fn grant(&self, package_name: &str, permission: &str, user: Option<&str>) -> Result<()> {
        self.permission("grant", package_name, permission, user)
    }

    /// Revoke a runtime permission (`pm revoke`). Fails if the package is not installed.
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
        self.shell.exec(args)?;
        // Android 14 exits with 0, without a message, for an unknown package
        if !self.is_installed(package_name, user)? {
            return Err(Error::PackageNotFoundError(package_name.to_string()));
        }
        Ok(())
    }

    /// Revert all the runtime permissions to their default state.
    pub fn reset_permissions(&self) -> Result<()> {
        self.shell.exec(["pm", "reset-permissions"]).map(|_| ())
    }

    /// The installed packages (`pm list packages`), optionally only those whose name
    /// contains `name_filter`.
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

    /// The runtime permissions of a package, for user 0.
    pub fn runtime_permissions(&self, package_name: &str) -> Result<Vec<RuntimePermission>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        Ok(SimplePackageReader::new(&dump)?.runtime_permissions())
    }

    /// The install-time permissions of a package.
    pub fn install_permissions(&self, package_name: &str) -> Result<Vec<InstallPermission>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        Ok(SimplePackageReader::new(&dump)?.install_permissions())
    }

    /// The permissions requested by the manifest of a package.
    pub fn requested_permissions(&self, package_name: &str) -> Result<Vec<String>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        Ok(SimplePackageReader::new(&dump)?.requested_permissions())
    }

    /// The known `pkgFlags` of a package.
    pub fn package_flags(&self, package_name: &str) -> Result<Vec<PackageFlags>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        Ok(SimplePackageReader::new(&dump)?.package_flags())
    }

    /// True if the package is a system app (it has a copy on a system partition).
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

    /// Uninstall a package (`cmd package uninstall`). For a system app it only removes
    /// the updates: see [`crate::Client::remove_system_app`] to remove a system app.
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

    /// Where the copies of a package are: the system copy, the update in `/data/app`...
    /// Fails with [`Error::PackageNotFoundError`] if the package manager doesn't know it.
    pub fn system_app_info(&self, package_name: &str) -> Result<SystemAppInfo> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        SystemAppInfo::parse(package_name, &dump)
    }

    /// Remove the updates (in `/data/app`) of a system app: the system copy is used again.
    /// Does nothing if the app has no updates.
    ///
    /// Like "Uninstall updates" in the Android settings, it resets the app data when the
    /// system copy is older than the update.
    pub fn uninstall_system_updates(&self, package_name: &str) -> Result<()> {
        if self.system_app_info(package_name)?.update.is_none() {
            return Ok(());
        }
        // it prints "Success" but can exit with 1 (Android 14, 15)
        let output =
            self.shell
                .exec_raw(["pm", "uninstall-system-updates", package_name], None, None)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.contains("Success") {
            if stdout.contains("Unknown command")
                || String::from_utf8_lossy(&output.stderr).contains("Unknown command")
            {
                // before Android 10: `pm uninstall` of a system app removes its updates
                return self
                    .shell
                    .exec(["pm", "uninstall", package_name])
                    .map(|_| ());
            }
            let command = format!(
                "adb {} shell pm uninstall-system-updates {package_name}",
                self.shell.client().addr
            );
            return Err(crate::error::CommandFailure::new(command, &output).into());
        }
        match self.system_app_info(package_name)?.update {
            None => Ok(()),
            Some(update) => Err(Error::SystemAppError(format!(
                "the update of {package_name} is still installed: {}",
                update.code_path
            ))),
        }
    }
}

/// One copy (APK or APK directory) of a package.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageCopy {
    /// The APK directory (or the APK, for an APK placed directly in `priv-app`).
    pub code_path: String,
    /// The version code.
    pub version_code: Option<i64>,
    /// The version name.
    pub version_name: Option<String>,
    /// Signature ids: equal ids mean the same signing certificates.
    pub signatures: Vec<String>,
}

/// The state of a package, with its system copy and its update.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SystemAppInfo {
    /// The package name.
    pub package_name: String,
    /// The copy that the package manager runs.
    pub active: PackageCopy,
    /// The copy on a system partition (`/system/priv-app/...`): the active one if
    /// there is no update. `None` for apps that are not system apps.
    pub system_copy: Option<PackageCopy>,
    /// The update of a system app, in `/data/app`.
    pub update: Option<PackageCopy>,
    /// In a `priv-app` directory.
    pub privileged: bool,
    /// `(user id, installed)`. `pm uninstall --user <id>` of a system app only hides
    /// it: it stays on the device, with `installed=false`.
    pub users: Vec<(u32, bool)>,
}

impl SystemAppInfo {
    pub(crate) fn parse(package_name: &str, dump: &str) -> Result<SystemAppInfo> {
        let not_found = || Error::PackageNotFoundError(package_name.to_string());
        let reader = SimplePackageReader::new(dump).map_err(|_| not_found())?;
        let copy = |reader: &SimplePackageReader| -> Option<PackageCopy> {
            Some(PackageCopy {
                code_path: reader.get_code_path()?.to_string(),
                version_code: reader.get_version_code(),
                version_name: reader.get_version_name().map(str::to_string),
                signatures: reader.signatures(),
            })
        };
        let active = copy(&reader).ok_or_else(not_found)?;
        let is_system = reader.is_system();
        let (system_copy, update) = if reader.is_updated_system_app() {
            let hidden = SimplePackageReader::hidden_system_package(dump).and_then(|r| copy(&r));
            (hidden, Some(active.clone()))
        } else if is_system {
            (Some(active.clone()), None)
        } else {
            (None, None)
        };
        Ok(SystemAppInfo {
            package_name: package_name.to_string(),
            active,
            system_copy,
            update,
            privileged: reader.is_privileged(),
            users: reader.users(),
        })
    }

    /// True if the package has a copy on a system partition.
    pub fn is_system_app(&self) -> bool {
        self.system_copy.is_some()
    }

    /// The users for which the package is hidden (`pm uninstall --user <id>`).
    pub fn hidden_for_users(&self) -> Vec<u32> {
        self.users
            .iter()
            .filter(|(_, installed)| !installed)
            .map(|(user, _)| *user)
            .collect()
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
