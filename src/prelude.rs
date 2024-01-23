use simple_cmd::CommandBuilder;

use crate::types::{Adb, Client, ConnectionType};

pub(crate) trait CommandBuilderExt {
	fn client<C>(self, client: C) -> Self
	where
		C: Into<Client>;

	fn addr<C>(self, addr: C) -> Self
	where
		C: Into<ConnectionType>;

	fn shell(client: &Client) -> CommandBuilder;

	fn adb(adb: &Adb) -> CommandBuilder;
}

impl CommandBuilderExt for CommandBuilder {
	fn client<C>(self, client: C) -> Self
	where
		C: Into<Client>,
	{
		self.args(client.into().addr)
	}

	fn addr<C>(self, addr: C) -> Self
	where
		C: Into<ConnectionType>,
	{
		self.args(addr.into())
	}

	fn shell(client: &Client) -> CommandBuilder {
		CommandBuilder::from(client).arg("shell")
	}

	fn adb(adb: &Adb) -> CommandBuilder {
		CommandBuilder::new(adb)
	}
}
