use std::fmt::{Debug, Display};
use std::net::Ipv4Addr;
use std::path::PathBuf;

pub mod command;
pub mod debug;
pub mod intent;

pub mod macros;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Adb(PathBuf);

pub struct Shell {}

pub struct Client {}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum AddressType {
    Ip(IpV4AddrAndPort),
    Name(String),
    Transport(u8),
}

#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct DeviceAddress(AddressType);

#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Device(DeviceAddress);

#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct IpV4AddrAndPort {
    pub ip: Ipv4Addr,
    pub port: u16,
}

pub trait AdbDevice: Display + Debug + Send + Sync {
    fn addr(&self) -> &DeviceAddress;
    fn args(&self) -> Vec<String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SELinuxType {
    Enforcing,
    Permissive,
}

pub mod adb;
pub mod client;
pub mod input;
pub mod shell;
pub mod util;

mod impls;
mod scanner;
mod tests;
