use fmt::Debug;
use std::ffi::OsStr;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::net::{AddrParseError, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use crate::errors::AdbError::InvalidDeviceError;
use crate::errors::{AdbError, ParseSELinuxTypeError};
use crate::input::{KeyCode, KeyEventType};
use crate::process::CommandBuilder;
use crate::traits::Vec8ToString;
use crate::traits::{AdbDevice, AsArgs};
use crate::types::AddressType::Sock;
use crate::types::PackageFlags::{AllowBackup, AllowClearUserData, HasCode, System, UpdatedSystemApp};
use crate::types::{
	AddressType, DeviceAddress, Extra, InstallLocationOption, InstallOptions, Intent, ListPackageDisplayOptions, ListPackageFilter, LogcatLevel, LogcatTag, PackageFlags, RebootType, SELinuxType,
	ScreenRecordOptions, UninstallOptions,
};
use crate::{Adb, Device};
use crate::{AdbClient, AdbShell};
use async_trait::async_trait;
use lazy_static::lazy_static;
use regex::Regex;
use rustix::path::Arg;
use tokio::process::Command;

impl Vec8ToString for Vec<u8> {
	fn as_str(&self) -> Option<&str> {
		match std::str::from_utf8(self) {
			Ok(s) => Some(s),
			Err(_) => None,
		}
	}
}

impl Extend<KeyCode> for Vec<&str> {
	fn extend<T: IntoIterator<Item = KeyCode>>(&mut self, iter: T) {
		for element in iter {
			self.push(element.into());
		}
	}
}

impl DeviceAddress {
	pub fn address_type(&self) -> &AddressType {
		&self.0
	}

	pub fn serial(&self) -> Option<String> {
		match &self.0 {
			AddressType::Sock(sock) => Some(sock.to_string()),
			AddressType::Name(name) => Some(name.to_string()),
			AddressType::Transport(_) => None,
		}
	}

	pub fn transport_id(&self) -> Option<u8> {
		match self.0 {
			AddressType::Transport(id) => Some(id),
			_ => None,
		}
	}

	pub fn from_serial(input: &str) -> Result<DeviceAddress, AdbError> {
		Ok(DeviceAddress(AddressType::Name(input.to_string())))
	}

	pub fn from_transport_id(id: u8) -> Result<DeviceAddress, AdbError> {
		Ok(DeviceAddress(AddressType::Transport(id)))
	}

	pub fn from_ip(input: &str) -> Result<DeviceAddress, AddrParseError> {
		let addr: Result<SocketAddr, AddrParseError> = input.parse();
		match addr {
			Ok(addr) => Ok(DeviceAddress(AddressType::Sock(addr))),
			Err(err) => Err(err),
		}
	}
}

impl Display for DeviceAddress {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match &self.0 {
			AddressType::Name(name) => write!(f, "name:{}", name),
			AddressType::Transport(id) => write!(f, "transport_id:{}", id),
			AddressType::Sock(addr) => write!(f, "ip:{}", addr),
		}
	}
}

impl Debug for DeviceAddress {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match &self.0 {
			AddressType::Name(name) => write!(f, "{}", name),
			AddressType::Transport(id) => write!(f, "{}", id),
			AddressType::Sock(addr) => write!(f, "{}", addr),
		}
	}
}

impl Device {
	pub fn try_from_address(value: &DeviceAddress) -> Result<Device, AdbError> {
		match value.address_type() {
			AddressType::Sock(addr) => Device::try_from_sock_addr(addr),
			AddressType::Name(name) => Device::try_from_serial(name),
			AddressType::Transport(id) => Device::try_from_transport_id(*id),
		}
	}

	pub fn try_from_ip(input: &str) -> Result<Device, AddrParseError> {
		DeviceAddress::from_ip(input).map(|address| Device(address))
	}

	pub fn try_from_sock_addr(input: &SocketAddr) -> Result<Device, AdbError> {
		Ok(Device(DeviceAddress(AddressType::Sock(input.clone()))))
	}

	pub fn try_from_serial(input: &str) -> Result<Device, AdbError> {
		DeviceAddress::from_serial(input).map(|address| Device(address))
	}

	pub fn try_from_transport_id(id: u8) -> Result<Device, AdbError> {
		DeviceAddress::from_transport_id(id).map(|address| Device(address))
	}

	#[allow(dead_code)]
	fn try_from_device(input: &str) -> Result<Device, AdbError> {
		lazy_static! {
			static ref RE: Regex =
				Regex::new("(?P<ip>[^\\s]+)[\\s]+device product:(?P<device_product>[^\\s]+)\\smodel:(?P<model>[^\\s]+)\\sdevice:(?P<device>[^\\s]+)\\stransport_id:(?P<transport_id>[^\\s]+)").unwrap();
		}

		if !RE.is_match(input) {
			let msg = format!("Invalid IP address: {}", input);
			return Err(InvalidDeviceError(msg));
		}

		if let Some(cap) = RE.captures(input) {
			let ip = cap.name("ip").ok_or(InvalidDeviceError("Device serial not found".to_string()))?.as_str();
			DeviceAddress::from_ip(ip).map(|address| Device(address)).map_err(|e| AdbError::from(e))
		} else {
			Err(AdbError::InvalidDeviceAddressError(input.to_string()))
		}
	}

	pub fn args(&self) -> Vec<String> {
		match &self.0 .0 {
			AddressType::Sock(addr) => vec!["-s".to_string(), addr.to_string()],
			AddressType::Name(name) => vec!["-s".to_string(), name.to_string()],
			AddressType::Transport(id) => vec!["-t".to_string(), id.to_string()],
		}
	}
}

impl From<&str> for Device {
	fn from(value: &str) -> Self {
		Device::try_from_serial(value).unwrap()
	}
}

impl FromStr for Device {
	type Err = AddrParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let addr: Result<SocketAddr, AddrParseError> = s.parse();
		match addr {
			Ok(a) => Ok(Self(DeviceAddress(Sock(a)))),
			Err(e) => Err(e),
		}
	}
}

impl Display for Device {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl Debug for Device {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "Device{{address={:?}}}", self.0)
	}
}

#[async_trait]
impl AdbDevice for Device {
	fn addr(&self) -> &DeviceAddress {
		&self.0
	}

	fn args(&self) -> Vec<String> {
		self.args()
	}
}

impl<'a> From<&'a Device> for &'a dyn AdbDevice {
	fn from(value: &'a Device) -> Self {
		value
	}
}

impl RebootType {
	pub(crate) fn value(&self) -> String {
		String::from(match *self {
			RebootType::Bootloader => "bootloader",
			RebootType::Recovery => "recovery",
			RebootType::Sideload => "sideload",
			RebootType::SideloadAutoReboot => "sideload-auto-reboot",
		})
	}
}

impl Display for LogcatLevel {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			LogcatLevel::Verbose => write!(f, "V"),
			LogcatLevel::Debug => write!(f, "D"),
			LogcatLevel::Info => write!(f, "I"),
			LogcatLevel::Warn => write!(f, "W"),
			LogcatLevel::Error => write!(f, "E"),
		}
	}
}

impl Display for LogcatTag {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "{}:{}", self.name, self.level)
	}
}

impl TryFrom<&dyn AdbDevice> for Device {
	type Error = AdbError;

	fn try_from(value: &dyn AdbDevice) -> Result<Self, Self::Error> {
		Device::try_from_address(value.addr())
	}
}

impl AsRef<OsStr> for Adb {
	fn as_ref(&self) -> &OsStr {
		self.0.as_os_str()
	}
}

impl Default for Adb {
	fn default() -> Self {
		Adb::new().unwrap()
	}
}

impl TryFrom<&str> for PackageFlags {
	type Error = AdbError;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value {
			"SYSTEM" => Ok(System),
			"HAS_CODE" => Ok(HasCode),
			"ALLOW_CLEAR_USER_DATA" => Ok(AllowClearUserData),
			"UPDATED_SYSTEM_APP" => Ok(UpdatedSystemApp),
			"ALLOW_BACKUP" => Ok(AllowBackup),
			_ => Err(AdbError::NameNotFoundError(value.to_string())),
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

impl Default for InstallLocationOption {
	fn default() -> Self {
		InstallLocationOption::Auto
	}
}

impl IntoIterator for InstallOptions {
	type Item = String;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args = vec![];
		match self.user.as_ref() {
			None => {}
			Some(user) => args.push(format!("--user {:}", user)),
		}

		match self.package_name.as_ref() {
			None => {}
			Some(user) => args.push(format!("--pkg {:}", user)),
		}

		match self.install_location.as_ref() {
			None => {}
			Some(s) => args.push(format!("--install-location {:}", s)),
		}

		if self.dont_kill {
			args.push("--dont-kill".to_string());
		}

		if self.restrict_permissions {
			args.push("--restrict-permissions".to_string());
		}

		if self.grant_permissions {
			args.push("-g".to_string());
		}

		if self.force {
			args.push("-f".to_string());
		}

		if self.replace_existing_application {
			args.push("-r".to_string());
		}

		if self.allow_version_downgrade {
			args.push("-d".to_string());
		}

		args.into_iter()
	}
}

impl Display for InstallOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args = self.clone().into_iter().collect::<Vec<_>>();
		write!(f, "{:}", args.join(" "))
	}
}

impl IntoIterator for ListPackageDisplayOptions {
	type Item = String;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args: Vec<String> = vec![];
		if self.show_uid {
			args.push("-U".into());
		}

		if self.show_version_code {
			args.push("--show-versioncode".into());
		}

		if self.include_uninstalled {
			args.push("-u".into());
		}

		if self.show_apk_file {
			args.push("-f".into());
		}
		args.into_iter()
	}
}

impl From<&UninstallOptions> for Vec<String> {
	fn from(value: &UninstallOptions) -> Self {
		let mut args: Vec<String> = vec![];
		if value.keep_data {
			args.push("-k".into());
		}

		match value.user.as_ref() {
			None => {}
			Some(s) => {
				args.push("--user".into());
				args.push(s.into());
			}
		}

		match value.version_code.as_ref() {
			None => {}
			Some(s) => {
				args.push("--versionCode".into());
				args.push(format!("{:}", s));
			}
		}

		args
	}
}

impl Display for UninstallOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args: Vec<String> = From::<&UninstallOptions>::from(self);
		write!(f, "{:}", args.join(" "))
	}
}

impl Display for ListPackageDisplayOptions {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let args = self.clone().into_iter().collect::<Vec<_>>();
		write!(f, "{:}", args.join(" "))
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

impl IntoIterator for ListPackageFilter {
	type Item = String;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args: Vec<String> = vec![];
		if self.show_only_disabled {
			args.push("-d".into());
		}
		if self.show_only_enabed {
			args.push("-e".into());
		}
		if self.show_only_system {
			args.push("-s".into());
		}
		if self.show_only3rd_party {
			args.push("-3".into());
		}
		if self.apex_only {
			args.push("--apex-only".into());
		}

		match self.uid.as_ref() {
			None => {}
			Some(s) => args.push(format!("--uid {:}", s)),
		}

		match self.user.as_ref() {
			None => {}
			Some(s) => args.push(format!("--user {:}", s)),
		}
		args.into_iter()
	}
}

impl Display for ListPackageFilter {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:}", self.clone().into_iter().collect::<Vec<_>>().join(" "))
	}
}

impl From<Adb> for Command {
	fn from(value: Adb) -> Self {
		Command::new(value.0.as_os_str())
	}
}

impl From<AdbClient> for CommandBuilder {
	fn from(value: AdbClient) -> Self {
		CommandBuilder::shell(&value.adb, &value.device)
	}
}

impl<'a> From<&'a Adb> for &'a OsStr {
	fn from(value: &'a Adb) -> Self {
		value.0.as_os_str()
	}
}

impl Default for ScreenRecordOptions {
	fn default() -> Self {
		Self::new()
	}
}

impl IntoIterator for ScreenRecordOptions {
	type Item = String;
	type IntoIter = std::vec::IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		let mut args: Vec<String> = vec![];
		if let Some(bitrate) = self.bitrate {
			args.push("--bit-rate".to_string());
			args.push(format!("{:}", bitrate));
		}

		if let Some(timelimit) = self.timelimit {
			args.push(String::from("--time-limit"));
			args.push(format!("{:}", timelimit.as_secs()));
		}

		if self.rotate.unwrap_or(false) {
			args.push(String::from("--rotate"))
		}

		if self.bug_report.unwrap_or(false) {
			args.push(String::from("--bugreport"))
		}

		if self.verbose {
			args.push(String::from("--verbose"))
		}

		if let Some(size) = self.size {
			args.push(String::from("--size"));
			args.push(format!("{:}x{:}", size.0, size.1));
		}
		args.into_iter()
	}
}

impl ScreenRecordOptions {
	pub fn new() -> Self {
		ScreenRecordOptions {
			bitrate: Some(20000000),
			timelimit: Some(Duration::from_secs(10)),
			rotate: None,
			bug_report: None,
			size: None,
			verbose: false,
		}
	}
}

impl Intent {
	pub fn new() -> Intent {
		Intent::default()
	}
	pub fn from_action(action: &str) -> Intent {
		let mut intent = Intent::new();
		intent.action = Some(action.to_string());
		intent
	}
}

impl<'a> AsArgs<&'a str> for Vec<&'a str> {
	fn as_args(&self) -> Vec<&'a str> {
		self.clone()
	}
}

impl Extra {
	pub fn put_string_extra(&mut self, name: &str, value: &str) -> &mut Self {
		self.es.insert(name.to_string(), value.to_string());
		self
	}

	pub fn put_bool_extra(&mut self, name: &str, value: bool) -> &mut Self {
		self.ez.insert(name.to_string(), value);
		self
	}

	pub fn put_int_extra(&mut self, name: &str, value: i32) -> &mut Self {
		self.ei.insert(name.to_string(), value);
		self
	}

	pub fn put_long_extra(&mut self, name: &str, value: i64) -> &mut Self {
		self.el.insert(name.to_string(), value);
		self
	}

	pub fn put_string_array_extra(&mut self, name: &str, value: Vec<String>) -> &mut Self {
		self.esa.insert(name.to_string(), value);
		self
	}
}

impl Display for Intent {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		let mut args: Vec<String> = vec![];

		if let Some(action) = self.action.as_ref() {
			args.push(format!("-a {:}", action));
		}

		if let Some(data) = self.data.as_ref() {
			args.push(format!("-d {:}", data));
		}

		if let Some(mime_type) = self.mime_type.as_ref() {
			args.push(format!("-t {:}", mime_type));
		}

		if let Some(category) = self.category.as_ref() {
			args.push(format!("-c {:}", category));
		}

		if let Some(component) = self.component.as_ref() {
			args.push(format!("-n {:}", component));
		}

		if let Some(package) = self.package.as_ref() {
			args.push(format!("-p {:}", package));
		}

		if let Some(user_id) = self.user_id.as_ref() {
			args.push(format!("--user {:}", user_id));
		}

		if self.receiver_foreground {
			args.push("--receiver-foreground".to_string());
		}

		if self.wait {
			args.push("-W".to_string());
		}

		args.push(format!("{:}", self.extra));

		write!(f, "{:}", args.join(" "))
	}
}

impl Display for Extra {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		let mut output: Vec<String> = Vec::new();

		if !self.es.is_empty() {
			self.es.iter().for_each(|entry| {
				output.push(format!("--es {:} {:}", entry.0, entry.1));
			});
		}

		if !self.ez.is_empty() {
			self.ez.iter().for_each(|entry| {
				output.push(format!("--ez {:} {:}", entry.0, entry.1));
			});
		}

		if !self.ei.is_empty() {
			self.ei.iter().for_each(|entry| {
				output.push(format!("--ei {:} {:}", entry.0, entry.1));
			});
		}

		if !self.el.is_empty() {
			self.el.iter().for_each(|entry| {
				output.push(format!("--el {:} {:}", entry.0, entry.1));
			});
		}

		if !self.ef.is_empty() {
			self.ef.iter().for_each(|entry| {
				output.push(format!("--ef {:} {:}", entry.0, entry.1));
			});
		}

		if !self.eu.is_empty() {
			self.eu.iter().for_each(|entry| {
				output.push(format!("--eu {:} {:}", entry.0, entry.1));
			});
		}

		if !self.ecn.is_empty() {
			self.ecn.iter().for_each(|entry| {
				output.push(format!("--ecn {:} {:}", entry.0, entry.1));
			});
		}

		if !self.eia.is_empty() {
			self.eia.iter().for_each(|entry| {
				output.push(format!("--eia {:} {:}", entry.0, entry.1.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")));
			});
		}

		if !self.ela.is_empty() {
			self.ela.iter().for_each(|entry| {
				output.push(format!("--ela {:} {:}", entry.0, entry.1.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")));
			});
		}

		if !self.efa.is_empty() {
			self.efa.iter().for_each(|entry| {
				output.push(format!("--efa {:} {:}", entry.0, entry.1.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")));
			});
		}

		if !self.esa.is_empty() {
			self.esa.iter().for_each(|entry| {
				output.push(format!("--efa {:} {:}", entry.0, entry.1.join(",")));
			});
		}

		if self.grant_read_uri_permission {
			output.push("--grant-read-uri-permission".to_string());
		}

		if self.grant_write_uri_permission {
			output.push("--grant-write-uri-permission".to_string());
		}

		if self.exclude_stopped_packages {
			output.push("--exclude-stopped-packages".to_string());
		}

		if self.include_stopped_packages {
			output.push("--include-stopped-packages".to_string());
		}
		write!(f, "{:}", output.join(" "))
	}
}

impl Display for SELinuxType {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			SELinuxType::Enforcing => write!(f, "Enforcing"),
			SELinuxType::Permissive => write!(f, "Permissive"),
		}
	}
}

impl SELinuxType {
	pub fn to_string(&self) -> String {
		format!("{:}", self)
	}
}

impl TryFrom<Vec<u8>> for SELinuxType {
	type Error = ParseSELinuxTypeError;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		let opt_string = Arg::as_str(&value)?;
		opt_string.try_into()
	}
}

impl TryFrom<&str> for SELinuxType {
	type Error = ParseSELinuxTypeError;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		match value.trim() {
			"Enforcing" => Ok(SELinuxType::Enforcing),
			"Permissive" => Ok(SELinuxType::Permissive),
			o => Err(ParseSELinuxTypeError {
				msg: Some(format!("invalid value: {:}", o)),
			}),
		}
	}
}

impl TryFrom<Device> for AdbClient {
	type Error = AdbError;

	fn try_from(value: Device) -> Result<Self, Self::Error> {
		AdbClient::try_from_device(value)
	}
}

impl<'a> Into<AdbShell<'a>> for &'a AdbClient {
	fn into(self: &'a AdbClient) -> AdbShell<'a> {
		self.shell()
	}
}

impl From<KeyEventType> for &str {
	fn from(value: KeyEventType) -> Self {
		return match value {
			KeyEventType::LongPress => "--longpress",
			KeyEventType::DoubleTap => "--doubletap",
		};
	}
}
