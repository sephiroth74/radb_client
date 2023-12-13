use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use strum_macros::{Display, IntoStaticStr};

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum AddressType {
	Sock(SocketAddr),
	Name(String),
	Transport(u8),
}

#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct DeviceAddress(pub(crate) AddressType);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SELinuxType {
	Enforcing,
	Permissive,
}

pub enum RebootType {
	Bootloader,
	Recovery,
	Sideload,
	SideloadAutoReboot,
}

pub struct LogcatOptions {
	/// -e    Only prints lines where the log message matches <expr>, where <expr> is a regular expression.
	pub expr: Option<String>,

	/// -d    Dumps the log to the screen and exits.
	pub dump: bool,

	/// -f <filename>    Writes log message output to <filename>. The default is stdout.
	pub filename: Option<String>,

	/// -s    Equivalent to the filter expression '*:S', which sets priority for all tags to silent and is used to precede a list of filter expressions that add content.
	pub tags: Option<Vec<LogcatTag>>,

	/// -v <format>    Sets the output format for log messages. The default is the threadtime format
	pub format: Option<String>,

	/// -t '<time>'    Prints the most recent lines since the specified time. This option includes -d functionality.
	/// See the -P option for information about quoting parameters with embedded spaces.
	pub since: Option<chrono::DateTime<chrono::Local>>,

	// --pid=<pid> ...
	pub pid: Option<i32>,

	pub timeout: Option<Duration>,
}

pub enum LogcatLevel {
	Verbose,
	Debug,
	Info,
	Warn,
	Error,
}

pub struct LogcatTag {
	pub name: String,
	pub level: LogcatLevel,
}

#[derive(Debug, Default)]
pub struct Intent {
	pub action: Option<String>,
	pub data: Option<String>,
	pub mime_type: Option<String>,
	pub category: Option<String>,
	pub component: Option<String>,
	pub package: Option<String>,
	pub user_id: Option<String>,
	pub flags: u32,
	pub receiver_foreground: bool,
	pub wait: bool,
	pub extra: Extra,
}

#[derive(Debug, Default)]
pub struct Extra {
	pub es: HashMap<String, String>,
	pub ez: HashMap<String, bool>,
	pub ei: HashMap<String, i32>,
	pub el: HashMap<String, i64>,
	pub ef: HashMap<String, f32>,
	pub eu: HashMap<String, String>,
	pub ecn: HashMap<String, String>,
	pub eia: HashMap<String, Vec<i32>>,
	pub ela: HashMap<String, Vec<i64>>,
	pub efa: HashMap<String, Vec<f32>>,
	pub esa: HashMap<String, Vec<String>>,
	pub grant_read_uri_permission: bool,
	pub grant_write_uri_permission: bool,
	pub exclude_stopped_packages: bool,
	pub include_stopped_packages: bool,
}

#[derive(IntoStaticStr, Display)]
#[allow(non_camel_case_types)]
pub enum DumpsysPriority {
	CRITICAL,
	HIGH,
	NORMAL,
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ScreenRecordOptions {
	/// --bit-rate 4000000
	/// Set the video bit rate, in bits per second. Value may be specified as bits or megabits, e.g. '4000000' is equivalent to '4M'.
	/// Default 20Mbps.
	pub bitrate: Option<u64>,

	/// --time-limit=120 (in seconds)
	/// Set the maximum recording time, in seconds. Default / maximum is 180
	pub timelimit: Option<Duration>,

	/// --rotate
	/// Rotates the output 90 degrees. This feature is experimental.
	pub rotate: Option<bool>,

	/// --bugreport
	/// Add additional information, such as a timestamp overlay, that is helpful in videos captured to illustrate bugs.
	pub bug_report: Option<bool>,

	/// --size 1280x720
	/// Set the video size, e.g. "1280x720". Default is the device's main display resolution (if supported), 1280x720 if not.
	/// For best results, use a size supported by the AVC encoder.
	pub size: Option<(u16, u16)>,

	/// --verbose
	/// Display interesting information on stdout
	pub verbose: bool,
}

#[derive(IntoStaticStr)]
#[allow(non_camel_case_types)]
pub enum SettingsType {
	global,
	system,
	secure,
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

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum InstallLocationOption {
	// 0=auto, 1=internal only, 2=prefer external
	Auto,
	InternalOnly,
	PreferExternal,
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

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum PackageFlags {
	System,
	HasCode,
	AllowClearUserData,
	UpdatedSystemApp,
	AllowBackup,
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
