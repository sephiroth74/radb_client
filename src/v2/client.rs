use std::time::Duration;

use simple_cmd::prelude::OutputExt;
use simple_cmd::CommandBuilder;

use crate::v2::prelude::*;
use crate::v2::types::{Adb, AdbDevice, AddressType, Client};

static GET_STATE_TIMEOUT: u64 = 200u64;

impl Client {
	pub fn new(adb: Adb, addr: AddressType, debug: bool) -> Self {
		Client { adb, addr, debug }
	}

	pub fn is_connected(&self) -> bool {
		let mut command = CommandBuilder::from(self);
		command = command
			.arg("get-state")
			.timeout(Some(Duration::from_millis(GET_STATE_TIMEOUT)));
		let output = command.build().output();
		return if let Ok(output) = output { output.success() } else { false };
	}

	pub fn with_debug(mut self, debug: bool) -> Self {
		self.debug = debug;
		self
	}
}

impl TryFrom<AddressType> for Client {
	type Error = crate::v2::error::Error;

	fn try_from(value: AddressType) -> Result<Self, Self::Error> {
		let adb = Adb::new()?;
		Ok(Client::new(adb, value, false))
	}
}

impl TryFrom<AdbDevice> for Client {
	type Error = crate::v2::error::Error;

	fn try_from(value: AdbDevice) -> Result<Self, Self::Error> {
		value.addr.try_into()
	}
}

impl From<&Client> for CommandBuilder {
	fn from(value: &Client) -> Self {
		CommandBuilder::adb(&value.adb).addr(value.addr).with_debug(value.debug)
	}
}

#[cfg(test)]
mod test {
	use crate::v2::types::{AddressType, Client};

	#[test]
	fn test_new_client() {
		let address: AddressType = AddressType::try_from_ip("192.168.1.34:5555").expect("failed to parse ip address");
		let mut client = Client::try_from(address).expect("Failed to create Client from address");
		client = client.with_debug(true);
		let connected = client.is_connected();
		println!("connected: {}", connected);

		let address: AddressType = AddressType::Transport(4);
		let mut client = Client::try_from(address).expect("Failed to create Client from address");
		client = client.with_debug(true);
		let connected = client.is_connected();
		println!("connected: {}", connected);
	}
}
