use std::net::SocketAddr;
use std::path::PathBuf;

use strum_macros::{Display, IntoStaticStr};

#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Adb(pub(crate) PathBuf);

#[allow(dead_code)]
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum ConnectionType {
	TcpIp(SocketAddr),
	Transport(u8),
	USB,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Client {
	pub adb: Adb,
	pub addr: ConnectionType,
	pub debug: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Shell<'a> {
	pub(crate) parent: &'a Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActivityManager<'a> {
	pub(crate) parent: &'a Shell<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageManager<'a> {
	pub(crate) parent: &'a Shell<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdbDevice {
	pub name: String,
	pub product: String,
	pub model: String,
	pub device: String,
	pub addr: ConnectionType,
}

#[derive(Debug, Display, Eq, PartialEq, Hash, Clone)]
pub enum Wakefulness {
	Awake,
	Asleep,
	Dreaming,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum Reconnect {
	Device,
	Offline,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum UserOption {
	UserId(String),
	All,
	Current,
	None,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum MemoryStatus {
	Hidden,
	RunningModerate,
	Background,
	RunningLow,
	Moderate,
	RunningCritical,
	Complete,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Package {
	pub package_name: String,
	pub file_name: Option<String>,
	pub version_code: Option<i32>,
	pub uid: Option<i32>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RuntimePermission {
	pub name: String,
	pub granted: bool,
	pub flags: Vec<String>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct InstallPermission {
	pub name: String,
	pub granted: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, IntoStaticStr, Display)]
pub enum PackageFlags {
	System,
	HasCode,
	AllowClearUserData,
	UpdatedSystemApp,
	AllowBackup,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UninstallOptions {
	// -k
	pub keep_data: bool,
	// --user
	pub user: Option<String>,
	// --versionCode
	pub version_code: Option<i32>,
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct InstallOptions {
	// --user: install under the given user.
	pub user: Option<String>,
	// --dont-kill: installing a new feature split, don't kill running app
	pub dont_kill: bool,
	// --restrict-permissions: don't whitelist restricted permissions at install
	pub restrict_permissions: bool,
	// --pkg: specify expected package name of app being installed
	pub package_name: Option<String>,
	// --install-location: force the install location:
	// 0=auto, 1=internal only, 2=prefer external
	pub install_location: Option<InstallLocationOption>,
	// -g: grant all runtime permissions
	pub grant_permissions: bool,
	// -f: force
	pub force: bool,
	// -r replace existing application
	pub replace_existing_application: bool,
	// -d: allow version code downgrade
	pub allow_version_downgrade: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListPackageFilter {
	// -d: filter to only show disabled packages
	pub show_only_disabled: bool,
	// -e: filter to only show enabled packages
	pub show_only_enabed: bool,
	// -s: filter to only show system packages
	pub show_only_system: bool,
	// -3: filter to only show third party packages
	pub show_only3rd_party: bool,
	// --apex-only: only show APEX packages
	pub apex_only: bool,
	// --uid UID: filter to only show packages with the given UID
	pub uid: Option<String>,
	// --user USER_ID: only list packages belonging to the given user
	pub user: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum InstallLocationOption {
	// 0=auto, 1=internal only, 2=prefer external
	Auto,
	InternalOnly,
	PreferExternal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPackageDisplayOptions {
	// -U: also show the package UID
	pub show_uid: bool,
	// --show-versioncode: also show the version code
	pub show_version_code: bool,
	// -u: also include uninstalled packages
	pub include_uninstalled: bool,
	// -f: see their associated file
	pub show_apk_file: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RebootType {
	Bootloader,
	Recovery,
	Sideload,
	SideloadAutoReboot,
}
