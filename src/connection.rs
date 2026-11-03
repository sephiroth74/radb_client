//! [`ConnectionType`]: how adb selects the target device (`-s`, `-t`, `-d`, `-e`).

use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use crate::error::{Error, Result};

/// Default adb port for tcp/ip connections.
pub const DEFAULT_ADB_PORT: u16 = 5555;

/// How adb selects the target device.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[non_exhaustive]
pub enum ConnectionType {
    /// A device connected over tcp/ip (`-s ip:port`).
    TcpIp(SocketAddr),
    /// Any device by serial (`-s serial`): `emulator-5554`, a USB serial, an mDNS name...
    /// The serial stays the same when adbd restarts (`adb root`, reboot).
    Serial(String),
    /// A transport id (`-t id`). The id changes every time the connection is
    /// re-established (`adb root`, reboot, adb server restart): prefer [`ConnectionType::Serial`].
    Transport(u64),
    /// The only device connected via USB (`-d`).
    USB,
    /// The only running emulator (`-e`).
    Emulator,
}

impl ConnectionType {
    /// A tcp/ip device.
    pub fn from_socket_addr<I: Into<SocketAddr>>(socket_addr: I) -> Self {
        ConnectionType::TcpIp(socket_addr.into())
    }

    /// Parse `ip:port` or just `ip` (the port defaults to 5555).
    ///
    /// ```
    /// use radb_client::ConnectionType;
    ///
    /// let conn = ConnectionType::try_from_ip("192.168.1.101").unwrap();
    /// assert_eq!(conn.to_string(), "192.168.1.101:5555");
    /// ```
    pub fn try_from_ip(value: &str) -> Result<ConnectionType> {
        if let Ok(addr) = value.parse::<SocketAddr>() {
            return Ok(ConnectionType::TcpIp(addr));
        }
        match value.parse::<IpAddr>() {
            Ok(ip) => Ok(ConnectionType::TcpIp(SocketAddr::new(ip, DEFAULT_ADB_PORT))),
            Err(_) => Err(Error::AddressParseError(value.to_string())),
        }
    }

    /// A device by serial (`-s <serial>`).
    pub fn serial<S: Into<String>>(serial: S) -> Self {
        ConnectionType::Serial(serial.into())
    }

    /// The emulator listening on the given console port (`emulator-<port>`).
    pub fn emulator(console_port: u16) -> Self {
        ConnectionType::Serial(format!("emulator-{console_port}"))
    }

    /// The adb arguments that select this device.
    pub fn as_args(&self) -> Vec<OsString> {
        match self {
            ConnectionType::TcpIp(sock) => vec!["-s".into(), sock.to_string().into()],
            ConnectionType::Serial(serial) => vec!["-s".into(), serial.into()],
            ConnectionType::Transport(id) => vec!["-t".into(), id.to_string().into()],
            ConnectionType::USB => vec!["-d".into()],
            ConnectionType::Emulator => vec!["-e".into()],
        }
    }
}

impl Display for ConnectionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionType::TcpIp(sock) => write!(f, "{sock}"),
            ConnectionType::Serial(serial) => write!(f, "{serial}"),
            ConnectionType::Transport(id) => write!(f, "transport_id:{id}"),
            ConnectionType::USB => write!(f, "usb"),
            ConnectionType::Emulator => write!(f, "emulator"),
        }
    }
}

impl From<SocketAddr> for ConnectionType {
    fn from(value: SocketAddr) -> Self {
        ConnectionType::TcpIp(value)
    }
}

/// Same as [`ConnectionType::try_from_ip`].
impl FromStr for ConnectionType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        ConnectionType::try_from_ip(s)
    }
}

impl TryFrom<&str> for ConnectionType {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        ConnectionType::try_from_ip(value)
    }
}

#[cfg(test)]
mod test {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use super::*;

    #[test]
    fn test_parse_address() {
        let ip = "192.168.1.34:5555";
        let sock_addr: SocketAddr = ip.parse().unwrap();
        let expected = ConnectionType::TcpIp(sock_addr);

        assert_eq!(ConnectionType::from_socket_addr(sock_addr), expected);
        assert_eq!(ConnectionType::try_from_ip(ip).unwrap(), expected);
        assert_eq!(ip.parse::<ConnectionType>().unwrap(), expected);
        assert_eq!(ConnectionType::from(sock_addr), expected);
        assert_eq!(ConnectionType::try_from(ip).unwrap(), expected);

        // the port defaults to 5555
        assert_eq!(
            ConnectionType::try_from_ip("192.168.1.34").unwrap(),
            expected
        );
        assert_eq!(
            ConnectionType::try_from_ip("192.168.1.34:5556").unwrap(),
            ConnectionType::TcpIp("192.168.1.34:5556".parse().unwrap())
        );

        assert!(matches!(
            ConnectionType::from_str("invalid"),
            Err(Error::AddressParseError(_))
        ));
    }

    #[test]
    fn test_display() {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 1), 5555));
        assert_eq!(ConnectionType::TcpIp(addr).to_string(), "192.168.1.1:5555");
        assert_eq!(ConnectionType::emulator(5554).to_string(), "emulator-5554");
        assert_eq!(ConnectionType::Transport(4).to_string(), "transport_id:4");
        assert_eq!(ConnectionType::USB.to_string(), "usb");
        assert_eq!(ConnectionType::Emulator.to_string(), "emulator");
    }

    #[test]
    fn test_args() {
        let to_strings = |c: ConnectionType| -> Vec<String> {
            c.as_args()
                .into_iter()
                .map(|s| s.into_string().unwrap())
                .collect()
        };
        assert_eq!(
            to_strings(ConnectionType::try_from_ip("10.0.0.1").unwrap()),
            ["-s", "10.0.0.1:5555"]
        );
        assert_eq!(
            to_strings(ConnectionType::serial("R58M123ABC")),
            ["-s", "R58M123ABC"]
        );
        assert_eq!(
            to_strings(ConnectionType::emulator(5556)),
            ["-s", "emulator-5556"]
        );
        assert_eq!(to_strings(ConnectionType::Transport(300)), ["-t", "300"]);
        assert_eq!(to_strings(ConnectionType::USB), ["-d"]);
        assert_eq!(to_strings(ConnectionType::Emulator), ["-e"]);
    }
}
