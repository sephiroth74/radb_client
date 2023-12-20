use cmd::{Cmd, CommandBuilder};

use crate::traits::AdbDevice;
use crate::{Adb, AdbClient};

pub(crate) trait CommandBuilderExt {
	fn device<'d, D>(self, device: D) -> CommandBuilder
	where
		D: Into<&'d dyn AdbDevice>;

	fn shell<'d, D>(adb: &Adb, device: D) -> CommandBuilder
	where
		D: Into<&'d dyn AdbDevice>;

	fn adb(adb: &Adb) -> CommandBuilder;
}

impl CommandBuilderExt for CommandBuilder {
	fn device<'d, D>(self, device: D) -> CommandBuilder
	where
		D: Into<&'d dyn AdbDevice>,
	{
		self.args(device.into().args())
	}

	fn shell<'d, D>(adb: &Adb, device: D) -> CommandBuilder
	where
		D: Into<&'d dyn AdbDevice>,
	{
		CommandBuilder::adb(adb).args(device.into().args()).arg("shell")
	}

	fn adb(adb: &Adb) -> CommandBuilder {
		Cmd::builder(adb)
	}
}

impl From<&Adb> for CommandBuilder {
	fn from(adb: &Adb) -> Self {
		let builder = CommandBuilder::new(adb.as_os_str());
		builder
	}
}

impl From<AdbClient> for CommandBuilder {
	fn from(value: AdbClient) -> Self {
		CommandBuilder::shell(&value.adb, &value.device)
	}
}
