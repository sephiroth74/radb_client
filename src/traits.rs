use crate::DeviceAddress;

use std::fmt::{Debug, Display};

pub trait AdbDevice: Display + Debug + Send + Sync {
	fn addr(&self) -> &DeviceAddress;
	fn args(&self) -> Vec<String>;
}

pub trait AsArgs: Display + Send + Sync {
	fn as_args(&self) -> Vec<String>;
}
