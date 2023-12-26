/// cargo test --color=always --bin randroid tests -- --test-threads=1 --show-output
#[cfg(test)]
mod tests {
	use anyhow::anyhow;
	use std::fmt::{Display, Formatter};
	use std::fs::{read_to_string, remove_file, File};
	use std::io::{BufRead, ErrorKind, Write as IoWrite};
	use std::path::{Path, PathBuf};
	use std::process::{ChildStdout, Command, ExitStatus, Stdio};
	use std::str::FromStr;
	use std::sync::{Arc, Mutex, Once};
	use std::thread::sleep;
	use std::time::{Duration, Instant};
	use std::{env, io, thread, vec};

	use chrono::Local;
	use crossbeam_channel::{bounded, tick, Receiver, Select};
	use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
	use once_cell::sync::Lazy;
	use regex::Regex;
	use rustix::path::Arg;
	use signal_hook::consts::SIGINT;
	use signal_hook::iterator::Signals;
	use simple_cmd::output_ext::OutputExt;
	use simple_cmd::{Cmd, CommandBuilder};
	use tracing::{debug, error, info, subscriber, trace, warn};
	use tracing_appender::non_blocking::WorkerGuard;
	use tracing_subscriber::prelude::*;
	use tracing_subscriber::{fmt, reload, Registry};

	use crate::dump_util::SimplePackageReader;
	use crate::scanner::Scanner;
	use crate::types::{
		DumpsysPriority, FFPlayOptions, InputSource, InstallLocationOption, InstallOptions, KeyCode, KeyEventType, LogcatLevel, LogcatOptions, LogcatTag, MotionEvent, PackageFlags, SELinuxType,
		ScreenRecordOptions, SettingsType, UninstallOptions,
	};
	use crate::{intent, Adb, AdbClient, Client, Device, PackageManager};

	static INIT: Once = Once::new();

	static ADB: Lazy<Adb> = Lazy::new(|| Adb::new().unwrap());

	static DEVICE_IP: Lazy<String> = Lazy::new(|| String::from("192.168.1.6:5555"));

	// region MACROS

	macro_rules! client {
		() => {
			DEVICE_IP.as_str().parse::<Device>().unwrap().try_into().unwrap()
		};

		($addr:expr) => {
			$addr.parse::<Device>().unwrap().try_into().unwrap()
		};
	}

	macro_rules! assert_client_connected {
		($client:expr) => {
			let result = $client.connect(Some(std::time::Duration::from_secs(1)));
			debug_assert!(result.is_ok(), "failed to connect client: {:?}", $client);
		};
	}

	macro_rules! assert_client_root {
		($client:expr) => {
			if let Ok(root) = $client.is_root() {
				if !root {
					let is_rooted = $client.root().expect("failed to root client (1)");
					debug_assert!(is_rooted, "failed to root client (2)");
				}
			} else {
				let is_rooted = !$client.root().expect("failed to root client (3)");
				debug_assert!(is_rooted, "failed to root client (4)");
			}
		};
	}

	macro_rules! assert_client_unroot {
		($client:expr) => {
			if let Ok(root) = $client.is_root() {
				if root {
					let success = $client.unroot().expect("failed to unroot client (1)");
					debug_assert!(success, "failed to unroot client (2)");
				}
			} else {
				let success = !$client.unroot().expect("failed to unroot client (3)");
				debug_assert!(success, "failed to unroot client (4)");
			}
		};
	}

	static GUARDS: Lazy<Arc<Mutex<Vec<WorkerGuard>>>> = Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

	macro_rules! init_log {
		() => {
			INIT.call_once(|| {
				let timer = time::format_description::parse("[hour]:[minute]:[second].[subsecond digits:3]").unwrap();
				let time_offset = time::UtcOffset::current_local_offset().unwrap_or_else(|_| time::UtcOffset::UTC);
				let timer = fmt::time::OffsetTime::new(time_offset, timer);

				let registry = Registry::default();
				let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());
				let layer1 = fmt::layer()
					.with_thread_names(false)
					.with_thread_ids(false)
					.with_line_number(false)
					.with_file(false)
					.with_target(true)
					.with_timer(timer)
					.with_writer(non_blocking);

				let (layer, _reload_handle) = reload::Layer::new(layer1);
				let subscriber = registry.with(layer);
				subscriber::set_global_default(subscriber).unwrap();
				GUARDS.lock().unwrap().push(guard);
			})
		};
	}

	// endregion MACROS

	#[test]
	fn test_copy() {
		init_log!();

		let client: AdbClient = client!();
		test_copy_client(&client);
	}

	fn test_copy_client<'a>(client: &'a AdbClient) {
		let cloned = AdbClient::copy(client);
		thread::spawn(move || {
			let _ = cloned.name();
		});
	}

	#[test]
	fn test_tracing() {
		info!("starting test_tracing");
		init_log!();

		let registry = Registry::default();
		let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stdout());
		let layer1 = fmt::layer().with_line_number(true).with_file(true).with_writer(non_blocking);
		let (layer, _reload_handle) = reload::Layer::new(layer1);
		let subscriber = registry.with(layer);
		subscriber::set_global_default(subscriber).unwrap();

		info!("info message");
		debug!("debug message");
		trace!("trace message");
		warn!("warn message");
		error!("error message");

		//let _ = reload_handle.modify(|layer| {
		//	let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::empty());
		//	*layer.writer_mut() = non_blocking;
		//});

		info!("info message 2");
		debug!("debug message 2");
		trace!("trace message 2");
		warn!("warn message 2");
		error!("error message 2");

		//thread::sleep(Duration::from_secs(5));
	}

	#[test]
	fn test_parse() {
		init_log!();
		let device: Device = "192.168.1.128".parse().unwrap();
		let client: AdbClient = device.try_into().unwrap();
		debug!("client: {:?}", client);
	}

	#[test]
	fn test_connect() {
		init_log!();
		let device: Device = DEVICE_IP.parse().unwrap();
		let client: AdbClient = device.try_into().unwrap();
		client.connect(Some(Duration::from_secs(10))).unwrap();
	}

	#[test]
	fn test_is_connected() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert!(client.is_connected());
	}

	#[test]
	fn test_whoami() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let whoami = client.shell().whoami().expect("whoami failed");

		GUARDS.lock().unwrap().clear();

		debug!("whoami: {:?}", whoami);
		debug_assert!(whoami.is_some(), "unknown whoami");
	}

	#[test]
	fn test_remount() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		client.root().expect("root failed");
		client.remount().expect("remount failed");
	}

	#[test]
	fn test_root() {
		init_log!();

		let client: AdbClient = client!();
		assert_client_connected!(client);

		let success = client.root().expect("root failed");
		debug_assert!(success, "root failed");
	}

	#[test]
	fn test_is_root() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let was_rooted = client.is_root().expect("is_root failed");
		debug!("is_root = {}", was_rooted);

		if was_rooted {
			client.unroot().expect("failed to unroot");
		} else {
			client.root().expect("failed to root");
		}

		let is_rooted = client.is_root().expect("is_root failed");
		assert_ne!(is_rooted, was_rooted);
	}

	#[test]
	fn test_which() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let w = client.shell().which("busybox").expect("which failed");
		debug_assert!(w.is_some(), "which failed");
		let result = w.unwrap();
		trace!("result: {:?}", result);
		assert_eq!(result.as_str(), "/vendor/bin/busybox");
	}

	#[test]
	fn test_getprop() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let output = client.shell().getprop("wifi.interface").expect("getprop failed");
		assert_eq!("wlan0", output.trim_end());

		let stb_name = client.shell().getprop("persist.sys.stb.name").expect("failed to read persist.sys.stb.name");
		debug!("stb name: `{:}`", stb_name.trim_end());
		assert!(stb_name.len() > 1);
	}

	#[test]
	fn test_setprop() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();

		let prop = shell.getprop("dalvik.vm.heapsize").unwrap();
		assert!(!prop.is_empty());

		shell.setprop("dalvik.vm.heapsize", "512m").unwrap();
		assert_eq!(shell.getprop("dalvik.vm.heapsize").unwrap(), "512m");

		shell.setprop("debug.hwui.overdraw", "").unwrap();
		assert_eq!(shell.getprop("debug.hwui.overdraw").unwrap(), "");
	}

	#[test]
	fn test_getprop_types() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		for (key, key_type) in client.shell().getprop_types().unwrap() {
			trace!("{:} = {:?}", key, key_type);
		}
	}

	#[test]
	fn test_get_device_mac_address() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let address = client.get_mac_address().expect("failed to get mac address");
		debug!("mac address: `{:?}`", address.to_string());
	}

	#[test]
	fn test_get_device_wlan_address() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let address = client.get_wlan_address().expect("failed to get wlan0 address");
		debug!("wlan0 address: `{:?}`", address.to_string());
	}

	#[test]
	fn test_cat() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let output = client.shell().cat("/timeshift/conf/tvlib-aot.properties").expect("cat failed");
		assert!(output.lines().into_iter().all(|f| f.is_ok()));
		assert!(output.lines().into_iter().filter(|f| f.is_ok()).all(|l| l.is_ok()));

		trace!("output: {:?}", simple_cmd::Vec8ToString::as_str(&output));

		assert_client_unroot!(client);
	}

	#[test]
	fn test_getprops() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let properties = client.shell().getprops().expect("getprops failed");
		assert!(properties.len() > 0);

		for prop in properties {
			trace!("property: {:?}", prop);
		}
	}

	#[test]
	fn test_exists() {
		init_log!();

		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let exists = client.shell().exists("/timeshift/conf/tvlib-aot.properties").unwrap();
		assert_eq!(true, exists);

		assert_client_unroot!(client);
	}

	#[test]
	fn test_is_file() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let f1 = client.shell().is_file("/timeshift/conf/tvlib-aot.properties").unwrap();
		assert_eq!(true, f1);

		let f2 = client.shell().is_file("/timeshift/conf/").unwrap();
		assert_eq!(false, f2);

		assert_client_unroot!(client);
	}

	#[test]
	fn test_is_dir() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let f1 = client.shell().is_dir("/timeshift/conf/tvlib-aot.properties").unwrap();
		assert_eq!(false, f1);
		let f2 = client.shell().is_dir("/timeshift/conf/").unwrap();
		assert_eq!(true, f2);

		assert_client_unroot!(client);
	}

	#[test]
	fn test_disconnect() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert!(client.disconnect().expect("disconnect failed"));
		assert!(!client.is_connected());
	}

	#[test]
	fn test_disconnect_all() {
		init_log!();
		assert!(Client::disconnect_all(&ADB).expect("disconnect all failed"));
	}

	#[test]
	fn test_list_dir() {
		init_log!();

		let client: AdbClient = client!();

		assert_client_connected!(client);
		assert_client_root!(client);

		let lines = client.shell().ls("/system", Some("-lpALF")).expect("list dir failed");

		for line in lines {
			let file: Result<DeviceFile, ParseError> = DeviceFile::parse(line.as_str());
			if file.is_ok() {
				let f = file.unwrap();
				trace!("{:}", f);

				if f.file_name() == "vendor" {
					assert!(f.is_symlink());
					assert!(!f.is_dir());
					assert!(!f.is_file());
				}
			}
		}
	}

	#[test]
	fn test_list_settings() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();

		let settings = shell.list_settings(SettingsType::system).expect("list settings failed");
		assert!(settings.len() > 0);
		eprintln!("{:#?}", settings);

		for s in settings {
			let value = shell.get_setting(SettingsType::system, s.key.as_str()).expect("get setting failed").expect("parse value failed");
			eprintln!("{} = {} [{:}]", s.key, s.value, value);
		}
	}

	#[test]
	fn test_list_dumpsys() {
		init_log!();

		let client: AdbClient = client!();
		assert_client_connected!(client);

		let output = client.shell().dumpsys_list(false, Some(DumpsysPriority::CRITICAL)).expect("dumpsys failed");

		for line in output {
			trace!("{:?}", line);
		}
	}

	#[test]
	fn test_save_screencap() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		assert!(client.shell().exists("/sdcard/Download").unwrap());

		if client.shell().exists("/sdcard/Download/screencap.png").unwrap() {
			// remove the file
			client.shell().rm("/sdcard/Download/screencap.png", None).unwrap();
		}

		client.shell().save_screencap("/sdcard/Download/screencap.png").expect("save screencap failed");

		assert!(client.shell().exists("/sdcard/Download").unwrap());

		client.shell().rm("/sdcard/Download/screencap.png", None).unwrap();
	}

	#[test]
	fn test_is_screen_on() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let result = client.shell().is_screen_on().expect("is screen on failed");
		assert_eq!(result, true);
	}

	#[test]
	fn test_screen_record() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let mut options = ScreenRecordOptions::default();
		options.verbose = true;
		options.timelimit = Some(Duration::from_secs(12));

		let remote_file = "/sdcard/Download/screenrecord.mp4";
		let local_file = env::current_dir().unwrap().join("screenrecord.mp4");
		let shell = client.shell();

		if local_file.exists() {
			remove_file(&local_file).unwrap();
		}

		let receiver = sigint_notifier().unwrap();

		match shell.screen_record(Some(options), remote_file, Some(receiver)) {
			Ok(t) => trace!("Screen Record Ok: {:?}", t),
			Err(e) => {
				error!("{:}", e)
			}
		}

		trace!("need to sleep a bit..");
		sleep(Duration::from_secs(2));

		client.pull(remote_file, local_file.as_path()).unwrap();

		if local_file.exists() {
			remove_file(&local_file).unwrap();
		}
	}

	#[test]
	fn test_get_events() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let events = client.shell().get_events().unwrap();
		assert!(events.len() > 0);

		for event in events {
			trace!("event: {}, {}", event.0, event.1)
		}
	}

	#[test]
	fn test_send_events() {
		init_log!();
		let client: AdbClient = client!();
		let shell = client.shell();
		assert_client_connected!(client);
		assert_client_root!(client);

		let events: Vec<_> = shell.get_events().unwrap().iter().map(|x| x.0.as_str().to_string()).collect();
		println!("events: {:#?}", events);

		let event = if events.contains(&"/dev/input/event3".to_string()) {
			"/dev/input/event3"
		} else {
			"/dev/input/event0"
		};

		trace!("using event: {:?}", event);

		// KEYCODE_DPAD_RIGHT (action DOWN)
		shell.send_event(event, 0x0001, 0x006a, 0x00000001).unwrap();
		shell.send_event(event, 0x0000, 0x0000, 0x00000000).unwrap();

		// KEYCODE_DPAD_RIGHT (action UP)
		shell.send_event(event, 0x0001, 0x006a, 0x00000000).unwrap();
		shell.send_event(event, 0x0000, 0x0000, 0x00000000).unwrap();
	}

	#[test]
	fn test_command() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let local_path = env::current_dir().unwrap().join("output.txt");
		trace!("local_path: {:?}", local_path.as_path());

		if local_path.exists() {
			remove_file(local_path.as_path()).expect("failed to delete local file");
		}

		let output_file = File::create(local_path.as_path()).unwrap();

		let builder: CommandBuilder = <AdbClient as Into<CommandBuilder>>::into(client)
			.args(vec![
				"ls", "-la", "/",
			])
			.stdout(Some(Stdio::from(output_file)));
		trace!("builder: {:?}", &builder);

		let output = builder.build().output().unwrap();
		debug!("output: {:?}", output);

		let file_output = read_to_string(local_path.as_path()).unwrap();
		file_output.lines().into_iter().for_each(|line| {
			trace!("line: {:?}", line);
		});

		remove_file(local_path.as_path()).expect("failed to delete local file");
	}

	#[test]
	fn test_clear_logcat() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		client.clear_logcat().expect("failed to clear logcat");
	}

	#[test]
	fn test_list_devices() {
		init_log!();

		let adb = Adb::default();
		let devices = adb.devices().expect("failed to list devices");

		info!("Found {} devices", devices.len());
		for device in devices {
			trace!("Found device {:#?}", device);
		}
	}

	#[test]
	fn test_push() {
		init_log!();

		let client: AdbClient = client!();
		let shell = client.shell();

		assert_client_connected!(client);

		let remote_path = PathBuf::from("/sdcard/Download/text.txt");
		if shell.exists(remote_path.as_path()).unwrap() {
			shell.rm(remote_path.as_path(), None).unwrap();
		}

		let local_path = env::current_dir().unwrap().join("test.txt");
		if local_path.exists() {
			remove_file(local_path.as_path()).unwrap();
		}

		let mut file = File::create(&local_path).unwrap();

		file.write("hello world".as_bytes()).unwrap();
		file.flush().unwrap();

		if shell.exists(remote_path.as_path().to_str().unwrap()).unwrap() {
			shell
				.exec(
					vec![
						"rm",
						remote_path.as_path().to_str().unwrap(),
					],
					None,
					None,
				)
				.unwrap();
		}

		let result = client.push(local_path.as_path(), remote_path.as_path().to_str().unwrap()).unwrap();
		trace!("{:?}", result);

		assert!(shell.exists(remote_path.as_path().to_str().unwrap()).unwrap());
		shell
			.exec(
				vec![
					"rm",
					remote_path.as_path().to_str().unwrap(),
				],
				None,
				None,
			)
			.unwrap();

		remove_file(local_path.as_path()).unwrap();
	}

	#[test]
	fn test_logcat() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let receiver = sigint_notifier().unwrap();
		let timeout = Some(Duration::from_secs(5));
		let since = Some(Local::now() - chrono::Duration::seconds(30));

		let options = LogcatOptions {
			expr: None,
			dump: false,
			filename: None,
			tags: None,
			format: None,
			since,
			pid: None,
			timeout,
		};

		let output = client.logcat(options, Some(receiver.clone()));

		match output {
			Ok(o) => {
				if o.status.success() || o.kill() || o.interrupt() {
					let stdout = o.stdout;
					let lines = stdout.lines().map(|l| l.unwrap());
					for _line in lines {
						//trace!("{}", line);
					}
				} else if o.error() {
					warn!("{:?}", o);
				} else {
					error!("{:?}", o);
				}
			}
			Err(err) => {
				warn!("{}", err);
			}
		}
	}

	#[test]
	fn test_client_api_level() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let api_level = client.api_level().unwrap();
		assert!(api_level.parse::<u8>().unwrap() > 0);
		trace!("api level: {:?}", api_level);
	}

	#[test]
	fn test_client_name() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let name = client.name().unwrap();
		assert!(name.is_some());

		let string = name.unwrap();
		assert!(string.len() > 0);

		debug!("device name: {:?}", string);
	}

	#[test]
	fn test_client_version() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let name = client.version().unwrap();
		debug!("client version: {:?}", name);
	}

	#[test]
	fn test_client_uuid() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();

		let result = shell.exec(vec!["scmuuid_test"], None, None).unwrap();
		assert!(result.success());
		assert!(result.has_stdout());

		let stdout = Arg::as_str(&result.stdout).unwrap().to_string();
		let output = stdout.as_str();

		debug_assert!(output.len() > 0, "output is empty");

		let chip_id = parse_scmuuid(output, ScmuuIdType::ChipId).expect("failed to get ChipId");
		debug_assert!(!chip_id.is_empty(), "chip id is empty");

		let verimatrix_chip_id = parse_scmuuid(output, ScmuuIdType::VerimatrixChipId).expect("failed to get VerimatrixChipId");
		debug_assert!(!verimatrix_chip_id.is_empty(), "verimatrix chip id is empty");

		let uid = parse_scmuuid(output, ScmuuIdType::UUID).expect("failed to get UUID");
		debug_assert!(!uid.is_empty(), "uuid is empty");

		debug!("chipId: {:}", chip_id);
		debug!("verimatrixChipId: {:}", verimatrix_chip_id);
		debug!("uuid: {:}", uid);

		let uid_value = uuid::Uuid::from_str(uid.as_str()).unwrap();
		debug!("UUID => {:#?}", uid_value);
	}

	#[test]
	fn test_save_screencap_locally() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let tilde = dirs::desktop_dir().unwrap().join("screencap.png");
		let output = tilde.as_path();
		debug!("target local file: {:?}", output.to_str());

		if output.exists() {
			remove_file(output).expect("Error deleting file");
		}

		let file = File::create(output).expect("failed to create file");
		let _result = client.save_screencap(file).expect("failed to save screencap");
		debug!("ok. done => {:?}", output);

		remove_file(output).unwrap();
	}

	#[test]
	fn test_copy_screencap() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		client.copy_screencap().unwrap();
		debug!("screencap copied");
	}

	#[test]
	fn test_get_boot_id() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let boot_id = client.get_boot_id().expect("failed to get boot id");
		debug!("boot_id: {:#?}", boot_id)
	}

	#[test]
	fn test_send_broadcast() {
		init_log!();
		let client: AdbClient = client!();
		let shell = client.shell();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.standalone";
		let mut intent = intent!["swisscom.android.tv.action.PRINT_SESSION_INFO"];
		intent.component = Some(format![
			"{:}/.receiver.PropertiesReceiver",
			package_name
		]);
		intent.extra.put_string_extra("swisscom.android.tv.extra.TAG", "SESSION_INFO");
		intent.wait = true;

		trace!("{:}", intent);
		let _result = shell.broadcast(&intent).unwrap();

		let cancel = sigint_notifier().unwrap();
		let timeout = Some(Duration::from_secs(5));
		let since = Some(Local::now() - chrono::Duration::seconds(15));

		let options = LogcatOptions {
			expr: None,
			dump: true,
			filename: None,
			tags: Some(vec![
				LogcatTag {
					name: "SESSION_INFO".to_string(),
					level: LogcatLevel::Info,
				},
			]),
			format: None,
			since,
			pid: None,
			timeout,
		};

		let output = client.logcat(options, Some(cancel.clone()));
		assert!(output.is_ok());

		let o = output.unwrap();

		assert!(o.success());
		assert!(!o.kill());
		assert!(!o.interrupt());

		let stdout = o.stdout;

		let re = Regex::new(".* SESSION_INFO:\\s*(?P<session>\\{[^}]+})").unwrap();
		let line = stdout
			.lines()
			.map(|l| l.unwrap())
			.filter_map(|line| {
				if re.is_match(line.as_str()) {
					match re.captures(line.as_str()) {
						None => None,
						Some(captures) => match captures.name("session") {
							None => None,
							Some(c) => Some(c.as_str().to_string()),
						},
					}
				} else {
					None
				}
			})
			.collect::<Vec<_>>();

		assert_eq!(line.len(), 1);
		debug!("line: {:#?}", line.first().unwrap());
	}

	#[test]
	fn test_get_enforce() {
		init_log!();
		let client: AdbClient = client!();
		let shell = client.shell();
		assert_client_connected!(client);
		assert_client_root!(client);

		let enforce = shell.get_enforce().unwrap();
		debug!("enforce = {:}", enforce);
	}

	#[test]
	fn test_set_enforce() {
		init_log!();
		let client: AdbClient = client!();
		let shell = client.shell();

		assert_client_connected!(client);
		assert_client_root!(client);

		let enforce1 = shell.get_enforce().unwrap();
		debug!("enforce = {:}", enforce1);

		let _result = if enforce1 == SELinuxType::Permissive {
			shell.set_enforce(SELinuxType::Enforcing).unwrap();
		} else {
			shell.set_enforce(SELinuxType::Permissive).unwrap();
		};
		assert_ne!(enforce1, shell.get_enforce().unwrap());
	}

	#[test]
	fn test_bugreport() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let p = dirs::desktop_dir().unwrap().join("bugreport.zip");

		if p.exists() {
			remove_file(p.as_path()).unwrap();
		}

		client.bug_report(Some(p.as_path())).unwrap();

		assert_client_unroot!(client);
	}

	#[test]
	fn test_mount_unmount() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		client.mount("/system").unwrap();
		client.unmount("/system").unwrap();

		assert_client_unroot!(client);
	}

	#[test]
	fn test_wait_for_device() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		client.wait_for_device(Some(Duration::from_secs(5))).unwrap();
		assert_client_connected!(client);
		assert_client_unroot!(client);
	}

	#[test]
	fn test_reboot_and_wait() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		client.reboot(None).unwrap();
		client.wait_for_device(None).unwrap();
		assert_client_connected!(client);
	}

	#[test]
	fn test_pm_list_packages() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let pm: PackageManager = client.pm();
		let result = pm.list_packages(None, None, None).unwrap();

		trace!("result: {:#?}", result);
		assert!(result.len() > 0);

		result.iter().for_each(|p| {
			assert!(p.file_name.is_some());
			assert!(p.version_code.is_some());
			assert!(p.uid.is_some());
		});
	}

	#[test]
	fn test_pm_path() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let pm: PackageManager = client.pm();
		let path = pm.path("com.swisscom.aot.library.standalone", None).unwrap();
		trace!("path: {:?}", path)
	}

	#[test]
	fn test_pm_is_system() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let pm: PackageManager = client.pm();
		let result = pm.is_system("com.swisscom.aot.library.standalone").unwrap();
		trace!("result: {:#?}", result)
	}

	#[test]
	fn test_pm_get_package_flags() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.webclient";

		let pm: PackageManager = client.pm();
		let result = pm.package_flags(package_name).unwrap();
		trace!("result: {:#?}", result);

		let path = pm.path(package_name, None).unwrap();
		trace!("path: {:#?}", path);

		if result.contains(&PackageFlags::System) && result.contains(&PackageFlags::UpdatedSystemApp) {
			assert!(!path.starts_with("/system/"))
		} else if result.contains(&PackageFlags::System) {
			assert!(path.starts_with("/system/"))
		} else {
			assert!(path.starts_with("/data/"))
		}
	}

	#[test]
	fn test_pm_is_installed() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let pm: PackageManager = client.pm();
		let result = pm.is_installed("com.swisscom.aot.library.standalone", None).unwrap();
		trace!("path: {:?}", result);
	}

	#[test]
	fn test_pm_install() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let apk = Path::new("/Users/alessandro/Documents/git/swisscom/app-service/app-service/build/outputs/apk/ip2300/release/app-service-ip2300-release.apk");
		assert!(apk.exists());

		let remote_path = "/data/local/tmp";
		let remote_file = format!("{:}/{:}", remote_path, apk.file_name().unwrap().to_str().unwrap());
		client.push(apk, remote_path).unwrap();

		let pm: PackageManager = client.pm();

		let result = pm
			.install(
				remote_file,
				Some(InstallOptions {
					user: None,
					dont_kill: false,
					restrict_permissions: false,
					package_name: Some(package_name.to_string()),
					install_location: Some(InstallLocationOption::Auto),
					grant_permissions: true,
					force: true,
					replace_existing_application: false,
					allow_version_downgrade: true,
				}),
			)
			.unwrap();

		client.pm().is_installed(package_name, None).unwrap();

		trace!("path: {:?}", result);
	}

	#[test]
	fn test_pm_uninstall() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let installed = client.pm().is_installed(package_name, None).unwrap();

		if !installed {
			warn!("package `{:}` not installed", package_name);
			return;
		}

		assert!(installed);

		let package = client.pm().list_packages(None, None, Some(package_name)).unwrap().first().unwrap().to_owned();

		client
			.pm()
			.uninstall(
				package_name,
				Some(UninstallOptions {
					keep_data: false,
					user: Some("0".to_string()),
					version_code: package.version_code,
				}),
			)
			.unwrap();

		assert!(!client.pm().is_installed(package_name, None).unwrap());
	}

	#[test]
	fn test_pm_runtime_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let result = client.pm().dump_runtime_permissions(package_name).unwrap();
		assert!(result.len() > 0);
		trace!("result: {:#?}", result);
	}

	#[test]
	fn test_pm_grant_runtime_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().grant(package_name, Some("1000"), "android.permission.ACCESS_FINE_LOCATION").unwrap();
		assert!(client
			.pm()
			.dump_runtime_permissions(package_name)
			.unwrap()
			.iter()
			.any(|p| p.name == "android.permission.ACCESS_FINE_LOCATION"));
	}

	#[test]
	fn test_pm_revoke_runtime_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().revoke(package_name, Some("1000"), "android.permission.ACCESS_FINE_LOCATION").unwrap();
	}

	#[test]
	fn test_pm_disable() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().disable(package_name, Some("1000")).unwrap();
	}

	#[test]
	fn test_pm_enable() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().enable(package_name, Some("1000")).unwrap();
	}

	#[test]
	fn test_pm_dump_requested_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let requested_permissions = client.pm().requested_permissions(package_name).unwrap();
		assert!(requested_permissions.len() > 0);

		trace!("requested permissions: {:#?}", requested_permissions);
	}

	#[test]
	fn test_pm_install_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let result = client.pm().install_permissions(package_name).unwrap();
		assert!(result.len() > 0);
		trace!("result: {:#?}", result);
	}

	#[test]
	fn test_pm_operations() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().clear(package_name, None).unwrap();
	}

	#[test]
	fn test_dump() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.ui";
		let dump = client.pm().dump(package_name, None).expect("failed to dump package");
		assert!(!dump.is_empty());
	}

	#[test]
	fn test_pm_package_reader() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.ui";
		let dump = client.pm().dump(package_name, None).unwrap();
		let reader = SimplePackageReader::new(dump.as_str()).unwrap();

		let mut result = reader.get_version_name().unwrap();
		trace!("version_name: {:#?}", result);

		result = reader.get_first_install_time().unwrap();
		trace!("first_install_time: {:#?}", result);

		result = reader.get_last_update_time().unwrap();
		trace!("last_update_time: {:#?}", result);

		result = reader.get_timestamp().unwrap();
		trace!("timestamp: {:#?}", result);

		result = reader.get_data_dir().unwrap();
		trace!("dataDir: {:#?}", result);

		result = reader.get_user_id().unwrap();
		trace!("userId: {:#?}", result);

		result = reader.get_code_path().unwrap();
		trace!("codePath: {:#?}", result);

		result = reader.get_resource_path().unwrap();
		trace!("resourcePath: {:#?}", result);

		let version_code = reader.get_version_code().unwrap();
		trace!("versionCode: {:#?}", version_code);
	}

	#[test]
	fn test_am_broadcast() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.standalone";
		let mut intent = intent!["swisscom.android.tv.action.PRINT_SESSION_INFO"];
		intent.component = Some(format![
			"{:}/.receiver.PropertiesReceiver",
			package_name
		]);
		intent.extra.put_string_extra("swisscom.android.tv.extra.TAG", "SESSION_INFO");
		intent.wait = true;

		trace!("{:}", intent);
		let _result = client.am().broadcast(&intent).unwrap();
	}

	#[test]
	fn test_am_start() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let mut intent = intent!["android.intent.action.VIEW"];
		intent.data = Some("http://www.google.com".to_string());
		intent.wait = true;

		trace!("{:}", intent);
		let _result = client.am().start(&intent).unwrap();
	}

	#[test]
	fn test_am_force_stop() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);
		client.am().force_stop("com.swisscom.aot.ui").expect("unable to force stop package");
	}

	#[test]
	fn test_shell_send_key_event() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_keyevent(KeyCode::KEYCODE_1, Some(KeyEventType::DoubleTap), Some(InputSource::dpad)).unwrap();
	}

	#[test]
	fn test_shell_send_motion() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_motion(None, MotionEvent::DOWN, (1000, 600)).unwrap();
		shell.send_motion(None, MotionEvent::MOVE, (1000, 600)).unwrap();
		shell.send_motion(None, MotionEvent::MOVE, (1000, 100)).unwrap();
		shell.send_motion(None, MotionEvent::UP, (1000, 100)).unwrap();
	}

	#[test]
	fn test_shell_send_drag_and_drop() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_draganddrop(None, Some(Duration::from_millis(1500)), (1800, 600), (1700, 100)).unwrap();
	}

	#[test]
	fn test_shell_send_press() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_press(None).unwrap();
	}

	#[test]
	fn test_shell_send_keycombination() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell
			.send_keyevent_combination(
				None,
				vec![
					KeyCode::KEYCODE_1,
					KeyCode::KEYCODE_3,
				],
			)
			.unwrap();
	}

	#[test]
	fn test_shell_send_key_code() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_keycode(82, None, None).unwrap();
	}

	#[test]
	fn test_shell_send_key_events() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell
			.send_keyevents(
				vec![
					KeyCode::KEYCODE_1,
					KeyCode::KEYCODE_9,
				],
				Some(InputSource::dpad),
			)
			.unwrap();
	}

	#[test]
	fn test_is_awake() {
		init_log!();
		let client: AdbClient = client!();
		let is_awake = client.is_awake().unwrap();
		debug!("is_awake: {}", is_awake);
	}

	#[test]
	fn test_get_awake_status() {
		init_log!();
		let client: AdbClient = client!();
		let status = client.get_wakefulness().unwrap();
		debug!("device status: {}", status);
	}

	#[test]
	fn test_shell_dumpsys() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		let result = shell.dumpsys(Some("adb"), None, None, true, false, false, None).unwrap();

		trace!("result: {:}", result.stdout.to_string_lossy().to_string());
	}

	//
	//#[tokio::test]
	//async fn test_command_pipe() {
	//    init_log!();
	//    let client: AdbClient = client!();
	//
	//    tokio::join!(async {
	//		let mut cmd1 = <AdbClient as Into<CommandBuilder>>::into(client);
	//		cmd1.arg("while true; do screenrecord --output-format=h264 -; done");
	//
	//		let mut cmd2 = CommandBuilder::new("ffplay");
	//		cmd2.args(vec!["-framerate", "30", "-probesize", "32", "-sync", "video", "-vf", "scale=800:-1", "-"]);
	//
	//		let output = CommandBuilder::pipe_with_timeout(cmd1, cmd2, Duration::from_secs(10)).unwrap();
	//
	//		trace!("exit status: {:?}", output.status);
	//
	//		if output.status.success() {
	//			for line in output.stdout.lines() {
	//				debug!("stdout => {:}", line.unwrap().trim_end());
	//			}
	//		} else {
	//			for line in output.stderr.lines() {
	//				warn!("stderr => {:}", line.unwrap().trim_end());
	//			}
	//		}
	//	});
	//}

	#[test]
	fn test_scan() {
		init_log!();

		let progress_style = ProgressStyle::with_template("{prefix:.cyan.bold/blue.bold}: {elapsed_precise} [{bar:40.cyan/blue}] {percent:.bold}% ETA: [{eta}]. {msg} ")
			.unwrap()
			.progress_chars("=> ");

		let multi_progress = MultiProgress::new();
		let progress = multi_progress.add(ProgressBar::new(255));
		progress.set_style(progress_style.clone());
		progress.set_prefix("Elapsed");

		let (tx, rx) = bounded(255);
		//let log_level = log::max_level();
		//log::set_max_level(LevelFilter::Off);

		let adb = Adb::new().unwrap();

		let scanner = Scanner::new();
		let start = Instant::now();

		scanner.scan(&adb, tx.clone());

		let elapsed = start.elapsed();

		drop(tx);

		let mut result = Vec::new();
		for client in rx {
			progress.inc(1);

			if let Some(client) = client {
				result.push(client);
			}
		}

		//log::set_max_level(log_level);

		debug!("Time elapsed for scanning is: {:?}ms", elapsed.as_millis());
		debug!("Found {:} devices", result.len());

		for device in result.iter() {
			info!("device: {:}", device);
		}
	}

	#[test]
	fn test_screen_mirror2() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let cancel_signal = ctrl_channel().unwrap();

		let screen_record_options = ScreenRecordOptions {
			bitrate: Some(4_000_000),
			timelimit: Some(Duration::from_secs(10)),
			rotate: None,
			bug_report: None,
			size: Some((1920, 1080)),
			verbose: false,
		};

		let play_options = FFPlayOptions::default();

		let result = client.shell().screen_mirror(Some(screen_record_options), Some(play_options), Some(cancel_signal)).unwrap();
		trace!("result: {:#?}", result);
	}

	#[test]
	fn test_screen_mirror() {
		init_log!();

		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let ffplay = which::which("ffplay").expect("ffplay not found in PATH");

		let child1 = client
			.shell()
			.to_command()
			.args(vec![
				"shell",
				"while true; do screenrecord --output-format=h264 -; done",
			])
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();

		let out: ChildStdout = child1.stdout.ok_or(io::Error::new(ErrorKind::InvalidData, "child stdout unavailable")).unwrap();
		let fd: Stdio = out.try_into().unwrap();

		let mut command2 = Command::new(ffplay.clone());
		command2
			.args(vec![
				"-framerate",
				"60",
				"-probesize",
				"32",
				"-sync",
				"video",
				"-",
			])
			.stdout(Stdio::piped());
		command2.stdin(fd);

		//command2.stdin(Stdio::from(child1.stdout.unwrap()))
		let child2 = command2.spawn().unwrap();

		//let output = child2.wait_with_output().unwrap();
		let output = child2.wait_with_output().unwrap();

		trace!("exit status: {:?}", output.status);
	}

	#[test]
	fn test_try_send_events() {
		init_log!();
		let client: AdbClient = client!();
		let now = Instant::now();

		let r = client.shell().try_send_keyevent(KeyCode::KEYCODE_DPAD_UP, None, None).unwrap();
		debug!("result = {:?}", r);

		let r = client.shell().try_send_keyevent(KeyCode::KEYCODE_DPAD_UP, None, None).unwrap();
		debug!("result = {:?}", r);

		let r = client.shell().try_send_keyevent(KeyCode::KEYCODE_DPAD_UP, None, None).unwrap();
		debug!("result = {:?}", r);

		let r = client.shell().try_send_keyevent(KeyCode::KEYCODE_DPAD_UP, None, None).unwrap();
		debug!("result = {:?}", r);

		let elapsed = now.elapsed();
		debug!("elapsed = {:?}ms", elapsed.as_millis());

		debug_assert!(elapsed < Duration::from_secs(1), "elapsed = {:?}", elapsed);
	}

	fn ctrl_channel() -> Result<Receiver<()>, ctrlc::Error> {
		let (sender, receiver) = bounded(1);
		ctrlc::set_handler(move || {
			println!("sending CTRL+C to ctrl_channel");
			let _ = sender.send(());
		})?;
		Ok(receiver)
	}

	#[test]
	fn test_signals() {
		init_log!();

		let mut command = std::process::Command::new("adb");
		command.args(["logcat"]);
		command.stdout(Stdio::piped());
		command.stderr(Stdio::piped());
		let mut child = command.spawn().unwrap();

		let stdout = child.stdout.take();
		let stderr = child.stderr.take();
		let (sender, receiver) = crossbeam_channel::bounded(1);

		let ctrl_c_events = ctrl_channel().unwrap();
		let ticks = tick(Duration::from_millis(10_000));

		trace!("Starting test...");

		let child_thread = thread::Builder::new()
			.name("cmd_wait".to_string())
			.spawn(move || {
				info!("[thread] Starting thread...");

				let mut sel = Select::new();

				let oper1 = sel.recv(&ctrl_c_events);
				let oper2 = sel.recv(&ticks);
				let mut killed = false;

				loop {
					match sel.try_ready() {
						Err(_) => {
							if let Ok(Some(status)) = child.try_wait() {
								trace!("[thread:err] Exit Status Received... {:}", status);
								let _ = sender.send(status).unwrap();
								break;
							}
						}

						Ok(i) if i == oper1 && !killed => {
							trace!("[thread] CTRL+C");
							sel.remove(oper1);
							let _ = child.kill();
							killed = true;
						}

						Ok(i) if i == oper2 && !killed => {
							trace!("[thread] TIMEOUT");
							sel.remove(oper2);
							let _ = child.kill();
							killed = true;
						}

						Ok(i) => {
							if let Ok(Some(status)) = child.try_wait() {
								trace!("[thread:{:}] Exit Status Received... {:}", i, status);
								let _ = sender.send(status).unwrap();
								break;
							}
						}
					}
				}
			})
			.unwrap();

		let output = Cmd::read_to_end(stdout, stderr).unwrap();

		if let Err(_err) = child_thread.join() {
			warn!("failed to join the thread!");
		}

		trace!("thread stopped. Now waiting for receiver...");

		#[allow(unused_assignments)]
		let mut status: Option<ExitStatus> = None;

		match receiver.recv_timeout(Duration::from_secs(10)) {
			Ok(exit_status) => {
				status = Some(exit_status);
				trace!("received exit status: {:?}", exit_status);
			}
			Err(err) => {
				error!("failed to receive exit status: {:?}", err);
				status = None;
			}
		}

		debug!("stdout = {:?}", output.0.to_string_lossy());
		debug!("stderr = {:?}", output.1.to_string_lossy());
		debug!("status = {:?}", status);
	}

	#[test]
	fn test_get_command() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let command = client.shell().get_command_path("avbctl").unwrap();
		assert_eq!("/system/bin/avbctl", command.as_str());

		let _ = client.shell().get_command_path("this_should_not_exist!").expect_err("Error expected!");
	}

	#[test]
	fn test_get_verity() {
		init_log!();
		let client: AdbClient = client!();
		let verity_enabled = client.get_verity().unwrap();
		println!("verity enabled: {verity_enabled}");
	}

	#[test]
	fn test_enabled_disable_verity() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let enabled = client.get_verity().unwrap();
		println!("verity enabled: {enabled}");

		if enabled {
			client.disable_verity().unwrap();
		} else {
			client.enable_verity().unwrap();
		}
		client.reboot(None).unwrap();
		client.wait_for_device(None).unwrap();

		assert_client_root!(client);
		let enabled2 = client.get_verity().unwrap();
		println!("now verity enabled: {enabled2}");
		assert_ne!(enabled, enabled2);
	}

	// Creates a channel that gets a message every time `SIGINT` is signalled.
	fn sigint_notifier() -> io::Result<Receiver<()>> {
		let (s, r) = bounded(1);
		let mut signals = Signals::new(&[SIGINT])?;

		thread::spawn(move || {
			for _ in signals.forever() {
				if s.send(()).is_err() {
					break;
				}
			}
		});

		Ok(r)
	}

	fn parse_scmuuid(output: &str, scmuu_id_type: ScmuuIdType) -> anyhow::Result<String> {
		let re = match scmuu_id_type {
			ScmuuIdType::UUID => Regex::new("(?m)^UUID:\\s*(?P<id>[0-9a-zA-Z-]+)"),
			ScmuuIdType::VerimatrixChipId => Regex::new("(?m)^VMXCHIPID:\\s*(?P<id>[0-9a-zA-Z-]+)"),
			ScmuuIdType::ChipId => Regex::new("(?m)^CHIPID:\\s*(?P<id>[0-9a-zA-Z-]+)"),
		}?;

		let captures = re.captures(output).ok_or(anyhow!("not found"))?;
		Ok(captures.name("id").ok_or(anyhow!("capture not found"))?.as_str().to_string())
	}

	enum ScmuuIdType {
		UUID,
		VerimatrixChipId,
		ChipId,
	}

	#[derive(Debug, Clone)]
	#[allow(dead_code)]
	struct DeviceFile {
		raw_value: String,
		perms: String,
		links: i128,
		owner: String,
		group: String,
		size: i64,
		date: String,
		time: String,
		name: String,
	}

	#[derive(Debug, Clone)]
	struct ParseError;

	impl Display for ParseError {
		fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
			write!(f, "failed to parse line")
		}
	}

	impl Display for DeviceFile {
		fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
			write!(
				f,
				"{:} {:} {:} {:} {:} {:} {:} {:}",
				self.perms,
				self.links,
				self.owner,
				self.group,
				self.size,
				self.date,
				self.time,
				self.file_name()
			)
		}
	}

	impl FromStr for DeviceFile {
		type Err = ParseError;

		fn from_str(s: &str) -> Result<Self, Self::Err> {
			let re = Regex::new("\\s+").unwrap();
			let fields: Vec<&str> = re.splitn(s, 8).collect();

			if fields.len() < 8 {
				return Err(ParseError);
			}

			let perms = fields.get(0).unwrap().to_string();
			let links = fields.get(1).unwrap().parse::<i128>().map_err(|_| ParseError)?;
			let owner = fields.get(2).unwrap().to_string();
			let group = fields.get(3).unwrap().to_string();
			let size = fields.get(4).unwrap().parse::<i64>().map_err(|_| ParseError)?;
			let date = fields.get(5).unwrap().to_string();
			let time = fields.get(6).unwrap().to_string();
			let name = fields.get(7).unwrap().to_string();

			Ok(DeviceFile {
				raw_value: s.to_string(),
				perms,
				links,
				owner,
				group,
				size,
				date,
				time,
				name,
			})
		}
	}

	impl DeviceFile {
		#[inline]
		pub fn parse<F: FromStr>(s: &str) -> Result<F, F::Err> {
			FromStr::from_str(s)
		}

		pub fn file_name(&self) -> &str {
			if self.is_symlink() {
				self.get_src_symlink_name()
			} else {
				self.name.as_str()
			}
		}

		fn get_src_symlink_name(&self) -> &str {
			let v: Vec<&str> = self.name.splitn(2, " -> ").collect();
			v.get(0).unwrap().trim()
		}

		#[allow(dead_code)]
		fn get_dst_symlink_name(&self) -> &str {
			let v: Vec<&str> = self.name.splitn(2, " -> ").collect();
			v.get(1).unwrap().trim()
		}

		pub fn is_dir(&self) -> bool {
			self.test_perm(b'd')
		}

		pub fn is_file(&self) -> bool {
			self.test_perm(b'-')
		}

		pub fn is_symlink(&self) -> bool {
			self.test_perm(b'l')
		}

		fn test_perm(&self, c: u8) -> bool {
			self.perms.as_bytes()[0] == c
		}
	}
}
