use std::ffi::OsStr;
use std::io::BufRead;
use std::time::Duration;

use futures::future::IntoFuture;
use lazy_static::lazy_static;
use props_rs::Property;
use regex::Regex;
use rustix::path::Arg;
use strum_macros::{Display, IntoStaticStr};
use tokio::sync::oneshot::Receiver;

use crate::command::{CommandBuilder, ProcessResult, Result};
use crate::errors::AdbError::Unknown;
use crate::errors::{AdbError, CommandError};
use crate::input::{InputSource, KeyCode, KeyEventType, MotionEvent};
use crate::intent::Intent;
use crate::traits::{AdbDevice, AsArgs};
use crate::util::Vec8ToString;
use crate::{Adb, SELinuxType, Shell};

#[derive(IntoStaticStr, Display)]
#[allow(non_camel_case_types)]
pub enum DumpsysPriority {
	CRITICAL,
	HIGH,
	NORMAL,
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ScreenRecordOptions {
	/// --bit-rate 4000000
	/// Set the video bit rate, in bits per second. Value may be specified as bits or megabits, e.g. '4000000' is equivalent to '4M'.
	/// Default 20Mbps.
	pub bitrate: Option<u64>,

	/// --time-limit=120 (in seconds)
	/// Set the maximum recording time, in seconds. Default / maximum is 180
	pub timelimit: Option<Duration>,

	/// --rotate
	/// Rotates the output 90 degrees. This feature is experimental.
	pub rotate: Option<bool>,

	/// --bugreport
	/// Add additional information, such as a timestamp overlay, that is helpful in videos captured to illustrate bugs.
	pub bug_report: Option<bool>,

	/// --size 1280x720
	/// Set the video size, e.g. "1280x720". Default is the device's main display resolution (if supported), 1280x720 if not.
	/// For best results, use a size supported by the AVC encoder.
	pub size: Option<(u16, u16)>,

	/// --verbose
	/// Display interesting information on stdout
	pub verbose: bool,
}

#[derive(IntoStaticStr)]
#[allow(non_camel_case_types)]
pub enum SettingsType {
	global,
	system,
	secure,
}

impl Shell {
	pub async fn exec<'a, D, T>(adb: &Adb, device: D, args: Vec<T>, signal: Option<IntoFuture<Receiver<()>>>) -> Result<ProcessResult>
	where
		T: Into<String> + AsRef<OsStr>,
		D: Into<&'a dyn AdbDevice>,
	{
		CommandBuilder::shell(adb, device).args(args).with_signal(signal).output().await
	}

	pub async fn exec_timeout<'a, D, T>(adb: &Adb, device: D, args: Vec<T>, timeout: Option<Duration>, signal: Option<IntoFuture<Receiver<()>>>) -> Result<ProcessResult>
	where
		T: Into<String> + AsRef<OsStr>,
		D: Into<&'a dyn AdbDevice>,
	{
		CommandBuilder::shell(adb, device).args(args).with_timeout(timeout).with_signal(signal).output().await
	}

	pub async fn list_settings<'a, D>(adb: &Adb, device: D, settings_type: SettingsType) -> Result<Vec<Property>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let output = Shell::exec(adb, device, vec!["settings", "list", settings_type.into()], None).await?;
		let result = props_rs::parse(&output.stdout())?;
		Ok(result)
	}

	pub async fn get_setting<'a, D>(adb: &Adb, device: D, settings_type: SettingsType, key: &str) -> Result<Option<String>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		Vec8ToString::as_str(&Shell::exec(adb, device, vec!["settings", "get", settings_type.into(), key], None).await?.stdout())
			.map(|s| Some(s.trim_end().to_string()))
			.ok_or(Unknown("unexpected error".to_string()))
	}

	pub async fn put_setting<'a, D>(adb: &Adb, device: D, settings_type: SettingsType, key: &str, value: &str) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		Shell::exec(adb, device, vec!["settings", "put", settings_type.into(), key, value], None).await?;
		Ok(())
	}

	pub async fn delete_setting<'a, D>(adb: &Adb, device: D, settings_type: SettingsType, key: &str) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		Shell::exec(adb, device, vec!["settings", "delete", settings_type.into(), key], None).await?;
		Ok(())
	}

	pub async fn ls<'d, 't, D, T>(adb: &Adb, device: D, path: T, options: Option<&str>) -> Result<Vec<String>>
	where
		D: Into<&'d dyn AdbDevice>,
		T: Into<&'t str> + AsRef<OsStr> + Arg,
	{
		let mut args = vec!["ls"];
		if let Some(options) = options {
			args.push(options);
		}
		args.push(path.into());

		let stdout = Shell::exec(adb, device, args, None).await?.stdout();
		let lines = stdout.lines().filter_map(|s| s.ok()).collect();
		Ok(lines)
	}

	pub async fn dumpsys_list<'a, D>(adb: &Adb, device: D, proto_only: bool, priority: Option<DumpsysPriority>) -> Result<Vec<String>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["dumpsys", "-l"];
		if proto_only {
			args.push("--proto");
		}

		if let Some(priority) = priority {
			args.push("--priority");
			args.push(priority.into());
		}

		let output: Vec<String> = Shell::exec(adb, device, args, None)
			.await?
			.stdout()
			.lines()
			.filter_map(|f| match f {
				Ok(s) => {
					let line = String::from(s.trim());
					match line {
						x if x.ends_with(':') => None,
						x => Some(x),
					}
				}
				Err(_) => None,
			})
			.collect();

		Ok(output)
	}

	///
	/// usage: dumpsys
	//         To dump all services.
	//or:
	//       dumpsys [-t TIMEOUT] [--priority LEVEL] [--pid] [--thread] [--help | -l | --skip SERVICES | SERVICE [ARGS]]
	//         --help: shows this help
	//         -l: only list services, do not dump them
	//         -t TIMEOUT_SEC: TIMEOUT to use in seconds instead of default 10 seconds
	//         -T TIMEOUT_MS: TIMEOUT to use in milliseconds instead of default 10 seconds
	//         --pid: dump PID instead of usual dump
	//         --thread: dump thread usage instead of usual dump
	//         --proto: filter services that support dumping data in proto format. Dumps
	//               will be in proto format.
	//         --priority LEVEL: filter services based on specified priority
	//               LEVEL must be one of CRITICAL | HIGH | NORMAL
	//         --skip SERVICES: dumps all services but SERVICES (comma-separated list)
	//         SERVICE [ARGS]: dumps only service SERVICE, optionally passing ARGS to it
	pub async fn dumpsys<'d, D>(
		adb: &Adb,
		device: D,
		service: Option<&str>,
		arguments: Option<Vec<String>>,
		timeout: Option<Duration>,
		pid: bool,
		thread: bool,
		proto: bool,
		skip: Option<Vec<String>>,
	) -> Result<ProcessResult>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut args = vec!["dumpsys".to_string()];

		if let Some(timeout) = timeout {
			args.push("-T".to_string());
			args.push(timeout.as_millis().to_string());
		}

		if pid {
			args.push("--pid".to_string());
		} else if thread {
			args.push("--thread".to_string());
		}

		if proto {
			args.push("--proto".to_string());
		}

		if let Some(skip) = skip {
			args.push("--skip".to_string());
			args.push(skip.join(","));
		} else if let Some(service) = service {
			args.push(service.to_string());

			if let Some(arguments) = arguments {
				args.extend(arguments);
			}
		}

		Shell::exec(adb, device, args, None).await
	}

	pub async fn screen_record<'d, D>(adb: &Adb, device: D, options: Option<ScreenRecordOptions>, output: &str, signal: Option<IntoFuture<Receiver<()>>>) -> Result<ProcessResult>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut args = vec![String::from("screenrecord")];

		if let Some(options) = options {
			args.extend(options.as_args());
		}

		args.push(output.to_string());
		CommandBuilder::shell(adb, device).args(args).with_signal(signal).output().await
	}

	pub async fn save_screencap<'d, 't, D, T>(adb: &Adb, device: D, path: T) -> Result<ProcessResult>
	where
		D: Into<&'d dyn AdbDevice>,
		T: Into<&'t str> + AsRef<OsStr> + Arg,
	{
		Shell::exec(adb, device, vec!["screencap", "-p", path.into()], None).await
	}

	pub async fn is_screen_on<'a, D>(adb: &Adb, device: D) -> Result<bool>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let process_result = Shell::exec(adb, device, vec!["dumpsys input_method | egrep 'mInteractive=(true|false)'"], None).await?;
		let result = Vec8ToString::as_str(&process_result.stdout())
			.map(|f| f.contains("mInteractive=true"))
			.ok_or(CommandError::from("unexpected error"))?;
		Ok(result)
	}

	pub async fn send_swipe<'a, D>(adb: &Adb, device: D, from_pos: (i32, i32), to_pos: (i32, i32), duration: Option<Duration>, source: Option<InputSource>) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];
		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("swipe");

		let pos_string = format!("{:?} {:?} {:?} {:?}", from_pos.0, from_pos.1, to_pos.0, to_pos.1);
		args.push(pos_string.as_str());

		#[allow(unused_assignments)]
		let mut duration_str: String = String::from("");

		if let Some(duration) = duration {
			duration_str = duration.as_millis().to_string();
			args.push(duration_str.as_str());
		}

		Shell::exec(adb, device, args, None).await?;
		Ok(())
	}

	pub async fn send_tap<'a, D>(adb: &Adb, device: D, position: (i32, i32), source: Option<InputSource>) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];
		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("tap");

		let pos0 = format!("{:?}", position.0);
		let pos1 = format!("{:?}", position.1);

		args.push(pos0.as_str());
		args.push(pos1.as_str());

		Shell::exec(adb, device, args, None).await?;
		Ok(())
	}

	pub async fn send_char<'a, D>(adb: &Adb, device: D, text: &char, source: Option<InputSource>) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];
		if let Some(source) = source {
			args.push(source.into());
		}

		let formatted = format!("{:}", text);

		args.push("text");
		args.push(formatted.as_str());
		Shell::exec(adb, device, args, None).await?;
		Ok(())
	}

	pub async fn send_text<'a, D>(adb: &Adb, device: D, text: &str, source: Option<InputSource>) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];
		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("text");
		let formatted = format!("{:?}", text);
		args.push(formatted.as_str());

		Shell::exec(adb, device, args, None).await?;
		Ok(())
	}

	pub async fn send_event<'a, D>(adb: &Adb, device: D, event: &str, code_type: i32, code: i32, value: i32) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		Shell::exec(
			adb,
			device,
			vec!["sendevent", event, format!("{}", code_type).as_str(), format!("{}", code).as_str(), format!("{}", value).as_str()],
			None,
		)
		.await?;
		Ok(())
	}

	pub async fn send_motion<'a, D>(adb: &Adb, device: D, source: Option<InputSource>, motion: MotionEvent, pos: (i32, i32)) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];
		if let Some(source) = source {
			args.push(source.into());
		}
		args.push("motionevent");
		args.push(motion.into());

		let pos0 = pos.0.to_string();
		let pos1 = pos.1.to_string();

		args.push(pos0.as_str());
		args.push(pos1.as_str());
		Shell::exec(adb, device, args, None).await.map(|_| ())
	}

	pub async fn send_draganddrop<'a, D>(adb: &Adb, device: D, source: Option<InputSource>, duration: Option<Duration>, from_pos: (i32, i32), to_pos: (i32, i32)) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input".to_string()];
		if let Some(source) = source {
			let string: &str = source.into();
			args.push(string.to_string());
		}
		args.push("draganddrop".to_string());

		let pos0 = from_pos.0.to_string();
		let pos1 = from_pos.1.to_string();
		let pos2 = to_pos.0.to_string();
		let pos3 = to_pos.1.to_string();

		args.push(pos0);
		args.push(pos1);
		args.push(pos2);
		args.push(pos3);

		if let Some(duration) = duration {
			let duration_str = duration.as_millis().to_string();
			args.push(duration_str);
		}

		Shell::exec(adb, device, args, None).await.map(|_| ())
	}

	pub async fn send_press<'a, D>(adb: &Adb, device: D, source: Option<InputSource>) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];
		if let Some(source) = source {
			args.push(source.into());
		}
		args.push("press");
		Shell::exec(adb, device, args, None).await?;
		Ok(())
	}

	pub async fn send_keyevent<'a, D>(adb: &Adb, device: D, keycode: KeyCode, event_type: Option<KeyEventType>, source: Option<InputSource>) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keyevent");

		if let Some(event_type) = event_type {
			args.push(event_type.into());
		}

		args.push(keycode.into());
		Shell::exec(adb, device, args, None).await?;
		Ok(())
	}

	pub async fn send_keyevents<'a, D>(adb: &Adb, device: D, keycodes: Vec<KeyCode>, source: Option<InputSource>) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keyevent");
		args.extend(keycodes.iter().map(|k| k.into()).collect::<Vec<&str>>());

		Shell::exec(adb, device, args, None).await?;
		Ok(())
	}

	pub async fn send_keycombination<'a, D>(adb: &Adb, device: D, source: Option<InputSource>, keycodes: Vec<KeyCode>) -> Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keycombination");
		args.extend(keycodes);
		Shell::exec(adb, device, args, None).await?;
		Ok(())
	}

	pub async fn get_events<'a, D>(adb: &Adb, device: D) -> Result<Vec<(String, String)>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let result = Shell::exec(adb, device, vec!["getevent", "-S"], None).await?.stdout();

		lazy_static! {
			static ref RE: Regex = Regex::new("^add\\s+device\\s+[0-9]+:\\s(?P<event>[^\n]+)\\s*name:\\s*\"(?P<name>[^\"]+)\"\n?").unwrap();
		}

		let mut v: Vec<(String, String)> = vec![];
		let mut string = Arg::as_str(&result)?;

		loop {
			let captures = RE.captures(string);
			if let Some(cap) = captures {
				let e = cap.name("event");
				let n = cap.name("name");

				if e.is_some() && n.is_some() {
					v.push((e.ok_or(AdbError::ParseInputError())?.as_str().to_string(), n.ok_or(AdbError::ParseInputError())?.as_str().to_string()));
				}

				string = &string[cap[0].len()..]
			} else {
				break;
			}
		}
		Ok(v)
	}

	pub async fn stat<'d, D>(adb: &Adb, device: D, path: &OsStr) -> Result<file_mode::Mode>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Arg::as_str(&Shell::exec(adb, device, vec!["stat", "-L", "-c", "'%a'", format!("{:?}", path).as_str()], None).await?.stdout())?
			.trim_end()
			.parse::<u32>()?;

		let mode = file_mode::Mode::from(output);
		Ok(mode)
	}

	async fn test_file<'d, D, T: Arg>(adb: &Adb, device: D, path: T, mode: &str) -> Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Shell::exec(adb, device, vec![format!("test -{:} {:?} && echo 1 || echo 0", mode, path.as_str()?).as_str()], None).await;

		match Vec8ToString::as_str(&output?.stdout()) {
			Some(s) => Ok(s.trim_end() == "1"),
			None => Ok(false),
		}
	}

	pub async fn exists<'d, D, T: Arg>(adb: &Adb, device: D, path: T) -> Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::test_file(adb, device, path, "e").await
	}

	pub async fn rm<'d, 't, D, T>(adb: &Adb, device: D, path: T, options: Option<Vec<&str>>) -> Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
		T: Arg,
	{
		let mut args = vec!["rm"];
		if let Some(options) = options {
			args.extend(options);
		}
		args.push(path.as_str()?);

		Shell::exec(adb, device, args, None).await.map(|_| true)
	}

	pub async fn is_file<'d, D, T: Arg>(adb: &Adb, device: D, path: T) -> Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::test_file(adb, device, path, "f").await
	}

	pub async fn is_dir<'d, D, T: Arg>(adb: &Adb, device: D, path: T) -> Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::test_file(adb, device, path, "d").await
	}

	pub async fn is_symlink<'d, D, T: Arg>(adb: &Adb, device: D, path: T) -> Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::test_file(adb, device, path, "h").await
	}

	pub async fn getprop<'d, D>(adb: &Adb, device: D, key: &str) -> Result<String>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let result = Shell::exec(adb, device, vec!["getprop", key], None).await.map(|s| s.stdout())?;
		Ok(Arg::as_str(&result).map(|f| f.trim_end())?.to_string())
	}

	pub async fn setprop<'d, D, T: Arg>(adb: &Adb, device: D, key: &str, value: T) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut new_value = value.as_str()?;
		if new_value == "" {
			new_value = "\"\""
		}

		Shell::exec(adb, device, vec!["setprop", key, new_value], None).await.map(|_| ())
	}

	pub async fn clear_prop<'d, D, T: Arg>(adb: &Adb, device: D, key: &str) -> Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::setprop(adb, device, key, "").await
	}

	pub async fn getprop_type<'d, D>(adb: &Adb, device: D, key: &str) -> Result<Vec<u8>>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::exec(adb, device, vec!["getprop", "-T", key], None).await.map(|s| s.stdout())
	}

	pub async fn getprops<'a, D>(adb: &Adb, device: D) -> Result<Vec<Property>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let output = Shell::exec(adb, device, vec!["getprop"], None).await;

		lazy_static! {
			static ref RE: Regex = Regex::new("(?m)^\\[(.*)\\]\\s*:\\s*\\[([^\\]]*)\\]$").unwrap();
		}

		let mut result: Vec<Property> = Vec::new();

		for line in output?.stdout().lines().filter_map(|l| l.ok()) {
			let captures = RE.captures(line.as_str());
			if let Some(cap1) = captures {
				let k = cap1.get(1);
				let v = cap1.get(2);
				if k.is_some() && v.is_some() {
					result.push(Property {
						key: k.ok_or(AdbError::ParseInputError())?.as_str().to_string(),
						value: v.ok_or(AdbError::ParseInputError())?.as_str().to_string(),
					});
				}
			}
		}
		Ok(result)
	}

	pub async fn cat<'d, D, P: Arg>(adb: &Adb, device: D, path: P) -> Result<Vec<u8>>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::exec(adb, device, vec!["cat", path.as_str()?], None).await.map(|s| s.stdout())
	}

	pub async fn which<'a, D>(adb: &Adb, device: D, command: &str) -> Result<Option<String>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let output = Shell::exec(adb, device, vec!["which", command], None).await;
		output.map(|s| Vec8ToString::as_str(&s.stdout()).map(|ss| String::from(ss.trim_end())))
	}

	/// Returns the current user runnign adb
	///
	/// # Arguments
	///
	/// * `adb`: adb path
	/// * `device`: connected device
	///
	/// returns: Result<Option<String>, Error>
	///
	/// # Examples
	///
	/// ```
	/// use radb_client::Device;
	/// use radb_client::types::AdbClient;
	///
	/// async fn get_user() {
	///     let client: AdbClient = "192.168.1.24:5555".parse::<Device>().unwrap().try_into().unwrap();
	///     client.connect(None).await.unwrap();
	///     let output = client.shell().whoami().unwrap();
	/// }
	/// ```
	pub async fn whoami<'a, T>(adb: &Adb, device: T) -> Result<Option<String>>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let result = Shell::exec(adb, device, vec!["whoami"], None).await?;
		Ok(Vec8ToString::as_str(&result.stdout()).map(|s| s.trim().to_string()))
	}

	pub async fn is_root<'a, T>(adb: &Adb, device: T) -> Result<bool>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let whoami = Shell::whoami(adb, device).await?;
		match whoami {
			Some(s) => Ok(s == "root"),
			None => Ok(false),
		}
	}

	pub async fn broadcast<'a, T>(adb: &Adb, device: T, intent: &Intent) -> Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let _result = Shell::exec(adb, device, vec!["am", "broadcast", format!("{:}", intent).as_str()], None).await?;
		Ok(())
	}

	pub async fn start<'a, T>(adb: &Adb, device: T, intent: &Intent) -> Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let _result = Shell::exec(adb, device, vec!["am", "start", format!("{:}", intent).as_str()], None).await?;
		Ok(())
	}

	pub async fn start_service<'a, T>(adb: &Adb, device: T, intent: &Intent) -> Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let _result = Shell::exec(adb, device, vec!["am", "startservice", format!("{:}", intent).as_str()], None).await?;
		Ok(())
	}

	pub async fn force_stop<'a, T>(adb: &Adb, device: T, package_name: &str) -> Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let _result = Shell::exec(adb, device, vec!["am", "force-stop", package_name], None).await?;
		Ok(())
	}

	pub async fn get_enforce<'a, T>(adb: &Adb, device: T) -> Result<SELinuxType>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let result = Shell::exec(adb, device, vec!["getenforce"], None).await?.stdout();
		let enforce: SELinuxType = SELinuxType::try_from(result)?;
		Ok(enforce)
	}

	pub async fn set_enforce<'a, T>(adb: &Adb, device: T, enforce: SELinuxType) -> Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let new_value = match enforce {
			SELinuxType::Permissive => "0",
			SELinuxType::Enforcing => "1",
		};

		Shell::exec(adb, device, vec!["setenforce", new_value], None).await.map(|_| ())
	}
}
