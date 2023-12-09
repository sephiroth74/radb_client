use crate::{Adb, Device};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbClient {
    pub(crate) adb: Adb,
    pub device: Device,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbShell<'a> {
    pub(crate) parent: &'a AdbClient,
}
