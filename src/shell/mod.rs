//! [`Shell`]: the commands run on the device by `adb shell`.
//!
//! `adb shell` joins its arguments with spaces and the device `sh` parses the line again,
//! so [`Shell::exec`] quotes every argument (see [`crate::utils::shell_quote`]). The
//! submodules group the commands by domain: input events, properties, settings, files,
//! screen.

use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::process::{Child, Output, Stdio};
use std::time::Duration;

use crossbeam_channel::Receiver;
use simple_cmd::debug::CommandDebug;
use simple_cmd::CommandBuilder;
use strum::IntoStaticStr;

use crate::am::ActivityManager;
use crate::client::Client;
use crate::command::{quote_args, run, run_unchecked, stdout_string};
use crate::error::{Error, Result};
use crate::pm::PackageManager;

mod fs;
mod input;
mod keycode;
mod props;
mod screen;
mod settings;

pub use fs::{ChmodMode, ChmodModeSpecs, ChmodOptions};
pub use input::{InputSource, KeyEventType, MotionEvent};
pub use keycode::KeyCode;
pub use props::{PropType, Property};
pub use screen::{FFPlayOptions, ScreenRecordOptions};
pub use settings::SettingsType;

/// The `adb shell` interface of a [`Client`]. It only borrows the client: it is
/// `Copy`, and cheap to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shell<'a> {
    pub(crate) parent: &'a Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// The SELinux mode (`getenforce` / `setenforce`).
pub enum SELinuxType {
    /// The policy is enforced.
    Enforcing,
    /// The violations are only logged.
    Permissive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr, strum::Display)]
#[allow(non_camel_case_types)]
/// `dumpsys --priority`.
pub enum DumpsysPriority {
    /// The critical services.
    CRITICAL,
    /// The high priority services.
    HIGH,
    /// The other services.
    NORMAL,
}

/// Options of `dumpsys`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DumpsysOptions {
    /// `-T`: timeout for each service.
    pub timeout: Option<Duration>,
    /// `--pid`: only dump the pid of each service.
    pub pid: bool,
    /// `--thread`: only dump the thread usage of each service.
    pub thread: bool,
    /// `--proto`: protobuf output.
    pub proto: bool,
    /// `--priority`: only dump the services with this priority.
    pub priority: Option<DumpsysPriority>,
    /// `--skip`: services to skip (only when dumping all services).
    pub skip: Vec<String>,
}

impl<'a> Shell<'a> {
    /// The client of this shell.
    pub fn client(&self) -> &'a Client {
        self.parent
    }

    fn command(&self) -> CommandBuilder {
        self.parent.command().arg("shell")
    }

    /// Run a program on the device and return its output. Fails if it exits
    /// with a non-zero status.
    ///
    /// Each argument is quoted: it reaches the program unchanged, without any
    /// shell expansion. Use [`Shell::exec_script`] for pipes, redirections,
    /// variables...
    ///
    /// ```no_run
    /// use radb_client::{Client, ConnectionType};
    ///
    /// let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
    /// let output = client.shell().exec(["ls", "-l", "/sdcard/My Documents"]).unwrap();
    /// println!("{}", String::from_utf8_lossy(&output.stdout));
    /// ```
    pub fn exec<I, S>(&self, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run(self.command().args(quote_args(args)))
    }

    /// Like [`Shell::exec`], with an optional cancel signal and timeout. A command
    /// stopped by the signal or the timeout is not an error: the output collected
    /// so far is returned.
    pub fn exec_with<I, S>(
        &self,
        args: I,
        cancel: Option<Receiver<()>>,
        timeout: Option<Duration>,
    ) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run(self
            .command()
            .args(quote_args(args))
            .signal(cancel)
            .timeout(timeout))
    }

    /// Run a shell script (`sh -c`) on the device. Fails if it exits with a
    /// non-zero status. Quote the values you interpolate with
    /// [`crate::utils::shell_quote`].
    ///
    /// ```no_run
    /// use radb_client::{Client, ConnectionType};
    ///
    /// let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
    /// let output = client.shell().exec_script("ls /system/app | wc -l").unwrap();
    /// ```
    pub fn exec_script(&self, script: &str) -> Result<Output> {
        run(self.command().arg(script))
    }

    /// The low-level `adb shell <args>`: the arguments are not quoted (adb joins
    /// them with spaces and the device shell parses the line again), and the exit
    /// status is not checked.
    pub fn exec_raw<I, S>(
        &self,
        args: I,
        cancel: Option<Receiver<()>>,
        timeout: Option<Duration>,
    ) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run_unchecked(self.command().args(args).signal(cancel).timeout(timeout))
    }

    /// Start a program on the device without waiting for it. The arguments are
    /// quoted, as in [`Shell::exec`]; stdin, stdout and stderr are discarded.
    pub fn spawn<I, S>(&self, args: I) -> Result<Child>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = std::process::Command::new(self.parent.adb.path());
        cmd.args(self.parent.addr.as_args())
            .arg("shell")
            .args(quote_args(args))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if self.parent.debug {
            cmd.debug();
        }
        Ok(cmd.spawn()?)
    }

    /// [`Shell::exec`], returning the trimmed stdout.
    pub(crate) fn exec_string<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        stdout_string(&self.exec(args)?)
    }

    /// The activity manager (`am`).
    pub fn am(&self) -> ActivityManager<'a> {
        ActivityManager { shell: *self }
    }

    /// The package manager (`pm`).
    pub fn pm(&self) -> PackageManager<'a> {
        PackageManager { shell: *self }
    }

    /// The user adbd runs as (`root` or `shell`).
    pub fn whoami(&self) -> Result<String> {
        self.exec_string(["whoami"])
    }

    /// True if adbd runs as root.
    pub fn is_root(&self) -> Result<bool> {
        Ok(self.whoami()?.eq_ignore_ascii_case("root"))
    }

    /// Remount a partition read-write. Root is required.
    pub fn mount<T: AsRef<str>>(&self, dir: T) -> Result<()> {
        self.exec(["mount", "-o", "rw,remount", dir.as_ref()])
            .map(|_| ())
    }

    /// Remount a partition read-only. Root is required.
    pub fn unmount<T: AsRef<str>>(&self, dir: T) -> Result<()> {
        self.exec(["mount", "-o", "ro,remount", dir.as_ref()])
            .map(|_| ())
    }

    /// The full path of a command on the device, `None` if it is not installed.
    pub fn which<T: AsRef<str>>(&self, command: T) -> Result<Option<String>> {
        match self.exec_string(["command", "-v", command.as_ref()]) {
            Ok(path) if path.is_empty() => Ok(None),
            Ok(path) => Ok(Some(path)),
            Err(err) if err.exit_code() == Some(1) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Fails with [`Error::AvbctlNotInstalled`] if `avbctl` is not available.
    pub fn check_avbctl(&self) -> Result<()> {
        match self.which("avbctl")? {
            Some(_) => Ok(()),
            None => Err(Error::AvbctlNotInstalled),
        }
    }

    /// True if `avbctl` is available.
    pub fn has_avbctl(&self) -> Result<bool> {
        Ok(self.which("avbctl")?.is_some())
    }

    /// The verity status, using `avbctl`.
    pub fn get_verity(&self) -> Result<bool> {
        self.check_avbctl()?;
        Ok(self
            .exec_string(["avbctl", "get-verity"])?
            .contains("enabled"))
    }

    /// Disable verity using `avbctl`. Root is required.
    pub fn disable_verity(&self) -> Result<()> {
        self.check_avbctl()?;
        self.exec(["avbctl", "disable-verity"]).map(|_| ())
    }

    /// Enable verity using `avbctl`. Root is required.
    pub fn enable_verity(&self) -> Result<()> {
        self.check_avbctl()?;
        self.exec(["avbctl", "enable-verity"]).map(|_| ())
    }

    /// The SELinux mode (`getenforce`).
    pub fn get_enforce(&self) -> Result<SELinuxType> {
        self.exec_string(["getenforce"])?.as_str().try_into()
    }

    /// Change the SELinux mode. Root is required.
    pub fn set_enforce(&self, enforce: SELinuxType) -> Result<()> {
        let value = match enforce {
            SELinuxType::Enforcing => "1",
            SELinuxType::Permissive => "0",
        };
        self.exec(["setenforce", value]).map(|_| ())
    }

    /// True if the screen is on (interactive).
    pub fn is_screen_on(&self) -> Result<bool> {
        let dump = self.exec_string(["dumpsys", "input_method"])?;
        parse_interactive(&dump)
    }

    /// The input devices, as `(device path, name)`.
    pub fn get_events(&self) -> Result<Vec<(String, String)>> {
        Ok(parse_getevent(&self.exec_string(["getevent", "-S"])?))
    }

    /// The services that `dumpsys` can dump.
    pub fn dumpsys_list(
        &self,
        proto_only: bool,
        priority: Option<DumpsysPriority>,
    ) -> Result<Vec<String>> {
        let mut args = vec!["dumpsys", "-l"];
        if proto_only {
            args.push("--proto");
        }
        if let Some(priority) = priority {
            args.push("--priority");
            args.push(priority.into());
        }
        Ok(parse_dumpsys_list(&self.exec_string(args)?))
    }

    /// Run `dumpsys`, for one service (with its arguments) or for all of them.
    ///
    /// ```no_run
    /// use radb_client::{Client, ConnectionType, DumpsysOptions};
    ///
    /// let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
    /// let dump = client
    ///     .shell()
    ///     .dumpsys(Some("activity"), &["processes"], DumpsysOptions::default())
    ///     .unwrap();
    /// ```
    pub fn dumpsys(
        &self,
        service: Option<&str>,
        arguments: &[&str],
        options: DumpsysOptions,
    ) -> Result<String> {
        let output = self.exec(dumpsys_args(service, arguments, options))?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

fn dumpsys_args(service: Option<&str>, arguments: &[&str], options: DumpsysOptions) -> Vec<String> {
    let mut args = vec!["dumpsys".to_string()];
    if let Some(timeout) = options.timeout {
        args.extend(["-T".to_string(), timeout.as_millis().to_string()]);
    }
    if options.pid {
        args.push("--pid".to_string());
    } else if options.thread {
        args.push("--thread".to_string());
    }
    if options.proto {
        args.push("--proto".to_string());
    }
    if let Some(priority) = options.priority {
        args.extend(["--priority".to_string(), priority.to_string()]);
    }
    match service {
        Some(service) => {
            args.push(service.to_string());
            args.extend(arguments.iter().map(|s| s.to_string()));
        }
        None if !options.skip.is_empty() => {
            args.extend(["--skip".to_string(), options.skip.join(",")]);
        }
        None => {}
    }
    args
}

/// Parse `mInteractive=` from `dumpsys input_method`.
fn parse_interactive(dump: &str) -> Result<bool> {
    dump.split_whitespace()
        .find_map(|token| token.strip_prefix("mInteractive="))
        .map(|value| value == "true")
        .ok_or_else(|| Error::ParseError("mInteractive in dumpsys input_method".to_string()))
}

/// Parse the output of `getevent -S`.
fn parse_getevent(output: &str) -> Vec<(String, String)> {
    let mut result = vec![];
    let mut device: Option<String> = None;
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("add device") {
            device = rest
                .split_once(':')
                .map(|(_, path)| path.trim().to_string());
        } else if let Some(name) = line.strip_prefix("name:") {
            if let Some(device) = device.take() {
                result.push((device, name.trim().trim_matches('"').to_string()));
            }
        }
    }
    result
}

/// Parse the output of `dumpsys -l`.
fn parse_dumpsys_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.ends_with(':'))
        .map(str::to_string)
        .collect()
}

impl Display for SELinuxType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SELinuxType::Enforcing => write!(f, "Enforcing"),
            SELinuxType::Permissive => write!(f, "Permissive"),
        }
    }
}

impl TryFrom<&str> for SELinuxType {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        match value.trim() {
            "Enforcing" => Ok(SELinuxType::Enforcing),
            "Permissive" => Ok(SELinuxType::Permissive),
            other => Err(Error::ParseError(format!("SELinux mode `{other}`"))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test_files/fixtures/api35");

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{FIXTURES}/{name}")).unwrap()
    }

    #[test]
    fn test_parse_interactive() {
        assert!(parse_interactive(&fixture("dumpsys_input_method.txt")).unwrap());
        assert!(!parse_interactive("  mSystemReady=true mInteractive=false\n").unwrap());
        assert!(parse_interactive("").is_err());
    }

    #[test]
    fn test_parse_getevent() {
        let events = parse_getevent(&fixture("getevent.txt"));
        assert!(!events.is_empty());
        assert_eq!(
            events[0],
            ("/dev/input/event0".to_string(), "gpio-keys".to_string())
        );
        assert!(events
            .iter()
            .all(|(device, _)| device.starts_with("/dev/input/event")));
    }

    #[test]
    fn test_parse_dumpsys_list() {
        let services = parse_dumpsys_list(&fixture("dumpsys_list.txt"));
        assert!(services.contains(&"activity".to_string()));
        assert!(services.contains(&"package".to_string()));
        assert!(!services.iter().any(|s| s.ends_with(':')));
    }

    #[test]
    fn test_dumpsys_args() {
        assert_eq!(
            dumpsys_args(None, &[], DumpsysOptions::default()),
            ["dumpsys"]
        );
        let options = DumpsysOptions {
            timeout: Some(Duration::from_secs(2)),
            proto: true,
            priority: Some(DumpsysPriority::CRITICAL),
            skip: vec!["meminfo".to_string(), "gfxinfo".to_string()],
            ..Default::default()
        };
        assert_eq!(
            dumpsys_args(None, &[], options.clone()),
            [
                "dumpsys",
                "-T",
                "2000",
                "--proto",
                "--priority",
                "CRITICAL",
                "--skip",
                "meminfo,gfxinfo"
            ]
        );
        // --skip only applies when dumping all the services
        assert_eq!(
            dumpsys_args(Some("activity"), &["processes"], options),
            [
                "dumpsys",
                "-T",
                "2000",
                "--proto",
                "--priority",
                "CRITICAL",
                "activity",
                "processes"
            ]
        );
    }

    #[test]
    fn test_selinux_type() {
        assert_eq!(
            SELinuxType::try_from("Enforcing\n").unwrap(),
            SELinuxType::Enforcing
        );
        assert_eq!(
            SELinuxType::try_from("Permissive").unwrap(),
            SELinuxType::Permissive
        );
        assert!(SELinuxType::try_from("Disabled").is_err());
    }
}
