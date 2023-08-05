use std::ffi::OsStr;

use std::fs::File;

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use futures::future::IntoFuture;
use log::debug;
use tokio::process::Command;
use tokio::sync::oneshot::Receiver;

use crate::command::{CommandBuilder, Error, ProcessResult, Result};
use crate::debug::CommandDebug;
use crate::util::Vec8ToString;
use crate::{Adb, Shell};
use crate::{AdbDevice, Client};

pub enum RebootType {
    Bootloader,
    Recovery,
    Sideload,
    SideloadAutoReboot,
}

pub struct LogcatOptions {
    /// -e    Only prints lines where the log message matches <expr>, where <expr> is a regular expression.
    pub expr: Option<String>,

    /// -d    Dumps the log to the screen and exits.
    pub dump: bool,

    /// -f <filename>    Writes log message output to <filename>. The default is stdout.
    pub filename: Option<String>,

    /// -s    Equivalent to the filter expression '*:S', which sets priority for all tags to silent and is used to precede a list of filter expressions that add content.
    pub tags: Option<Vec<LogcatTag>>,

    /// -v <format>    Sets the output format for log messages. The default is the threadtime format
    pub format: Option<String>,

    /// -t '<time>'    Prints the most recent lines since the specified time. This option includes -d functionality.
    /// See the -P option for information about quoting parameters with embedded spaces.
    pub since: Option<chrono::DateTime<chrono::Local>>,

    // --pid=<pid> ...
    pub pid: Option<i32>,

    pub timeout: Option<Duration>,
}

pub enum LogcatLevel {
    Verbose,
    Debug,
    Info,
    Warn,
    Error,
}

pub struct LogcatTag {
    pub name: String,
    pub level: LogcatLevel,
}

#[allow(dead_code)]
impl Client {
    pub async fn logcat<'d, D>(adb: &Adb, device: D, options: LogcatOptions, recv: Option<IntoFuture<Receiver<()>>>) -> Result<ProcessResult>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let mut command = CommandBuilder::device(adb, device);

        command.arg("logcat");

        if options.expr.is_some() {
            command.args(["-e", options.expr.unwrap().as_str()]);
        }

        if options.dump {
            command.arg("-d");
        }

        if options.filename.is_some() {
            command.args(["-f", options.filename.unwrap().as_str()]);
        }

        if options.format.is_some() {
            command.args(["-v", options.format.unwrap().as_str()]);
        }

        if options.pid.is_some() {
            command.args(["--pid", format!("{}", options.pid.unwrap()).as_str()]);
        }

        if options.since.is_some() {
            command.args([
                "-T",
                options
                    .since
                    .unwrap()
                    .format("%m-%d %H:%M:%S.%3f")
                    .to_string()
                    .as_str(),
            ]);
        }

        if options.tags.is_some() {
            let tags = options.tags.unwrap();
            if !tags.is_empty() {
                for tag in tags {
                    command.arg(format!("{:}", tag).as_str());
                }
                command.arg("*:S");
            }
        }

        command
            .with_timeout(options.timeout)
            .with_signal(recv)
            .output()
            .await
    }

    pub async fn name<'d, D>(adb: &Adb, device: D) -> Result<Option<String>>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let output = Shell::getprop(adb, device, "ro.build.product").await?;
        Ok(output.as_str().map(|s| s.trim_end().to_string()))
    }

    pub async fn api_level<'d, D>(adb: &Adb, device: D) -> Result<u8>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let result = Shell::getprop(adb, device, "ro.build.version.sdk").await?;
        let string = result
            .as_str()
            .ok_or(Error::from("Failed to convert result into str"))?
            .trim_end();
        string.parse::<u8>().map_err(From::from)
    }

    pub async fn version<'d, D>(adb: &Adb, device: D) -> Result<u8>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let result = Shell::getprop(adb, device, "ro.build.version.release").await?;
        let string = result
            .as_str()
            .ok_or(Error::from("Failed to convert result into str"))?
            .trim_end();
        string.parse::<u8>().map_err(From::from)
    }

    pub async fn pull<'a, 'd, T, D>(adb: &Adb, device: D, src: T, dst: &Path) -> Result<ProcessResult>
    where
        T: Into<&'a str> + AsRef<OsStr>,
        D: Into<&'d dyn AdbDevice>,
    {
        CommandBuilder::device(adb, device)
            .arg("pull")
            .arg(src)
            .arg(dst)
            .output()
            .await
    }

    pub async fn push<'a, 'd, T, D>(adb: &Adb, device: D, src: &Path, dst: T) -> Result<ProcessResult>
    where
        T: Into<&'a str> + AsRef<OsStr>,
        D: Into<&'d dyn AdbDevice>,
    {
        CommandBuilder::device(adb, device)
            .arg("push")
            .arg(src)
            .arg(dst)
            .output()
            .await
    }

    pub async fn save_screencap<'d, D>(adb: &Adb, device: D, output: &Path) -> Result<()>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let args = vec!["exec-out", "screencap", "-p"];
        let file = File::create(output)?;
        let pipe_out = Stdio::from(file);
        let output = Command::new(adb.as_os_str())
            .args(device.into().args())
            .args(args)
            .stdout(pipe_out)
            .stderr(Stdio::piped())
            .debug()
            .status()
            .await?;

        debug!("output: {:}", output);
        Ok(())
    }

    pub async fn wait_for_device<'d, D>(adb: &Adb, device: D, timeout: Option<Duration>) -> Result<()>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        CommandBuilder::device(adb, device)
            .args([
                "wait-for-device",
                "shell",
                "while [[ -z $(getprop sys.boot_completed) ]]; do sleep 1; done; input keyevent 143",
            ])
            .with_timeout(timeout)
            .output()
            .await?;
        Ok(())
    }

    /// Attempt to run adb as root
    pub async fn root<'a, T>(adb: &Adb, device: T) -> Result<bool>
    where
        T: Into<&'a dyn AdbDevice>,
    {
        let d = device.into();

        if Shell::is_root(adb, d).await? {
            return Ok(true);
        }
        CommandBuilder::device(adb, d).arg("root").output().await?;
        Ok(true)
    }

    pub async fn unroot<'d, D>(adb: &Adb, device: D) -> Result<bool>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        CommandBuilder::device(adb, device)
            .arg("unroot")
            .output()
            .await?;
        Ok(true)
    }

    pub async fn is_connected<'d, D>(adb: &Adb, device: D) -> bool
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let output = CommandBuilder::device(adb, device)
            .arg("get-state")
            .output()
            .await;
        output.is_ok()
    }

    pub async fn connect<'d, D>(adb: &Adb, device: D) -> anyhow::Result<()>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let d = device.into();

        if Client::is_connected(adb, d).await {
            return Ok(());
        }

        let serial = d.addr().serial().expect("Host[:Port] required");

        CommandBuilder::new(adb.as_os_str())
            .args(["connect", serial.as_str()])
            .output()
            .await?;

        match Client::is_connected(adb, d).await {
            true => Ok(()),
            false => Err(anyhow::Error::msg("Could not connect to device")),
        }
    }

    pub async fn disconnect<'d, D>(adb: &Adb, device: D) -> Result<bool>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let serial = device.into().addr().serial().expect("Host[:Port] required");
        CommandBuilder::new(adb.as_os_str())
            .args(["disconnect", serial.as_str()])
            .output()
            .await?;
        Ok(true)
    }

    pub async fn disconnect_all(adb: &Adb) -> Result<bool> {
        CommandBuilder::new(adb.0.as_path())
            .args(["disconnect"])
            .output()
            .await?;
        Ok(true)
    }

    pub async fn reboot<'d, D>(adb: &Adb, device: D, reboot_type: Option<RebootType>) -> Result<()>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let mut args: Vec<&str> = vec!["reboot"];
        let s = reboot_type.map(|f| f.value()).unwrap_or_default();
        args.push(s.as_str());
        let _output = CommandBuilder::device(adb, device)
            .args(args)
            .output()
            .await?;
        Ok(())
    }

    pub async fn remount<'a, T>(adb: &Adb, device: T) -> Result<()>
    where
        T: Into<&'a dyn AdbDevice>,
    {
        CommandBuilder::device(adb, device.into())
            .arg("remount")
            .output()
            .await?;
        Ok(())
    }

    pub async fn disable_verity<'a, T>(adb: &Adb, device: T) -> Result<()>
    where
        T: Into<&'a dyn AdbDevice>,
    {
        CommandBuilder::device(adb, device.into())
            .arg("disable-verity")
            .output()
            .await?;
        Ok(())
    }

    pub async fn mount<'d, D>(adb: &Adb, device: D, dir: &str) -> Result<()>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        Shell::exec(adb, device, vec!["mount -o rw,remount", dir], None).await?;
        Ok(())
    }

    pub async fn unmount<'d, D>(adb: &Adb, device: D, dir: &str) -> Result<()>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        Shell::exec(adb, device, vec!["mount -o ro,remount", dir], None).await?;
        Ok(())
    }

    pub async fn bug_report<'d, D>(adb: &Adb, device: D, output: Option<&str>) -> Result<ProcessResult>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        let args = match output {
            Some(s) => vec!["bugreport", s],
            None => vec!["bugreport"],
        };
        CommandBuilder::device(adb, device)
            .args(args)
            .output()
            .await
    }

    pub async fn clear_logcat<'d, D>(adb: &Adb, device: D) -> Result<()>
    where
        D: Into<&'d dyn AdbDevice>,
    {
        CommandBuilder::device(adb, device)
            .args(["logcat", "-b", "all", "-c"])
            .output()
            .await?;
        Ok(())
    }
}
