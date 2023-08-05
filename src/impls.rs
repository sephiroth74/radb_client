use std::ffi::OsStr;
use std::fmt;
use std::fmt::Formatter;
use std::net::{AddrParseError, Ipv4Addr};
use std::num::ParseIntError;
use std::str::FromStr;
use std::time::Duration;

use lazy_static::lazy_static;
use regex::Regex;

use crate::client::{LogcatLevel, LogcatTag, RebootType};
use crate::shell::ScreenRecordOptions;
use crate::{Adb, AdbDevice, AddressType, Device, DeviceAddress, IpV4AddrAndPort};

impl DeviceAddress {
    pub fn address_type(&self) -> &AddressType {
        &self.0
    }

    pub fn serial(&self) -> Option<String> {
        match &self.0 {
            AddressType::Ip(ip) => Some(ip.to_string()),
            AddressType::Name(name) => Some(name.to_string()),
            AddressType::Transport(_) => None,
        }
    }

    pub fn transport_id(&self) -> Option<u8> {
        match self.0 {
            AddressType::Ip(_) => None,
            AddressType::Name(_) => None,
            AddressType::Transport(id) => Some(id),
        }
    }

    pub fn from_serial(input: &str) -> anyhow::Result<DeviceAddress> {
        Ok(DeviceAddress(AddressType::Name(input.to_string())))
    }

    pub fn from_transport_id(id: u8) -> anyhow::Result<DeviceAddress> {
        Ok(DeviceAddress(AddressType::Transport(id)))
    }

    pub fn from_ip(input: &str) -> anyhow::Result<DeviceAddress> {
        lazy_static! {
            static ref RE: Regex = Regex::new("([0-9]{1,3}.[0-9]{1,3}.[0-9]{1,3}.[0-9]{1,3}):?([0-9]*)?").unwrap();
        }

        let mut port: u16 = 5555;
        let mut ip: Option<Ipv4Addr> = None;

        if !RE.is_match(input) {
            let msg = format!("Invalid IP address: {}", input);
            return Err(anyhow::Error::msg(msg));
        }

        for cap in RE.captures_iter(input) {
            let input = &cap[1];
            let address = input.parse().map_err(anyhow::Error::msg)?;
            ip = Some(address);
            if cap.get(2).is_some() && !cap[2].is_empty() {
                port = cap[2].parse::<u16>()?;
            }
        }

        match ip {
            None => Err(anyhow::Error::msg("Invalid ip address")),
            Some(address) => Ok(DeviceAddress(AddressType::Ip(IpV4AddrAndPort { ip: address, port }))),
        }
    }
}

impl fmt::Display for DeviceAddress {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &self.0 {
            AddressType::Name(name) => write!(f, "name:{}", name),
            AddressType::Transport(id) => write!(f, "transport_id:{}", id),
            AddressType::Ip(ip) => write!(f, "ip:{}", ip),
        }
    }
}

impl fmt::Debug for DeviceAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.0 {
            AddressType::Name(name) => write!(f, "{}", name),
            AddressType::Transport(id) => write!(f, "{}", id),
            AddressType::Ip(ip) => write!(f, "{}", ip),
        }
    }
}

impl Device {
    pub fn try_from_address(value: &DeviceAddress) -> anyhow::Result<Device> {
        match value.address_type() {
            AddressType::Ip(ip) => Device::from_ip_and_port(ip),
            AddressType::Name(name) => Device::from_serial(name),
            AddressType::Transport(id) => Device::from_transport_id(*id),
        }
    }

    pub fn from_ip(input: &str) -> anyhow::Result<Device> {
        DeviceAddress::from_ip(input).map(|address| Device(address))
    }

    pub fn from_ip_and_port(input: &IpV4AddrAndPort) -> anyhow::Result<Device> {
        Ok(Device(DeviceAddress(AddressType::Ip(input.clone()))))
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
            AddressType::Ip(ip) => vec!["-s".to_string(), ip.to_string()],
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

impl fmt::Display for Device {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Device{{address={:?}}}", self.0)
    }
}

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

impl fmt::Display for LogcatLevel {
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

impl fmt::Display for LogcatTag {
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

impl fmt::Display for IpV4AddrAndPort {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

impl FromStr for IpV4AddrAndPort {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        lazy_static! {
            static ref RE: Regex = Regex::new("([0-9]{1,3}.[0-9]{1,3}.[0-9]{1,3}.[0-9]{1,3}):?([0-9]*)?").unwrap();
        }
        let mut port: u16 = 5555;
        let mut ip: Option<Ipv4Addr> = None;

        if !RE.is_match(input) {
            return Err(format!("Invalid IP address: {}", input));
        }

        for cap in RE.captures_iter(input) {
            let input = &cap[1];
            let address = input.parse().map_err(|e: AddrParseError| e.to_string())?;
            ip = Some(address);
            if cap.get(2).is_some() && !cap[2].is_empty() {
                port = cap[2]
                    .parse::<u16>()
                    .map_err(|e: ParseIntError| e.to_string())?;
            }
        }

        match ip {
            None => Err("Invalid ip address".to_string()),
            Some(address) => Ok(IpV4AddrAndPort { ip: address, port }),
        }
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
