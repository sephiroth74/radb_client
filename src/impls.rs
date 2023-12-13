use fmt::Debug;
use std::ffi::OsStr;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::net::{AddrParseError, SocketAddr};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::IntoFuture;
use lazy_static::lazy_static;
use mac_address::MacAddress;
use props_rs::Property;
use regex::Regex;
use rustix::path::Arg;
use tokio::process::Command;
use tokio::sync::oneshot::Receiver;

use crate::am::ActivityManager;
use crate::client::{LogcatLevel, LogcatOptions, LogcatTag, RebootType};
use crate::command::{CommandBuilder, ProcessResult};
use crate::errors::AdbError::InvalidDeviceError;
use crate::errors::{AdbError, ParseSELinuxTypeError};
use crate::input::{InputSource, KeyCode, KeyEventType, MotionEvent};
use crate::intent::{Extra, Intent};
use crate::pm::PackageManager;
use crate::shell::{DumpsysPriority, ScreenRecordOptions, SettingsType};
use crate::traits::{AdbDevice, AsArgs};
use crate::types::{AdbClient, AdbShell};
use crate::AddressType::Sock;
use crate::{Adb, AddressType, Client, Device, DeviceAddress, SELinuxType, Shell};

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

impl AdbClient {
	pub fn try_from_device(device: Device) -> Result<AdbClient, AdbError> {
		match Adb::new() {
			Ok(adb) => Ok(AdbClient { adb, device }),
			Err(err) => Err(err),
		}
	}

	pub async fn is_connected(&self) -> bool {
		Client::is_connected(&self.adb, &self.device).await
	}

	/// Try to connect to the inner device.
	///
	/// # Arguments
	///
	/// * `timeout`: optional timeout for connecting
	///
	/// returns: Result<(), Error>
	///
	/// # Examples
	///
	/// ```
	/// use radb_client::Device;
	/// use radb_client::types::AdbClient;
	///
	/// pub async fn connect() {
	///  let device: Device = "192.168.1.24:5555".parse().unwrap();
	///  let client: AdbClient = device.try_into().unwrap();
	///  client.connect(None).await.unwrap();
	/// }
	/// ```
	pub async fn connect(&self, timeout: Option<std::time::Duration>) -> Result<(), AdbError> {
		Client::connect(&self.adb, &self.device, timeout).await
	}

	pub async fn disconnect(&self) -> crate::command::Result<bool> {
		Client::disconnect(&self.adb, &self.device).await
	}

	pub async fn root(&self) -> crate::command::Result<bool> {
		Client::root(&self.adb, &self.device).await
	}

	pub async fn unroot(&self) -> crate::command::Result<bool> {
		Client::unroot(&self.adb, &self.device).await
	}

	pub async fn is_root(&self) -> crate::command::Result<bool> {
		Client::is_root(&self.adb, &self.device).await
	}

	pub async fn remount(&self) -> crate::command::Result<()> {
		Client::remount(&self.adb, &self.device).await
	}

	pub async fn mount<T: Arg>(&self, dir: T) -> crate::command::Result<()> {
		Client::mount(&self.adb, &self.device, dir).await
	}

	pub async fn unmount<T: Arg>(&self, dir: T) -> crate::command::Result<()> {
		Client::unmount(&self.adb, &self.device, dir).await
	}

	pub async fn bug_report<T: Arg>(&self, output: Option<T>) -> crate::command::Result<ProcessResult> {
		Client::bug_report(&self.adb, &self.device, output).await
	}

	///
	/// Root is required
	///
	pub async fn disable_verity(&self) -> crate::command::Result<()> {
		Client::disable_verity(&self.adb, &self.device).await
	}

	///
	/// Root is required
	///
	pub async fn get_mac_address(&self) -> crate::command::Result<MacAddress> {
		Client::get_mac_address(&self.adb, &self.device).await
	}

	///
	/// Root is required
	pub async fn get_wlan_address(&self) -> crate::command::Result<MacAddress> {
		Client::get_wlan_address(&self.adb, &self.device).await
	}

	pub async fn pull<'s, S, D>(&self, src: S, dst: D) -> crate::command::Result<ProcessResult>
	where
		S: Into<&'s str> + AsRef<OsStr> + Arg,
		D: AsRef<Path>,
	{
		Client::pull(&self.adb, &self.device, src, dst).await
	}

	pub async fn push<'d, S, D>(&self, src: S, dst: D) -> crate::command::Result<ProcessResult>
	where
		D: Into<&'d str> + AsRef<OsStr> + Arg,
		S: AsRef<Path>,
	{
		Client::push(&self.adb, &self.device, src, dst).await
	}

	pub async fn clear_logcat(&self) -> crate::command::Result<()> {
		Client::clear_logcat(&self.adb, &self.device).await
	}

	pub async fn logcat(&self, options: LogcatOptions, recv: Option<IntoFuture<Receiver<()>>>) -> crate::command::Result<ProcessResult> {
		Client::logcat(&self.adb, &self.device, options, recv).await
	}

	pub async fn api_level(&self) -> crate::command::Result<u8> {
		Client::api_level(&self.adb, &self.device).await
	}

	pub async fn version(&self) -> crate::command::Result<u8> {
		Client::version(&self.adb, &self.device).await
	}

	pub async fn name(&self) -> crate::command::Result<Option<String>> {
		Ok(Client::name(&self.adb, &self.device).await.ok())
	}

	pub async fn save_screencap(&self, output: File) -> crate::command::Result<()> {
		Client::save_screencap(&self.adb, &self.device, output).await
	}

	pub async fn copy_screencap(&self) -> crate::command::Result<()> {
		Client::copy_screencap(&self.adb, &self.device).await
	}

	pub async fn get_boot_id(&self) -> crate::command::Result<uuid::Uuid> {
		Client::get_boot_id(&self.adb, &self.device).await
	}

	pub async fn reboot(&self, reboot_type: Option<RebootType>) -> crate::command::Result<()> {
		Client::reboot(&self.adb, &self.device, reboot_type).await
	}

	pub async fn wait_for_device(&self, timeout: Option<Duration>) -> crate::command::Result<()> {
		Client::wait_for_device(&self.adb, &self.device, timeout).await
	}

	pub fn shell(&self) -> AdbShell {
		AdbShell { parent: self }
	}

	pub fn pm(&self) -> PackageManager {
		PackageManager { parent: AdbShell { parent: self } }
	}

	pub fn am(&self) -> ActivityManager {
		ActivityManager { parent: AdbShell { parent: self } }
	}
}

impl<'a> Into<AdbShell<'a>> for &'a AdbClient {
	fn into(self: &'a AdbClient) -> AdbShell<'a> {
		self.shell()
	}
}

impl<'a> AdbShell<'a> {
	pub fn pm(&self) -> PackageManager {
		PackageManager { parent: self.clone() }
	}

	pub async fn whoami(&self) -> crate::command::Result<Option<String>> {
		Shell::whoami(&self.parent.adb, &self.parent.device).await
	}

	pub async fn which(&self, command: &str) -> crate::command::Result<Option<String>> {
		Shell::which(&self.parent.adb, &self.parent.device, command).await
	}

	pub async fn getprop(&self, key: &str) -> crate::command::Result<String> {
		let value = Shell::getprop(&self.parent.adb, &self.parent.device, key).await?;
		Arg::as_str(&value).map(|f| f.to_string()).map_err(|e| AdbError::Errno(e))
	}

	pub async fn setprop<T: Arg>(&self, key: &str, value: T) -> crate::command::Result<()> {
		Shell::setprop(&self.parent.adb, &self.parent.device, key, value).await
	}

	pub async fn getprop_type(&self, key: &str) -> crate::command::Result<String> {
		let result = Shell::getprop_type(&self.parent.adb, &self.parent.device, key).await?;
		Ok(Arg::as_str(&result)?.to_string())
	}

	pub async fn cat<T: Arg>(&self, path: T) -> crate::command::Result<Vec<u8>> {
		Shell::cat(&self.parent.adb, &self.parent.device, path).await
	}

	pub async fn getprops(&self) -> crate::command::Result<Vec<Property>> {
		Shell::getprops(&self.parent.adb, &self.parent.device).await
	}

	pub async fn exists<T: Arg>(&self, path: T) -> crate::command::Result<bool> {
		Shell::exists(&self.parent.adb, &self.parent.device, path).await
	}

	pub async fn rm<'s, S: Arg>(&self, path: S, options: Option<Vec<&str>>) -> crate::command::Result<bool> {
		Shell::rm(&self.parent.adb, &self.parent.device, path, options).await
	}

	pub async fn is_file<T: Arg>(&self, path: T) -> crate::command::Result<bool> {
		Shell::is_file(&self.parent.adb, &self.parent.device, path).await
	}

	pub async fn is_dir<T: Arg>(&self, path: T) -> crate::command::Result<bool> {
		Shell::is_dir(&self.parent.adb, &self.parent.device, path).await
	}

	pub async fn is_symlink<T: Arg>(&self, path: T) -> crate::command::Result<bool> {
		Shell::is_symlink(&self.parent.adb, &self.parent.device, path).await
	}

	///
	/// List directory
	pub async fn ls<'t, T>(&self, path: T, options: Option<&str>) -> crate::command::Result<Vec<String>>
	where
		T: Into<&'t str> + AsRef<OsStr> + Arg,
	{
		Shell::ls(&self.parent.adb, &self.parent.device, path, options).await
	}

	pub async fn save_screencap<'t, T: Into<&'t str> + AsRef<OsStr> + Arg>(&self, path: T) -> crate::command::Result<ProcessResult> {
		Shell::save_screencap(&self.parent.adb, &self.parent.device, path).await
	}

	///
	/// Root is required
	///
	pub async fn list_settings(&self, settings_type: SettingsType) -> crate::command::Result<Vec<Property>> {
		Shell::list_settings(&self.parent.adb, &self.parent.device, settings_type).await
	}

	///
	/// Root is required
	pub async fn get_setting(&self, settings_type: SettingsType, key: &str) -> crate::command::Result<Option<String>> {
		Shell::get_setting(&self.parent.adb, &self.parent.device, settings_type, key).await
	}

	pub async fn put_setting(&self, settings_type: SettingsType, key: &str, value: &str) -> crate::command::Result<()> {
		Shell::put_setting(&self.parent.adb, &self.parent.device, settings_type, key, value).await
	}

	pub async fn delete_setting(&self, settings_type: SettingsType, key: &str) -> crate::command::Result<()> {
		Shell::delete_setting(&self.parent.adb, &self.parent.device, settings_type, key).await
	}

	pub async fn dumpsys_list(&self, proto_only: bool, priority: Option<DumpsysPriority>) -> crate::command::Result<Vec<String>> {
		Shell::dumpsys_list(&self.parent.adb, &self.parent.device, proto_only, priority).await
	}

	pub async fn dumpsys(
		&self,
		service: Option<&str>,
		arguments: Option<Vec<String>>,
		timeout: Option<Duration>,
		pid: bool,
		thread: bool,
		proto: bool,
		skip: Option<Vec<String>>,
	) -> crate::command::Result<ProcessResult> {
		Shell::dumpsys(&self.parent.adb, &self.parent.device, service, arguments, timeout, pid, thread, proto, skip).await
	}

	pub async fn is_screen_on(&self) -> crate::command::Result<bool> {
		Shell::is_screen_on(&self.parent.adb, &self.parent.device).await
	}

	pub async fn screen_record(&self, options: Option<ScreenRecordOptions>, output: &str, signal: Option<IntoFuture<Receiver<()>>>) -> crate::command::Result<ProcessResult> {
		Shell::screen_record(&self.parent.adb, &self.parent.device, options, output, signal).await
	}

	pub async fn get_events(&self) -> crate::command::Result<Vec<(String, String)>> {
		Shell::get_events(&self.parent.adb, &self.parent.device).await
	}

	///
	/// Root may be required
	pub async fn send_event(&self, event: &str, code_type: i32, code: i32, value: i32) -> crate::command::Result<()> {
		Shell::send_event(&self.parent.adb, &self.parent.device, event, code_type, code, value).await
	}

	pub async fn send_motion(&self, source: Option<InputSource>, motion: MotionEvent, pos: (i32, i32)) -> crate::command::Result<()> {
		Shell::send_motion(&self.parent.adb, &self.parent.device, source, motion, pos).await
	}

	pub async fn send_draganddrop(&self, source: Option<InputSource>, duration: Option<Duration>, from_pos: (i32, i32), to_pos: (i32, i32)) -> crate::command::Result<()> {
		Shell::send_draganddrop(&self.parent.adb, &self.parent.device, source, duration, from_pos, to_pos).await
	}

	pub async fn send_press(&self, source: Option<InputSource>) -> crate::command::Result<()> {
		Shell::send_press(&self.parent.adb, &self.parent.device, source).await
	}

	pub async fn send_keycombination(&self, source: Option<InputSource>, keycodes: Vec<KeyCode>) -> crate::command::Result<()> {
		Shell::send_keycombination(&self.parent.adb, &self.parent.device, source, keycodes).await
	}

	pub async fn exec<T>(&self, args: Vec<T>, signal: Option<IntoFuture<Receiver<()>>>) -> crate::command::Result<ProcessResult>
	where
		T: Into<String> + AsRef<OsStr>,
	{
		Shell::exec(&self.parent.adb, &self.parent.device, args, signal).await
	}

	pub async fn exec_timeout<T>(&self, args: Vec<T>, timeout: Option<Duration>, signal: Option<IntoFuture<Receiver<()>>>) -> crate::command::Result<ProcessResult>
	where
		T: Into<String> + AsRef<OsStr>,
	{
		Shell::exec_timeout(&self.parent.adb, &self.parent.device, args, timeout, signal).await
	}

	pub async fn broadcast(&self, intent: &Intent) -> crate::command::Result<()> {
		Shell::broadcast(&self.parent.adb, &self.parent.device, intent).await
	}

	pub async fn start(&self, intent: &Intent) -> crate::command::Result<()> {
		Shell::start(&self.parent.adb, &self.parent.device, intent).await
	}

	pub async fn start_service(&self, intent: &Intent) -> crate::command::Result<()> {
		Shell::start_service(&self.parent.adb, &self.parent.device, intent).await
	}

	pub async fn force_stop(&self, package_name: &str) -> crate::command::Result<()> {
		Shell::force_stop(&self.parent.adb, &self.parent.device, package_name).await
	}

	pub async fn get_enforce(&self) -> crate::command::Result<SELinuxType> {
		Shell::get_enforce(&self.parent.adb, &self.parent.device).await
	}

	pub async fn set_enforce(&self, enforce: SELinuxType) -> crate::command::Result<()> {
		Shell::set_enforce(&self.parent.adb, &self.parent.device, enforce).await
	}

	pub async fn send_keyevent(&self, keycode: KeyCode, event_type: Option<KeyEventType>, source: Option<InputSource>) -> crate::command::Result<()> {
		Shell::send_keyevent(&self.parent.adb, &self.parent.device, keycode, event_type, source).await
	}

	pub async fn send_keyevents(&self, keycodes: Vec<KeyCode>, source: Option<InputSource>) -> crate::command::Result<()> {
		Shell::send_keyevents(&self.parent.adb, &self.parent.device, keycodes, source).await
	}
}
