use std::ffi::OsStr;

use std::io::BufRead;

use std::path::Path;

use anyhow;
use lazy_static::lazy_static;
use regex::Regex;
use which::which;

use crate::command::CommandBuilder;
use crate::AdbDevice;

use super::Adb;
use super::Device;

impl Adb {
    pub fn new() -> which::Result<Adb> {
        let adb = which("adb")?;
        Ok(Adb(adb))
    }

    pub fn from(path: &Path) -> which::Result<Adb> {
        if !path.exists() {
            return Err(which::Error::CannotFindBinaryPath);
        }
        Ok(Adb(path.to_path_buf()))
    }

    pub fn as_os_str(&self) -> &OsStr {
        self.as_ref()
    }

    pub async fn devices(&self) -> anyhow::Result<Vec<Box<dyn AdbDevice>>, anyhow::Error> {
        let output = CommandBuilder::new(self.0.as_path())
            .args(["devices", "-l"])
            .output()
            .await
            .map_err(anyhow::Error::msg)?;

        lazy_static! {
            static ref RE: Regex =
                Regex::new("(?P<ip>[^\\s]+)[\\s]+(device|offline) product:(?P<device_product>[^\\s]+)\\smodel:(?P<model>[^\\s]+)\\sdevice:(?P<device>[^\\s]+)\\stransport_id:(?P<transport_id>[^\\s]+)").unwrap();
        }

        let mut devices: Vec<Box<dyn AdbDevice>> = vec![];
        let stdout = output.stdout();
        for line in stdout.lines() {
            let line_str = line.map_err(|_| anyhow::Error::msg("Line failed"))?;

            if RE.is_match(line_str.as_str()) {
                let captures = RE.captures(line_str.as_str());
                match captures {
                    None => {}
                    Some(c) => {
                        let ip = c.name("ip").unwrap().as_str();
                        let tr = c
                            .name("transport_id")
                            .unwrap()
                            .as_str()
                            .parse::<u8>()
                            .unwrap();
                        let device = Device::from_ip(ip).or(Device::from_transport_id(tr)
                            .or(Device::from_serial(line_str.as_str())));
                        if let Ok(d) = device {
                            devices.push(Box::new(d))
                        }
                    }
                }
            }
        }
        anyhow::Ok(devices)
    }

    pub fn device(&self, input: &str) -> anyhow::Result<Box<dyn AdbDevice>> {
        let d = Device::from_serial(input)?;
        Ok(Box::new(d))
    }
}
