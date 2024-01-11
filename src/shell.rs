use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, ErrorKind};
use std::process::{Command, ExitStatus, Output};
use std::sync::Mutex;
use std::time::Duration;

use cached::{Cached, SizedCache};
use crossbeam::channel::Receiver;
use lazy_static::lazy_static;
use regex::Regex;
use rustix::path::Arg;
use simple_cmd::debug::CommandDebug;
use simple_cmd::CommandBuilder;

use crate::cmd_ext::CommandBuilderExt;
use crate::errors::AdbError;
use crate::errors::AdbError::Unknown;
use crate::traits::AdbDevice;
use crate::types::{DumpsysPriority, FFPlayOptions, Intent, PropType, Property, ScreenRecordOptions, SettingsType};
use crate::types::{InputSource, KeyCode, KeyEventType, MotionEvent, SELinuxType};
use crate::{Adb, AdbShell, PackageManager, Shell};

lazy_static! {
	static ref RE_GET_PROPS: Regex = Regex::new("(?m)^\\[(.*)\\]\\s*:\\s*\\[([^\\]]*)\\]$").unwrap();
	static ref COMMANDS_CACHE: Mutex<SizedCache<String, Option<String>>> = Mutex::new(SizedCache::with_size(10));
}

impl Shell {
	pub fn exec<'a, D, T>(
		adb: &Adb,
		device: D,
		args: Vec<T>,
		cancel: Option<Receiver<()>>,
		timeout: Option<Duration>,
	) -> crate::Result<Output>
	where
		T: Into<String> + AsRef<OsStr>,
		D: Into<&'a dyn AdbDevice>,
	{
		let builder = CommandBuilder::shell(adb, device).args(args).signal(cancel).timeout(timeout);
		Ok(builder.build().output()?)
	}

	pub fn try_exec<'a, D, T>(
		adb: &Adb,
		device: D,
		args: Vec<T>,
		cancel: Option<Receiver<()>>,
		timeout: Option<Duration>,
	) -> crate::Result<Option<ExitStatus>>
	where
		T: Into<String> + AsRef<OsStr>,
		D: Into<&'a dyn AdbDevice>,
	{
		let builder = CommandBuilder::shell(adb, device).args(args).signal(cancel).timeout(timeout);
		Ok(builder.build().run()?)
	}

	pub fn list_settings<'a, D>(adb: &Adb, device: D, settings_type: SettingsType) -> crate::Result<Vec<Property>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let output = Shell::exec(
			adb,
			device,
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

	pub fn get_setting<'a, D>(adb: &Adb, device: D, settings_type: SettingsType, key: &str) -> crate::Result<Option<String>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		simple_cmd::Vec8ToString::as_str(
			&Shell::exec(
				adb,
				device,
				vec![
					"settings",
					"get",
					settings_type.into(),
					key,
				],
				None,
				None,
			)?
			.stdout,
		)
		.map(|s| Some(s.trim_end().to_string()))
		.ok_or(Unknown("unexpected error".to_string()))
	}

	pub fn put_setting<'a, D>(adb: &Adb, device: D, settings_type: SettingsType, key: &str, value: &str) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		Shell::exec(
			adb,
			device,
			vec![
				"settings",
				"put",
				settings_type.into(),
				key,
				value,
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn delete_setting<'a, D>(adb: &Adb, device: D, settings_type: SettingsType, key: &str) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		Shell::exec(
			adb,
			device,
			vec![
				"settings",
				"delete",
				settings_type.into(),
				key,
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn ls<'d, 't, D, T>(adb: &Adb, device: D, path: T, options: Option<&str>) -> crate::Result<Vec<String>>
	where
		D: Into<&'d dyn AdbDevice>,
		T: Into<&'t str> + AsRef<OsStr> + Arg,
	{
		let mut args = vec!["ls"];
		if let Some(options) = options {
			args.push(options);
		}
		args.push(path.into());

		let stdout = Shell::exec(adb, device, args, None, None)?.stdout;
		let lines = stdout.lines().filter_map(|s| s.ok()).collect();
		Ok(lines)
	}

	pub fn dumpsys_list<'a, D>(
		adb: &Adb,
		device: D,
		proto_only: bool,
		priority: Option<DumpsysPriority>,
	) -> crate::Result<Vec<String>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
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

		let output: Vec<String> = Shell::exec(adb, device, args, None, None)?
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
	pub fn dumpsys<'d, D>(
		adb: &Adb,
		device: D,
		service: Option<&str>,
		arguments: Option<Vec<String>>,
		timeout: Option<Duration>,
		pid: bool,
		thread: bool,
		proto: bool,
		skip: Option<Vec<String>>,
	) -> crate::Result<Output>
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

		Shell::exec(adb, device, args, None, None)
	}

	pub fn screen_record<'d, D>(
		adb: &Adb,
		device: D,
		options: Option<ScreenRecordOptions>,
		output: &str,
		cancel: Option<Receiver<()>>,
	) -> crate::Result<Output>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut args = vec![String::from("screenrecord")];

		if let Some(options) = options {
			args.extend(options);
		}

		args.push(output.to_string());
		let command = CommandBuilder::shell(adb, device).args(args).signal(cancel);
		Ok(command.build().output()?)
	}

	pub fn screen_mirror<'d, D>(
		adb: &Adb,
		device: D,
		options: Option<ScreenRecordOptions>,
		play_options: Option<FFPlayOptions>,
		cancel: Option<Receiver<()>>,
	) -> crate::Result<Output>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut screenrecord_options = options.unwrap_or(ScreenRecordOptions::default());
		screenrecord_options.verbose = false;
		screenrecord_options.bug_report = None;
		//let screenrecord_arg = format!("while true; do screenrecord --output-format=h264 {:} -; done", screenrecord_options);
		let screenrecord_arg = format!("screenrecord --output-format=h264 {:} -", screenrecord_options);

		let builder = CommandBuilder::shell(adb, device)
			.args(vec![screenrecord_arg.as_str()])
			.signal(cancel)
			.with_debug(true);

		let command1 = builder.build();

		let mut command2 = Command::new(which::which("ffplay")?);

		let ffplay_options = play_options.unwrap_or(FFPlayOptions::default());

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

		//command2.args(vec!["-loglevel", "verbose",
		//                   "-an",
		//                   "-autoexit",
		//                   "-framerate", "30",
		//                   "-probesize", "300",
		//                   "-vf", "scale=1024:-1",
		//                   "-sync", "video",
		//                   "-",
		//]);

		//command2.stdout(Stdio::piped());
		//command2.stderr(Stdio::piped());
		command2.debug();
		command1.pipe(command2).map_err(|e| AdbError::CmdError(e))

		/*
		let mut args = vec![];

		let screenrecord_arg = if let Some(options) = options {
			format!("while true; do screenrecord {} -; done", options)
		} else {
			format!("while true; do screenrecord {} -; done", ScreenRecordOptions::default())
		};

		args.push(screenrecord_arg);

		let command1 = CommandBuilder::shell(adb, device).args(args).signal(cancel).with_debug(true);
		let mut command2 = Command::new("ffplay");

		if let Some(play_options) = play_options {
			command2.args(play_options.into_iter());
		} else {
			command2.args(FFPlayOptions::default().into_iter());
		}

		command2.args(&["-loglevel", "repeat+level+verbose", "-an", "-autoexit", "-sync", "video", "-", ]);
		command2.stdout(Stdio::piped());
		command2.debug();
		command1.build().pipe(command2).map_err(|e| AdbError::CmdError(e))
		*/
	}

	pub fn save_screencap<'d, 't, D, T>(adb: &Adb, device: D, path: T) -> crate::Result<Output>
	where
		D: Into<&'d dyn AdbDevice>,
		T: Into<&'t str> + AsRef<OsStr> + Arg,
	{
		Shell::exec(
			adb,
			device,
			vec![
				"screencap",
				"-p",
				path.into(),
			],
			None,
			None,
		)
	}

	pub fn is_screen_on<'a, D>(adb: &Adb, device: D) -> crate::Result<bool>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let process_result = Shell::exec(
			adb,
			device,
			vec!["dumpsys input_method | egrep 'mInteractive=(true|false)'"],
			None,
			None,
		)?;
		let result = simple_cmd::Vec8ToString::as_str(&process_result.stdout)
			.map(|f| f.contains("mInteractive=true"))
			.ok_or(AdbError::ParseInputError())?;
		Ok(result)
	}

	pub fn send_swipe<'a, D>(
		adb: &Adb,
		device: D,
		from_pos: (i32, i32),
		to_pos: (i32, i32),
		duration: Option<Duration>,
		source: Option<InputSource>,
	) -> crate::Result<()>
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

		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_tap<'a, D>(adb: &Adb, device: D, position: (i32, i32), source: Option<InputSource>) -> crate::Result<()>
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

		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_char<'a, D>(adb: &Adb, device: D, text: &char, source: Option<InputSource>) -> crate::Result<()>
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
		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn try_send_char<'a, D>(adb: &Adb, device: D, text: &char, source: Option<InputSource>) -> crate::Result<()>
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
		Shell::try_exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_text<'a, D>(adb: &Adb, device: D, text: &str, source: Option<InputSource>) -> crate::Result<()>
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

		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn try_send_text<'a, D>(adb: &Adb, device: D, text: &str, source: Option<InputSource>) -> crate::Result<()>
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

		Shell::try_exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_event<'a, D>(adb: &Adb, device: D, event: &str, code_type: i32, code: i32, value: i32) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		Shell::exec(
			adb,
			device,
			vec![
				"sendevent",
				event,
				format!("{}", code_type).as_str(),
				format!("{}", code).as_str(),
				format!("{}", value).as_str(),
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn try_send_event<'a, D>(adb: &Adb, device: D, event: &str, code_type: i32, code: i32, value: i32) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		Shell::try_exec(
			adb,
			device,
			vec![
				"sendevent",
				event,
				format!("{}", code_type).as_str(),
				format!("{}", code).as_str(),
				format!("{}", value).as_str(),
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn send_motion<'a, D>(
		adb: &Adb,
		device: D,
		source: Option<InputSource>,
		motion: MotionEvent,
		pos: (i32, i32),
	) -> crate::Result<()>
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
		Shell::exec(adb, device, args, None, None).map(|_| ())
	}

	pub fn send_draganddrop<'a, D>(
		adb: &Adb,
		device: D,
		source: Option<InputSource>,
		duration: Option<Duration>,
		from_pos: (i32, i32),
		to_pos: (i32, i32),
	) -> crate::Result<()>
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

		Shell::exec(adb, device, args, None, None).map(|_| ())
	}

	pub fn send_press<'a, D>(adb: &Adb, device: D, source: Option<InputSource>) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];
		if let Some(source) = source {
			args.push(source.into());
		}
		args.push("press");
		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_keyevent<'a, D>(
		adb: &Adb,
		device: D,
		keycode: KeyCode,
		event_type: Option<KeyEventType>,
		source: Option<InputSource>,
	) -> crate::Result<()>
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
		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_keycode<'a, D>(
		adb: &Adb,
		device: D,
		keycode: u32,
		event_type: Option<KeyEventType>,
		source: Option<InputSource>,
	) -> crate::Result<()>
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

		let keycode_str = keycode.to_string();

		args.push(keycode_str.as_str());
		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn try_send_keyevent<'a, D>(
		adb: &Adb,
		device: D,
		keycode: KeyCode,
		event_type: Option<KeyEventType>,
		source: Option<InputSource>,
	) -> crate::Result<()>
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
		Shell::try_exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn try_send_keycode<'a, D>(
		adb: &Adb,
		device: D,
		keycode: u32,
		event_type: Option<KeyEventType>,
		source: Option<InputSource>,
	) -> crate::Result<()>
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

		let keycode_str = keycode.to_string();
		args.push(keycode_str.as_str());
		Shell::try_exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_keyevents<'a, D>(adb: &Adb, device: D, keycodes: Vec<KeyCode>, source: Option<InputSource>) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keyevent");
		args.extend(keycodes.iter().map(|k| k.into()).collect::<Vec<&str>>());

		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_keycodes<'a, D>(adb: &Adb, device: D, keycodes: Vec<u32>, source: Option<InputSource>) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keyevent");
		let keycodes_string = keycodes.iter().map(|k| k.to_string()).collect::<Vec<_>>();
		args.extend(keycodes_string.iter().map(|k| k.as_str()).collect::<Vec<&str>>());

		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn try_send_keyevents<'a, D>(adb: &Adb, device: D, keycodes: Vec<KeyCode>, source: Option<InputSource>) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keyevent");
		args.extend(keycodes.iter().map(|k| k.into()).collect::<Vec<&str>>());

		Shell::try_exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn try_send_keycodes<'a, D>(adb: &Adb, device: D, keycodes: Vec<u32>, source: Option<InputSource>) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keyevent");
		let keycodes_string = keycodes.iter().map(|k| k.to_string()).collect::<Vec<_>>();
		args.extend(keycodes_string.iter().map(|k| k.as_str()).collect::<Vec<&str>>());

		Shell::try_exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_keyevent_combination<'a, D>(
		adb: &Adb,
		device: D,
		source: Option<InputSource>,
		keycodes: Vec<KeyCode>,
	) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keycombination");
		args.extend(keycodes);
		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn send_keycode_combination<'a, D>(
		adb: &Adb,
		device: D,
		source: Option<InputSource>,
		keycodes: Vec<u32>,
	) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keycombination");
		let keycodes_string = keycodes.iter().map(|k| k.to_string()).collect::<Vec<_>>();
		args.extend(keycodes_string.iter().map(|k| k.as_str()).collect::<Vec<&str>>());
		Shell::exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn try_send_keyevent_combination<'a, D>(
		adb: &Adb,
		device: D,
		source: Option<InputSource>,
		keycodes: Vec<KeyCode>,
	) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keycombination");
		args.extend(keycodes);
		Shell::try_exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn try_send_keycode_combination<'a, D>(
		adb: &Adb,
		device: D,
		source: Option<InputSource>,
		keycodes: Vec<u32>,
	) -> crate::Result<()>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let mut args = vec!["input"];

		if let Some(source) = source {
			args.push(source.into());
		}

		args.push("keycombination");
		let keycodes_string = keycodes.iter().map(|k| k.to_string()).collect::<Vec<_>>();
		args.extend(keycodes_string.iter().map(|k| k.as_str()).collect::<Vec<&str>>());
		Shell::try_exec(adb, device, args, None, None)?;
		Ok(())
	}

	pub fn get_events<'a, D>(adb: &Adb, device: D) -> crate::Result<Vec<(String, String)>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let result = Shell::exec(
			adb,
			device,
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
						e.ok_or(AdbError::ParseInputError())?.as_str().to_string(),
						n.ok_or(AdbError::ParseInputError())?.as_str().to_string(),
					));
				}

				string = &string[cap[0].len()..]
			} else {
				break;
			}
		}
		Ok(v)
	}

	pub fn file_mode<'d, D, T>(adb: &Adb, device: D, path: T) -> crate::Result<file_mode::Mode>
	where
		D: Into<&'d dyn AdbDevice>,
		T: Arg,
	{
		let output = Arg::as_str(
			&Shell::exec(
				adb,
				device,
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

	fn test_file<'d, D, T: Arg>(adb: &Adb, device: D, path: T, mode: &str) -> crate::Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Shell::exec(
			adb,
			device,
			vec![format!("test -{:} {:?} && echo 1 || echo 0", mode, path.as_str()?).as_str()],
			None,
			None,
		);

		match simple_cmd::Vec8ToString::as_str(&output?.stdout) {
			Some(s) => Ok(s.trim_end() == "1"),
			None => Ok(false),
		}
	}

	pub fn get_command_path<'d, D, T: Arg>(adb: &Adb, device: D, command: T) -> Option<String>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		if let Ok(command_string) = command.as_str() {
			let cloned_device = device.into();

			let mut binding = COMMANDS_CACHE.lock().unwrap();
			let cache_key = format!("{}{}", cloned_device.addr(), command_string);

			binding
				.cache_get_or_set_with(cache_key.clone(), || {
					Shell::exec(
						adb,
						cloned_device,
						vec![format!("command -v {}", command_string).as_str()],
						None,
						None,
					)
					.and_then(|result| {
						Arg::as_str(&result.stdout)
							.map(|s| s.trim().to_string())
							.map_err(|e| AdbError::Errno(e))
					})
					.and_then(|result| {
						if result.is_empty() {
							Err(AdbError::IoError(std::io::Error::from(ErrorKind::NotFound)))
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

	pub fn exists<'d, D, T: Arg>(adb: &Adb, device: D, path: T) -> crate::Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::test_file(adb, device, path, "e")
	}

	pub fn rm<'d, 't, D, T>(adb: &Adb, device: D, path: T, options: Option<Vec<&str>>) -> crate::Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
		T: Arg,
	{
		let mut args = vec!["rm"];
		if let Some(options) = options {
			args.extend(options);
		}
		args.push(path.as_str()?);

		Shell::exec(adb, device, args, None, None).map(|_| true)
	}

	pub fn is_file<'d, D, T: Arg>(adb: &Adb, device: D, path: T) -> crate::Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::test_file(adb, device, path, "f")
	}

	pub fn is_dir<'d, D, T: Arg>(adb: &Adb, device: D, path: T) -> crate::Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::test_file(adb, device, path, "d")
	}

	pub fn is_symlink<'d, D, T: Arg>(adb: &Adb, device: D, path: T) -> crate::Result<bool>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::test_file(adb, device, path, "h")
	}

	pub fn getprop<'d, D>(adb: &Adb, device: D, key: &str) -> crate::Result<String>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let result = Shell::exec(
			adb,
			device,
			vec![
				"getprop", key,
			],
			None,
			None,
		)
		.map(|s| s.stdout)?;
		Ok(Arg::as_str(&result).map(|f| f.trim_end())?.to_string())
	}

	pub fn setprop<'d, D, T: Arg>(adb: &Adb, device: D, key: &str, value: T) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let mut new_value = value.as_str()?;
		if new_value == "" {
			new_value = "\"\""
		}

		Shell::exec(
			adb,
			device,
			vec![
				"setprop", key, new_value,
			],
			None,
			None,
		)
		.map(|_| ())
	}

	pub fn clear_prop<'d, D, T: Arg>(adb: &Adb, device: D, key: &str) -> crate::Result<()>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::setprop(adb, device, key, "")
	}

	pub fn getprop_type<'d, D>(adb: &Adb, device: D, key: &str) -> crate::Result<Vec<u8>>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::exec(
			adb,
			device,
			vec![
				"getprop", "-T", key,
			],
			None,
			None,
		)
		.map(|s| s.stdout)
	}

	pub fn getprop_types<'d, D>(adb: &Adb, device: D) -> crate::Result<HashMap<String, PropType>>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		let output = Shell::exec(
			adb,
			device,
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

	pub fn getprops<'a, D>(adb: &Adb, device: D) -> crate::Result<Vec<Property>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let output = Shell::exec(adb, device, vec!["getprop"], None, None);

		let mut result: Vec<Property> = Vec::new();

		for line in output?.stdout.lines().filter_map(|l| l.ok()) {
			let captures = RE_GET_PROPS.captures(line.as_str());
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

	pub fn cat<'d, D, P: Arg>(adb: &Adb, device: D, path: P) -> crate::Result<Vec<u8>>
	where
		D: Into<&'d dyn AdbDevice>,
	{
		Shell::exec(
			adb,
			device,
			vec![
				"cat",
				path.as_str()?,
			],
			None,
			None,
		)
		.map(|s| s.stdout)
	}

	pub fn which<'a, D>(adb: &Adb, device: D, command: &str) -> crate::Result<Option<String>>
	where
		D: Into<&'a dyn AdbDevice>,
	{
		let output = Shell::exec(
			adb,
			device,
			vec![
				"which", command,
			],
			None,
			None,
		);
		output.map(|s| simple_cmd::Vec8ToString::as_str(&s.stdout).map(|ss| String::from(ss.trim_end())))
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
	/// use radb_client::AdbClient;
	///
	/// async fn get_user() {
	///     let client: AdbClient = "192.168.1.24:5555".parse::<Device>().unwrap().try_into().unwrap();
	///     client.connect(None).await.unwrap();
	///     let output = client.shell().whoami().unwrap();
	/// }
	/// ```
	pub fn whoami<'a, T>(adb: &Adb, device: T) -> crate::Result<Option<String>>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let result = Shell::exec(adb, device, vec!["whoami"], None, None)?;
		Ok(simple_cmd::Vec8ToString::as_str(&result.stdout).map(|s| s.trim().to_string()))
	}

	pub fn is_root<'a, T>(adb: &Adb, device: T) -> crate::Result<bool>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let whoami = Shell::whoami(adb, device)?;
		match whoami {
			Some(s) => Ok(s == "root"),
			None => Ok(false),
		}
	}

	pub fn broadcast<'a, T>(adb: &Adb, device: T, intent: &Intent) -> crate::Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let _result = Shell::exec(
			adb,
			device,
			vec![
				"am",
				"broadcast",
				format!("{:}", intent).as_str(),
			],
			None,
			Some(Duration::from_secs(1)),
		)?;
		Ok(())
	}

	pub fn start<'a, T>(adb: &Adb, device: T, intent: &Intent) -> crate::Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let _result = Shell::exec(
			adb,
			device,
			vec![
				"am",
				"start",
				format!("{:}", intent).as_str(),
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn start_service<'a, T>(adb: &Adb, device: T, intent: &Intent) -> crate::Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let _result = Shell::exec(
			adb,
			device,
			vec![
				"am",
				"startservice",
				format!("{:}", intent).as_str(),
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn force_stop<'a, T>(adb: &Adb, device: T, package_name: &str) -> crate::Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let _result = Shell::exec(
			adb,
			device,
			vec![
				"am",
				"force-stop",
				package_name,
			],
			None,
			None,
		)?;
		Ok(())
	}

	pub fn get_enforce<'a, T>(adb: &Adb, device: T) -> crate::Result<SELinuxType>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let result = Shell::exec(adb, device, vec!["getenforce"], None, None)?.stdout;
		let enforce: SELinuxType = SELinuxType::try_from(result)?;
		Ok(enforce)
	}

	pub fn set_enforce<'a, T>(adb: &Adb, device: T, enforce: SELinuxType) -> crate::Result<()>
	where
		T: Into<&'a dyn AdbDevice>,
	{
		let new_value = match enforce {
			SELinuxType::Permissive => "0",
			SELinuxType::Enforcing => "1",
		};

		Shell::exec(
			adb,
			device,
			vec![
				"setenforce",
				new_value,
			],
			None,
			None,
		)
		.map(|_| ())
	}
}

impl<'a> AdbShell<'a> {
	pub fn to_command(&self) -> std::process::Command {
		CommandBuilder::shell(&self.parent.adb, &self.parent.device).into()
	}

	pub fn pm(&self) -> PackageManager {
		PackageManager { parent: self.clone() }
	}

	pub fn whoami(&self) -> crate::Result<Option<String>> {
		Shell::whoami(&self.parent.adb, &self.parent.device)
	}

	pub fn which(&self, command: &str) -> crate::Result<Option<String>> {
		Shell::which(&self.parent.adb, &self.parent.device, command)
	}

	pub fn getprop(&self, key: &str) -> crate::Result<String> {
		let value = Shell::getprop(&self.parent.adb, &self.parent.device, key)?;
		Arg::as_str(&value).map(|f| f.to_string()).map_err(|e| AdbError::Errno(e))
	}

	pub fn setprop<T: Arg>(&self, key: &str, value: T) -> crate::Result<()> {
		Shell::setprop(&self.parent.adb, &self.parent.device, key, value)
	}

	pub fn getprop_type(&self, key: &str) -> crate::Result<String> {
		let result = Shell::getprop_type(&self.parent.adb, &self.parent.device, key)?;
		Ok(Arg::as_str(&result)?.to_string())
	}

	pub fn cat<T: Arg>(&self, path: T) -> crate::Result<Vec<u8>> {
		Shell::cat(&self.parent.adb, &self.parent.device, path)
	}

	pub fn getprops(&self) -> crate::Result<Vec<Property>> {
		Shell::getprops(&self.parent.adb, &self.parent.device)
	}

	pub fn getprop_types(&self) -> crate::Result<HashMap<String, PropType>> {
		Shell::getprop_types(&self.parent.adb, &self.parent.device)
	}

	pub fn exists<T: Arg>(&self, path: T) -> crate::Result<bool> {
		Shell::exists(&self.parent.adb, &self.parent.device, path)
	}

	pub fn get_command_path<T: Arg>(&self, command: T) -> crate::Result<String> {
		Shell::get_command_path(&self.parent.adb, &self.parent.device, command)
			.ok_or(AdbError::IoError(std::io::Error::from(ErrorKind::NotFound)))
	}

	pub fn has_command<T: Arg>(&self, command: T) -> crate::Result<bool> {
		self.get_command_path(command).map(|_| true)
	}

	pub fn rm<'s, S: Arg>(&self, path: S, options: Option<Vec<&str>>) -> crate::Result<bool> {
		Shell::rm(&self.parent.adb, &self.parent.device, path, options)
	}

	pub fn is_file<T: Arg>(&self, path: T) -> crate::Result<bool> {
		Shell::is_file(&self.parent.adb, &self.parent.device, path)
	}

	pub fn is_dir<T: Arg>(&self, path: T) -> crate::Result<bool> {
		Shell::is_dir(&self.parent.adb, &self.parent.device, path)
	}

	pub fn is_symlink<T: Arg>(&self, path: T) -> crate::Result<bool> {
		Shell::is_symlink(&self.parent.adb, &self.parent.device, path)
	}

	///
	/// List directory
	pub fn ls<'t, T>(&self, path: T, options: Option<&str>) -> crate::Result<Vec<String>>
	where
		T: Into<&'t str> + AsRef<OsStr> + Arg,
	{
		Shell::ls(&self.parent.adb, &self.parent.device, path, options)
	}

	pub fn save_screencap<'t, T: Into<&'t str> + AsRef<OsStr> + Arg>(&self, path: T) -> crate::Result<Output> {
		Shell::save_screencap(&self.parent.adb, &self.parent.device, path)
	}

	///
	/// Root is required
	///
	pub fn list_settings(&self, settings_type: SettingsType) -> crate::Result<Vec<Property>> {
		Shell::list_settings(&self.parent.adb, &self.parent.device, settings_type)
	}

	///
	/// Root is required
	pub fn get_setting(&self, settings_type: SettingsType, key: &str) -> crate::Result<Option<String>> {
		Shell::get_setting(&self.parent.adb, &self.parent.device, settings_type, key)
	}

	pub fn put_setting(&self, settings_type: SettingsType, key: &str, value: &str) -> crate::Result<()> {
		Shell::put_setting(&self.parent.adb, &self.parent.device, settings_type, key, value)
	}

	pub fn delete_setting(&self, settings_type: SettingsType, key: &str) -> crate::Result<()> {
		Shell::delete_setting(&self.parent.adb, &self.parent.device, settings_type, key)
	}

	pub fn dumpsys_list(&self, proto_only: bool, priority: Option<DumpsysPriority>) -> crate::Result<Vec<String>> {
		Shell::dumpsys_list(&self.parent.adb, &self.parent.device, proto_only, priority)
	}

	pub fn dumpsys(
		&self,
		service: Option<&str>,
		arguments: Option<Vec<String>>,
		timeout: Option<Duration>,
		pid: bool,
		thread: bool,
		proto: bool,
		skip: Option<Vec<String>>,
	) -> crate::Result<Output> {
		Shell::dumpsys(
			&self.parent.adb,
			&self.parent.device,
			service,
			arguments,
			timeout,
			pid,
			thread,
			proto,
			skip,
		)
	}

	pub fn is_screen_on(&self) -> crate::Result<bool> {
		Shell::is_screen_on(&self.parent.adb, &self.parent.device)
	}

	pub fn screen_record(
		&self,
		options: Option<ScreenRecordOptions>,
		output: &str,
		signal: Option<Receiver<()>>,
	) -> crate::Result<Output> {
		Shell::screen_record(&self.parent.adb, &self.parent.device, options, output, signal)
	}

	pub fn screen_mirror(
		&self,
		options: Option<ScreenRecordOptions>,
		play_options: Option<FFPlayOptions>,
		cancel: Option<Receiver<()>>,
	) -> crate::Result<Output> {
		Shell::screen_mirror(&self.parent.adb, &self.parent.device, options, play_options, cancel)
	}

	pub fn get_events(&self) -> crate::Result<Vec<(String, String)>> {
		Shell::get_events(&self.parent.adb, &self.parent.device)
	}

	///
	/// Root may be required
	pub fn send_event(&self, event: &str, code_type: i32, code: i32, value: i32) -> crate::Result<()> {
		Shell::send_event(&self.parent.adb, &self.parent.device, event, code_type, code, value)
	}

	pub fn try_send_event(&self, event: &str, code_type: i32, code: i32, value: i32) -> crate::Result<()> {
		Shell::send_event(&self.parent.adb, &self.parent.device, event, code_type, code, value)
	}

	pub fn send_motion(&self, source: Option<InputSource>, motion: MotionEvent, pos: (i32, i32)) -> crate::Result<()> {
		Shell::send_motion(&self.parent.adb, &self.parent.device, source, motion, pos)
	}

	pub fn send_draganddrop(
		&self,
		source: Option<InputSource>,
		duration: Option<Duration>,
		from_pos: (i32, i32),
		to_pos: (i32, i32),
	) -> crate::Result<()> {
		Shell::send_draganddrop(&self.parent.adb, &self.parent.device, source, duration, from_pos, to_pos)
	}

	pub fn send_press(&self, source: Option<InputSource>) -> crate::Result<()> {
		Shell::send_press(&self.parent.adb, &self.parent.device, source)
	}

	pub fn send_keyevent_combination(&self, source: Option<InputSource>, keycodes: Vec<KeyCode>) -> crate::Result<()> {
		Shell::send_keyevent_combination(&self.parent.adb, &self.parent.device, source, keycodes)
	}

	pub fn send_keycode_combination(&self, source: Option<InputSource>, keycodes: Vec<u32>) -> crate::Result<()> {
		Shell::send_keycode_combination(&self.parent.adb, &self.parent.device, source, keycodes)
	}

	pub fn try_send_keyevent_combination(&self, source: Option<InputSource>, keycodes: Vec<KeyCode>) -> crate::Result<()> {
		Shell::try_send_keyevent_combination(&self.parent.adb, &self.parent.device, source, keycodes)
	}

	pub fn try_send_keycode_combination(&self, source: Option<InputSource>, keycodes: Vec<u32>) -> crate::Result<()> {
		Shell::try_send_keycode_combination(&self.parent.adb, &self.parent.device, source, keycodes)
	}

	pub fn send_char(&self, text: &char, source: Option<InputSource>) -> crate::Result<()> {
		Shell::send_char(&self.parent.adb, &self.parent.device, text, source)
	}

	pub fn try_send_char(&self, text: &char, source: Option<InputSource>) -> crate::Result<()> {
		Shell::try_send_char(&self.parent.adb, &self.parent.device, text, source)
	}

	pub fn send_text(&self, text: &str, source: Option<InputSource>) -> crate::Result<()> {
		Shell::send_text(&self.parent.adb, &self.parent.device, text, source)
	}

	pub fn try_send_text(&self, text: &str, source: Option<InputSource>) -> crate::Result<()> {
		Shell::try_send_text(&self.parent.adb, &self.parent.device, text, source)
	}

	pub fn exec<T>(&self, args: Vec<T>, cancel: Option<Receiver<()>>, timeout: Option<Duration>) -> crate::Result<Output>
	where
		T: Into<String> + AsRef<OsStr>,
	{
		Shell::exec(&self.parent.adb, &self.parent.device, args, cancel, timeout)
	}

	pub fn try_exec<T>(
		&self,
		args: Vec<T>,
		cancel: Option<Receiver<()>>,
		timeout: Option<Duration>,
	) -> crate::Result<Option<ExitStatus>>
	where
		T: Into<String> + AsRef<OsStr>,
	{
		Shell::try_exec(&self.parent.adb, &self.parent.device, args, cancel, timeout)
	}

	pub fn broadcast(&self, intent: &Intent) -> crate::Result<()> {
		Shell::broadcast(&self.parent.adb, &self.parent.device, intent)
	}

	pub fn start(&self, intent: &Intent) -> crate::Result<()> {
		Shell::start(&self.parent.adb, &self.parent.device, intent)
	}

	pub fn start_service(&self, intent: &Intent) -> crate::Result<()> {
		Shell::start_service(&self.parent.adb, &self.parent.device, intent)
	}

	pub fn force_stop(&self, package_name: &str) -> crate::Result<()> {
		Shell::force_stop(&self.parent.adb, &self.parent.device, package_name)
	}

	pub fn get_enforce(&self) -> crate::Result<SELinuxType> {
		Shell::get_enforce(&self.parent.adb, &self.parent.device)
	}

	pub fn set_enforce(&self, enforce: SELinuxType) -> crate::Result<()> {
		Shell::set_enforce(&self.parent.adb, &self.parent.device, enforce)
	}

	pub fn send_keyevent(
		&self,
		keycode: KeyCode,
		event_type: Option<KeyEventType>,
		source: Option<InputSource>,
	) -> crate::Result<()> {
		Shell::send_keyevent(&self.parent.adb, &self.parent.device, keycode, event_type, source)
	}

	pub fn send_keycode(&self, keycode: u32, event_type: Option<KeyEventType>, source: Option<InputSource>) -> crate::Result<()> {
		Shell::send_keycode(&self.parent.adb, &self.parent.device, keycode, event_type, source)
	}

	pub fn try_send_keyevent(
		&self,
		keycode: KeyCode,
		event_type: Option<KeyEventType>,
		source: Option<InputSource>,
	) -> crate::Result<()> {
		Shell::try_send_keyevent(&self.parent.adb, &self.parent.device, keycode, event_type, source)
	}

	pub fn try_send_keycode(
		&self,
		keycode: u32,
		event_type: Option<KeyEventType>,
		source: Option<InputSource>,
	) -> crate::Result<()> {
		Shell::try_send_keycode(&self.parent.adb, &self.parent.device, keycode, event_type, source)
	}

	pub fn send_keyevents(&self, keycodes: Vec<KeyCode>, source: Option<InputSource>) -> crate::Result<()> {
		Shell::send_keyevents(&self.parent.adb, &self.parent.device, keycodes, source)
	}

	pub fn send_keycodes(&self, keycodes: Vec<u32>, source: Option<InputSource>) -> crate::Result<()> {
		Shell::send_keycodes(&self.parent.adb, &self.parent.device, keycodes, source)
	}

	pub fn try_send_keyevents(&self, keycodes: Vec<KeyCode>, source: Option<InputSource>) -> crate::Result<()> {
		Shell::try_send_keyevents(&self.parent.adb, &self.parent.device, keycodes, source)
	}

	pub fn try_send_keycodes(&self, keycodes: Vec<u32>, source: Option<InputSource>) -> crate::Result<()> {
		Shell::try_send_keycodes(&self.parent.adb, &self.parent.device, keycodes, source)
	}

	pub fn file_mode<T: Arg>(&self, path: T) -> crate::Result<file_mode::Mode> {
		Shell::file_mode(&self.parent.adb, &self.parent.device, path)
	}
}
