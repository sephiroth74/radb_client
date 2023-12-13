use std::fmt::{Debug, Display};

use crate::input::KeyCode;
use crate::DeviceAddress;

pub trait AdbDevice: Display + Debug + Send + Sync {
	fn addr(&self) -> &DeviceAddress;
	fn args(&self) -> Vec<String>;
}

pub trait AsArgs<T>: Send + Sync {
	fn as_args(&self) -> Vec<T>;
}

impl Extend<KeyCode> for Vec<&str> {
	fn extend<T: IntoIterator<Item = KeyCode>>(&mut self, iter: T) {
		for element in iter {
			self.push(element.into());
		}
	}
}
