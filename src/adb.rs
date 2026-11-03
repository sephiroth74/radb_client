use std::ffi::OsStr;
use std::fmt::{Debug, Display, Formatter};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::str::FromStr;
use std::time::Duration;

use crossbeam_channel::Receiver;
use simple_cmd::prelude::OutputExt;
use simple_cmd::CommandBuilder;
use which::which;

use crate::command::{run, run_string, run_unchecked};
use crate::connection::ConnectionType;
use crate::error::{Error, Result};

/// The `adb` executable on the host.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Adb {
    path: PathBuf,
    debug: bool,
}

/// State of a device, as reported by `adb devices`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DeviceState {
    Device,
    Offline,
    Unauthorized,
    Authorizing,
    Connecting,
    Recovery,
    Rescue,
    Sideload,
    Bootloader,
    Host,
    NoPermissions,
    Unknown(String),
}

/// A device listed by `adb devices -l`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdbDevice {
    /// The serial used by adb: `emulator-5554`, `192.168.1.10:5555`, a USB serial...
    pub serial: String,
    pub state: DeviceState,
    pub product: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
    /// USB port, for USB devices.
    pub usb: Option<String>,
    pub transport_id: Option<u64>,
    /// The connection to use for this device (by serial, which survives adbd restarts).
    pub addr: ConnectionType,
}

impl Adb {
    /// Find `adb` in the user `PATH`.
    ///
    /// ```no_run
    /// use radb_client::Adb;
    /// let adb = Adb::new().expect("failed to find adb");
    /// ```
    pub fn new() -> Result<Adb> {
        Ok(Adb::from(which("adb")?))
    }

    /// Use the `adb` executable at the given path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Adb {
        Adb::from(path.as_ref().to_path_buf())
    }

    /// Log the command lines (at trace level, target `simple_cmd`) and command timeouts.
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn debug(&self) -> bool {
        self.debug
    }

    pub(crate) fn command(&self) -> CommandBuilder {
        CommandBuilder::new(&self.path).with_debug(self.debug)
    }

    /// Run `adb <selector> <args>`, failing if it exits with a non-zero status.
    ///
    /// ```no_run
    /// use radb_client::{Adb, ConnectionType};
    ///
    /// let adb = Adb::new().expect("failed to find adb");
    /// let addr = ConnectionType::try_from_ip("192.168.1.100:5555").unwrap();
    /// // adb -s 192.168.1.100:5555 get-state
    /// let output = adb.exec(&addr, ["get-state"], None, None).unwrap();
    /// println!("{}", String::from_utf8_lossy(&output.stdout));
    /// ```
    pub fn exec<I, S>(
        &self,
        addr: &ConnectionType,
        args: I,
        cancel: Option<Receiver<()>>,
        timeout: Option<Duration>,
    ) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run(self
            .command()
            .args(addr.as_args())
            .args(args)
            .signal(cancel)
            .timeout(timeout))
    }

    /// Check if mdns is available.
    pub fn mdns_check(&self) -> bool {
        run_unchecked(self.command().args(["mdns", "check"]))
            .map(|output| output.success())
            .unwrap_or(false)
    }

    /// List the devices known to the adb server, in any state.
    ///
    /// ```no_run
    /// use radb_client::{Adb, Client, DeviceState};
    ///
    /// let adb = Adb::new().unwrap();
    /// let emulator = adb
    ///     .list_devices()
    ///     .unwrap()
    ///     .into_iter()
    ///     .find(|d| d.serial.starts_with("emulator-") && d.state == DeviceState::Device)
    ///     .expect("no emulator found");
    /// let client = Client::new(adb, emulator.addr, false);
    /// ```
    pub fn list_devices(&self) -> Result<Vec<AdbDevice>> {
        let output = run_string(self.command().args(["devices", "-l"]))?;
        Ok(parse_devices(&output))
    }

    /// Disconnect all tcp/ip devices.
    pub fn disconnect_all(&self) -> Result<()> {
        run(self.command().arg("disconnect")).map(|_| ())
    }

    pub fn kill_server(&self) -> Result<()> {
        run(self.command().arg("kill-server")).map(|_| ())
    }

    pub fn start_server(&self) -> Result<()> {
        run(self.command().arg("start-server")).map(|_| ())
    }

    /// The version of the adb tool, e.g. `37.0.1-15733141`.
    pub fn version(&self) -> Result<String> {
        let output = run_string(self.command().arg("--version"))?;
        parse_version(&output).ok_or_else(|| Error::ParseError(format!("adb version: {output}")))
    }

    pub fn as_os_str(&self) -> &OsStr {
        self.path.as_os_str()
    }
}

impl DeviceState {
    pub fn is_online(&self) -> bool {
        *self == DeviceState::Device
    }
}

impl FromStr for DeviceState {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "device" => DeviceState::Device,
            "offline" => DeviceState::Offline,
            "unauthorized" => DeviceState::Unauthorized,
            "authorizing" => DeviceState::Authorizing,
            "connecting" => DeviceState::Connecting,
            "recovery" => DeviceState::Recovery,
            "rescue" => DeviceState::Rescue,
            "sideload" => DeviceState::Sideload,
            "bootloader" => DeviceState::Bootloader,
            "host" => DeviceState::Host,
            "no permissions" => DeviceState::NoPermissions,
            other => DeviceState::Unknown(other.to_string()),
        })
    }
}

impl Display for DeviceState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DeviceState::Device => "device",
            DeviceState::Offline => "offline",
            DeviceState::Unauthorized => "unauthorized",
            DeviceState::Authorizing => "authorizing",
            DeviceState::Connecting => "connecting",
            DeviceState::Recovery => "recovery",
            DeviceState::Rescue => "rescue",
            DeviceState::Sideload => "sideload",
            DeviceState::Bootloader => "bootloader",
            DeviceState::Host => "host",
            DeviceState::NoPermissions => "no permissions",
            DeviceState::Unknown(s) => s,
        };
        write!(f, "{s}")
    }
}

impl AdbDevice {
    pub fn is_online(&self) -> bool {
        self.state.is_online()
    }
}

impl Display for AdbDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.serial, self.state)?;
        for (key, value) in [
            ("product", &self.product),
            ("model", &self.model),
            ("device", &self.device),
        ] {
            if let Some(value) = value {
                write!(f, " {key}:{value}")?;
            }
        }
        if let Some(id) = self.transport_id {
            write!(f, " transport_id:{id}")?;
        }
        Ok(())
    }
}

/// Parse the output of `adb devices -l`.
pub(crate) fn parse_devices(output: &str) -> Vec<AdbDevice> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with("List of devices") && !line.starts_with('*')
        })
        .filter_map(parse_device_line)
        .collect()
}

fn parse_device_line(line: &str) -> Option<AdbDevice> {
    let mut tokens = line.split_whitespace();
    let serial = tokens.next()?.to_string();
    let mut state = tokens.next()?.to_string();
    let rest: Vec<&str> = tokens.collect();

    // "no permissions (reason...)" is the only state with spaces
    let mut fields = rest.as_slice();
    if state == "no" && rest.first() == Some(&"permissions") {
        state = "no permissions".to_string();
        fields = &rest[1..];
    }

    let mut device = AdbDevice {
        addr: match serial.parse::<SocketAddr>() {
            Ok(addr) => ConnectionType::TcpIp(addr),
            Err(_) => ConnectionType::Serial(serial.clone()),
        },
        serial,
        state: state.parse().unwrap_or(DeviceState::Unknown(state)),
        product: None,
        model: None,
        device: None,
        usb: None,
        transport_id: None,
    };

    for field in fields {
        if let Some((key, value)) = field.split_once(':') {
            match key {
                "product" => device.product = Some(value.to_string()),
                "model" => device.model = Some(value.to_string()),
                "device" => device.device = Some(value.to_string()),
                "usb" => device.usb = Some(value.to_string()),
                "transport_id" => device.transport_id = value.parse().ok(),
                _ => {}
            }
        }
    }
    Some(device)
}

/// Parse the output of `adb --version`.
pub(crate) fn parse_version(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("Version "))
        .map(|v| v.trim().to_string())
}

impl From<PathBuf> for Adb {
    fn from(path: PathBuf) -> Self {
        Adb { path, debug: false }
    }
}

impl Display for Adb {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}

impl Debug for Adb {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Adb").field(&self.path).finish()
    }
}

impl AsRef<OsStr> for Adb {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}

impl AsRef<Path> for Adb {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl From<Adb> for PathBuf {
    fn from(value: Adb) -> Self {
        value.path
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const DEVICES: &str = include_str!("../test_files/fixtures/adb_devices.txt");

    #[test]
    fn test_parse_devices() {
        let devices = parse_devices(DEVICES);
        assert_eq!(devices.len(), 6);

        let tcp = &devices[0];
        assert_eq!(tcp.serial, "192.168.1.114:5555");
        assert_eq!(tcp.state, DeviceState::Device);
        assert_eq!(
            tcp.addr,
            ConnectionType::TcpIp("192.168.1.114:5555".parse().unwrap())
        );
        assert_eq!(tcp.product.as_deref(), Some("SwisscomBox23"));
        assert_eq!(tcp.model.as_deref(), Some("IP2300"));
        assert_eq!(tcp.transport_id, Some(3));

        let emulator = &devices[1];
        assert_eq!(emulator.addr, ConnectionType::emulator(5554));
        assert_eq!(emulator.device.as_deref(), Some("emu64a"));
        assert!(emulator.is_online());

        // offline devices have no product/model, and transport ids can exceed 255
        let offline = &devices[2];
        assert_eq!(offline.state, DeviceState::Offline);
        assert_eq!(offline.product, None);
        assert_eq!(offline.transport_id, Some(300));

        let unauthorized = &devices[3];
        assert_eq!(unauthorized.state, DeviceState::Unauthorized);
        assert_eq!(unauthorized.usb.as_deref(), Some("1-1"));
        assert_eq!(unauthorized.addr, ConnectionType::serial("R58M123ABC"));

        assert_eq!(devices[4].state, DeviceState::Recovery);

        let mdns = &devices[5];
        assert_eq!(
            mdns.serial,
            "adb-28131FDH2000VM-QeZ8ZW._adb-tls-connect._tcp"
        );
        assert_eq!(mdns.addr, ConnectionType::serial(mdns.serial.clone()));
    }

    #[test]
    fn test_parse_devices_no_permissions() {
        let devices = parse_devices(
            "List of devices attached\n\
             * daemon not running; starting now at tcp:5037\n\
             0123456789 no permissions (missing udev rules? user is in the plugdev group); see [http://developer.android.com/tools/device.html] usb:3-2 transport_id:2\n",
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].state, DeviceState::NoPermissions);
        assert_eq!(devices[0].transport_id, Some(2));
    }

    #[test]
    fn test_parse_version() {
        let output = include_str!("../test_files/fixtures/api35/adb_version.txt");
        assert_eq!(parse_version(output).as_deref(), Some("37.0.1-15733141"));
        assert_eq!(parse_version("garbage"), None);
    }

    #[test]
    fn test_display() {
        let device = &parse_devices(DEVICES)[1];
        assert_eq!(
            device.to_string(),
            "emulator-5554 device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:4"
        );
        let adb = Adb::from_path("/usr/bin/adb");
        assert_eq!(adb.to_string(), "/usr/bin/adb");
        assert_eq!(format!("{adb:?}"), "Adb(\"/usr/bin/adb\")");
    }
}
