use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::path::Path;
use std::process::Stdio;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use mac_address::MacAddress;
use simple_cmd::debug::CommandDebug;
use simple_cmd::CommandBuilder;
use strum::Display;
use uuid::Uuid;

use crate::adb::{Adb, AdbDevice};
use crate::am::ActivityManager;
use crate::command::{run, run_string, run_unchecked, stdout_string, was_stopped};
use crate::connection::ConnectionType;
use crate::error::{CommandFailure, Error, Result};
use crate::pm::{PackageManager, UninstallOptions};
use crate::shell::Shell;

const GET_STATE_TIMEOUT: Duration = Duration::from_secs(2);
const ADBD_RESTART_TIMEOUT: Duration = Duration::from_secs(20);
const BOOT_COMPLETED_SCRIPT: &str =
    r#"while [ "$(getprop sys.boot_completed)" != "1" ]; do sleep 1; done"#;

/// A device, reached through the host `adb`.
///
/// ```no_run
/// use radb_client::{Client, ConnectionType};
///
/// let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
/// let sdk = client.shell().build_version_sdk().unwrap();
/// println!("{client} runs API {sdk}");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Client {
    pub adb: Adb,
    pub addr: ConnectionType,
    /// Log the command lines (at trace level, target `simple_cmd`) and command timeouts.
    pub debug: bool,
}

#[derive(Debug, Display, Eq, PartialEq, Hash, Clone, Copy)]
pub enum Wakefulness {
    Awake,
    Asleep,
    Dreaming,
    Dozing,
}

#[derive(Debug, Display, Eq, PartialEq, Hash, Clone, Copy)]
#[strum(serialize_all = "lowercase")]
pub enum Reconnect {
    Device,
    Offline,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
#[strum(serialize_all = "kebab-case")]
pub enum RebootType {
    Bootloader,
    Recovery,
    Sideload,
    SideloadAutoReboot,
    Userspace,
    Dra,
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct LogcatOptions {
    /// `-e expr`: only print lines where the log message matches the regular expression.
    pub expr: Option<String>,
    /// `-d`: dump the log and exit.
    pub dump: bool,
    /// `-f filename`: write the log to `filename` on the device.
    pub filename: Option<String>,
    /// `-s tag:level ...`: only print these tags (every other tag is silenced).
    pub tags: Option<Vec<LogcatTag>>,
    /// `-v format`: output format.
    pub format: Option<String>,
    /// `-T time`: print the lines since the given time.
    pub since: Option<chrono::DateTime<chrono::Local>>,
    /// `--pid=pid`: only print the logs of this process.
    pub pid: Option<i32>,
    /// Stop logcat after this time. The logs collected until then are returned.
    pub timeout: Option<Duration>,
}

#[derive(Debug, Display, Copy, Clone, Eq, PartialEq)]
pub enum LogcatLevel {
    #[strum(serialize = "V")]
    Verbose,
    #[strum(serialize = "D")]
    Debug,
    #[strum(serialize = "I")]
    Info,
    #[strum(serialize = "W")]
    Warn,
    #[strum(serialize = "E")]
    Error,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LogcatTag {
    pub name: String,
    pub level: LogcatLevel,
}

/// Options of `adb install`.
#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct AdbInstallOptions {
    /// `-d`: allow version code downgrade
    pub allow_version_downgrade: bool,
    /// `-t`: allow test packages
    pub allow_test_package: bool,
    /// `-r`: replace existing application
    pub replace: bool,
    /// `-l`: forward lock the app
    pub forward_lock: bool,
    /// `-s`: install on SD card instead of internal storage
    pub install_external: bool,
    /// `-g`: grant all runtime permissions
    pub grant_permissions: bool,
    /// `--instant`: install the app as an ephemeral install app
    pub instant: bool,
}

impl Client {
    pub fn new(adb: Adb, addr: ConnectionType, debug: bool) -> Self {
        Client { adb, addr, debug }
    }

    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    /// `adb <selector>`, ready for the arguments.
    pub(crate) fn command(&self) -> CommandBuilder {
        CommandBuilder::new(self.adb.path())
            .with_debug(self.debug)
            .args(self.addr.as_args())
    }

    /// The `adb shell` interface.
    pub fn shell(&self) -> Shell<'_> {
        Shell { parent: self }
    }

    /// Shortcut for `client.shell().am()`.
    pub fn am(&self) -> ActivityManager<'_> {
        self.shell().am()
    }

    /// Shortcut for `client.shell().pm()`.
    pub fn pm(&self) -> PackageManager<'_> {
        self.shell().pm()
    }

    /// Connect to a tcp/ip device, waiting at most `timeout` for `adb connect`.
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use radb_client::{Client, ConnectionType};
    ///
    /// let conn = ConnectionType::try_from_ip("192.168.1.101").unwrap();
    /// let client = Client::try_from(conn).unwrap();
    /// match client.connect(Some(Duration::from_secs(1))) {
    ///     Ok(_) => println!("client connected!"),
    ///     Err(err) => eprintln!("failed to connect: {err}"),
    /// }
    /// ```
    pub fn connect(&self, timeout: Option<Duration>) -> Result<()> {
        if self.is_connected() {
            return Ok(());
        }

        let addr = self.tcp_addr()?;
        run(self
            .adb
            .command()
            .arg("connect")
            .arg(addr.to_string())
            .timeout(timeout))?;

        if self.is_connected() {
            Ok(())
        } else {
            Err(std::io::ErrorKind::NotConnected.into())
        }
    }

    /// Disconnect a tcp/ip device.
    pub fn disconnect(&self) -> Result<()> {
        let addr = self.tcp_addr()?;
        run(self.adb.command().arg("disconnect").arg(addr.to_string())).map(|_| ())
    }

    fn tcp_addr(&self) -> Result<std::net::SocketAddr> {
        match &self.addr {
            ConnectionType::TcpIp(addr) => Ok(*addr),
            other => Err(Error::InvalidConnectionTypeError(format!(
                "{other} is not a tcp/ip connection"
            ))),
        }
    }

    /// True if adb sees the device and it is online.
    pub fn is_connected(&self) -> bool {
        run(self
            .command()
            .arg("get-state")
            .timeout(Some(GET_STATE_TIMEOUT)))
        .ok()
        .filter(|output| !was_stopped(output))
        .and_then(|output| stdout_string(&output).ok())
        .is_some_and(|state| state == "device")
    }

    /// Wait until the device is available and has completed the boot.
    pub fn wait_for_device(&self, timeout: Option<Duration>) -> Result<()> {
        let output = run(self
            .command()
            .args(["wait-for-device", "shell", BOOT_COMPLETED_SCRIPT])
            .timeout(timeout))?;
        if was_stopped(&output) {
            Err(Error::Timeout(format!("waiting for {}", self.addr)))
        } else {
            Ok(())
        }
    }

    pub fn get_wakefulness(&self) -> Result<Wakefulness> {
        let dump = self.shell().exec_string(["dumpsys", "power"])?;
        parse_wakefulness(&dump)
    }

    pub fn is_awake(&self) -> Result<bool> {
        Ok(self.get_wakefulness()? != Wakefulness::Asleep)
    }

    pub fn is_root(&self) -> Result<bool> {
        self.shell().is_root()
    }

    /// Restart adbd as root and wait until it is back.
    /// Fails on production builds, where adbd cannot run as root.
    pub fn root(&self) -> Result<()> {
        if self.is_root()? {
            return Ok(());
        }
        self.restart_adbd("root", true)
    }

    /// Restart adbd without root and wait until it is back.
    pub fn unroot(&self) -> Result<()> {
        if !self.is_root()? {
            return Ok(());
        }
        self.restart_adbd("unroot", false)
    }

    fn restart_adbd(&self, command: &str, root: bool) -> Result<()> {
        let command_line = format!("adb {} {command}", self.addr);
        let output = run(self.command().arg(command))?;
        let message = stdout_string(&output)?;
        if message.contains("cannot run as root") {
            return Err(CommandFailure::new(command_line, &output).into());
        }

        let start = Instant::now();
        loop {
            // adbd restarts: the device disappears for a moment
            let _ = run_unchecked(
                self.command()
                    .arg("wait-for-device")
                    .timeout(Some(Duration::from_secs(5))),
            );
            if self.is_root().ok() == Some(root) {
                return Ok(());
            }
            if start.elapsed() > ADBD_RESTART_TIMEOUT {
                return Err(Error::Timeout(format!(
                    "waiting for adbd after `{command_line}`"
                )));
            }
            sleep(Duration::from_millis(250));
        }
    }

    /// Save a PNG screenshot to a local file.
    ///
    /// ```no_run
    /// use std::fs::File;
    /// use radb_client::{Client, ConnectionType};
    ///
    /// let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
    /// let file = File::create("screencap.png").unwrap();
    /// client.save_screencap(file).unwrap();
    /// ```
    pub fn save_screencap(&self, output: File) -> Result<()> {
        let mut cmd = std::process::Command::new(self.adb.path());
        cmd.args(self.addr.as_args())
            .args(["exec-out", "screencap", "-p"])
            .stdout(Stdio::from(output))
            .stderr(Stdio::piped());
        if self.debug {
            cmd.debug();
        }
        let command_line = cmd.as_string();
        let result = cmd.output()?;
        if result.status.success() {
            Ok(())
        } else {
            Err(CommandFailure::new(command_line, &result).into())
        }
    }

    /// Copy a screenshot to the host clipboard.
    #[cfg(feature = "clipboard")]
    pub fn copy_screencap(&self) -> Result<()> {
        let path = std::env::temp_dir().join(format!("{}.png", Uuid::new_v4()));
        self.save_screencap(File::create(&path)?)?;
        let img = image::open(&path);
        let _ = std::fs::remove_file(&path);
        let img = img?.to_rgba8();

        let image_data = arboard::ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: std::borrow::Cow::from(img.as_raw().as_slice()),
        };
        arboard::Clipboard::new()?.set_image(image_data)?;
        Ok(())
    }

    /// Reboot the device, by default into the system image.
    pub fn reboot(&self, reboot_type: Option<RebootType>) -> Result<()> {
        let mut builder = self.command().arg("reboot");
        if let Some(reboot_type) = reboot_type {
            builder = builder.arg(reboot_type.to_string());
        }
        run(builder).map(|_| ())
    }

    /// Remount the partitions read-write. With `reboot_if_required`, adb reboots
    /// the device if that is needed for the remount.
    pub fn remount(&self, reboot_if_required: bool) -> Result<()> {
        let mut builder = self.command().arg("remount");
        if reboot_if_required {
            builder = builder.arg("-R");
        }
        run(builder).map(|_| ())
    }

    pub fn get_serialno(&self) -> Result<String> {
        run_string(self.command().arg("get-serialno"))
    }

    /// Reconnect the device, returning the adb message.
    pub fn reconnect(&self, reconnect_type: Option<Reconnect>) -> Result<String> {
        let mut builder = self.command().arg("reconnect");
        if let Some(reconnect_type) = reconnect_type {
            builder = builder.arg(reconnect_type.to_string());
        }
        run_string(builder)
    }

    /// Write a bug report to `output` (a zip file or a directory), by default
    /// `bugreport.zip` in the current directory.
    pub fn bug_report<P: AsRef<Path>>(&self, output: Option<P>) -> Result<()> {
        let mut builder = self.command().arg("bugreport");
        if let Some(output) = output {
            builder = builder.arg(output.as_ref());
        }
        run(builder).map(|_| ())
    }

    /// Clear all logcat buffers.
    pub fn clear_logcat(&self) -> Result<()> {
        run(self.command().args(["logcat", "-b", "all", "-c"])).map(|_| ())
    }

    /// Run logcat and return its output. Use `dump`, `timeout` or `cancel` to stop it.
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use radb_client::{Client, ConnectionType, LogcatOptions};
    ///
    /// let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
    /// let options = LogcatOptions { dump: true, ..Default::default() };
    /// let logs = client.logcat(options, None).unwrap();
    /// println!("{logs}");
    /// ```
    pub fn logcat(&self, options: LogcatOptions, cancel: Option<Receiver<()>>) -> Result<String> {
        let timeout = options.timeout;
        // `adb logcat` escapes its arguments itself
        let output = run(self
            .command()
            .arg("logcat")
            .args(options)
            .timeout(timeout)
            .signal(cancel))?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// The MAC address of `eth0`.
    pub fn get_mac_address(&self) -> Result<MacAddress> {
        self.read_mac_address("/sys/class/net/eth0/address")
    }

    /// The MAC address of `wlan0`.
    pub fn get_wlan_address(&self) -> Result<MacAddress> {
        self.read_mac_address("/sys/class/net/wlan0/address")
    }

    fn read_mac_address(&self, path: &str) -> Result<MacAddress> {
        let output = self.shell().cat(path)?;
        Ok(MacAddress::try_from(std::str::from_utf8(&output)?.trim())?)
    }

    /// The boot id (`/proc/sys/kernel/random/boot_id`), which changes at every boot.
    pub fn get_boot_id(&self) -> Result<Uuid> {
        let output = self.shell().cat("/proc/sys/kernel/random/boot_id")?;
        Ok(Uuid::try_parse(std::str::from_utf8(&output)?.trim())?)
    }

    pub fn disable_verity(&self) -> Result<()> {
        run(self.command().arg("disable-verity")).map(|_| ())
    }

    pub fn enable_verity(&self) -> Result<()> {
        run(self.command().arg("enable-verity")).map(|_| ())
    }

    /// Copy a file or directory from the device to the host.
    pub fn pull<S: AsRef<str>, D: AsRef<Path>>(&self, src: S, dst: D) -> Result<()> {
        run(self
            .command()
            .arg("pull")
            .arg(src.as_ref())
            .arg(dst.as_ref()))
        .map(|_| ())
    }

    /// Copy a file or directory from the host to the device.
    pub fn push<S: AsRef<Path>, D: AsRef<str>>(&self, src: S, dst: D) -> Result<()> {
        run(self
            .command()
            .arg("push")
            .arg(src.as_ref())
            .arg(dst.as_ref()))
        .map(|_| ())
    }

    /// Install an APK from the host.
    pub fn install<P: AsRef<Path>>(
        &self,
        path: P,
        options: Option<AdbInstallOptions>,
    ) -> Result<()> {
        let mut builder = self.command().arg("install");
        if let Some(options) = options {
            builder = builder.args(options);
        }
        run(builder.arg(path.as_ref())).map(|_| ())
    }

    /// Uninstall a package.
    ///
    /// ```no_run
    /// use radb_client::{Client, ConnectionType};
    ///
    /// let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
    /// client.uninstall("com.example.app", None).unwrap();
    /// ```
    pub fn uninstall(&self, package_name: &str, options: Option<UninstallOptions>) -> Result<()> {
        let mut builder = self.command().arg("uninstall");
        if let Some(options) = options {
            builder = builder.args(options);
        }
        run(builder.arg(package_name)).map(|_| ())
    }
}

/// Parse `mWakefulness=` from `dumpsys power`.
pub(crate) fn parse_wakefulness(dump: &str) -> Result<Wakefulness> {
    let value = dump
        .lines()
        .find_map(|line| line.trim().strip_prefix("mWakefulness="))
        .ok_or_else(|| Error::ParseError("mWakefulness in dumpsys power".to_string()))?;
    value.trim().try_into()
}

impl TryFrom<&str> for Wakefulness {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        match value.to_lowercase().as_str() {
            "awake" => Ok(Wakefulness::Awake),
            "asleep" => Ok(Wakefulness::Asleep),
            "dreaming" => Ok(Wakefulness::Dreaming),
            "dozing" => Ok(Wakefulness::Dozing),
            _ => Err(Error::ParseError(format!("wakefulness `{value}`"))),
        }
    }
}

impl IntoIterator for LogcatOptions {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let mut args: Vec<OsString> = vec![];
        if let Some(expr) = self.expr {
            args.extend(["-e".into(), expr.into()]);
        }
        if self.dump {
            args.push("-d".into());
        }
        if let Some(filename) = self.filename {
            args.extend(["-f".into(), filename.into()]);
        }
        if let Some(format) = self.format {
            args.extend(["-v".into(), format.into()]);
        }
        if let Some(pid) = self.pid {
            args.push(format!("--pid={pid}").into());
        }
        if let Some(since) = self.since {
            args.extend([
                "-T".into(),
                since.format("%m-%d %H:%M:%S%.3f").to_string().into(),
            ]);
        }
        if let Some(tags) = self.tags {
            if !tags.is_empty() {
                args.extend(tags.iter().map(|tag| tag.to_string().into()));
                args.push("*:S".into());
            }
        }
        args.into_iter()
    }
}

impl Display for LogcatTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.name, self.level)
    }
}

impl IntoIterator for AdbInstallOptions {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let flags = [
            (self.allow_version_downgrade, "-d"),
            (self.allow_test_package, "-t"),
            (self.replace, "-r"),
            (self.forward_lock, "-l"),
            (self.install_external, "-s"),
            (self.grant_permissions, "-g"),
            (self.instant, "--instant"),
        ];
        flags
            .into_iter()
            .filter(|(enabled, _)| *enabled)
            .map(|(_, flag)| flag.into())
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl TryFrom<ConnectionType> for Client {
    type Error = Error;

    /// A client using the `adb` found in `PATH`.
    fn try_from(value: ConnectionType) -> Result<Self> {
        Ok(Client::new(Adb::new()?, value, false))
    }
}

impl TryFrom<AdbDevice> for Client {
    type Error = Error;

    fn try_from(value: AdbDevice) -> Result<Self> {
        value.addr.try_into()
    }
}

impl TryFrom<&AdbDevice> for Client {
    type Error = Error;

    fn try_from(value: &AdbDevice) -> Result<Self> {
        value.addr.clone().try_into()
    }
}

impl Display for Client {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.addr.fmt(f)
    }
}

#[cfg(test)]
mod test {
    use chrono::TimeZone;

    use super::*;

    fn strings<I: IntoIterator<Item = OsString>>(args: I) -> Vec<String> {
        args.into_iter().map(|s| s.into_string().unwrap()).collect()
    }

    #[test]
    fn test_parse_wakefulness() {
        let dump = include_str!("../test_files/fixtures/api35/dumpsys_power.txt");
        assert_eq!(parse_wakefulness(dump).unwrap(), Wakefulness::Awake);
        assert_eq!(
            parse_wakefulness("  mWakefulness=Asleep\n").unwrap(),
            Wakefulness::Asleep
        );
        assert!(parse_wakefulness("nothing here").is_err());
    }

    #[test]
    fn test_logcat_options() {
        assert!(strings(LogcatOptions::default()).is_empty());

        let since = chrono::Local
            .with_ymd_and_hms(2026, 11, 3, 9, 5, 7)
            .unwrap();
        let options = LogcatOptions {
            expr: Some("Activity.*".to_string()),
            dump: true,
            format: Some("threadtime".to_string()),
            pid: Some(42),
            since: Some(since),
            tags: Some(vec![LogcatTag {
                name: "ActivityManager".to_string(),
                level: LogcatLevel::Info,
            }]),
            ..Default::default()
        };
        assert_eq!(
            strings(options),
            [
                "-e",
                "Activity.*",
                "-d",
                "-v",
                "threadtime",
                "--pid=42",
                "-T",
                "11-03 09:05:07.000",
                "ActivityManager:I",
                "*:S"
            ]
        );
    }

    #[test]
    fn test_adb_install_options() {
        assert!(strings(AdbInstallOptions::default()).is_empty());
        let options = AdbInstallOptions {
            replace: true,
            grant_permissions: true,
            allow_version_downgrade: true,
            ..Default::default()
        };
        assert_eq!(strings(options), ["-d", "-r", "-g"]);
    }

    #[test]
    fn test_display() {
        assert_eq!(
            RebootType::SideloadAutoReboot.to_string(),
            "sideload-auto-reboot"
        );
        assert_eq!(RebootType::Bootloader.to_string(), "bootloader");
        assert_eq!(Reconnect::Offline.to_string(), "offline");
        assert_eq!(LogcatLevel::Warn.to_string(), "W");
    }

    #[test]
    fn test_disconnect_requires_tcp() {
        let client = Client::new(Adb::from_path("adb"), ConnectionType::emulator(5554), false);
        assert!(matches!(
            client.disconnect(),
            Err(Error::InvalidConnectionTypeError(_))
        ));
    }
}
