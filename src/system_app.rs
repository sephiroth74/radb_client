//! Install, update and remove system apps (`/system/priv-app`, `/system/app`...).
//!
//! A system app can have two copies: the system copy, on a read-only system partition,
//! and an update in `/data/app` (installed by `adb install` / `pm install`). The package
//! manager runs the update, if there is one. See [`Client::install_system_app`].

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::apk::{native_libraries, ApkManifest};
use crate::client::{AdbInstallOptions, Client};
use crate::command::{run_unchecked, shell_quote};
use crate::error::{CommandFailure, Error, Result};
use crate::pm::{SystemAppInfo, UninstallOptions};

const BACKUP_DIR: &str = "/data/local/tmp/radb-system-app-backup";
const DEFAULT_SYSTEM_APP_DIR: &str = "/system/priv-app";
/// Longest first: `/system_ext` before `/system`.
const SYSTEM_PARTITIONS: [&str; 5] = ["/system_ext", "/system", "/product", "/vendor", "/odm"];
const MAX_DUPLICATES: usize = 3;
/// `adb install` errors that a new system copy can solve.
const INCOMPATIBLE_UPDATE: [&str; 5] = [
    "INSTALL_FAILED_UPDATE_INCOMPATIBLE",
    "INSTALL_FAILED_SHARED_USER_INCOMPATIBLE",
    "INSTALL_FAILED_VERSION_DOWNGRADE",
    "INSTALL_PARSE_FAILED_INCONSISTENT_CERTIFICATES",
    "signatures do not match",
];

/// How [`Client::install_system_app`] installs the APK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SystemAppStrategy {
    /// Install the APK as an update (`adb install -r`, no reboot) when the package is
    /// a system app signed with the same key and the new version is not lower than the
    /// system copy. Otherwise replace the system copy.
    ///
    /// An update with a version lower than the system copy is accepted by `adb install -d`,
    /// but the package manager deletes it at the next boot: `Auto` never does that.
    #[default]
    Auto,
    /// Always replace the system copy (and remove the updates).
    ReplaceSystemCopy,
}

/// What to do when a `priv-app` requests privileged permissions of the platform that the
/// privapp-permissions allowlist does not grant (the allowlist is not enforced for the
/// privileged permissions defined by apps). With `ro.control_privapp_permissions=enforce`
/// the device does not boot (`system_server` crashes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PrivappPermissionsPolicy {
    /// Fail with [`Error::PrivappPermissionsError`], before changing the device.
    #[default]
    Fail,
    /// Grant them in `<partition>/etc/permissions/privapp-permissions-radb-<package>.xml`.
    Allowlist,
    /// Install anyway (the device may not boot: see `rollback`).
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Options of [`Client::install_system_app`].
pub struct SystemAppInstallOptions {
    /// Update when possible, or always replace the system copy.
    pub strategy: SystemAppStrategy,
    /// The directory of the new system copy, e.g. `/system/priv-app/MyApp`. By default the
    /// directory of the current system copy (an APK placed directly in `priv-app/` moves to
    /// its own directory), or `/system/priv-app/<apk file name>` for a new system app.
    pub target_dir: Option<String>,
    /// Clear the app data after the installation. The package manager keeps it, even when
    /// the signature changes or the version is lower.
    pub wipe_data: bool,
    /// What to do with the privileged permissions missing from the allowlist.
    pub privapp_permissions: PrivappPermissionsPolicy,
    /// How long to wait for each reboot.
    pub boot_timeout: Duration,
    /// Restore the previous system copy if the device does not boot, or does not run the
    /// new one. The removed updates (`/data/app`) are not restored.
    pub rollback: bool,
}

impl Default for SystemAppInstallOptions {
    fn default() -> Self {
        SystemAppInstallOptions {
            strategy: SystemAppStrategy::Auto,
            target_dir: None,
            wipe_data: false,
            privapp_permissions: PrivappPermissionsPolicy::Fail,
            boot_timeout: Duration::from_secs(300),
            rollback: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// How [`Client::install_system_app`] installed the APK.
pub enum SystemAppInstallMethod {
    /// Installed as an update in `/data/app`, no reboot.
    Update,
    /// Installed as the system copy, with a reboot.
    SystemCopy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The result of [`Client::install_system_app`].
pub struct SystemAppInstallReport {
    /// Installed as an update, or as the system copy.
    pub method: SystemAppInstallMethod,
    /// The state before the installation (`None`: the package was not installed).
    pub before: Option<SystemAppInfo>,
    /// The state after the installation.
    pub after: SystemAppInfo,
    /// Privileged permissions added to the allowlist ([`PrivappPermissionsPolicy::Allowlist`]).
    pub allowlisted_permissions: Vec<String>,
    /// Other system copies of the package that were found and removed.
    pub removed_duplicates: Vec<String>,
    /// Users for which the package was hidden (`pm uninstall --user`) and is installed again.
    pub unhidden_users: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Options of [`Client::remove_system_app`].
pub struct SystemAppRemoveOptions {
    /// How long to wait for the reboot.
    pub boot_timeout: Duration,
    /// Restore the system copy if the device does not boot.
    pub rollback: bool,
}

impl Default for SystemAppRemoveOptions {
    fn default() -> Self {
        SystemAppRemoveOptions {
            boot_timeout: Duration::from_secs(300),
            rollback: true,
        }
    }
}

/// Where the new system copy goes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    dir: String,
    apk: String,
    partition: &'static str,
    privileged: bool,
}

/// Copies of files saved on the device, to restore them.
struct Backup {
    dir: String,
    paths: Vec<String>,
}

impl Client {
    /// Install an APK as a system app, or update a system app, whatever its current state:
    /// not installed, a system app with or without updates, signed with another key, with a
    /// higher version... Root (a `userdebug` or `eng` build) is needed to replace the system copy.
    ///
    /// To replace the system copy it:
    /// 1. checks the privileged permissions against the privapp-permissions allowlist;
    /// 2. remounts the system partitions read-write (`adb remount`, overlayfs on recent devices);
    /// 3. removes the updates in `/data/app`;
    /// 4. saves and removes the current system copy (also an APK placed directly in `priv-app/`);
    /// 5. writes the new one: the APK, its native libraries (the package manager does not
    ///    extract them for system apps), permissions and SELinux labels;
    /// 6. reboots, and checks that the package manager runs the new copy. If it runs another
    ///    system copy of the package, it removes that one too and reboots again.
    ///
    /// If the device does not boot (or does not run the new copy), the previous system copy
    /// is restored (`rollback`).
    ///
    /// ```no_run
    /// use radb_client::{Client, ConnectionType, SystemAppInstallOptions, SystemAppStrategy};
    ///
    /// let client = Client::try_from(ConnectionType::try_from_ip("192.168.1.114").unwrap()).unwrap();
    /// let options = SystemAppInstallOptions {
    ///     strategy: SystemAppStrategy::ReplaceSystemCopy,
    ///     wipe_data: true,
    ///     ..Default::default()
    /// };
    /// let report = client.install_system_app("app-release.apk", options).unwrap();
    /// println!("{:?}: {}", report.method, report.after.active.code_path);
    /// ```
    pub fn install_system_app<P: AsRef<Path>>(
        &self,
        apk: P,
        options: SystemAppInstallOptions,
    ) -> Result<SystemAppInstallReport> {
        let apk = apk.as_ref();
        let manifest = ApkManifest::read(apk)?;
        let package = manifest.package.as_str();
        let before = match self.pm().system_app_info(package) {
            Ok(info) => Some(info),
            Err(Error::PackageNotFoundError(_)) => None,
            Err(err) => return Err(err),
        };
        info!(
            "{package}: installing version {} ({:?}), current state: {before:?}",
            manifest.version_code, options.strategy
        );

        if choose_method(before.as_ref(), &manifest, options.strategy)
            == SystemAppInstallMethod::Update
        {
            let downgrade = before
                .as_ref()
                .and_then(|b| b.active.version_code)
                .is_some_and(|version| version > manifest.version_code);
            let install = AdbInstallOptions {
                replace: true,
                allow_version_downgrade: downgrade,
                ..Default::default()
            };
            match self.install(apk, Some(install)) {
                Ok(()) => return self.finish_update(&manifest, before, &options),
                Err(err) if is_incompatible_update(&err) => {
                    info!(
                        "{package}: cannot install as an update ({err}), replacing the system copy"
                    )
                }
                Err(err) => return Err(err),
            }
        }
        self.replace_system_copy(apk, &manifest, before, &options)
    }

    /// Remove a system app completely: its updates, all its system copies, its data, and
    /// the allowlist written by [`Client::install_system_app`]. Reboots the device.
    /// A regular (not system) app is uninstalled, without reboot.
    pub fn remove_system_app(&self, package: &str, options: SystemAppRemoveOptions) -> Result<()> {
        let info = self.pm().system_app_info(package)?;
        let Some(system_copy) = info.system_copy.clone() else {
            return self.pm().uninstall(package, None);
        };
        self.root()?;
        self.pm().uninstall_system_updates(package)?;
        self.remount_system(options.boot_timeout)?;

        let mut removed = BTreeSet::new();
        let mut path = system_copy.code_path;
        for _ in 0..MAX_DUPLICATES {
            let partition = partition_of(&path)?;
            let mut paths = vec![path.clone()];
            let allowlist = allowlist_path(partition, package);
            if self.shell().exists(&allowlist)? {
                paths.push(allowlist);
            }
            let backup = self.backup(package, &paths)?;
            for path in &paths {
                self.remove_system_path(path)?;
            }
            removed.insert(path.clone());

            let result = self.reboot_and_wait(options.boot_timeout).and_then(|_| {
                self.root()?;
                match self.pm().system_app_info(package) {
                    Err(Error::PackageNotFoundError(_)) => Ok(None),
                    Ok(info) => Ok(info.system_copy.map(|copy| copy.code_path)),
                    Err(err) => Err(err),
                }
            });
            match result {
                Ok(None) => {
                    self.discard_backup(&backup);
                    info!("{package}: removed {removed:?}");
                    return Ok(());
                }
                // another system copy of the package: remove it too
                Ok(Some(other)) if !removed.contains(&other) => {
                    info!("{package}: the package manager now runs {other}, removing it too");
                    self.discard_backup(&backup);
                    self.remount_system(options.boot_timeout)?;
                    path = other;
                }
                Ok(Some(other)) => {
                    return Err(Error::SystemAppError(format!(
                        "{package} is still installed: {other}"
                    )));
                }
                Err(err) if options.rollback => {
                    let restore = self.rollback(&backup, &[], options.boot_timeout);
                    return Err(Error::SystemAppError(format!(
                        "removing {package} failed: {err}; {restore}"
                    )));
                }
                Err(err) => return Err(err),
            }
        }
        Err(Error::SystemAppError(format!(
            "{package}: more than {MAX_DUPLICATES} system copies ({removed:?})"
        )))
    }

    fn finish_update(
        &self,
        manifest: &ApkManifest,
        before: Option<SystemAppInfo>,
        options: &SystemAppInstallOptions,
    ) -> Result<SystemAppInstallReport> {
        let package = &manifest.package;
        let mut after = self.pm().system_app_info(package)?;
        if after.active.version_code != Some(manifest.version_code) {
            return Err(Error::SystemAppError(format!(
                "{package}: installed, but the package manager runs version {:?}",
                after.active.version_code
            )));
        }
        let unhidden_users = self.unhide(&after)?;
        if options.wipe_data {
            self.pm().clear(package, None)?;
        }
        if !unhidden_users.is_empty() {
            after = self.pm().system_app_info(package)?;
        }
        Ok(SystemAppInstallReport {
            method: SystemAppInstallMethod::Update,
            before,
            after,
            allowlisted_permissions: vec![],
            removed_duplicates: vec![],
            unhidden_users,
        })
    }

    fn replace_system_copy(
        &self,
        apk: &Path,
        manifest: &ApkManifest,
        before: Option<SystemAppInfo>,
        options: &SystemAppInstallOptions,
    ) -> Result<SystemAppInstallReport> {
        let package = manifest.package.as_str();
        let target = target(before.as_ref(), apk, options.target_dir.as_deref())?;
        info!("{package}: new system copy in {}", target.apk);
        self.root()?;

        // 1. privileged permissions, before changing anything
        let allowlist = allowlist_path(target.partition, package);
        let mut allowlisted = vec![];
        if target.privileged && options.privapp_permissions != PrivappPermissionsPolicy::Ignore {
            let missing = self.missing_privapp_permissions(manifest, target.partition)?;
            if !missing.is_empty() {
                if options.privapp_permissions == PrivappPermissionsPolicy::Fail {
                    return Err(Error::PrivappPermissionsError {
                        package: package.to_string(),
                        permissions: missing,
                    });
                }
                allowlisted = missing;
            }
        }

        // 2. writable system partitions
        self.remount_system(options.boot_timeout)?;

        // 3. the updates, or the regular app with the same package
        match &before {
            // `pm uninstall-system-updates` resets the data when the system copy is older
            // than the update. Without the update files, the package manager reverts to the
            // (new) system copy at the next boot and keeps the data.
            Some(SystemAppInfo {
                update: Some(update),
                ..
            }) => self.remove_update_files(&update.code_path)?,
            Some(info) if !info.is_system_app() => {
                let keep = UninstallOptions {
                    keep_data: !options.wipe_data,
                    ..Default::default()
                };
                self.pm().uninstall(package, Some(keep))?
            }
            _ => {}
        }

        // 4. save and remove the old system copy
        let mut paths: Vec<String> = before
            .as_ref()
            .and_then(|info| info.system_copy.as_ref())
            .map(|copy| copy.code_path.clone())
            .into_iter()
            .collect();
        for path in [target.dir.clone(), allowlist.clone()] {
            if !paths.contains(&path) && self.shell().exists(&path)? {
                paths.push(path);
            }
        }
        // the permissions that a previous installation allowlisted
        if !allowlisted.is_empty() && paths.contains(&allowlist) {
            let previous = String::from_utf8_lossy(&self.shell().cat(&allowlist)?).into_owned();
            allowlisted.extend(parse_privapp_allowlist(&previous, package));
            allowlisted.sort();
            allowlisted.dedup();
        }
        let mut backup = self.backup(package, &paths)?;
        for path in &paths {
            self.remove_system_path(path)?;
        }

        // 5. the new system copy, 6. reboot and check
        let allowlist_file = (!allowlisted.is_empty()).then_some(allowlist.as_str());
        let mut removed_duplicates = vec![];
        let result = self
            .write_system_copy(apk, manifest, &target, &allowlisted, allowlist_file)
            .and_then(|_| {
                self.reboot_and_check(
                    manifest,
                    &target,
                    options,
                    &mut backup,
                    &mut removed_duplicates,
                )
            });

        let mut after = match result {
            Ok(after) => after,
            Err(err) if options.rollback => {
                let created: Vec<&str> = [Some(target.dir.as_str()), allowlist_file]
                    .into_iter()
                    .flatten()
                    .collect();
                let restore = self.rollback(&backup, &created, options.boot_timeout);
                return Err(Error::SystemAppError(format!(
                    "installing {package} failed: {err}; {restore}"
                )));
            }
            Err(err) => return Err(err),
        };
        self.discard_backup(&backup);

        let unhidden_users = self.unhide(&after)?;
        if options.wipe_data {
            self.pm().clear(package, None)?;
        }
        if !unhidden_users.is_empty() {
            after = self.pm().system_app_info(package)?;
        }
        Ok(SystemAppInstallReport {
            method: SystemAppInstallMethod::SystemCopy,
            before,
            after,
            allowlisted_permissions: allowlisted,
            removed_duplicates,
            unhidden_users,
        })
    }

    /// Reboot, and check that the package manager runs the new system copy. If it runs
    /// another system copy of the package (a duplicate), remove it and reboot again.
    fn reboot_and_check(
        &self,
        manifest: &ApkManifest,
        target: &Target,
        options: &SystemAppInstallOptions,
        backup: &mut Backup,
        removed_duplicates: &mut Vec<String>,
    ) -> Result<SystemAppInfo> {
        let package = &manifest.package;
        for _ in 0..=MAX_DUPLICATES {
            self.reboot_and_wait(options.boot_timeout)?;
            self.root()?;
            let after = self.pm().system_app_info(package).map_err(|err| {
                Error::SystemAppError(format!(
                    "after the reboot the package manager does not know {package} ({err}): \
                     it rejected the APK, see `adb logcat -d | grep PackageManager`"
                ))
            })?;
            let copy = after.system_copy.as_ref().ok_or_else(|| {
                Error::SystemAppError(format!("{package} is not a system app after the reboot"))
            })?;

            if copy.code_path.trim_end_matches('/') == target.dir {
                if copy.version_code != Some(manifest.version_code) || after.update.is_some() {
                    return Err(Error::SystemAppError(format!(
                        "{package}: the package manager runs version {:?} instead of {}",
                        after.active.version_code, manifest.version_code
                    )));
                }
                return Ok(after);
            }
            if removed_duplicates.contains(&copy.code_path)
                || partition_of(&copy.code_path).is_err()
            {
                break;
            }
            // a duplicate system copy, e.g. a forgotten APK in another directory
            warn!(
                "{package}: the package manager runs {} instead of {}: removing it",
                copy.code_path, target.dir
            );
            self.remount_system(options.boot_timeout)?;
            let saved = self.backup_more(backup, &copy.code_path)?;
            self.remove_system_path(&saved)?;
            removed_duplicates.push(copy.code_path.clone());
        }
        let active = self
            .pm()
            .system_app_info(package)
            .map(|i| i.active.code_path)
            .unwrap_or_default();
        Err(Error::SystemAppError(format!(
            "{package}: the package manager runs {active} instead of {}",
            target.dir
        )))
    }

    /// Write the APK, its native libraries and the allowlist, with their permissions and labels.
    fn write_system_copy(
        &self,
        apk: &Path,
        manifest: &ApkManifest,
        target: &Target,
        allowlisted: &[String],
        allowlist_file: Option<&str>,
    ) -> Result<()> {
        let shell = self.shell();
        let local = std::env::temp_dir().join(format!("radb-system-app-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&local)?;
        let result = (|| -> Result<()> {
            shell.exec(["mkdir", "-p", &target.dir])?;
            self.push(apk, &target.apk)?;

            // the package manager does not extract the native libraries of system apps
            let abis: Vec<String> = shell
                .getprop("ro.product.cpu.abilist")?
                .split(',')
                .map(|abi| abi.trim().to_string())
                .filter(|abi| !abi.is_empty())
                .collect();
            for lib in native_libraries(apk, &abis)? {
                let Some(isa) = instruction_set(&lib.abi) else {
                    continue;
                };
                let dir = format!("{}/lib/{isa}", target.dir);
                let file = local.join(format!("{isa}-{}", lib.file_name));
                std::fs::write(&file, &lib.data)?;
                shell.exec(["mkdir", "-p", &dir])?;
                self.push(&file, format!("{dir}/{}", lib.file_name))?;
            }

            if let Some(allowlist_file) = allowlist_file {
                let file = local.join("allowlist.xml");
                std::fs::write(&file, privapp_allowlist_xml(&manifest.package, allowlisted))?;
                self.push(&file, allowlist_file)?;
                shell.exec(["chown", "root:root", allowlist_file])?;
                shell.exec(["chmod", "644", allowlist_file])?;
            }

            let dir = shell_quote(&target.dir);
            shell.exec_script(&format!(
                "chown -R root:root {dir} && find {dir} -type d -exec chmod 755 {{}} + && find {dir} -type f -exec chmod 644 {{}} +"
            ))?;
            // overlayfs already labels the files: restorecon is for in-place remounts
            if let Err(err) = shell.exec(["restorecon", "-R", &target.dir]) {
                debug!("restorecon: {err}");
            }
            self.invalidate_package_cache(&target.dir)
        })();
        let _ = std::fs::remove_dir_all(&local);
        result
    }

    /// `adb remount`, then check that the system partition is writable. When the first
    /// remount needs a reboot (verity disabled, overlayfs created), reboot and remount again.
    fn remount_system(&self, boot_timeout: Duration) -> Result<()> {
        for attempt in 0..2 {
            let output = run_unchecked(self.command().arg("remount"))?;
            if !output.status.success() {
                return Err(
                    CommandFailure::new(format!("adb {} remount", self.addr), &output).into(),
                );
            }
            if self
                .shell()
                .exec_script("touch /system/.radb_probe && rm /system/.radb_probe")
                .is_ok()
            {
                return Ok(());
            }
            let text = String::from_utf8_lossy(&output.stdout).to_string()
                + &String::from_utf8_lossy(&output.stderr);
            if attempt == 0 && text.to_lowercase().contains("reboot") {
                info!("{}: remount needs a reboot", self.addr);
                self.reboot_and_wait(boot_timeout)?;
                self.root()?;
            }
        }
        Err(Error::SystemAppError(format!(
            "{}: the system partition is not writable after `adb remount`",
            self.addr
        )))
    }

    /// The privileged permissions requested by the APK that the allowlist does not grant
    /// (only with `ro.control_privapp_permissions=enforce`).
    fn missing_privapp_permissions(
        &self,
        manifest: &ApkManifest,
        partition: &str,
    ) -> Result<Vec<String>> {
        let shell = self.shell();
        if shell.getprop("ro.control_privapp_permissions")? != "enforce"
            || manifest.permissions.is_empty()
        {
            return Ok(vec![]);
        }
        // Not `pm list permissions -f`: on Android 14 it can crash halfway (while loading a
        // label) and print a partial list, which would hide privileged permissions.
        let privileged = parse_privileged_permissions(&shell.exec_string([
            "dumpsys",
            "package",
            "permissions",
        ])?);
        let requested: Vec<&String> = manifest
            .permissions
            .iter()
            .filter(|p| privileged.contains(*p))
            .collect();
        if requested.is_empty() {
            return Ok(vec![]);
        }
        let dir = shell_quote(partition);
        let xml = shell.exec_script(&format!(
            "cat {dir}/etc/permissions/*.xml {dir}/etc/sysconfig/*.xml 2>/dev/null; true"
        ))?;
        let allowed =
            parse_privapp_allowlist(&String::from_utf8_lossy(&xml.stdout), &manifest.package);
        Ok(requested
            .into_iter()
            .filter(|p| !allowed.contains(*p))
            .cloned()
            .collect())
    }

    /// `pm install-existing` for the users that hide the package.
    fn unhide(&self, info: &SystemAppInfo) -> Result<Vec<u32>> {
        let users = info.hidden_for_users();
        for user in &users {
            info!(
                "{}: installing for user {user}, where it was hidden",
                info.package_name
            );
            self.shell().exec([
                "pm",
                "install-existing",
                "--user",
                &user.to_string(),
                &info.package_name,
            ])?;
        }
        Ok(users)
    }

    /// Remove the files of an update (`/data/app/~~<id>/<package>-<id>`).
    fn remove_update_files(&self, code_path: &str) -> Result<()> {
        let path = code_path.trim_end_matches('/');
        if !path.starts_with("/data/app/") {
            return Err(Error::SystemAppError(format!(
                "unexpected update path {path}"
            )));
        }
        // the random `~~<id>` directory only contains this package
        let dir = match path.rsplit_once('/') {
            Some((parent, _))
                if parent
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| name.starts_with("~~")) =>
            {
                parent
            }
            _ => path,
        };
        info!("{}: removing the update {dir}", self.addr);
        self.shell().exec(["rm", "-rf", dir]).map(|_| ())
    }

    /// Remove a file or directory of a system partition. For an APK placed directly in
    /// `priv-app/` or `app/`, also its compiled code in `oat/`.
    fn remove_system_path(&self, path: &str) -> Result<()> {
        self.shell().exec(["rm", "-rf", path])?;
        if let Some((parent, file)) = path.rsplit_once('/') {
            if let Some(stem) = file.strip_suffix(".apk") {
                let oat = shell_quote(parent);
                let stem = shell_quote(stem);
                self.shell().exec_script(&format!(
                    "rm -f {oat}/oat/*/{stem}.odex {oat}/oat/*/{stem}.vdex {oat}/oat/*/{stem}.art"
                ))?;
            }
        }
        self.invalidate_package_cache(path)
    }

    /// The package manager caches the parsed packages (`/data/system/package_cache`), and
    /// uses a cache entry while it is newer than the APK (or its directory). Copied or
    /// restored files keep their old modification time, so the package manager would keep
    /// using the previous APK: remove the entries of `path`, and touch it.
    fn invalidate_package_cache(&self, path: &str) -> Result<()> {
        let path = path.trim_end_matches('/');
        let name = path.rsplit('/').next().unwrap_or_default();
        if name.is_empty() {
            return Ok(());
        }
        // the entries are named `<file name>-<flags>-<hash>`
        let quoted = shell_quote(path);
        self.shell()
            .exec_script(&format!(
                "rm -f /data/system/package_cache/*/{}-*; if [ -e {quoted} ]; then find {quoted} -exec touch {{}} +; fi",
                shell_quote(name)
            ))
            .map(|_| ())
    }

    fn backup(&self, package: &str, paths: &[String]) -> Result<Backup> {
        let mut backup = Backup {
            dir: format!("{BACKUP_DIR}/{package}"),
            paths: vec![],
        };
        self.shell().exec(["rm", "-rf", &backup.dir])?;
        self.shell().exec(["mkdir", "-p", &backup.dir])?;
        for path in paths {
            self.backup_more(&mut backup, path)?;
        }
        Ok(backup)
    }

    fn backup_more(&self, backup: &mut Backup, path: &str) -> Result<String> {
        let copy = format!("{}/{}", backup.dir, backup.paths.len());
        self.shell().exec(["cp", "-a", path, &copy])?;
        backup.paths.push(path.to_string());
        Ok(path.to_string())
    }

    fn discard_backup(&self, backup: &Backup) {
        if let Err(err) = self.shell().exec(["rm", "-rf", &backup.dir]) {
            warn!("failed to remove {}: {err}", backup.dir);
        }
    }

    /// Remove the new files, restore the backup and reboot. Works in a boot loop too:
    /// adbd runs before the system server. Returns what happened.
    fn rollback(&self, backup: &Backup, created: &[&str], boot_timeout: Duration) -> String {
        warn!("{}: rolling back from {}", self.addr, backup.dir);
        let result = (|| -> Result<()> {
            let _ = run_unchecked(
                self.command()
                    .arg("wait-for-device")
                    .timeout(Some(Duration::from_secs(60))),
            );
            self.root()?;
            self.remount_system(boot_timeout)?;
            let shell = self.shell();
            for path in created {
                shell.exec(["rm", "-rf", path])?;
            }
            for (index, path) in backup.paths.iter().enumerate() {
                shell.exec(["rm", "-rf", path])?;
                shell.exec(["cp", "-a", &format!("{}/{index}", backup.dir), path])?;
                self.invalidate_package_cache(path)?;
            }
            self.reboot_and_wait(boot_timeout)?;
            // adbd is not root after a reboot, and the backup belongs to root
            self.root()?;
            self.discard_backup(backup);
            Ok(())
        })();
        match result {
            Ok(()) => "the previous system copy was restored".to_string(),
            Err(err) => format!(
                "the rollback failed too ({err}): the previous files are in {} on the device ({:?})",
                backup.dir, backup.paths
            ),
        }
    }
}

fn choose_method(
    before: Option<&SystemAppInfo>,
    manifest: &ApkManifest,
    strategy: SystemAppStrategy,
) -> SystemAppInstallMethod {
    let system_version = before
        .and_then(|info| info.system_copy.as_ref())
        .and_then(|copy| copy.version_code);
    match (strategy, system_version) {
        (SystemAppStrategy::Auto, Some(version)) if manifest.version_code >= version => {
            SystemAppInstallMethod::Update
        }
        _ => SystemAppInstallMethod::SystemCopy,
    }
}

fn is_incompatible_update(err: &Error) -> bool {
    match err {
        Error::CommandFailed(failure) => {
            let text = format!("{} {}", failure.stdout, failure.stderr);
            INCOMPATIBLE_UPDATE
                .iter()
                .any(|marker| text.contains(marker))
        }
        _ => false,
    }
}

fn target(before: Option<&SystemAppInfo>, apk: &Path, target_dir: Option<&str>) -> Result<Target> {
    let dir = match (
        target_dir,
        before.and_then(|info| info.system_copy.as_ref()),
    ) {
        (Some(dir), _) => dir.trim_end_matches('/').to_string(),
        // an APK placed directly in priv-app/ moves to its own directory
        (None, Some(copy)) => match copy.code_path.strip_suffix(".apk") {
            Some(stem) => stem.to_string(),
            None => copy.code_path.trim_end_matches('/').to_string(),
        },
        (None, None) => {
            let name: String = apk
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || "._-".contains(c) {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("{DEFAULT_SYSTEM_APP_DIR}/{name}")
        }
    };
    let (parent, name) = dir
        .rsplit_once('/')
        .filter(|(parent, name)| {
            !name.is_empty() && (parent.ends_with("/priv-app") || parent.ends_with("/app"))
        })
        .ok_or_else(|| {
            Error::SystemAppError(format!(
                "{dir}: the system copy must be in a priv-app/ or app/ directory"
            ))
        })?;
    Ok(Target {
        apk: format!("{dir}/{name}.apk"),
        partition: partition_of(&dir)?,
        privileged: parent.ends_with("/priv-app"),
        dir,
    })
}

fn partition_of(path: &str) -> Result<&'static str> {
    SYSTEM_PARTITIONS
        .iter()
        .find(|partition| path.starts_with(&format!("{partition}/")))
        .copied()
        .ok_or_else(|| Error::SystemAppError(format!("{path} is not on a system partition")))
}

fn allowlist_path(partition: &str, package: &str) -> String {
    format!("{partition}/etc/permissions/privapp-permissions-radb-{package}.xml")
}

/// The `lib/` directory of an ABI.
fn instruction_set(abi: &str) -> Option<&'static str> {
    match abi {
        "arm64-v8a" => Some("arm64"),
        "armeabi-v7a" | "armeabi" => Some("arm"),
        "x86_64" => Some("x86_64"),
        "x86" => Some("x86"),
        "riscv64" => Some("riscv64"),
        _ => None,
    }
}

/// The privileged permissions defined by the platform, from `dumpsys package permissions`:
///
/// ```text
///   Permission [android.permission.INSTALL_PACKAGES] (a936ec1):
///     sourcePackage=android
///     uid=1000 gids=[] type=0 prot=signature|privileged
/// ```
///
/// Only the permissions of the platform (`sourcePackage=android`) are returned: Android
/// enforces the allowlist for them only. A privileged permission defined by an app (e.g.
/// `com.netflix.ninja.permission.DET`) is granted or not, but never stops the boot.
fn parse_privileged_permissions(output: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut current: Option<&str> = None;
    let mut platform = false;
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Permission [") {
            current = rest.split_once(']').map(|(name, _)| name);
            platform = false;
        } else if let Some(name) = current {
            if let Some(source) = line.strip_prefix("sourcePackage=") {
                platform = source.trim() == "android";
            }
            let level = line
                .split_whitespace()
                .find_map(|token| token.strip_prefix("prot="));
            if let Some(level) = level {
                // `signatureOrSystem` is the name of `signature|privileged` before Android 5
                let privileged = level
                    .split('|')
                    .any(|l| l == "privileged" || l == "signatureOrSystem");
                if privileged && platform {
                    result.insert(name.to_string());
                }
                current = None;
            }
        }
    }
    result
}

/// The permissions granted or denied to `package` by privapp-permissions files.
fn parse_privapp_allowlist(xml: &str, package: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let open = format!("package=\"{package}\"");
    let mut rest = xml;
    while let Some(start) = rest.find("<privapp-permissions") {
        rest = &rest[start..];
        let Some(head_end) = rest.find('>') else {
            break;
        };
        let head = &rest[..head_end];
        let body_end = rest.find("</privapp-permissions>").unwrap_or(rest.len());
        if head.contains(&open) && !head.ends_with('/') {
            let body = &rest[head_end..body_end];
            for tag in body.split('<').skip(1) {
                let tag = tag.trim_start();
                if tag.starts_with("permission") || tag.starts_with("deny-permission") {
                    if let Some(name) = tag
                        .split("name=\"")
                        .nth(1)
                        .and_then(|n| n.split('"').next())
                    {
                        result.insert(name.to_string());
                    }
                }
            }
        }
        rest = &rest[body_end.min(rest.len())..];
        if rest.len() <= 1 {
            break;
        }
        rest = &rest[1..];
    }
    result
}

fn privapp_allowlist_xml(package: &str, permissions: &[String]) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- written by radb_client -->\n<permissions>\n");
    xml.push_str(&format!(
        "    <privapp-permissions package=\"{package}\">\n"
    ));
    for permission in permissions {
        xml.push_str(&format!("        <permission name=\"{permission}\"/>\n"));
    }
    xml.push_str("    </privapp-permissions>\n</permissions>\n");
    xml
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::pm::PackageCopy;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test_files/fixtures/api35");

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{FIXTURES}/{name}")).unwrap()
    }

    fn copy(path: &str, version: i64) -> PackageCopy {
        PackageCopy {
            code_path: path.to_string(),
            version_code: Some(version),
            version_name: None,
            signatures: vec![],
        }
    }

    fn info(system: Option<PackageCopy>, update: Option<PackageCopy>) -> SystemAppInfo {
        SystemAppInfo {
            package_name: "com.example".to_string(),
            active: update.clone().or(system.clone()).unwrap(),
            system_copy: system,
            update,
            privileged: true,
            users: vec![(0, true)],
        }
    }

    fn manifest(version: i64) -> ApkManifest {
        ApkManifest {
            package: "com.example".to_string(),
            version_code: version,
            ..Default::default()
        }
    }

    #[test]
    fn test_choose_method() {
        use SystemAppInstallMethod::*;
        use SystemAppStrategy::*;
        let system = info(Some(copy("/system/priv-app/Example", 5)), None);
        assert_eq!(choose_method(Some(&system), &manifest(6), Auto), Update);
        assert_eq!(choose_method(Some(&system), &manifest(5), Auto), Update);
        // lower than the system copy: an update would be deleted at the next boot
        assert_eq!(choose_method(Some(&system), &manifest(4), Auto), SystemCopy);
        assert_eq!(
            choose_method(Some(&system), &manifest(6), ReplaceSystemCopy),
            SystemCopy
        );
        // not installed, or not a system app
        assert_eq!(choose_method(None, &manifest(1), Auto), SystemCopy);
        let regular = SystemAppInfo {
            system_copy: None,
            ..info(Some(copy("/data/app/x", 1)), None)
        };
        assert_eq!(
            choose_method(Some(&regular), &manifest(2), Auto),
            SystemCopy
        );
    }

    #[test]
    fn test_target() {
        let apk = Path::new("/tmp/My App-release.apk");
        let target = target(None, apk, None).unwrap();
        assert_eq!(target.dir, "/system/priv-app/My_App-release");
        assert_eq!(
            target.apk,
            "/system/priv-app/My_App-release/My_App-release.apk"
        );
        assert_eq!((target.partition, target.privileged), ("/system", true));

        // the directory of the current system copy
        let dir = info(Some(copy("/product/app/Example", 1)), None);
        let target = super::target(Some(&dir), apk, None).unwrap();
        assert_eq!(target.apk, "/product/app/Example/Example.apk");
        assert_eq!((target.partition, target.privileged), ("/product", false));

        // an APK placed directly in priv-app/ moves to its own directory
        let flat = info(
            Some(copy("/system/priv-app/standalone-release.apk", 1)),
            None,
        );
        let target = super::target(Some(&flat), apk, None).unwrap();
        assert_eq!(
            target.apk,
            "/system/priv-app/standalone-release/standalone-release.apk"
        );

        let target = super::target(Some(&flat), apk, Some("/system_ext/priv-app/New/")).unwrap();
        assert_eq!(
            (target.dir.as_str(), target.partition),
            ("/system_ext/priv-app/New", "/system_ext")
        );

        assert!(super::target(None, apk, Some("/data/app/New")).is_err());
        assert!(super::target(None, apk, Some("/system/framework/New")).is_err());
    }

    #[test]
    fn test_is_incompatible_update() {
        let failure = |stderr: &str| -> Error {
            CommandFailure {
                command: "adb install".to_string(),
                exit_code: Some(1),
                stdout: "Performing Streamed Install".to_string(),
                stderr: stderr.to_string(),
            }
            .into()
        };
        assert!(is_incompatible_update(&failure(
            "adb: failed to install x.apk: Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Existing package x signatures do not match newer version; ignoring!]"
        )));
        assert!(is_incompatible_update(&failure(
            "Failure [INSTALL_FAILED_VERSION_DOWNGRADE]"
        )));
        assert!(!is_incompatible_update(&failure(
            "Failure [INSTALL_FAILED_INSUFFICIENT_STORAGE]"
        )));
        assert!(!is_incompatible_update(&Error::Timeout("x".to_string())));
    }

    #[test]
    fn test_parse_privileged_permissions() {
        let privileged = parse_privileged_permissions(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/test_files/fixtures/api34/dumpsys_package_permissions.txt"
            ))
            .unwrap(),
        );
        assert!(privileged.contains("android.permission.INSTALL_PACKAGES"));
        assert!(privileged.contains("android.permission.WRITE_SECURE_SETTINGS"));
        assert!(!privileged.contains("android.permission.INTERNET"));
        assert!(!privileged.contains("android.permission.CAMERA"));
        // privileged, but defined by an app: the allowlist is not enforced for it
        assert!(
            !privileged.contains("com.google.android.apps.nexuslauncher.permission.READ_SETTINGS")
        );
    }

    #[test]
    fn test_parse_privapp_allowlist() {
        let xml = fixture("privapp-permissions-platform.xml");
        let shell = parse_privapp_allowlist(&xml, "com.android.shell");
        assert!(
            shell.contains("android.permission.INSTALL_PACKAGES"),
            "{shell:?}"
        );
        assert!(parse_privapp_allowlist(&xml, "com.nonexistent").is_empty());

        let ours = privapp_allowlist_xml("com.example", &["a.B".to_string(), "c.D".to_string()]);
        assert_eq!(
            parse_privapp_allowlist(&ours, "com.example"),
            BTreeSet::from(["a.B".to_string(), "c.D".to_string()])
        );
        let denied = r#"<privapp-permissions package="com.example"><deny-permission name="x.Y" /></privapp-permissions>"#;
        assert!(parse_privapp_allowlist(denied, "com.example").contains("x.Y"));
    }

    #[test]
    fn test_system_app_info() {
        let system = SystemAppInfo::parse(
            "it.sephiroth.radb.sysapp",
            &fixture("dumpsys_package_system_app.txt"),
        )
        .unwrap();
        assert_eq!(system.active.code_path, "/system/priv-app/RadbSysapp");
        assert_eq!(system.system_copy.as_ref().unwrap().version_code, Some(2));
        assert_eq!(system.update, None);
        assert!(system.privileged);
        assert_eq!(system.users, [(0, true)]);

        let updated = SystemAppInfo::parse(
            "it.sephiroth.radb.sysapp",
            &fixture("dumpsys_package_updated_system_app.txt"),
        )
        .unwrap();
        let update = updated.update.as_ref().unwrap();
        assert!(update.code_path.starts_with("/data/app/"));
        assert_eq!(update.version_code, Some(3));
        assert_eq!(updated.active, *update);
        let system_copy = updated.system_copy.as_ref().unwrap();
        assert_eq!(
            (system_copy.code_path.as_str(), system_copy.version_code),
            ("/system/priv-app/RadbSysapp", Some(2))
        );
        assert_eq!(system_copy.signatures, update.signatures);

        let hidden = SystemAppInfo::parse(
            "it.sephiroth.radb.sysapp",
            &fixture("dumpsys_package_hidden_system_app.txt"),
        )
        .unwrap();
        assert_eq!(hidden.hidden_for_users(), [0]);

        let regular = SystemAppInfo::parse(
            "it.sephiroth.android.app.app",
            &fixture("dumpsys_package_app.txt"),
        )
        .unwrap();
        assert!(!regular.is_system_app());
        assert!(!regular.privileged);

        assert!(matches!(
            SystemAppInfo::parse("com.nonexistent", "Dexopt state:\n"),
            Err(Error::PackageNotFoundError(_))
        ));
    }

    #[test]
    fn test_instruction_set() {
        assert_eq!(instruction_set("arm64-v8a"), Some("arm64"));
        assert_eq!(instruction_set("armeabi-v7a"), Some("arm"));
        assert_eq!(instruction_set("mips"), None);
    }
}
