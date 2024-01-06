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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
pub struct FFPlayOptions {
	pub framerate: Option<u16>,
	pub size: Option<(u16, u16)>,
	pub probesize: Option<u16>,
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

#[derive(Debug, Clone, Copy, Hash, PartialEq, IntoStaticStr)]
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

#[derive(Debug, Eq, PartialEq, Clone, Copy, IntoStaticStr)]
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

#[derive(IntoStaticStr)]
#[allow(non_camel_case_types)]
pub enum MotionEvent {
	DOWN,
	UP,
	MOVE,
	CANCEL,
}

#[derive(IntoStaticStr)]
#[allow(non_camel_case_types)]
pub enum InputSource {
	dpad,
	keyboard,
	mouse,
	touchpad,
	gamepad,
	touchnavigation,
	joystick,
	touchscreen,
	stylus,
	trackball,
}

#[derive(IntoStaticStr, Display, Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum KeyCode {
	KEYCODE_0,
	KEYCODE_11,
	KEYCODE_12,
	KEYCODE_1,
	KEYCODE_2,
	KEYCODE_3,
	KEYCODE_3D_MODE,
	KEYCODE_4,
	KEYCODE_5,
	KEYCODE_6,
	KEYCODE_7,
	KEYCODE_8,
	KEYCODE_9,
	KEYCODE_A,
	KEYCODE_ALL_APPS,
	KEYCODE_ALT_LEFT,
	KEYCODE_ALT_RIGHT,
	KEYCODE_APOSTROPHE,
	KEYCODE_APP_SWITCH,
	KEYCODE_ASSIST,
	KEYCODE_AT,
	KEYCODE_AVR_INPUT,
	KEYCODE_AVR_POWER,
	KEYCODE_B,
	KEYCODE_BACK,
	KEYCODE_BACKSLASH,
	KEYCODE_BOOKMARK,
	KEYCODE_BREAK,
	KEYCODE_BRIGHTNESS_DOWN,
	KEYCODE_BRIGHTNESS_UP,
	KEYCODE_BUTTON_10,
	KEYCODE_BUTTON_11,
	KEYCODE_BUTTON_12,
	KEYCODE_BUTTON_13,
	KEYCODE_BUTTON_14,
	KEYCODE_BUTTON_15,
	KEYCODE_BUTTON_16,
	KEYCODE_BUTTON_1,
	KEYCODE_BUTTON_2,
	KEYCODE_BUTTON_3,
	KEYCODE_BUTTON_4,
	KEYCODE_BUTTON_5,
	KEYCODE_BUTTON_6,
	KEYCODE_BUTTON_7,
	KEYCODE_BUTTON_8,
	KEYCODE_BUTTON_9,
	KEYCODE_BUTTON_A,
	KEYCODE_BUTTON_B,
	KEYCODE_BUTTON_C,
	KEYCODE_BUTTON_L1,
	KEYCODE_BUTTON_L2,
	KEYCODE_BUTTON_MODE,
	KEYCODE_BUTTON_R1,
	KEYCODE_BUTTON_R2,
	KEYCODE_BUTTON_SELECT,
	KEYCODE_BUTTON_START,
	KEYCODE_BUTTON_THUMBL,
	KEYCODE_BUTTON_THUMBR,
	KEYCODE_BUTTON_X,
	KEYCODE_BUTTON_Y,
	KEYCODE_BUTTON_Z,
	KEYCODE_C,
	KEYCODE_CALCULATOR,
	KEYCODE_CALENDAR,
	KEYCODE_CALL,
	KEYCODE_CAMERA,
	KEYCODE_CAPS_LOCK,
	KEYCODE_CAPTIONS,
	KEYCODE_CHANNEL_DOWN,
	KEYCODE_CHANNEL_UP,
	KEYCODE_CLEAR,
	KEYCODE_COMMA,
	KEYCODE_CONTACTS,
	KEYCODE_COPY,
	KEYCODE_CTRL_LEFT,
	KEYCODE_CTRL_RIGHT,
	KEYCODE_CUT,
	KEYCODE_D,
	KEYCODE_DEL,
	KEYCODE_DPAD_CENTER,
	KEYCODE_DPAD_DOWN,
	KEYCODE_DPAD_DOWN_LEFT,
	KEYCODE_DPAD_DOWN_RIGHT,
	KEYCODE_DPAD_LEFT,
	KEYCODE_DPAD_RIGHT,
	KEYCODE_DPAD_UP,
	KEYCODE_DPAD_UP_LEFT,
	KEYCODE_DPAD_UP_RIGHT,
	KEYCODE_DVR,
	KEYCODE_E,
	KEYCODE_EISU,
	KEYCODE_ENDCALL,
	KEYCODE_ENTER,
	KEYCODE_ENVELOPE,
	KEYCODE_EQUALS,
	KEYCODE_ESCAPE,
	KEYCODE_EXPLORER,
	KEYCODE_F10,
	KEYCODE_F11,
	KEYCODE_F12,
	KEYCODE_F1,
	KEYCODE_F2,
	KEYCODE_F3,
	KEYCODE_F4,
	KEYCODE_F5,
	KEYCODE_F6,
	KEYCODE_F7,
	KEYCODE_F8,
	KEYCODE_F9,
	KEYCODE_F,
	KEYCODE_FOCUS,
	KEYCODE_FORWARD,
	KEYCODE_FORWARD_DEL,
	KEYCODE_FUNCTION,
	KEYCODE_G,
	KEYCODE_GRAVE,
	KEYCODE_GUIDE,
	KEYCODE_H,
	KEYCODE_HEADSETHOOK,
	KEYCODE_HELP,
	KEYCODE_HENKAN,
	KEYCODE_HOME,
	KEYCODE_I,
	KEYCODE_INFO,
	KEYCODE_INSERT,
	KEYCODE_J,
	KEYCODE_K,
	KEYCODE_KANA,
	KEYCODE_KATAKANA_HIRAGANA,
	KEYCODE_L,
	KEYCODE_LANGUAGE_SWITCH,
	KEYCODE_LAST_CHANNEL,
	KEYCODE_LEFT_BRACKET,
	KEYCODE_M,
	KEYCODE_MANNER_MODE,
	KEYCODE_MEDIA_AUDIO_TRACK,
	KEYCODE_MEDIA_CLOSE,
	KEYCODE_MEDIA_EJECT,
	KEYCODE_MEDIA_FAST_FORWARD,
	KEYCODE_MEDIA_NEXT,
	KEYCODE_MEDIA_PAUSE,
	KEYCODE_MEDIA_PLAY,
	KEYCODE_MEDIA_PLAY_PAUSE,
	KEYCODE_MEDIA_PREVIOUS,
	KEYCODE_MEDIA_RECORD,
	KEYCODE_MEDIA_REWIND,
	KEYCODE_FAST_FORWARD,
	KEYCODE_MEDIA_SKIP_BACKWARD,
	KEYCODE_MEDIA_SKIP_FORWARD,
	KEYCODE_MEDIA_STEP_BACKWARD,
	KEYCODE_MEDIA_STEP_FORWARD,
	KEYCODE_MEDIA_STOP,
	KEYCODE_MEDIA_TOP_MENU,
	KEYCODE_MENU,
	KEYCODE_META_LEFT,
	KEYCODE_META_RIGHT,
	KEYCODE_MINUS,
	KEYCODE_MOVE_END,
	KEYCODE_MOVE_HOME,
	KEYCODE_MUHENKAN,
	KEYCODE_MUSIC,
	KEYCODE_MUTE,
	KEYCODE_N,
	KEYCODE_NAVIGATE_IN,
	KEYCODE_NAVIGATE_NEXT,
	KEYCODE_NAVIGATE_OUT,
	KEYCODE_NAVIGATE_PREVIOUS,
	KEYCODE_NOTIFICATION,
	KEYCODE_NUM,
	KEYCODE_NUM_LOCK,
	KEYCODE_NUMPAD_0,
	KEYCODE_NUMPAD_1,
	KEYCODE_NUMPAD_2,
	KEYCODE_NUMPAD_3,
	KEYCODE_NUMPAD_4,
	KEYCODE_NUMPAD_5,
	KEYCODE_NUMPAD_6,
	KEYCODE_NUMPAD_7,
	KEYCODE_NUMPAD_8,
	KEYCODE_NUMPAD_9,
	KEYCODE_NUMPAD_ADD,
	KEYCODE_NUMPAD_COMMA,
	KEYCODE_NUMPAD_DIVIDE,
	KEYCODE_NUMPAD_DOT,
	KEYCODE_NUMPAD_ENTER,
	KEYCODE_NUMPAD_EQUALS,
	KEYCODE_NUMPAD_LEFT_PAREN,
	KEYCODE_NUMPAD_MULTIPLY,
	KEYCODE_NUMPAD_RIGHT_PAREN,
	KEYCODE_NUMPAD_SUBTRACT,
	KEYCODE_O,
	KEYCODE_P,
	KEYCODE_PAGE_DOWN,
	KEYCODE_PAGE_UP,
	KEYCODE_PAIRING,
	KEYCODE_PASTE,
	KEYCODE_PERIOD,
	KEYCODE_PICTSYMBOLS,
	KEYCODE_PLUS,
	KEYCODE_POUND,
	KEYCODE_POWER,
	KEYCODE_PROFILE_SWITCH,
	KEYCODE_PROG_BLUE,
	KEYCODE_PROG_GREEN,
	KEYCODE_PROG_RED,
	KEYCODE_PROG_YELLOW,
	KEYCODE_Q,
	KEYCODE_R,
	KEYCODE_REFRESH,
	KEYCODE_RIGHT_BRACKET,
	KEYCODE_RO,
	KEYCODE_S,
	KEYCODE_SCROLL_LOCK,
	KEYCODE_SEARCH,
	KEYCODE_SEMICOLON,
	KEYCODE_SETTINGS,
	KEYCODE_SHIFT_LEFT,
	KEYCODE_SHIFT_RIGHT,
	KEYCODE_SLASH,
	KEYCODE_SLEEP,
	KEYCODE_SOFT_LEFT,
	KEYCODE_SOFT_RIGHT,
	KEYCODE_SOFT_SLEEP,
	KEYCODE_SPACE,
	KEYCODE_STAR,
	KEYCODE_STB_INPUT,
	KEYCODE_STB_POWER,
	KEYCODE_STEM_1,
	KEYCODE_STEM_2,
	KEYCODE_STEM_3,
	KEYCODE_STEM_PRIMARY,
	KEYCODE_SWITCH_CHARSET,
	KEYCODE_SYM,
	KEYCODE_SYSRQ,
	KEYCODE_SYSTEM_NAVIGATION_DOWN,
	KEYCODE_SYSTEM_NAVIGATION_LEFT,
	KEYCODE_SYSTEM_NAVIGATION_RIGHT,
	KEYCODE_SYSTEM_NAVIGATION_UP,
	KEYCODE_T,
	KEYCODE_TAB,
	KEYCODE_THUMBS_DOWN,
	KEYCODE_THUMBS_UP,
	KEYCODE_TV,
	KEYCODE_TV_ANTENNA_CABLE,
	KEYCODE_TV_AUDIO_DESCRIPTION,
	KEYCODE_TV_AUDIO_DESCRIPTION_MIX_DOWN,
	KEYCODE_TV_AUDIO_DESCRIPTION_MIX_UP,
	KEYCODE_TV_CONTENTS_MENU,
	KEYCODE_TV_DATA_SERVICE,
	KEYCODE_TV_INPUT,
	KEYCODE_TV_INPUT_COMPONENT_1,
	KEYCODE_TV_INPUT_COMPONENT_2,
	KEYCODE_TV_INPUT_COMPOSITE_1,
	KEYCODE_TV_INPUT_COMPOSITE_2,
	KEYCODE_TV_INPUT_HDMI_1,
	KEYCODE_TV_INPUT_HDMI_2,
	KEYCODE_TV_INPUT_HDMI_3,
	KEYCODE_TV_INPUT_HDMI_4,
	KEYCODE_TV_INPUT_VGA_1,
	KEYCODE_TV_MEDIA_CONTEXT_MENU,
	KEYCODE_TV_NETWORK,
	KEYCODE_TV_NUMBER_ENTRY,
	KEYCODE_TV_POWER,
	KEYCODE_TV_RADIO_SERVICE,
	KEYCODE_TV_SATELLITE,
	KEYCODE_TV_SATELLITE_BS,
	KEYCODE_TV_SATELLITE_CS,
	KEYCODE_TV_SATELLITE_SERVICE,
	KEYCODE_TV_TELETEXT,
	KEYCODE_TV_TERRESTRIAL_ANALOG,
	KEYCODE_TV_TERRESTRIAL_DIGITAL,
	KEYCODE_TV_TIMER_PROGRAMMING,
	KEYCODE_TV_ZOOM_MODE,
	KEYCODE_U,
	KEYCODE_UNKNOWN,
	KEYCODE_V,
	KEYCODE_VOICE_ASSIST,
	KEYCODE_VOLUME_DOWN,
	KEYCODE_VOLUME_MUTE,
	KEYCODE_VOLUME_UP,
	KEYCODE_W,
	KEYCODE_WAKEUP,
	KEYCODE_WINDOW,
	KEYCODE_X,
	KEYCODE_Y,
	KEYCODE_YEN,
	KEYCODE_Z,
	KEYCODE_ZENKAKU_HANKAKU,
	KEYCODE_ZOOM_IN,
	KEYCODE_ZOOM_OUT,
}

pub enum KeyEventType {
	LongPress,
	DoubleTap,
}

#[derive(Debug, IntoStaticStr)]
pub enum PropType {
	String,
	Bool,
	Int,
	Enum(Vec<String>),
	Unknown(String),
}

#[derive(Debug, Display, Eq, PartialEq, Hash)]
pub enum Wakefulness {
	Awake,
	Asleep,
	Dreaming,
}
