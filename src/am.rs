//! `am`: the activity manager.

use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::process::Output;

use crate::command::{quote_args, stdout_string};
use crate::error::{CommandFailure, Result};
use crate::shell::Shell;

/// The `am` interface of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivityManager<'a> {
    pub(crate) shell: Shell<'a>,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum UserOption {
    UserId(String),
    All,
    Current,
    None,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy, strum::Display)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryStatus {
    Hidden,
    RunningModerate,
    Background,
    RunningLow,
    Moderate,
    RunningCritical,
    Complete,
}

/// An intent, for `am start`, `am startservice`, `am broadcast`...
///
/// ```
/// use radb_client::Intent;
///
/// let mut intent = Intent::from_action("android.intent.action.VIEW");
/// intent.data = Some("https://example.com/a b".to_string());
/// intent.extra.put_string_extra("title", "it's here");
/// assert_eq!(
///     intent.to_string(),
///     r"-a android.intent.action.VIEW -d 'https://example.com/a b' --es title 'it'\''s here'"
/// );
/// ```
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Intent {
    /// `-a`
    pub action: Option<String>,
    /// `-d`
    pub data: Option<String>,
    /// `-t`
    pub mime_type: Option<String>,
    /// `-c`
    pub category: Option<String>,
    /// `-n`
    pub component: Option<String>,
    /// `-p`
    pub package: Option<String>,
    /// `--user`
    pub user_id: Option<String>,
    /// `-f`, if not zero
    pub flags: u32,
    /// `--receiver-foreground` (broadcasts only)
    pub receiver_foreground: bool,
    /// `-W`: wait for the launch to complete (activities only)
    pub wait: bool,
    pub extra: Extra,
}

/// Intent extras.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Extra {
    pub es: HashMap<String, String>,
    pub ez: HashMap<String, bool>,
    pub ei: HashMap<String, i32>,
    pub el: HashMap<String, i64>,
    pub ef: HashMap<String, f32>,
    pub eu: HashMap<String, String>,
    pub ecn: HashMap<String, String>,
    pub eia: HashMap<String, Vec<i32>>,
    pub ela: HashMap<String, Vec<i64>>,
    pub efa: HashMap<String, Vec<f32>>,
    pub esa: HashMap<String, Vec<String>>,
    pub grant_read_uri_permission: bool,
    pub grant_write_uri_permission: bool,
    pub exclude_stopped_packages: bool,
    pub include_stopped_packages: bool,
}

impl<'a> ActivityManager<'a> {
    fn am<I, S>(&self, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut command: Vec<std::ffi::OsString> = vec!["am".into()];
        command.extend(args.into_iter().map(|s| s.as_ref().to_os_string()));
        let output = self.shell.exec(&command)?;
        check_am_output(&command, output)
    }

    pub fn force_stop(&self, package_name: &str) -> Result<()> {
        self.am(["force-stop", package_name]).map(|_| ())
    }

    pub fn start(&self, intent: &Intent) -> Result<()> {
        self.am(intent_command("start", intent)).map(|_| ())
    }

    pub fn start_service(&self, intent: &Intent) -> Result<()> {
        self.am(intent_command("startservice", intent)).map(|_| ())
    }

    pub fn start_foreground_service(&self, intent: &Intent) -> Result<()> {
        self.am(intent_command("start-foreground-service", intent))
            .map(|_| ())
    }

    pub fn broadcast(&self, intent: &Intent) -> Result<()> {
        self.am(intent_command("broadcast", intent)).map(|_| ())
    }

    /// Kill the background processes of a package.
    pub fn kill(&self, user: UserOption, package_name: &str) -> Result<()> {
        let mut args = vec!["kill".to_string()];
        args.extend(user.args());
        args.push(package_name.to_string());
        self.am(args).map(|_| ())
    }

    /// Kill all the background processes.
    pub fn kill_all(&self) -> Result<()> {
        self.am(["kill-all"]).map(|_| ())
    }

    pub fn trim_memory(&self, process_name: &str, status: MemoryStatus) -> Result<()> {
        self.am(["send-trim-memory", process_name, &status.to_string()])
            .map(|_| ())
    }

    /// Crash a package or a process.
    pub fn crash(&self, user_id: Option<&str>, package_or_pid: &str) -> Result<()> {
        let mut args = vec!["crash"];
        if let Some(user_id) = user_id {
            args.extend(["--user", user_id]);
        }
        args.push(package_or_pid);
        self.am(args).map(|_| ())
    }

    /// The id of the current foreground user.
    pub fn get_current_user(&self) -> Result<String> {
        stdout_string(&self.am(["get-current-user"])?)
    }
}

fn intent_command(command: &str, intent: &Intent) -> Vec<String> {
    let mut args = vec![command.to_string()];
    args.extend(intent.args());
    args
}

/// Devices before Android 7 do not return the exit status of shell commands:
/// look for `am` error messages too.
fn check_am_output(command: &[std::ffi::OsString], output: Output) -> Result<Output> {
    let has_error = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes)
            .lines()
            .any(|line| line.starts_with("Error:") || line.starts_with("Error type"))
    };
    if has_error(&output.stdout) || has_error(&output.stderr) {
        let command = quote_args(command).join(" ");
        Err(CommandFailure::new(command, &output).into())
    } else {
        Ok(output)
    }
}

impl UserOption {
    fn args(&self) -> Vec<String> {
        match self {
            UserOption::UserId(user_id) => vec!["--user".to_string(), user_id.clone()],
            UserOption::All => vec!["--user".to_string(), "all".to_string()],
            UserOption::Current => vec!["--user".to_string(), "current".to_string()],
            UserOption::None => vec![],
        }
    }
}

impl Intent {
    pub fn new() -> Intent {
        Intent::default()
    }

    pub fn from_action(action: &str) -> Intent {
        Intent {
            action: Some(action.to_string()),
            ..Default::default()
        }
    }

    /// The `am` arguments of this intent (not quoted).
    pub fn args(&self) -> Vec<String> {
        let mut args: Vec<String> = vec![];
        let options = [
            ("-a", &self.action),
            ("-d", &self.data),
            ("-t", &self.mime_type),
            ("-c", &self.category),
            ("-n", &self.component),
            ("-p", &self.package),
            ("--user", &self.user_id),
        ];
        for (flag, value) in options {
            if let Some(value) = value {
                args.extend([flag.to_string(), value.clone()]);
            }
        }
        if self.flags != 0 {
            args.extend(["-f".to_string(), format!("{:#x}", self.flags)]);
        }
        if self.receiver_foreground {
            args.push("--receiver-foreground".to_string());
        }
        if self.wait {
            args.push("-W".to_string());
        }
        args.extend(self.extra.args());
        args
    }
}

impl Display for Intent {
    /// The intent arguments, quoted for the device shell.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", quote_args(self.args()).join(" "))
    }
}

/// `flag key value` for each entry, sorted by key.
fn extra_args<V, F>(args: &mut Vec<String>, flag: &str, map: &HashMap<String, V>, to_string: F)
where
    F: Fn(&V) -> String,
{
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in entries {
        args.extend([flag.to_string(), key.clone(), to_string(value)]);
    }
}

fn join<T: ToString>(values: &[T]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

impl Extra {
    pub fn put_string_extra(&mut self, name: &str, value: &str) -> &mut Self {
        self.es.insert(name.to_string(), value.to_string());
        self
    }

    pub fn put_bool_extra(&mut self, name: &str, value: bool) -> &mut Self {
        self.ez.insert(name.to_string(), value);
        self
    }

    pub fn put_int_extra(&mut self, name: &str, value: i32) -> &mut Self {
        self.ei.insert(name.to_string(), value);
        self
    }

    pub fn put_long_extra(&mut self, name: &str, value: i64) -> &mut Self {
        self.el.insert(name.to_string(), value);
        self
    }

    pub fn put_float_extra(&mut self, name: &str, value: f32) -> &mut Self {
        self.ef.insert(name.to_string(), value);
        self
    }

    pub fn put_string_array_extra(&mut self, name: &str, value: Vec<String>) -> &mut Self {
        self.esa.insert(name.to_string(), value);
        self
    }

    /// The `am` arguments of these extras (not quoted).
    pub fn args(&self) -> Vec<String> {
        let mut args = vec![];
        extra_args(&mut args, "--es", &self.es, String::clone);
        extra_args(&mut args, "--ez", &self.ez, bool::to_string);
        extra_args(&mut args, "--ei", &self.ei, i32::to_string);
        extra_args(&mut args, "--el", &self.el, i64::to_string);
        extra_args(&mut args, "--ef", &self.ef, f32::to_string);
        extra_args(&mut args, "--eu", &self.eu, String::clone);
        extra_args(&mut args, "--ecn", &self.ecn, String::clone);
        extra_args(&mut args, "--eia", &self.eia, |v| join(v));
        extra_args(&mut args, "--ela", &self.ela, |v| join(v));
        extra_args(&mut args, "--efa", &self.efa, |v| join(v));
        // commas inside the values are escaped
        extra_args(&mut args, "--esa", &self.esa, |v| {
            v.iter()
                .map(|s| s.replace(',', "\\,"))
                .collect::<Vec<_>>()
                .join(",")
        });
        let flags = [
            (
                self.grant_read_uri_permission,
                "--grant-read-uri-permission",
            ),
            (
                self.grant_write_uri_permission,
                "--grant-write-uri-permission",
            ),
            (self.exclude_stopped_packages, "--exclude-stopped-packages"),
            (self.include_stopped_packages, "--include-stopped-packages"),
        ];
        args.extend(
            flags
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, flag)| flag.to_string()),
        );
        args
    }
}

impl Display for Extra {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", quote_args(self.args()).join(" "))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_intent_args() {
        let mut intent = Intent {
            action: Some("android.intent.action.VIEW".to_string()),
            data: Some("https://example.com/a b".to_string()),
            component: Some("com.example/.Main".to_string()),
            user_id: Some("0".to_string()),
            flags: 0x10000000,
            wait: true,
            ..Default::default()
        };
        intent
            .extra
            .put_string_extra("b", "v w")
            .put_string_extra("a", "x")
            .put_bool_extra("flag", true)
            .put_int_extra("count", 3);
        assert_eq!(
            intent.args(),
            [
                "-a",
                "android.intent.action.VIEW",
                "-d",
                "https://example.com/a b",
                "-n",
                "com.example/.Main",
                "--user",
                "0",
                "-f",
                "0x10000000",
                "-W",
                "--es",
                "a",
                "x",
                "--es",
                "b",
                "v w",
                "--ez",
                "flag",
                "true",
                "--ei",
                "count",
                "3"
            ]
        );
    }

    #[test]
    fn test_extra_arrays() {
        let mut extra = Extra::default();
        extra.put_string_array_extra("names", vec!["a,b".to_string(), "c".to_string()]);
        extra.eia.insert("ints".to_string(), vec![1, 2]);
        extra.efa.insert("floats".to_string(), vec![1.5]);
        extra.grant_read_uri_permission = true;
        // string arrays use --esa (they were sent as --efa before 2.0)
        assert_eq!(
            extra.args(),
            [
                "--eia",
                "ints",
                "1,2",
                "--efa",
                "floats",
                "1.5",
                "--esa",
                "names",
                r"a\,b,c",
                "--grant-read-uri-permission"
            ]
        );
    }

    #[test]
    fn test_user_option() {
        assert_eq!(
            UserOption::UserId("10".to_string()).args(),
            ["--user", "10"]
        );
        assert_eq!(UserOption::All.args(), ["--user", "all"]);
        assert!(UserOption::None.args().is_empty());
        assert_eq!(
            MemoryStatus::RunningModerate.to_string(),
            "RUNNING_MODERATE"
        );
    }

    #[test]
    fn test_check_am_output() {
        let output = |stdout: &str| Output {
            status: std::process::ExitStatus::default(),
            stdout: stdout.as_bytes().to_vec(),
            stderr: vec![],
        };
        let command = vec!["am".into(), "start".into()];
        assert!(check_am_output(&command, output("Starting: Intent { }\n")).is_ok());
        let err = check_am_output(
            &command,
            output("Starting: Intent { }\nError type 3\nError: Activity class does not exist.\n"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("Activity class does not exist"));
    }
}
