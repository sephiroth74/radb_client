use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader};
use std::process::{ExitStatus, Output};
use std::sync::Mutex;
use std::time::Duration;

use cached::{Cached, SizedCache};
use cmd_lib::AsOsStr;
use crossbeam_channel::Receiver;
use lazy_static::lazy_static;
use regex::Regex;
use rustix::path::Arg;
use simple_cmd::debug::CommandDebug;
use simple_cmd::prelude::OutputExt;
use simple_cmd::CommandBuilder;

use crate::types::{
	DumpsysPriority, FFPlayOptions, InputSource, KeyCode, KeyEventType, PropType, Property, SELinuxType, ScreenRecordOptions,
	SettingsType,
};
use crate::v2::error::Error;
use crate::v2::prelude::*;
use crate::v2::result::Result;
use crate::v2::types::{ActivityManager, Shell};

lazy_static! {
	static ref RE_GET_PROPS: Regex = Regex::new("(?m)^\\[(.*)\\]\\s*:\\s*\\[([^\\]]*)\\]$").unwrap();
	static ref COMMANDS_CACHE: Mutex<SizedCache<String, Option<String>>> = Mutex::new(SizedCache::with_size(10));
}

impl<'a> Shell<'a> {
	/// executes custom command over the shell interface
	pub fn exec<I, S>(&self, args: I, cancel: Option<Receiver<()>>, timeout: Option<Duration>) -> Result<Output>
	where
		I: IntoIterator<Item = S>,
		S: AsRef<OsStr>,
	{
		let builder = CommandBuilder::shell(self.parent).args(args).signal(cancel).timeout(timeout);
		Ok(builder.build().output()?)
	}

	pub fn try_exec<I, S>(&self, args: I, cancel: Option<Receiver<()>>, timeout: Option<Duration>) -> Result<Option<ExitStatus>>
	where
		I: IntoIterator<Item = S>,
		S: AsRef<OsStr>,
	{
		let builder = CommandBuilder::shell(self.parent).args(args).signal(cancel).timeout(timeout);
		Ok(builder.build().run()?)
	}

	/// return if adb is running as root
	pub fn is_root(&self) -> Result<bool> {
		let whoami = self.whoami()?;
		Ok(whoami.eq_ignore_ascii_case("root"))
	}

	/// Returns the current running adb user
	///
	/// # Example
	///
	/// ```rust
	/// use radb_client::v2::types::Client;
	/// use radb_client::v2::types::ConnectionType;
	///
	/// fn get_user() {
	///     let client: Client = Client::try_from(ConnectionType::try_from_ip("192.168.1.42:5555")).unwrap();
	///     client.connect(None).unwrap();
	///     let output = client.shell().whoami().unwrap();
	/// }
	/// ```
	pub fn whoami(&self) -> Result<String> {
		let output = self.exec(vec!["whoami"], None, None)?;
		Ok(Arg::as_str(&output.stdout)?.trim().to_owned())
	}

	pub fn mount<T: Arg>(&self, dir: T) -> Result<()> {
		self.exec(
			vec![
				"mount -o rw,remount",
				dir.as_str()?,
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn unmount<T: Arg>(&self, dir: T) -> Result<()> {
		self.exec(
			vec![
				"mount -o ro,remount",
				dir.as_str()?,
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn cat<P: Arg>(&self, path: P) -> Result<Vec<u8>> {
		self.exec(
			vec![
				"cat",
				path.as_str()?,
			],
			None,
			None,
		)
		.map(|s| s.stdout)
	}

	/// Check if avbctl is available on the connected device
	fn check_avbctl(&self) -> Result<()> {
		self.get_command_path("avbctl")
			.map(|_| ())
			.ok_or(std::io::ErrorKind::NotFound.into())
	}

	/// Returns if avbctl is available
	#[allow(dead_code)]
	fn has_avbctl(&self) -> Result<bool> {
		self.check_avbctl().map(|_| true)
	}

	pub fn get_command_path<T: Arg>(&self, command: T) -> Option<String> {
		if let Ok(command_string) = command.as_str() {
			let mut binding = COMMANDS_CACHE.lock().unwrap();
			let cache_key = format!("{}{}", self.parent.addr, command_string);

			binding
				.cache_get_or_set_with(cache_key.clone(), || {
					self.exec(vec![format!("command -v {}", command_string).as_str()], None, None)
						.and_then(|result| Ok(Arg::as_str(&result.stdout)?.trim().to_string()))
						.and_then(|result| {
							if result.is_empty() {
								Err(std::io::ErrorKind::NotFound.into())
							} else {
								Ok(result)
							}
						})
						.ok()
				})
				.clone()
		} else {
			None
		}
	}

	pub fn which<T: Arg>(&self, command: T) -> Option<String> {
		if let Ok(command) = command.as_str() {
			let output = self.exec(
				vec![
					"which", command,
				],
				None,
				None,
			);
			if let Ok(output) = output {
				simple_cmd::Vec8ToString::as_str(&output.stdout).map(|ss| String::from(ss.trim_end()))
			} else {
				None
			}
		} else {
			None
		}
	}

	/// Returns the verity status
	pub fn get_verity(&self) -> Result<bool> {
		let _ = self.check_avbctl()?;
		let output = self.exec(
			vec![
				"avbctl",
				"get-verity",
			],
			None,
			None,
		)?;
		let string = Arg::as_str(&output.stdout)?;
		Ok(string.contains("enabled"))
	}

	/// Disable verity using the avbctl service, if available
	pub fn disable_verity(&self) -> Result<()> {
		let _ = self.check_avbctl()?;
		let output = self.exec(
			vec![
				"avbctl",
				"disable-verity",
			],
			None,
			None,
		)?;

		if output.error() {
			Err(output.into())
		} else {
			Ok(())
		}
	}

	/// Enable verity using the avbctl service, if available
	pub fn enable_verity(&self) -> Result<()> {
		let _ = self.check_avbctl()?;
		let output = self.exec(
			vec![
				"avbctl",
				"enable-verity",
			],
			None,
			None,
		)?;
		if output.error() {
			Err(output.into())
		} else {
			Ok(())
		}
	}

	/// Returns the selinux enforce status
	pub fn get_enforce(&self) -> Result<SELinuxType> {
		let result = self.exec(vec!["getenforce"], None, None)?.stdout;
		let enforce: SELinuxType = SELinuxType::try_from(result)?;
		Ok(enforce)
	}

	/// Change the selinux enforce type. root is required
	pub fn set_enforce(&self, enforce: SELinuxType) -> Result<()> {
		let new_value = match enforce {
			SELinuxType::Permissive => "0",
			SELinuxType::Enforcing => "1",
		};

		self.exec(
			vec![
				"setenforce",
				new_value,
			],
			None,
			None,
		)
		.map(|_| ())
	}

	/// Returns true if the screen is on
	pub fn is_screen_on(&self) -> Result<bool> {
		let process_result = self.exec(vec!["dumpsys input_method | egrep 'mInteractive=(true|false)'"], None, None)?;
		let result = rustix::path::Arg::as_str(&process_result.stdout).map(|f| f.contains("mInteractive=true"))?;
		Ok(result)
	}

	pub fn send_keyevent(&self, keycode: KeyCode, event_type: Option<KeyEventType>, source: Option<InputSource>) -> Result<()> {
		let result = self.exec(self.make_keyevent(keycode, event_type, source), None, None)?;
		Shell::handle_result(result)
	}

	pub fn send_keycode(&self, keycode: u32, event_type: Option<KeyEventType>, source: Option<InputSource>) -> Result<()> {
		Shell::handle_result(self.exec(self.make_keycode(keycode, event_type, source), None, None)?)
	}

	pub fn send_swipe(
		&self,
		from_pos: (i32, i32),
		to_pos: (i32, i32),
		duration: Option<Duration>,
		source: Option<InputSource>,
	) -> Result<()> {
		Shell::handle_result(self.exec(self.make_swipe(from_pos, to_pos, duration, source), None, None)?)
	}

	pub fn try_send_keyevent(
		&self,
		keycode: KeyCode,
		event_type: Option<KeyEventType>,
		source: Option<InputSource>,
	) -> Result<Option<ExitStatus>> {
		self.try_exec(self.make_keyevent(keycode, event_type, source), None, None)
	}

	pub fn try_send_keycode(
		&self,
		keycode: u32,
		event_type: Option<KeyEventType>,
		source: Option<InputSource>,
	) -> Result<Option<ExitStatus>> {
		self.try_exec(self.make_keycode(keycode, event_type, source), None, None)
	}

	pub fn try_send_swipe(
		&self,
		from_pos: (i32, i32),
		to_pos: (i32, i32),
		duration: Option<Duration>,
		source: Option<InputSource>,
	) -> Result<Option<ExitStatus>> {
		self.try_exec(self.make_swipe(from_pos, to_pos, duration, source), None, None)
	}

	fn make_swipe(
		&self,
		from_pos: (i32, i32),
		to_pos: (i32, i32),
		duration: Option<Duration>,
		source: Option<InputSource>,
	) -> Vec<String> {
		let mut args = vec!["input".to_string()];
		if let Some(source) = source {
			let source_str: &str = source.into();
			args.push(source_str.to_string());
		}

		args.push("swipe".to_string());

		let pos_string = format!("{:?} {:?} {:?} {:?}", from_pos.0, from_pos.1, to_pos.0, to_pos.1);
		args.push(pos_string);

		#[allow(unused_assignments)]
		let mut duration_str: String = String::from("");

		if let Some(duration) = duration {
			duration_str = duration.as_millis().to_string();
			args.push(duration_str);
		}

		args
	}

	fn make_keyevent(&self, keycode: KeyCode, event_type: Option<KeyEventType>, source: Option<InputSource>) -> Vec<String> {
		let mut args = vec!["input".into()];

		if let Some(source) = source {
			let source_str: &str = source.into();
			args.push(source_str.to_string());
		}

		args.push("keyevent".into());

		if let Some(event_type) = event_type {
			let event_type_str: &str = event_type.into();
			args.push(event_type_str.to_string());
		}

		args.push(keycode.to_string());
		args
	}

	fn make_keycode(&self, keycode: u32, event_type: Option<KeyEventType>, source: Option<InputSource>) -> Vec<String> {
		let mut args = vec!["input".to_string()];

		if let Some(source) = source {
			let source_src: &str = source.into();
			args.push(source_src.to_string());
		}

		args.push("keyevent".to_string());

		if let Some(event_type) = event_type {
			let event_tyoe_str: &str = event_type.into();
			args.push(event_tyoe_str.to_string());
		}

		let keycode_str = keycode.to_string();

		args.push(keycode_str);
		args
	}

	pub fn get_events(&self) -> Result<Vec<(String, String)>> {
		let result = self
			.exec(
				vec![
					"getevent", "-S",
				],
				None,
				None,
			)?
			.stdout;

		lazy_static! {
			static ref RE: Regex =
				Regex::new("^add\\s+device\\s+[0-9]+:\\s(?P<event>[^\n]+)\\s*name:\\s*\"(?P<name>[^\"]+)\"\n?").unwrap();
		}

		let mut v: Vec<(String, String)> = vec![];
		let mut string = Arg::as_str(&result)?;

		loop {
			let captures = RE.captures(string);
			if let Some(cap) = captures {
				let e = cap.name("event");
				let n = cap.name("name");

				if e.is_some() && n.is_some() {
					v.push((
						e.ok_or(Error::ParseInputError())?.as_str().to_string(),
						n.ok_or(Error::ParseInputError())?.as_str().to_string(),
					));
				}

				string = &string[cap[0].len()..]
			} else {
				break;
			}
		}
		Ok(v)
	}

	pub fn file_mode<T: Arg>(&self, path: T) -> Result<file_mode::Mode> {
		let output = Arg::as_str(
			&self
				.exec(
					vec![
						"stat",
						"-L",
						"-c",
						"'%a'",
						path.as_str()?,
					],
					None,
					None,
				)?
				.stdout,
		)?
		.trim_end()
		.parse::<u32>()?;

		let mode = file_mode::Mode::from(output);
		Ok(mode)
	}

	pub fn list_settings(&self, settings_type: SettingsType) -> Result<Vec<Property>> {
		let output = self.exec(
			vec![
				"settings",
				"list",
				settings_type.into(),
			],
			None,
			None,
		)?;

		let reader = BufReader::new(output.stdout.as_slice());
		let hashmap = java_properties::read(reader)?;
		let result = hashmap.into_iter().map(|(key, value)| Property { key, value }).collect();
		Ok(result)
	}

	pub fn get_setting(&self, settings_type: SettingsType, key: &str) -> Result<Option<String>> {
		let output = &self
			.exec(
				vec![
					"settings",
					"get",
					settings_type.into(),
					key,
				],
				None,
				None,
			)?
			.stdout;

		if !output.is_empty() {
			let o = Arg::as_str(&output).map(|s| s.trim().to_string())?;
			if &"null" == &o {
				Ok(None)
			} else {
				Ok(Some(o))
			}
		} else {
			Ok(None)
		}
	}

	pub fn put_setting<S: Into<String>>(&self, settings_type: SettingsType, key: &str, value: S) -> Result<()> {
		let result = self.exec(
			vec![
				"settings",
				"put",
				settings_type.into(),
				key,
				&value.into(),
			],
			None,
			None,
		)?;
		Shell::handle_result(result)
	}

	pub fn delete_setting(&self, settings_type: SettingsType, key: &str) -> Result<()> {
		let result = self.exec(
			vec![
				"settings",
				"delete",
				settings_type.into(),
				key,
			],
			None,
			None,
		)?;
		Shell::handle_result(result)
	}

	pub fn ls<T: Arg>(&self, path: T, command_args: Option<Vec<OsString>>) -> Result<Vec<String>> {
		let mut args = vec!["ls".as_os_str()];

		if let Some(command_args) = command_args {
			args.extend(command_args);
		}

		args.push(path.as_str()?.as_os_str());

		let stdout = self.exec(args, None, None)?.stdout;
		let lines = stdout.lines().filter_map(|s| s.ok()).collect();
		Ok(lines)
	}

	pub fn exists<T: Arg>(&self, path: T) -> Result<bool> {
		self.test_file(path, "e")
	}

	pub fn rm<T: Arg>(&self, path: T, options: Option<Vec<&str>>) -> Result<()> {
		let mut args = vec!["rm"];
		if let Some(options) = options {
			args.extend(options);
		}
		args.push(path.as_str()?);
		let result = self.exec(args, None, None)?;
		Shell::handle_result(result)
	}

	pub fn is_file<T: Arg>(&self, path: T) -> Result<bool> {
		self.test_file(path, "f")
	}

	pub fn is_dir<T: Arg>(&self, path: T) -> Result<bool> {
		self.test_file(path, "d")
	}

	pub fn is_symlink<T: Arg>(&self, path: T) -> Result<bool> {
		self.test_file(path, "h")
	}

	pub fn test_file<T: Arg>(&self, path: T, mode: &str) -> Result<bool> {
		let output = self.exec(
			vec![format!("test -{:} {:?} && echo 1 || echo 0", mode, path.as_str()?).as_str()],
			None,
			None,
		);

		match simple_cmd::Vec8ToString::as_str(&output?.stdout) {
			Some(s) => Ok(s.trim_end() == "1"),
			None => Ok(false),
		}
	}

	pub fn dumpsys_list(&self, proto_only: bool, priority: Option<DumpsysPriority>) -> Result<Vec<String>> {
		let mut args = vec![
			"dumpsys", "-l",
		];
		if proto_only {
			args.push("--proto");
		}

		if let Some(priority) = priority {
			args.push("--priority");
			args.push(priority.into());
		}

		let output: Vec<String> = self
			.exec(args, None, None)?
			.stdout
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
	///         To dump all services.
	///or:
	///       dumpsys [-t TIMEOUT] [--priority LEVEL] [--pid] [--thread] [--help | -l | --skip SERVICES | SERVICE [ARGS]]
	///         --help: shows this help
	///         -l: only list services, do not dump them
	///         -t TIMEOUT_SEC: TIMEOUT to use in seconds instead of default 10 seconds
	///         -T TIMEOUT_MS: TIMEOUT to use in milliseconds instead of default 10 seconds
	///         --pid: dump PID instead of usual dump
	///         --thread: dump thread usage instead of usual dump
	///         --proto: filter services that support dumping data in proto format. Dumps
	///               will be in proto format.
	///         --priority LEVEL: filter services based on specified priority
	///               LEVEL must be one of CRITICAL | HIGH | NORMAL
	///         --skip SERVICES: dumps all services but SERVICES (comma-separated list)
	///         SERVICE [ARGS]: dumps only service SERVICE, optionally passing ARGS to it
	pub fn dumpsys(
		&self,
		service: Option<&str>,
		arguments: Option<Vec<String>>,
		timeout: Option<Duration>,
		pid: bool,
		thread: bool,
		proto: bool,
		skip: Option<Vec<String>>,
	) -> Result<Output> {
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

		self.exec(args, None, None)
	}

	pub fn screen_record<T: Arg>(
		&self,
		options: Option<ScreenRecordOptions>,
		output: T,
		cancel: Option<Receiver<()>>,
	) -> Result<Output> {
		let mut args = vec![String::from("screenrecord")];

		if let Some(options) = options {
			args.extend(options);
		}

		args.push(output.as_str()?.to_string());
		let command = CommandBuilder::shell(self.parent).args(args).signal(cancel);
		Ok(command.build().output()?)
	}

	pub fn screen_mirror(
		&self,
		screenrecord_options: ScreenRecordOptions,
		play_options: FFPlayOptions,
		cancel: Option<Receiver<()>>,
	) -> Result<Output> {
		let screenrecord_arg = format!("screenrecord --output-format=h264 {:} -", screenrecord_options);

		let builder = CommandBuilder::shell(self.parent)
			.args(vec![screenrecord_arg.as_str()])
			.signal(cancel)
			.with_debug(true);

		let command1 = builder.build();
		let mut command2 = std::process::Command::new(which::which("ffplay")?);
		let ffplay_options = play_options;
		command2.args(ffplay_options.into_iter());
		command2.args(&[
			"-loglevel",
			"repeat+level+verbose",
			"-an",
			"-autoexit",
			"-sync",
			"video",
			"-",
		]);

		if self.parent.debug {
			command2.debug();
		}
		command1.pipe(command2).map_err(|e| Error::from(e))
	}

	pub fn save_screencap<T>(&self, path: T) -> Result<Output>
	where
		T: Arg,
	{
		self.exec(
			vec![
				"screencap",
				"-p",
				path.as_str()?,
			],
			None,
			None,
		)
	}

	pub fn getprop(&self, key: &str) -> Result<String> {
		let result = self
			.exec(
				vec![
					"getprop", key,
				],
				None,
				None,
			)
			.map(|s| s.stdout)?;
		Ok(Arg::as_str(&result).map(|f| f.trim_end())?.to_string())
	}

	pub fn getprops(&self) -> Result<Vec<Property>> {
		let output = self.exec(["getprop"], None, None);
		let mut result: Vec<Property> = Vec::new();

		for line in output?.stdout.lines().filter_map(|l| l.ok()) {
			let captures = RE_GET_PROPS.captures(line.as_str());
			if let Some(cap1) = captures {
				let k = cap1.get(1);
				let v = cap1.get(2);
				if k.is_some() && v.is_some() {
					result.push(Property {
						key: k.ok_or(Error::ParseInputError())?.as_str().to_string(),
						value: v.ok_or(Error::ParseInputError())?.as_str().to_string(),
					});
				}
			}
		}
		Ok(result)
	}

	pub fn getprops_type(&self) -> Result<HashMap<String, PropType>> {
		let output = self
			.exec(
				vec![
					"getprop", "-T",
				],
				None,
				None,
			)
			.map(|s| s.stdout)?;
		let hash_map = output
			.lines()
			.filter_map(|l| l.ok())
			.into_iter()
			.filter_map(|line| {
				let captures = RE_GET_PROPS.captures(line.as_str());
				if let Some(captures) = captures {
					if captures.len() == 3 {
						let k = captures.get(1).unwrap().as_str();
						let v = captures.get(2).unwrap().as_str();
						let prop_type = PropType::from(v);
						Some((k.to_string(), prop_type))
					} else {
						None
					}
				} else {
					None
				}
			})
			.collect::<HashMap<_, _>>();
		Ok(hash_map)
	}

	pub fn setprop<T: Arg>(&self, key: &str, value: T) -> Result<()> {
		let mut new_value = value.as_str()?;
		if new_value == "" {
			new_value = "\"\""
		}

		self.exec(
			vec![
				"setprop", key, new_value,
			],
			None,
			None,
		)
		.map(|_| ())
	}

	pub fn clear_prop(&self, key: &str) -> Result<()> {
		self.setprop(key, "")
	}

	pub fn getprop_type(&self, key: &str) -> Result<PropType> {
		self.exec(
			vec![
				"getprop", "-T", key,
			],
			None,
			None,
		)
		.map(|s| s.stdout)
		.map(|s| PropType::try_from(s))?
	}

	pub fn am(&self) -> ActivityManager {
		ActivityManager { parent: self }
	}

	pub(crate) fn handle_result(result: Output) -> Result<()> {
		if result.error() && !result.kill() && !result.interrupt() {
			Err(result.into())
		} else {
			Ok(())
		}
	}
}

#[cfg(test)]
mod test {
	use std::time::Duration;

	use simple_cmd::prelude::OutputExt;
	use strum::IntoEnumIterator;

	use crate::types::{DumpsysPriority, KeyCode, PropType, SELinuxType, ScreenRecordOptions, SettingsType};
	use crate::v2::test::test::*;

	#[test]
	fn test_who_am_i() {
		init_log();
		let client = connect_emulator();
		let whoami = client.shell().whoami().expect("failed to get user");
		println!("whoami: {whoami}");
		assert!(!whoami.is_empty());
	}

	#[test]
	fn test_is_root() {
		init_log();
		let client = connect_client(connection_from_tcpip());
		let whoami = client.shell().whoami().expect("failed to get user");
		println!("whoami: {whoami}");

		let result: crate::v2::result::Result<bool> = client.shell().is_root();
		let is_root = result.expect("failed to get root status");
		if whoami.eq_ignore_ascii_case("root") {
			assert!(is_root);
		} else {
			assert!(!is_root);
		}
	}

	#[test]
	fn test_mount() {
		init_log();
		let client = connect_client(connection_from_tcpip());
		client.root().expect("failed to root");
		let _ = client.shell().mount("/system").expect("failed to mount");
		let _ = client.shell().unmount("/system").expect("failed to unmount");
	}

	#[test]
	fn test_check_avbctl() {
		init_log();
		let client = connect_emulator();
		client.shell().check_avbctl().expect_err("check_avbctl should fail");

		let client = connect_tcp_ip_client();
		let _result = client.shell().check_avbctl().expect("failed to check_avbctl");
	}

	#[test]
	fn test_get_command_path() {
		init_log();
		let client = connect_emulator();
		let path = client.shell().get_command_path("sh").expect("failed to get sh path");
		println!("path: {path}");
		assert_eq!("/system/bin/sh", path);

		let client = connect_tcp_ip_client();
		let path = client.shell().get_command_path("sh").expect("failed to get sh path");
		println!("path: {path}");
		assert_eq!("/system/bin/sh", path);
	}

	#[test]
	fn test_which() {
		init_log();
		let client = connect_emulator();
		let path = client.shell().which("sh").expect("failed to get sh path");
		println!("path: {path}");
		assert_eq!("/system/bin/sh", path);

		let client = connect_tcp_ip_client();
		let path = client.shell().which("sh").expect("failed to get sh path");
		println!("path: {path}");
		assert_eq!("/system/bin/sh", path);
	}

	#[test]
	fn test_get_verity() {
		init_log();
		let client = connect_tcp_ip_client();
		let has_avbctl = client.shell().has_avbctl().expect("failed to check for avbctl");
		println!("has_avbctl: {has_avbctl}");
		assert!(has_avbctl);

		let verity = client.shell().get_verity().expect("failed to get verity status");
		println!("verity status: {verity}");
	}

	#[test]
	fn test_toggle_verity() {
		init_log();
		let client = connect_tcp_ip_client();
		client.root().expect("failed to root");
		let enabled = client.shell().get_verity().expect("failed to get verity");
		println!("verity is enabled: {enabled}");

		if enabled {
			client.shell().disable_verity().expect("failed to disable verity");
		} else {
			client.shell().enable_verity().expect("failed to disable verity");
		}

		client.reboot(None).expect("failed to reboot device");
		client
			.wait_for_device(Some(Duration::from_secs(120)))
			.expect("failed to wait for device");

		client.root().expect("failed to root");
		let verity_enabled = client.shell().get_verity().expect("failed to get verity");
		println!("verity is now enabled: {verity_enabled}");

		assert_ne!(enabled, verity_enabled);
	}

	#[test]
	fn test_get_enforce() {
		let client = connect_emulator();
		let enforce: SELinuxType = client.shell().get_enforce().expect("failed to get enforce");
		println!("enforce: {enforce}");

		assert_eq!(SELinuxType::Enforcing, enforce);

		let client = connect_tcp_ip_client();
		let enforce = client.shell().get_enforce().expect("failed to get enforce");
		println!("enforce: {enforce}");
	}

	#[test]
	fn test_set_enforce() {
		init_log();

		let client = connect_tcp_ip_client();
		root_client(&client);
		let enforce = client.shell().get_enforce().expect("failed to get enforce");
		println!("enforce: {enforce}");

		if enforce == SELinuxType::Enforcing {
			client
				.shell()
				.set_enforce(SELinuxType::Permissive)
				.expect("failed to change enforce type");
		} else {
			client
				.shell()
				.set_enforce(SELinuxType::Enforcing)
				.expect("failed to change enforce type");
		}

		let now_enforce = client.shell().get_enforce().expect("failed to get enforce");
		println!("now enforce: {now_enforce}");
		assert_ne!(enforce, now_enforce);
	}

	#[test]
	fn test_is_screen_on() {
		let client = connect_emulator();
		let screen_on = client.shell().is_screen_on().expect("failed to get screen status");
		println!("is screen on: {screen_on}");

		client
			.shell()
			.send_keyevent(KeyCode::KEYCODE_POWER, None, None)
			.expect("failed to send keyevent");

		std::thread::sleep(Duration::from_millis(500));

		let screen_on2 = client.shell().is_screen_on().expect("failed to get screen status");
		println!("is screen on: {screen_on2}");

		assert_ne!(screen_on, screen_on2);
	}

	#[test]
	fn test_get_events() {
		init_log();
		let client = connect_emulator();
		let events = client.shell().get_events().expect("failed to get events");
		assert!(!events.is_empty());
		println!("events: {:#?}", events);
	}

	#[test]
	fn test_file_mode() {
		init_log();
		let client = connect_emulator();
		let mode = client
			.shell()
			.file_mode("/system/build.prop")
			.expect("failed to get file_mode");
		println!("file mode: {}", mode);
	}

	#[test]
	fn test_list_settings() {
		init_log();
		let client = connect_emulator();

		for setting in SettingsType::iter() {
			let settings = client
				.shell()
				.list_settings(setting)
				.expect(&format!("failed to list {:?} settings", setting));
			println!("setting[{setting:?}]: {settings:#?}");
			assert!(!settings.is_empty());
		}
	}

	#[test]
	fn test_get_setting() {
		init_log();
		let client = connect_emulator();
		let setting = client
			.shell()
			.get_setting(SettingsType::global, "theater_mode_on")
			.expect("failed to get setting");
		println!("setting: {setting:?}");
	}

	#[test]
	fn test_put_setting() {
		init_log();
		let client = connect_emulator();
		let setting = client
			.shell()
			.get_setting(SettingsType::global, "theater_mode_on")
			.expect("failed to get setting")
			.unwrap();

		let new_value = if setting == "0" { "1" } else { "0" };
		client
			.shell()
			.put_setting(SettingsType::global, "theater_mode_on", new_value)
			.expect("failed to put setting");

		let setting = client
			.shell()
			.get_setting(SettingsType::global, "theater_mode_on")
			.expect("failed to read settings")
			.unwrap();
		assert_eq!(new_value, setting);
	}

	#[test]
	fn test_delete_setting() {
		init_log();
		let client = connect_emulator();
		client
			.shell()
			.put_setting(SettingsType::global, "my_custom_setting", "1")
			.expect("failed to put settings");

		let value = client
			.shell()
			.get_setting(SettingsType::global, "my_custom_setting")
			.expect("failed to read settings")
			.unwrap();
		assert_eq!("1", value);

		client
			.shell()
			.delete_setting(SettingsType::global, "my_custom_setting")
			.expect("failed to delete settings");
		let value = client
			.shell()
			.get_setting(SettingsType::global, "my_custom_setting")
			.expect("failed to read settings");
		assert_eq!(None, value);
	}

	#[test]
	fn test_ls() {
		init_log();
		let client = connect_emulator();
		let ls = client.shell().ls("/system", None).expect("failed to list dir");
		assert!(!ls.is_empty());
		println!("ls: {:?}", ls);
	}

	#[test]
	fn test_dumpsys_list() {
		init_log();
		let client = connect_emulator();
		let list = client
			.shell()
			.dumpsys_list(false, Some(DumpsysPriority::NORMAL))
			.expect("failed to list dumpsys");
		assert!(!list.is_empty());
		println!("list: {list:?}");
	}

	#[test]
	fn test_dumpsys() {
		init_log();
		let client = connect_emulator();
		let dump = client
			.shell()
			.dumpsys(Some("adb"), None, None, false, false, false, None)
			.expect("failed to dumpsys meminfo");
		println!("dump: {dump:?}");
		assert!(dump.success());
		assert!(!dump.stdout.is_empty());
	}

	#[test]
	fn test_screen_record() {
		init_log();
		let client = connect_emulator();

		let mut options = ScreenRecordOptions::default();
		options.verbose = true;
		options.timelimit = Some(Duration::from_secs(12));

		let remote_file = "/sdcard/Download/screenrecord.mp4";
		let local_file = temp_dir().join("screenrecord.mp4");
		println!("local file: {local_file:?}");

		if local_file.exists() {
			std::fs::remove_file(&local_file).unwrap();
		}

		let receiver = sigint_notifier().unwrap();

		match client.shell().screen_record(Some(options), remote_file, Some(receiver)) {
			Ok(t) => println!("Screen Record Ok: {:?}", t),
			Err(e) => {
				panic!("{:}", e)
			}
		}

		println!("need to sleep a bit..");
		std::thread::sleep(Duration::from_secs(2));

		client.pull(remote_file, local_file.as_path()).unwrap();
		assert!(local_file.exists());

		if local_file.exists() {
			std::fs::remove_file(&local_file).unwrap();
		}
	}

	#[test]
	fn test_screen_mirror() {
		init_log();
		let client = connect_emulator();
		let signal = ctrl_channel().unwrap();
		client
			.shell()
			.screen_mirror(Default::default(), Default::default(), Some(signal))
			.expect("failed to screen mirror");
	}

	#[test]
	fn test_save_screencap() {
		init_log();
		let client = connect_emulator();

		assert!(client.shell().exists("/sdcard/Download").unwrap());

		if client.shell().exists("/sdcard/Download/screencap.png").unwrap() {
			// remove the file
			client.shell().rm("/sdcard/Download/screencap.png", None).unwrap();
		}

		client
			.shell()
			.save_screencap("/sdcard/Download/screencap.png")
			.expect("save screencap failed");

		assert!(client.shell().exists("/sdcard/Download").unwrap());

		let tmp_dir = temp_dir();
		println!("temp_dir: {tmp_dir:?}");

		client
			.pull("/sdcard/Download/screencap.png", tmp_dir)
			.expect("failed to pull file");

		client.shell().rm("/sdcard/Download/screencap.png", None).unwrap();
	}

	#[test]
	fn test_get_prop() {
		init_log();
		let client = connect_emulator();
		let prop = client.shell().getprop("ro.build.product").expect("failed to get prop");
		assert!(!prop.is_empty());
		assert!(prop.starts_with("emulator"));
		println!("prop: {prop}");
	}

	#[test]
	fn test_get_props() {
		init_log();
		let client = connect_emulator();
		let props = client.shell().getprops().expect("failed to list props");
		assert!(!props.is_empty());

		let prop = props.iter().find(|item| item.key == "ro.build.product").unwrap();
		println!("prop = {prop:?}");

		let prop2 = client.shell().getprop("ro.build.product").unwrap();
		assert_eq!(prop.value, prop2);
	}

	#[test]
	fn test_get_props_type() {
		init_log();
		let client = connect_emulator();
		let props = client.shell().getprops_type().expect("failed to get props type");
		assert!(!props.is_empty());

		let prop_type = props.get("ro.build.product").unwrap();
		println!("prop type: {:?}", prop_type);

		assert_eq!(&PropType::String, prop_type);
	}

	#[test]
	fn test_set_prop() {
		init_log();
		let client = connect_emulator();
		let prop = client.shell().getprop("log.tag.stats_log").expect("failed to getprop");
		let new_prop = if prop == "I" { "D" } else { "I" };

		client
			.shell()
			.setprop("log.tag.stats_log", new_prop)
			.expect("failed to set prop");

		let prop = client.shell().getprop("log.tag.stats_log").expect("failed to getprop");
		assert_eq!(new_prop, prop);
	}

	#[test]
	fn test_clean_prop() {
		init_log();
		let client = connect_emulator();

		client.shell().clear_prop("log.tag.stats_log").expect("failed to clear prop");
		let prop = client.shell().getprop("log.tag.stats_log").expect("failed to getprop");
		assert_eq!("", prop);

		client.shell().setprop("log.tag.stats_log", "I").unwrap();
	}

	#[test]
	fn test_get_prop_type() {
		init_log();
		let client = connect_emulator();
		let prop = client
			.shell()
			.getprop_type("log.tag.stats_log")
			.expect("failed to get prop type");
		assert_eq!(PropType::String, prop);
	}

	#[test]
	fn test_send_swipe() {
		init_log();
		let client = connect_emulator();
		client
			.shell()
			.send_swipe((0, 0), (500, 500), None, None)
			.expect("failed to send swipe");
		client
			.shell()
			.try_send_swipe((0, 0), (500, 500), None, None)
			.expect("failed to send swipe");
	}

	#[test]
	fn test_send_keyevent() {
		init_log();
		let client = connect_emulator();

		client
			.shell()
			.send_keyevent(KeyCode::KEYCODE_SETTINGS, None, None)
			.expect("failed to send keyevent");

		println!("sleeping...");

		std::thread::sleep(Duration::from_secs(5));

		client
			.shell()
			.try_send_keyevent(KeyCode::KEYCODE_HOME, None, None)
			.expect("failed to send keyevent");
	}

	#[test]
	fn test_send_keycode() {
		init_log();
		let client = connect_emulator();

		client.shell().send_keycode(26, None, None).expect("failed to send keyevent");

		println!("sleeping...");

		std::thread::sleep(Duration::from_secs(5));

		client
			.shell()
			.try_send_keycode(26, None, None)
			.expect("failed to send keyevent");
	}
}
