use std::fmt::{Debug, Display};

use crate::types::DeviceAddress;

pub trait AdbDevice: Display + Debug + Send + Sync {
	fn addr(&self) -> &DeviceAddress;
	fn args(&self) -> Vec<String>;
}

pub trait AsArgs<T>: Send + Sync {
	fn as_args(&self) -> Vec<T>;
}
