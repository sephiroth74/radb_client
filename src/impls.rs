use fmt::Debug;
use std::ffi::OsStr;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::net::{AddrParseError, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use lazy_static::lazy_static;
use mac_address::MacAddress;
use props_rs::Property;
use regex::Regex;
use which::Path;

use crate::{Adb, AddressType, Client, Device, DeviceAddress, SELinuxType, Shell};
use crate::AddressType::Sock;
use crate::client::{LogcatLevel, LogcatTag, RebootType};
use crate::intent::{Extra, Intent};
use crate::shell::ScreenRecordOptions;
use crate::traits::AdbDevice;
use crate::types::{AdbClient, AdbShell};
use crate::util::Vec8ToString;

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

    pub fn from_serial(input: &str) -> anyhow::Result<DeviceAddress> {
        Ok(DeviceAddress(AddressType::Name(input.to_string())))
    }

    pub fn from_transport_id(id: u8) -> anyhow::Result<DeviceAddress> {
        Ok(DeviceAddress(AddressType::Transport(id)))
    }

    pub fn from_ip(input: &str) -> anyhow::Result<DeviceAddress> {
        let addr: anyhow::Result<SocketAddr> = input.parse().context("failed to parse ip address");
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
    pub fn try_from_address(value: &DeviceAddress) -> anyhow::Result<Device> {
        match value.address_type() {
            AddressType::Sock(addr) => Device::from_sock_addr(addr),
            AddressType::Name(name) => Device::from_serial(name),
            AddressType::Transport(id) => Device::from_transport_id(*id),
        }
    }

    pub fn from_ip(input: &str) -> anyhow::Result<Device> {
        DeviceAddress::from_ip(input).map(|address| Device(address))
    }

    pub fn from_sock_addr(input: &SocketAddr) -> anyhow::Result<Device> {
        Ok(Device(DeviceAddress(AddressType::Sock(input.clone()))))
    }

    pub fn from_serial(input: &str) -> anyhow::Result<Device> {
        DeviceAddress::from_serial(input).map(|address| Device(address))
    }

    pub fn from_transport_id(id: u8) -> anyhow::Result<Device> {
        DeviceAddress::from_transport_id(id).map(|address| Device(address))
    }

    #[allow(dead_code)]
    fn from_device(input: &str) -> anyhow::Result<Device> {
        lazy_static! {
            static ref RE: Regex =
                Regex::new("(?P<ip>[^\\s]+)[\\s]+device product:(?P<device_product>[^\\s]+)\\smodel:(?P<model>[^\\s]+)\\sdevice:(?P<device>[^\\s]+)\\stransport_id:(?P<transport_id>[^\\s]+)").unwrap();
        }

        if !RE.is_match(input) {
            let msg = format!("Invalid IP address: {}", input);
            return Err(anyhow::Error::msg(msg));
        }

        let cap = RE.captures(input).unwrap();
        let ip = cap
            .name("ip")
            .ok_or(anyhow::Error::msg("Device serial not found"))?
            .as_str();
        DeviceAddress::from_ip(ip).map(|address| Device(address))
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
        Device::from_serial(value).unwrap()
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
    type Error = anyhow::Error;

    fn try_from(value: &dyn AdbDevice) -> Result<Self, Self::Error> {
        Device::try_from_address(value.addr())
    }
}

impl AsRef<OsStr> for Adb {
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
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

impl From<ScreenRecordOptions> for Vec<String> {
    fn from(value: ScreenRecordOptions) -> Self {
        let mut args: Vec<String> = vec![];
        if value.bitrate.is_some() {
            args.push(String::from("--bit-rate"));
            args.push(format!("{:}", value.bitrate.unwrap()));
        }

        if value.timelimit.is_some() {
            args.push(String::from("--time-limit"));
            args.push(format!("{:}", value.timelimit.unwrap().as_secs()));
        }

        if value.rotate.unwrap_or(false) {
            args.push(String::from("--rotate"))
        }

        if value.bug_report.unwrap_or(false) {
            args.push(String::from("--bugreport"))
        }

        if value.verbose {
            args.push(String::from("--verbose"))
        }

        if value.size.is_some() {
            let size = value.size.unwrap();
            args.push(String::from("--size"));
            args.push(format!("{:}x{:}", size.0, size.1));
        }
        args
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

        if self.action.is_some() {
            args.push(format!("-a {:}", self.action.as_ref().unwrap()));
        }

        if self.data.is_some() {
            args.push(format!("-d {:}", self.data.as_ref().unwrap()));
        }

        if self.mime_type.is_some() {
            args.push(format!("-t {:}", self.mime_type.as_ref().unwrap()));
        }

        if self.category.is_some() {
            args.push(format!("-c {:}", self.category.as_ref().unwrap()));
        }

        if self.component.is_some() {
            args.push(format!("-n {:}", self.component.as_ref().unwrap()));
        }

        if self.package.is_some() {
            args.push(format!("-p {:}", self.package.as_ref().unwrap()));
        }

        if self.user_id.is_some() {
            args.push(format!("--user {:}", self.user_id.as_ref().unwrap()));
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
                output.push(format!(
                    "--eia {:} {:}",
                    entry.0,
                    entry
                        .1
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            });
        }

        if !self.ela.is_empty() {
            self.ela.iter().for_each(|entry| {
                output.push(format!(
                    "--ela {:} {:}",
                    entry.0,
                    entry
                        .1
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            });
        }

        if !self.efa.is_empty() {
            self.efa.iter().for_each(|entry| {
                output.push(format!(
                    "--efa {:} {:}",
                    entry.0,
                    entry
                        .1
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                ));
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
    type Error = anyhow::Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let opt_string = value.as_str();
        match opt_string {
            None => Err(anyhow!("invalid string")),
            Some(s) => s.try_into(),
        }
    }
}

impl TryFrom<&str> for SELinuxType {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim() {
            "Enforcing" => Ok(SELinuxType::Enforcing),
            "Permissive" => Ok(SELinuxType::Permissive),
            _ => Err(anyhow!("not found")),
        }
    }
}

impl TryFrom<Device> for AdbClient {
    type Error = anyhow::Error;

    fn try_from(value: Device) -> Result<Self, Self::Error> {
        AdbClient::from_device(value)
    }
}

impl AdbClient {
    pub fn from_device(device: Device) -> anyhow::Result<AdbClient> {
        match Adb::new().context("adb not found") {
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
    pub async fn connect(&self, timeout: Option<std::time::Duration>) -> anyhow::Result<()> {
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

    ///
    /// Root is required
    ///
    pub async fn disable_verity(&self) -> crate::command::Result<()> {
        Client::disable_verity(&self.adb, &self.device).await
    }

    ///
    /// Root is required
    ///
    pub async fn get_mac_address(&self) -> anyhow::Result<MacAddress> {
        Client::get_mac_address(&self.adb, &self.device).await
    }

    ///
    /// Root is required
    pub async fn get_wlan_address(&self) -> anyhow::Result<MacAddress> {
        Client::get_wlan_address(&self.adb, &self.device).await
    }

    pub fn shell(&self) -> AdbShell {
        AdbShell { parent: self }
    }
}

impl<'a> AdbShell<'a> {
    pub async fn whoami(&self) -> crate::command::Result<Option<String>> {
        Shell::whoami(&self.parent.adb, &self.parent.device).await
    }

    pub async fn which(&self, command: &str) -> crate::command::Result<Option<String>> {
        Shell::which(&self.parent.adb, &self.parent.device, command).await
    }

    pub async fn getprop(&self, key: &str) -> crate::command::Result<Vec<u8>> {
        Shell::getprop(&self.parent.adb, &self.parent.device, key).await
    }

    pub async fn cat<'t, T>(&self, path: T) -> crate::command::Result<Vec<u8>>
    where
        T: Into<&'t str> + AsRef<OsStr>,
    {
        Shell::cat(&self.parent.adb, &self.parent.device, path).await
    }

    pub async fn getprops(&self) -> crate::command::Result<Vec<Property>> {
        Shell::getprops(&self.parent.adb, &self.parent.device).await
    }

    pub async fn exists<'t, T>(&self, path: T) -> crate::command::Result<bool>
    where
        T: Into<&'t str> + AsRef<OsStr>,
    {
        Shell::exists(&self.parent.adb, &self.parent.device, path).await
    }

    pub async fn is_file<'t, T>(&self, path: T) -> crate::command::Result<bool>
    where
        T: Into<&'t str> + AsRef<OsStr>,
    {
        Shell::is_file(&self.parent.adb, &self.parent.device, path).await
    }

    pub async fn is_dir<'t, T>(&self, path: T) -> crate::command::Result<bool>
    where
        T: Into<&'t str> + AsRef<OsStr>,
    {
        Shell::is_dir(&self.parent.adb, &self.parent.device, path).await
    }

    pub async fn is_symlink<'t, T>(&self, path: T) -> crate::command::Result<bool>
    where
        T: Into<&'t str> + AsRef<OsStr>,
    {
        Shell::is_symlink(&self.parent.adb, &self.parent.device, path).await
    }

    pub async fn list_dir<'t, T>(&self, path: T) -> crate::command::Result<Vec<String>>
        where
            T: Into<&'t str> + AsRef<OsStr> + Into<std::path::PathBuf>,
    {
        Shell::list_dir(&self.parent.adb, &self.parent.device, path).await
    }
}
