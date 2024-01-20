use std::ffi::OsString;
use std::fmt::{Debug, Display, Formatter};
use std::net::{AddrParseError, SocketAddr};
use std::str::FromStr;
use std::vec::IntoIter;

use crate::v2::error::Error;
use crate::v2::traits::AsArgs;
use crate::v2::types::AddressType;

#[allow(dead_code)]
impl AddressType {
	fn values(&self) -> Vec<OsString> {
		match self {
			AddressType::Sock(sock) => vec![
				"-s".into(),
				sock.to_string().into(),
			],
			AddressType::Transport(id) => vec![
				"-t".into(),
				id.to_string().into(),
			],
			AddressType::USB => vec!["-d".into()],
		}
	}

	pub fn from_socket_addr<I: Into<SocketAddr>>(socket_addr: I) -> Self {
		AddressType::Sock(socket_addr.into())
	}

	pub fn try_from_ip(value: &str) -> crate::v2::result::Result<AddressType> {
		Ok(AddressType::Sock(value.parse()?))
	}
}

impl AsArgs<OsString> for AddressType {
	fn as_args(&self) -> Vec<OsString> {
		self.values()
	}
}

impl Display for AddressType {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			AddressType::Sock(sock) => write!(f, "ip:{sock}"),
			AddressType::Transport(id) => write!(f, "transport_id:{id}"),
			AddressType::USB => write!(f, "usb"),
		}
	}
}

impl Debug for AddressType {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		let mut debug = f.debug_struct("AddressType");
		match self {
			AddressType::Sock(sock) => debug.field("ip", sock),
			AddressType::Transport(id) => debug.field("transport_id", id),
			AddressType::USB => debug.field("usb", &""),
		};
		debug.finish()
	}
}

impl From<SocketAddr> for AddressType {
	fn from(value: SocketAddr) -> Self {
		AddressType::from_socket_addr(value)
	}
}

impl FromStr for AddressType {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let addr: Result<SocketAddr, AddrParseError> = s.parse();
		match addr {
			Ok(addr) => Ok(AddressType::Sock(addr)),
			Err(_err) => Err(Error::AddressParseError),
		}
	}
}

impl TryFrom<&str> for AddressType {
	type Error = Error;

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		Self::from_str(value)
	}
}

impl IntoIterator for AddressType {
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
	use std::sync::{Arc, Mutex, Once};

	use once_cell::sync::Lazy;
	use simple_cmd::debug::CommandDebug;
	use tracing_appender::non_blocking::WorkerGuard;

	use crate::v2::types::AddressType;

	static INIT: Once = Once::new();
	static GUARDS: Lazy<Arc<Mutex<Vec<WorkerGuard>>>> = Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

	macro_rules! init_log {
		() => {
			INIT.call_once(|| {
				use tracing_subscriber::prelude::*;

				let registry = tracing_subscriber::Registry::default();
				let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());
				let layer1 = tracing_subscriber::fmt::layer()
					.with_thread_names(false)
					.with_thread_ids(false)
					.with_line_number(false)
					.with_file(false)
					.with_target(false)
					.with_level(false)
					.without_time()
					.with_writer(non_blocking);

				let subscriber = registry.with(layer1);
				tracing::subscriber::set_global_default(subscriber).unwrap();
				GUARDS.lock().unwrap().push(guard);
			})
		};
	}

	#[test]
	fn test_parse_address() {
		let ip = "192.168.1.34:5555";
		let sock_addr = ip.parse::<SocketAddr>().expect("failed to parse ip address");
		let _address: AddressType = AddressType::from_socket_addr(sock_addr);
		let _address: AddressType = AddressType::try_from_ip(ip).expect("failed to parse ip address");
		let _address: AddressType = ip.parse().unwrap();
		let _address = AddressType::from(sock_addr);
		let _address = AddressType::from_str(ip);

		AddressType::from_str("invalid").expect_err("Expected error");
	}

	#[test]
	fn test_display() {
		assert_eq!("usb", AddressType::USB.to_string());
		assert_eq!("transport_id:4", AddressType::Transport(4).to_string());
		assert_eq!(
			"ip:192.168.1.1:5555",
			AddressType::Sock("192.168.1.1:5555".parse().unwrap()).to_string()
		);
	}

	#[test]
	fn test_debug() {
		let addr = format!("{:#?}", AddressType::USB);
		println!("{addr}");
		assert_eq!("AddressType {\n    usb: \"\",\n}", addr);

		let addr = format!("{:#?}", AddressType::Transport(4));
		println!("{addr}");
		assert_eq!("AddressType {\n    transport_id: 4,\n}", addr);

		let addr = format!("{:#?}", AddressType::Sock("192.168.1.1:5555".parse().unwrap()));
		println!("{addr}");
		assert_eq!("AddressType {\n    ip: 192.168.1.1:5555,\n}", addr);
	}

	#[test]
	fn test_copy() {
		let addr = AddressType::USB;
		let addr2 = addr.clone();
		assert_eq!(addr, addr2);

		let addr = AddressType::Transport(4);
		let addr2 = addr.clone();
		assert_eq!(addr, addr2);

		let addr = AddressType::try_from("192.168.1.1:5555").unwrap();
		let addr2 = addr.clone();
		assert_eq!(addr, addr2);
	}

	#[test]
	fn test_args() {
		init_log!();
		let addr = AddressType::USB;
		let mut builder = std::process::Command::new("adb");
		let cmd = builder.args(addr).arg("get-state").debug();
		let output = cmd.output();
		println!("output: {output:?}");

		let addr = AddressType::Transport(4);
		let mut builder = std::process::Command::new("adb");
		let cmd = builder.args(addr).arg("get-state").debug();
		let output = cmd.output();
		println!("output: {output:?}");

		let addr = AddressType::Sock(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 34), 5555)));
		let mut builder = std::process::Command::new("adb");
		let cmd = builder.args(addr).arg("get-state").debug();
		let output = cmd.output();
		println!("output: {output:?}");
	}
}
