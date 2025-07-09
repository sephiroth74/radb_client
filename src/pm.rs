use std::ffi::OsString;
use std::time::Duration;

use lazy_static::lazy_static;
use regex::Regex;
use rustix::path::Arg;

use crate::dump_util::{package_flags, runtime_permissions};
use crate::error::Error;
use crate::result::Result;
use crate::shell::handle_result;
use crate::types::{
    InstallOptions, InstallPermission, ListPackageDisplayOptions, ListPackageFilter, Package,
    PackageFlags, PackageManager, RuntimePermission, SimplePackageReader, UninstallOptions,
};

static DUMP_TIMEOUT: Option<Duration> = Some(Duration::from_secs(1));

macro_rules! build_pm_operation {
    ($name:tt, $operation_name:tt, $typ:ty, $typ2:ty) => {
        pub fn $name(&self, package_name: $typ, user: $typ2) -> Result<()> {
            self.operation($operation_name, package_name, user)
        }
    };
}

impl<'a> PackageManager<'a> {
    /// Return the path of a given package name.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package to query.
    /// * `user` - Optional user ID as a string slice.
    ///
    /// # Returns
    /// * `Result<String>` - The path to the package, or an error if not found.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let shell = client.shell();
    /// let pm = shell.pm();
    /// let path = pm.path("com.example.app", None).unwrap();
    /// println!("APK path: {}", path);
    /// ```
    pub fn path(&self, package_name: &str, user: Option<&str>) -> Result<String> {
        let mut args = vec!["pm", "path"];
        if let Some(u) = user {
            args.push("--user");
            args.push(u);
        }
        args.push(package_name);
        let result = self.parent.exec(args, None, None)?.stdout;
        let output = Arg::as_str(&result)?.trim();
        let split = output
            .split_once("package:")
            .map(|s| s.1.to_string())
            .ok_or(Error::PackageNotFoundError(package_name.to_string()));
        split
    }

    /// Grant a permission to a given package.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package.
    /// * `permission` - The permission to grant.
    /// * `user` - Optional user ID as a string slice.
    ///
    /// # Returns
    /// * `Result<()>` - Ok if successful, or an error if the command fails.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let shell = client.shell();
    /// let pm = shell.pm();
    /// pm.grant("com.example.app", "android.permission.CAMERA", None).unwrap();
    /// ```
    pub fn grant(&self, package_name: &str, permission: &str, user: Option<&str>) -> Result<()> {
        let mut args = vec!["pm", "grant"];
        if let Some(u) = user {
            args.extend(vec!["--user", u]);
        }
        args.push(package_name);
        args.push(permission);
        handle_result(self.parent.exec(args, None, None)?)
    }

    /// Revoke a permission from a given package.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package.
    /// * `permission` - The permission to revoke.
    /// * `user` - Optional user ID as a string slice.
    ///
    /// # Returns
    /// * `Result<()>` - Ok if successful, or an error if the command fails.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// pm.revoke("com.example.app", "android.permission.CAMERA", None).unwrap();
    /// ```
    pub fn revoke(&self, package_name: &str, permission: &str, user: Option<&str>) -> Result<()> {
        let mut args = vec!["pm", "revoke"];
        if let Some(u) = user {
            args.extend(vec!["--user", u]);
        }
        args.push(package_name);
        args.push(permission);
        handle_result(self.parent.exec(args, None, None)?)
    }

    /// Revert all runtime permissions to their default state.
    ///
    /// # Returns
    /// * `Result<()>` - Ok if successful, or an error if the command fails.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// pm.reset_permissions().unwrap();
    /// ```
    pub fn reset_permissions(&self) -> Result<()> {
        handle_result(
            self.parent
                .exec(vec!["pm", "reset-permissions"], None, None)?,
        )
    }

    /// List installed packages with optional filters and display options.
    ///
    /// # Arguments
    /// * `filters` - Filters for package listing (e.g., only enabled, only system).
    /// * `display` - Display options (e.g., show version code, show UID).
    /// * `name_filter` - Optional filter for package name substring.
    ///
    /// # Returns
    /// * `Result<Vec<Package>>` - A vector of `Package` structs.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// let packages = pm.list_packages(Default::default(), Default::default(), None).unwrap();
    /// for pkg in packages { println!("{}", pkg.package_name); }
    /// ```
    pub fn list_packages(
        &self,
        filters: ListPackageFilter,
        display: ListPackageDisplayOptions,
        name_filter: Option<&str>,
    ) -> Result<Vec<Package>> {
        let mut args = vec!["pm".into(), "list".into(), "packages".into()];

        args.extend(filters);
        args.extend(display);

        if let Some(name) = name_filter {
            args.push(name.into());
        }

        let output = self.parent.exec(args, None, None)?.stdout;
        let string = Arg::as_str(&output)?;

        lazy_static! {
			static ref RE: Regex = Regex::new(
				"package:((?P<file>.*\\.apk)=)?(?P<name>\\S+)(\\s(versionCode|uid):(\\d+))?(\\s(versionCode|uid):(\\d+))?"
			)
			.unwrap();
		}

        let captures = RE.captures_iter(string);
        let result = captures
            .into_iter()
            .filter_map(|m| {
                if m.len() == 10 {
                    let name = m.name("name")?.as_str();
                    let file_name = m.name("file").map(|s| s.as_str().to_string());

                    let (version_code_str, uid_str) = match m.get(5).map(|m| m.as_str()) {
                        Some("versionCode") => {
                            (m.get(6).map(|m| m.as_str()), m.get(9).map(|m| m.as_str()))
                        }
                        Some("uid") => (m.get(9).map(|m| m.as_str()), m.get(6).map(|m| m.as_str())),
                        _ => (None, None),
                    };

                    let version_code = if let Some(vcode) = version_code_str {
                        Some(vcode.parse::<i32>().ok()?)
                    } else {
                        None
                    };

                    let uid = if let Some(uid) = uid_str {
                        Some(uid.parse::<i32>().ok()?)
                    } else {
                        None
                    };

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

    /// Dump information about a package.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package to dump.
    /// * `timeout` - Optional timeout for the operation.
    ///
    /// # Returns
    /// * `Result<String>` - The dump output as a string.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// let dump = pm.dump("com.example.app", None).unwrap();
    /// println!("Dump: {}", dump);
    /// ```
    pub fn dump(&self, package_name: &str, timeout: Option<Duration>) -> Result<String> {
        let args = vec!["pm", "dump", package_name.into()];
        let result = self.parent.exec(args, None, timeout)?.stdout;
        Ok(Arg::as_str(&result)?.to_string())
    }

    /// Get requested runtime permissions for a package.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package.
    ///
    /// # Returns
    /// * `Result<Vec<RuntimePermission>>` - A vector of runtime permissions.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// let perms = pm.runtime_permissions("com.example.app").unwrap();
    /// for perm in perms { println!("{}", perm.name); }
    /// ```
    pub fn runtime_permissions(&self, package_name: &str) -> Result<Vec<RuntimePermission>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        runtime_permissions(dump.as_str())
    }

    /// Get the install permissions for a package.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package.
    ///
    /// # Returns
    /// * `Result<Vec<InstallPermission>>` - A vector of install permissions.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// let perms = pm.install_permissions("com.example.app").unwrap();
    /// for perm in perms { println!("{}", perm.name); }
    /// ```
    pub fn install_permissions(&self, package_name: &str) -> Result<Vec<InstallPermission>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        let sdk_int = self.parent.build_version_sdk()?;
        SimplePackageReader::new(dump.as_str(), sdk_int)
            .and_then(|pr| Ok(pr.install_permissions().unwrap_or(vec![])))
    }

    /// Get the requested permissions installed for a package.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package.
    ///
    /// # Returns
    /// * `Result<Vec<String>>` - A vector of permission names.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// let perms = pm.requested_permissions("com.example.app").unwrap();
    /// for perm in perms { println!("{}", perm); }
    /// ```
    pub fn requested_permissions(&self, package_name: &str) -> Result<Vec<String>> {
        let dump = self.dump(package_name, DUMP_TIMEOUT)?;
        let sdk_int = self.parent.build_version_sdk()?;
        SimplePackageReader::new(dump.as_str(), sdk_int)
            .and_then(|pr| Ok(pr.requested_permissions().unwrap_or(vec![])))
    }

    /// Get the package flags for a package.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package.
    ///
    /// # Returns
    /// * `Result<Vec<PackageFlags>>` - A vector of package flags.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// let flags = pm.package_flags("com.example.app").unwrap();
    /// for flag in flags { println!("{}", flag); }
    /// ```
    pub fn package_flags(&self, package_name: &str) -> Result<Vec<PackageFlags>> {
        let result = self.dump(package_name, DUMP_TIMEOUT)?;
        package_flags(result.as_str())
    }

    /// Returns true if the package is a system package.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package.
    ///
    /// # Returns
    /// * `Result<bool>` - True if the package is a system package.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// let is_system = pm.is_system("com.example.app").unwrap();
    /// println!("Is system: {}", is_system);
    /// ```
    pub fn is_system(&self, package_name: &str) -> Result<bool> {
        Ok(self
            .package_flags(package_name)?
            .contains(&PackageFlags::System))
    }

    /// Returns true if the package is installed for the given user.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package.
    /// * `user` - Optional user ID as a string slice.
    ///
    /// # Returns
    /// * `Result<bool>` - True if the package is installed.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// let installed = pm.is_installed("com.example.app", None).unwrap();
    /// println!("Installed: {}", installed);
    /// ```
    pub fn is_installed(&self, package_name: &str, user: Option<&str>) -> Result<bool> {
        let r = self.path(package_name, user).map(|f| f.len() > 0);
        match r {
            Ok(r) => Ok(r),
            Err(err) => match err {
                Error::PackageNotFoundError(_) => Ok(false),
                Error::CommandError(simple_cmd::Error::CommandError(err)) => {
                    if err.stderr.is_empty() && err.stdout.is_empty() {
                        Ok(false)
                    } else {
                        Err(Error::CommandError(err.into()))
                    }
                }
                _ => Err(err),
            },
        }
    }

    /// Uninstall a package from the device.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package to uninstall.
    /// * `options` - Optional uninstall options.
    ///
    /// # Returns
    /// * `Result<()>` - Ok if successful, or an error if the command fails.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// pm.uninstall("com.example.app", None).unwrap();
    /// ```
    pub fn uninstall(&self, package_name: &str, options: Option<UninstallOptions>) -> Result<()> {
        let mut args: Vec<OsString> = vec!["cmd".into(), "package".into(), "uninstall".into()];
        match options {
            None => {}
            Some(options) => args.extend(options.into_iter()),
        }
        args.push(package_name.into());
        handle_result(self.parent.exec(args, None, None)?)
    }

    /// Install a package from the given source path.
    ///
    /// # Arguments
    /// * `src` - The source path of the APK file.
    /// * `options` - Optional install options.
    ///
    /// # Returns
    /// * `Result<()>` - Ok if successful, or an error if the command fails.
    ///
    /// # Example
    /// ```rust
    /// use radb_client::types::{Client, ConnectionType};
    /// let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
    /// let pm = client.shell().pm();
    /// pm.install("/data/local/tmp/app.apk", None).unwrap();
    /// ```
    pub fn install<T: Arg>(&self, src: T, options: Option<InstallOptions>) -> Result<()> {
        let mut args: Vec<OsString> = vec!["cmd".into(), "package".into(), "install".into()];
        match options {
            None => {}
            Some(options) => args.extend(options),
        }
        args.push(src.as_str()?.into());
        handle_result(self.parent.exec(args, None, None)?)
    }

    build_pm_operation!(clear, "clear", &str, Option<&str>);

    build_pm_operation!(suspend, "suspend", &str, Option<&str>);

    build_pm_operation!(unsuspend, "unsuspend", &str, Option<&str>);

    build_pm_operation!(hide, "hide", &str, Option<&str>);

    build_pm_operation!(unhide, "unhide", &str, Option<&str>);

    build_pm_operation!(default_state, "default-state", &str, Option<&str>);

    build_pm_operation!(disable_until_used, "disable-until-used", &str, Option<&str>);

    build_pm_operation!(disable_user, "disable-user", &str, Option<&str>);

    build_pm_operation!(disable, "disable", &str, Option<&str>);

    build_pm_operation!(enable, "enable", &str, Option<&str>);

    pub(crate) fn operation(
        &self,
        operation: &str,
        package_or_component: &str,
        user: Option<&str>,
    ) -> Result<()> {
        let mut args = vec!["pm", operation];
        if let Some(u) = user {
            args.extend(vec!["--user", u]);
        }
        args.push(package_or_component);
        handle_result(self.parent.exec(args, None, None)?)
    }
}

#[cfg(test)]
mod test {
    use itertools::Itertools;

    use crate::test::test::*;
    use crate::types::{
        InstallLocationOption, InstallOptions, ListPackageDisplayOptions, ListPackageFilter,
        SimplePackageReader,
    };

    #[test]
    fn test_path() {
        init_log();
        let client = connect_emulator();
        let path = client
            .shell()
            .pm()
            .path("com.android.bluetooth", None)
            .expect("failed to get package path");
        println!("path: {path}");
        assert!(!path.is_empty());
    }

    #[test]
    fn test_revoke() {
        init_log();
        let client = connect_tcp_ip_client();
        client
            .shell()
            .pm()
            .revoke(
                "com.swisscom.aot.library.standalone",
                "android.permission.BLUETOOTH_SCAN",
                None,
            )
            .expect("failed to revoke permission");
    }

    #[test]
    fn test_grant() {
        init_log();
        let client = connect_tcp_ip_client();
        client
            .shell()
            .pm()
            .grant(
                "com.swisscom.aot.library.standalone",
                "android.permission.BLUETOOTH_SCAN",
                None,
            )
            .expect("failed to grant permission");
    }

    #[test]
    fn test_enable_package() {
        init_log();
        let client = connect_tcp_ip_client();
        root_client(&client);

        let package_name = "com.android.bluetooth";
        let user_id = Some("0");

        client
            .shell()
            .pm()
            .clear(package_name, user_id)
            .expect("failed to clear package");
        client
            .shell()
            .pm()
            .suspend(package_name, user_id)
            .expect("failed to suspend package");
        client
            .shell()
            .pm()
            .unsuspend(package_name, user_id)
            .expect("failed to unsuspend package");
        client
            .shell()
            .pm()
            .hide(package_name, user_id)
            .expect("failed to hide package");
        client
            .shell()
            .pm()
            .unhide(package_name, user_id)
            .expect("failed to unhide package");
        client
            .shell()
            .pm()
            .default_state(package_name, user_id)
            .expect("failed to set default-state for package");
        client
            .shell()
            .pm()
            .disable_until_used(package_name, user_id)
            .expect("failed to set default-state for package");
        client
            .shell()
            .pm()
            .disable_user(package_name, user_id)
            .expect("failed to disable user for package");
        client
            .shell()
            .pm()
            .enable(package_name, user_id)
            .expect("failed to enable package");
        client
            .shell()
            .pm()
            .disable(package_name, user_id)
            .expect("failed to disable package");
        client
            .shell()
            .pm()
            .enable(package_name, user_id)
            .expect("failed to enable package");
    }

    #[test]
    fn test_reset_permissions() {
        init_log();
        let client = connect_emulator();
        client
            .shell()
            .pm()
            .reset_permissions()
            .expect("failed to reset permissions");
    }

    #[test]
    fn test_list_packages() {
        init_log();
        let client = connect_emulator();
        let packages = client
            .shell()
            .pm()
            .list_packages(
                ListPackageFilter {
                    show_only_disabled: false,
                    show_only_enabed: true,
                    show_only_system: false,
                    show_only3rd_party: true,
                    apex_only: false,
                    uid: None,
                    user: None,
                },
                ListPackageDisplayOptions {
                    show_uid: true,
                    show_version_code: true,
                    include_uninstalled: true,
                    show_apk_file: true,
                },
                None,
            )
            .expect("failed to list packages");

        assert!(!packages.is_empty());

        for package in packages {
            println!("package: {package}");
        }
    }

    #[test]
    fn test_dump() {
        init_log();
        let client = connect_emulator();
        let sdk_int = client.shell().build_version_sdk().unwrap();
        let dump = client
            .shell()
            .pm()
            .dump("com.android.bluetooth", None)
            .expect("failed to dump package");
        assert!(!&dump.is_empty());
        //println!("dump: {dump}");

        let reader = SimplePackageReader::new(dump.as_str(), sdk_int).unwrap();
        let is_system = reader.is_system();
        let code_path = reader.get_code_path();
        let resource_path = reader.get_resource_path();
        let data_dir = reader.get_data_dir();
        let version_code = reader.get_version_code();
        println!("is system: {is_system:?}");
        println!("code path: {code_path:?}");
        println!("resource path: {resource_path:?}");
        println!("data dir: {data_dir:?}");
        println!("version code: {version_code:?}");

        let package_path = reader.dexopt.get_package_path("com.android.bluetooth");
        println!("package path: {package_path:?}");
    }

    #[test]
    fn test_dump_runtime_permissions() {
        init_log();
        let client = connect_tcp_ip_client();
        let permissions = client
            .shell()
            .pm()
            .runtime_permissions("com.swisscom.aot.library.standalone")
            .expect("failed to get runtime permissions");

        assert!(!permissions.is_empty());

        for permission in permissions {
            println!("permission: {permission}");
        }
    }

    #[test]
    fn test_install_permissions() {
        init_log();
        let client = connect_tcp_ip_client();
        let permissions = client
            .shell()
            .pm()
            .install_permissions("com.swisscom.aot.library.standalone")
            .expect("failed to get install permissions");

        assert!(!permissions.is_empty());

        for permission in permissions {
            println!("permission: {permission}");
        }
    }

    #[test]
    fn test_requested_permissions() {
        init_log();
        let client = connect_tcp_ip_client();
        let permissions = client
            .shell()
            .pm()
            .requested_permissions("com.swisscom.aot.library.standalone")
            .expect("failed to get requested permissions");

        assert!(!permissions.is_empty());

        for permission in permissions {
            println!("permission: {permission}");
        }
    }

    #[test]
    fn test_package_flags() {
        init_log();
        let client = connect_tcp_ip_client();
        let flags = client
            .shell()
            .pm()
            .package_flags("com.swisscom.aot.library.standalone")
            .expect("failed to get package flags");

        assert!(!flags.is_empty());
        println!(
            "package flags: {}",
            flags.iter().map(|p| format!("{}", p)).join(",")
        );
    }

    #[test]
    fn test_is_system() {
        init_log();
        let client = connect_emulator();
        let result = client
            .shell()
            .pm()
            .is_system("com.android.bluetooth")
            .expect("failed to call is_system");
        println!("is_system: {result}");
        assert!(result);
    }

    #[test]
    fn test_is_installed() {
        init_log();
        let client = connect_emulator();
        let result = client
            .shell()
            .pm()
            .is_installed("com.android.bluetooth", None)
            .expect("failed to call is_installed");
        assert!(result);

        let result = client
            .shell()
            .pm()
            .is_installed("com.android.bluetooth", Some("0"))
            .expect("failed to call is_installed");
        assert!(result);

        let result = client
            .shell()
            .pm()
            .is_installed("com.android.xxx", None)
            .expect("failed to call is_installed");
        assert!(!result);
    }

    #[test]
    fn test_install_uninstall() {
        init_log();
        let client = connect_emulator();
        let test_files_dir = test_files_dir();

        println!("test_files_dir: {:?}", test_files_dir);

        let path = test_files_dir.join("app-debug.apk");
        let target_dir = "/data/local/tmp";
        let target_file = format!("{target_dir}/app-debug.apk");
        let package_name = "it.sephiroth.android.app.app";

        let is_installed = client
            .shell()
            .pm()
            .is_installed(package_name, None)
            .expect("failed to check if package is installed");
        if is_installed {
            let package_path = client
                .shell()
                .pm()
                .path(package_name, None)
                .expect("failed to get package path");
            client
                .shell()
                .pm()
                .uninstall(package_name, None)
                .and(
                    client
                        .shell()
                        .rm(package_path, vec!["-fr"])
                        .or_else(|_| Ok(())),
                )
                .expect("failed to uninstall package");
            assert!(!client
                .shell()
                .pm()
                .is_installed(package_name, None)
                .unwrap());
        }

        let is_dir = client
            .shell()
            .is_dir(target_dir)
            .expect("failed to check for dir");
        assert!(is_dir);

        let exists = client
            .shell()
            .exists(&target_file)
            .expect("failed to check for device file");
        if exists {
            client
                .shell()
                .rm(&target_file, vec![])
                .expect("failed to delete file");
        }

        let _ = client.push(path, target_dir).expect("failed to push file");

        client
            .shell()
            .pm()
            .install(
                &target_file,
                Some(InstallOptions {
                    user: None,
                    dont_kill: false,
                    restrict_permissions: false,
                    package_name: None,
                    install_location: Some(InstallLocationOption::Auto),
                    grant_permissions: false,
                    force: true,
                    replace_existing_application: true,
                    allow_version_downgrade: true,
                }),
            )
            .expect("failed to install package");

        let is_installed = client
            .shell()
            .pm()
            .is_installed(package_name, None)
            .expect("failed to check if package is installed");
        assert!(is_installed);
    }
}
