use std::ffi::OsString;
use std::fmt::{Debug, Display, Formatter};
use std::net::{AddrParseError, SocketAddr};
use std::str::FromStr;
use std::vec::IntoIter;

use crate::error::Error;
use crate::traits::AsArgs;
use crate::types::ConnectionType;

#[allow(dead_code)]
impl ConnectionType {
	fn values(&self) -> Vec<OsString> {
		match self {
			ConnectionType::TcpIp(sock) => vec![
				"-s".into(),
				sock.to_string().into(),
			],
			ConnectionType::Transport(id) => vec![
				"-t".into(),
				id.to_string().into(),
			],
			ConnectionType::USB => vec!["-d".into()],
		}
	}

	pub fn from_socket_addr<I: Into<SocketAddr>>(socket_addr: I) -> Self {
		ConnectionType::TcpIp(socket_addr.into())
	}

	pub fn try_from_ip(value: &str) -> crate::result::Result<ConnectionType> {
		Ok(ConnectionType::TcpIp(value.parse()?))
	}
}

impl AsArgs<OsString> for ConnectionType {
	fn as_args(&self) -> Vec<OsString> {
		self.values()
	}
}

impl Display for ConnectionType {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			ConnectionType::TcpIp(sock) => write!(f, "ip:{sock}"),
			ConnectionType::Transport(id) => write!(f, "transport_id:{id}"),
			ConnectionType::USB => write!(f, "usb"),
		}
	}
}

impl Debug for ConnectionType {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let mut debug = f.debug_struct("AddressType");
		match self {
			ConnectionType::TcpIp(sock) => debug.field("ip", sock),
			ConnectionType::Transport(id) => debug.field("transport_id", id),
			ConnectionType::USB => debug.field("usb", &""),
		};
		debug.finish()
	}
}

impl From<SocketAddr> for ConnectionType {
	fn from(value: SocketAddr) -> Self {
		ConnectionType::from_socket_addr(value)
	}
}

impl FromStr for ConnectionType {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let addr: Result<SocketAddr, AddrParseError> = s.parse();
		match addr {
			Ok(addr) => Ok(ConnectionType::TcpIp(addr)),
			Err(_err) => Err(Error::AddressParseError),
		}
	}
}

impl TryFrom<&str> for ConnectionType {
	type Error = Error;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		Self::from_str(value)
	}
}

impl IntoIterator for ConnectionType {
	type Item = OsString;
	type IntoIter = IntoIter<Self::Item>;

	fn into_iter(self) -> Self::IntoIter {
		self.as_args().into_iter()
	}
}

#[cfg(test)]
mod test {
	use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
	use std::str::FromStr;

	use crate::test::test::init_log;
	use simple_cmd::debug::CommandDebug;

	use crate::types::ConnectionType;

	#[test]
	fn test_parse_address() {
		let ip = "192.168.1.34:5555";
		let sock_addr = ip.parse::<SocketAddr>().expect("failed to parse ip address");
		let _address: ConnectionType = ConnectionType::from_socket_addr(sock_addr);
		let _address: ConnectionType = ConnectionType::try_from_ip(ip).expect("failed to parse ip address");
		let _address: ConnectionType = ip.parse().unwrap();
		let _address = ConnectionType::from(sock_addr);
		let _address = ConnectionType::from_str(ip);

		ConnectionType::from_str("invalid").expect_err("Expected error");
	}

	#[test]
	fn test_display() {
		assert_eq!("usb", ConnectionType::USB.to_string());
		assert_eq!("transport_id:4", ConnectionType::Transport(4).to_string());
		assert_eq!(
			"ip:192.168.1.1:5555",
			ConnectionType::TcpIp("192.168.1.1:5555".parse().unwrap()).to_string()
		);
	}

	#[test]
	fn test_debug() {
		let addr = format!("{:#?}", ConnectionType::USB);
		println!("{addr}");
		assert_eq!("AddressType {\n    usb: \"\",\n}", addr);

		let addr = format!("{:#?}", ConnectionType::Transport(4));
		println!("{addr}");
		assert_eq!("AddressType {\n    transport_id: 4,\n}", addr);

		let addr = format!("{:#?}", ConnectionType::TcpIp("192.168.1.1:5555".parse().unwrap()));
		println!("{addr}");
		assert_eq!("AddressType {\n    ip: 192.168.1.1:5555,\n}", addr);
	}

	#[test]
	fn test_copy() {
		let addr = ConnectionType::USB;
		let addr2 = addr.clone();
		assert_eq!(addr, addr2);

		let addr = ConnectionType::Transport(4);
		let addr2 = addr.clone();
		assert_eq!(addr, addr2);

		let addr = ConnectionType::try_from("192.168.1.1:5555").unwrap();
		let addr2 = addr.clone();
		assert_eq!(addr, addr2);
	}

	#[test]
	fn test_args() {
		init_log();
		let addr = ConnectionType::USB;
		let mut builder = std::process::Command::new("adb");
		let cmd = builder.args(addr).arg("get-state").debug();
		let output = cmd.output();
		println!("output: {output:?}");

		let addr = ConnectionType::Transport(4);
		let mut builder = std::process::Command::new("adb");
		let cmd = builder.args(addr).arg("get-state").debug();
		let output = cmd.output();
		println!("output: {output:?}");

		let addr = ConnectionType::TcpIp(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 34), 5555)));
		let mut builder = std::process::Command::new("adb");
		let cmd = builder.args(addr).arg("get-state").debug();
		let output = cmd.output();
		println!("output: {output:?}");
	}
}
