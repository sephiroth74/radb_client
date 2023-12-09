use crate::DeviceAddress;
use std::fmt::{Debug, Display};

pub trait AdbDevice: Display + Debug + Send + Sync {
    fn addr(&self) -> &DeviceAddress;
    fn args(&self) -> Vec<String>;
}
