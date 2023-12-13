/// cargo test --color=always --bin randroid tests -- --test-threads=1 --show-output
#[cfg(test)]
mod tests {
	use std::fmt::{Display, Formatter};
	use std::fs::{read_to_string, remove_file, File};
	use std::io::{BufRead, Write};
	use std::os::fd::{AsRawFd, FromRawFd};
	use std::path::{Path, PathBuf};
	use std::process::Stdio;
	use std::str::FromStr;
	use std::sync::Once;
	use std::time::Duration;
	use std::{env, vec};

	use anyhow::anyhow;
	use chrono::Local;
	use futures::{StreamExt, TryFutureExt};
	use log::*;
	use once_cell::sync::Lazy;
	use regex::Regex;
	use rustix::path::Arg;
	use time::Instant;
	use tokio::process::Command;
	use tokio::sync::oneshot::{channel, Receiver, Sender};
	use tokio_util::codec::{FramedRead, LinesCodec};

	use crate::client::{LogcatLevel, LogcatOptions, LogcatTag};
	use crate::command::CommandBuilder;
	use crate::debug::CommandDebug;
	use crate::dump_util::SimplePackageReader;
	use crate::input::{InputSource, KeyCode, KeyEventType, MotionEvent};
	use crate::pm::{InstallLocationOption, InstallOptions, ListPackageDisplayOptions, ListPackageFilter, PackageFlags, PackageManager, UninstallOptions};
	use crate::scanner::Scanner;
	use crate::shell::{DumpsysPriority, ScreenRecordOptions, SettingsType};
	use crate::traits::AdbDevice;
	use crate::types::AdbClient;
	use crate::util::Vec8ToString;
	use crate::{intent, Adb, Client, Device, SELinuxType};

	static INIT: Once = Once::new();

	static ADB: Lazy<Adb> = Lazy::new(|| Adb::new().unwrap());

	static DEVICE_IP: Lazy<String> = Lazy::new(|| String::from("192.168.1.101:5555"));

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
			let result = $client.connect(Some(std::time::Duration::from_secs(1))).await;
			debug_assert!(result.is_ok(), "failed to connect client: {:?}", $client);
			trace!("connected!");
		};
	}

	macro_rules! assert_client_root {
		($client:expr) => {
			if let Ok(root) = $client.is_root().await {
				if !root {
					let is_rooted = $client.root().await.expect("failed to root client (1)");
					debug_assert!(is_rooted, "failed to root client (2)");
				}
			} else {
				let is_rooted = !$client.root().await.expect("failed to root client (3)");
				debug_assert!(is_rooted, "failed to root client (4)");
			}
		};
	}

	macro_rules! assert_client_unroot {
		($client:expr) => {
			if let Ok(root) = $client.is_root().await {
				if root {
					let success = $client.unroot().await.expect("failed to unroot client (1)");
					debug_assert!(success, "failed to unroot client (2)");
				}
			} else {
				let success = !$client.unroot().await.expect("failed to unroot client (3)");
				debug_assert!(success, "failed to unroot client (4)");
			}
		};
	}

	macro_rules! init_log {
		() => {
			INIT.call_once(|| {
				simple_logger::SimpleLogger::new().env().init().unwrap();
			})
		};
	}

	#[tokio::test]
	async fn test_connect() {
		init_log!();
		let device: Device = DEVICE_IP.parse().unwrap();
		let client: AdbClient = device.try_into().unwrap();
		client.connect(Some(Duration::from_secs(10))).await.unwrap();
	}

	#[tokio::test]
	async fn test_is_connected() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert!(client.is_connected().await);
	}

	#[tokio::test]
	async fn test_whoami() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let whoami = client.shell().whoami().await.expect("whoami failed");
		debug!("whoami: {:?}", whoami);
		debug_assert!(whoami.is_some(), "unknown whoami");
	}

	#[tokio::test]
	async fn test_remount() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		client.root().await.expect("root failed");
		client.remount().await.expect("remount failed");
	}

	#[tokio::test]
	async fn test_disable_verity() {
		init_log!();

		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		client.disable_verity().await.expect("disable_verity failed");
	}

	#[tokio::test]
	async fn test_root() {
		init_log!();

		let client: AdbClient = client!();
		assert_client_connected!(client);

		let success = client.root().await.expect("root failed");
		debug_assert!(success, "root failed");
	}

	#[tokio::test]
	async fn test_is_root() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let was_rooted = client.is_root().await.expect("is_root failed");
		debug!("is_root = {}", was_rooted);

		if was_rooted {
			client.unroot().await.expect("failed to unroot");
		} else {
			client.root().await.expect("failed to root");
		}

		let is_rooted = client.is_root().await.expect("is_root failed");
		assert_ne!(is_rooted, was_rooted);
	}

	#[tokio::test]
	async fn test_which() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let w = client.shell().which("busybox").await.expect("which failed");
		debug_assert!(w.is_some(), "which failed");
		let result = w.unwrap();
		trace!("result: {:?}", result);
		assert_eq!(result.as_str(), "/vendor/bin/busybox");
	}

	#[tokio::test]
	async fn test_getprop() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let output = client.shell().getprop("wifi.interface").await.expect("getprop failed");
		assert_eq!("wlan0", output.trim_end());

		let stb_name = client.shell().getprop("persist.sys.stb.name").await.expect("failed to read persist.sys.stb.name");
		debug!("stb name: `{:}`", stb_name.trim_end());
		assert!(stb_name.len() > 1);
	}

	#[tokio::test]
	async fn test_setprop() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();

		let prop = shell.getprop("dalvik.vm.heapsize").await.unwrap();
		assert!(!prop.is_empty());

		shell.setprop("dalvik.vm.heapsize", "512m").await.unwrap();
		assert_eq!(shell.getprop("dalvik.vm.heapsize").await.unwrap(), "512m");

		shell.setprop("debug.hwui.overdraw", "").await.unwrap();
		assert_eq!(shell.getprop("debug.hwui.overdraw").await.unwrap(), "");
	}

	#[tokio::test]
	async fn test_getprop_type() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		for prop in client.shell().getprops().await.unwrap() {
			let prop_type = client.shell().getprop_type(prop.key.as_str()).await.unwrap();
			trace!("{:} = {:}", prop.key, prop_type);
		}
	}

	#[tokio::test]
	async fn test_get_device_mac_address() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let address = client.get_mac_address().await.expect("failed to get mac address");
		debug!("mac address: `{:?}`", address.to_string());
	}

	#[tokio::test]
	async fn test_get_device_wlan_address() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let address = client.get_wlan_address().await.expect("failed to get wlan0 address");
		debug!("wlan0 address: `{:?}`", address.to_string());
	}

	#[tokio::test]
	async fn test_cat() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let output = client.shell().cat("/timeshift/conf/tvlib-aot.properties").await.expect("cat failed");
		assert!(output.lines().into_iter().all(|f| f.is_ok()));
		assert!(output.lines().into_iter().filter(|f| f.is_ok()).all(|l| l.is_ok()));

		trace!("output: {:?}", Vec8ToString::as_str(&output));

		assert_client_unroot!(client);
	}

	#[tokio::test]
	async fn test_getprops() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let properties = client.shell().getprops().await.expect("getprops failed");
		assert!(properties.len() > 0);

		for prop in properties {
			trace!("property: {:?}", prop);
		}
	}

	#[tokio::test]
	async fn test_exists() {
		init_log!();

		let client: AdbClient = client!();
		assert_client_connected!(client);

		let exists = client.shell().exists("/timeshift/conf/tvlib-aot.properties").await.unwrap();
		assert_eq!(true, exists);
	}

	#[tokio::test]
	async fn test_is_file() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let f1 = client.shell().is_file("/timeshift/conf/tvlib-aot.properties").await.unwrap();
		assert_eq!(true, f1);

		let f2 = client.shell().is_file("/timeshift/conf/").await.unwrap();
		assert_eq!(false, f2);
	}

	#[tokio::test]
	async fn test_is_dir() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let f1 = client.shell().is_dir("/timeshift/conf/tvlib-aot.properties").await.unwrap();
		assert_eq!(false, f1);
		let f2 = client.shell().is_dir("/timeshift/conf/").await.unwrap();
		assert_eq!(true, f2);
	}

	#[tokio::test]
	async fn test_disconnect() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert!(client.disconnect().await.expect("disconnect failed"));
		assert!(!client.is_connected().await);
	}

	#[tokio::test]
	async fn test_disconnect_all() {
		init_log!();
		assert!(Client::disconnect_all(&ADB).await.expect("disconnect all failed"));
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

	#[tokio::test]
	async fn test_list_dir() {
		init_log!();

		let client: AdbClient = client!();

		assert_client_connected!(client);
		assert_client_root!(client);

		let lines = client.shell().ls("/system", Some("-lpALF")).await.expect("list dir failed");

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

	#[tokio::test]
	async fn test_list_settings() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();

		let settings = shell.list_settings(SettingsType::system).await.expect("list settings failed");
		assert!(settings.len() > 0);
		eprintln!("{:#?}", settings);

		for s in settings {
			let value = shell.get_setting(SettingsType::system, s.key.as_str()).await.expect("get setting failed").expect("parse value failed");
			eprintln!("{} = {} [{:}]", s.key, s.value, value);
		}
	}

	#[tokio::test]
	async fn test_list_dumpsys() {
		init_log!();

		let client: AdbClient = client!();
		assert_client_connected!(client);

		let output = client.shell().dumpsys_list(false, Some(DumpsysPriority::CRITICAL)).await.expect("dumpsys failed");

		for line in output {
			trace!("{:?}", line);
		}
	}

	#[tokio::test]
	async fn test_screen_mirror() {
		init_log!();

		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let adb = Adb::new().unwrap();
		let device_ip = client.device.addr().to_string();

		tokio::join!(async {
			let child1 = <Adb as Into<Command>>::into(adb)
				.args(vec!["-s", device_ip.as_str(), "shell", "while true; do screenrecord --output-format=h264 -; done"])
				.stdout(Stdio::piped())
				.stderr(Stdio::piped())
				.debug()
				.spawn()
				.unwrap();

			let mut command2 = Command::new("ffplay");

			command2.args(vec!["-framerate", "60", "-probesize", "32", "-sync", "video", "-"]).stdout(Stdio::piped());

			unsafe {
				let fd = child1.stdout.as_ref().unwrap().as_raw_fd();
				command2.stdin(Stdio::from_raw_fd(fd));
			}

			//command2.stdin(Stdio::from(child1.stdout.unwrap()))
			let child2 = command2.debug().spawn().unwrap();

			//let output = child2.wait_with_output().unwrap();
			let output = child2.wait_with_output().await.unwrap();

			trace!("exit status: {:?}", output.status);

			if output.status.success() {
				for line in output.stdout.lines() {
					debug!("stdout => {:}", line.unwrap().trim_end());
				}
			} else {
				for line in output.stderr.lines() {
					warn!("stderr => {:}", line.unwrap().trim_end());
				}
			}
		});
	}

	#[tokio::test]
	async fn test_command_pipe() {
		init_log!();
		let client: AdbClient = client!();

		tokio::join!(async {
			let mut cmd1 = <AdbClient as Into<CommandBuilder>>::into(client);
			cmd1.arg("while true; do screenrecord --output-format=h264 -; done");

			let mut cmd2 = CommandBuilder::new("ffplay");
			cmd2.args(vec!["-framerate", "30", "-probesize", "32", "-sync", "video", "-vf", "scale=800:-1", "-"]);

			let output = CommandBuilder::pipe_with_timeout(&mut cmd1, &mut cmd2, Duration::from_secs(10)).await.unwrap();

			trace!("exit status: {:?}", output.status);

			if output.status.success() {
				for line in output.stdout.lines() {
					debug!("stdout => {:}", line.unwrap().trim_end());
				}
			} else {
				for line in output.stderr.lines() {
					warn!("stderr => {:}", line.unwrap().trim_end());
				}
			}
		});
	}
	//
	//#[tokio::test]
	//async fn test_command_pipe2() {
	//	init_log!();
	//	let client: AdbClient = client!();
	//
	//	tokio::join!(async {
	//		let mut cmd1 = <AdbClient as Into<CommandBuilder>>::into(client);
	//		cmd1.with_timeout(Some(Duration::from_secs(3)));
	//		cmd1.arg("while true; do screenrecord --output-format=h264 -; done");
	//
	//		let mut cmd2 = CommandBuilder::new("ffplay");
	//		cmd1.with_timeout(Some(Duration::from_secs(3)));
	//		cmd2.args(vec!["-framerate", "60", "-probesize", "32", "-sync", "video", "-"]);
	//
	//		let output = CommandBuilder::pipe_with_timeout(&mut cmd1, &mut cmd2, Duration::from_secs(3)).await.unwrap();
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

	#[tokio::test]
	async fn test_save_screencap() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		assert!(client.shell().exists("/sdcard/Download").await.unwrap());

		if client.shell().exists("/sdcard/Download/screencap.png").await.unwrap() {
			// remove the file
			client.shell().rm("/sdcard/Download/screencap.png", None).await.unwrap();
		}

		client.shell().save_screencap("/sdcard/Download/screencap.png").await.expect("save screencap failed");

		assert!(client.shell().exists("/sdcard/Download").await.unwrap());

		client.shell().rm("/sdcard/Download/screencap.png", None).await.unwrap();
	}

	#[tokio::test]
	async fn test_is_screen_on() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let result = client.shell().is_screen_on().await.expect("is screen on failed");
		assert_eq!(result, true);
	}

	#[tokio::test]
	async fn test_screen_record() {
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

		let (send, recv): (Sender<()>, Receiver<()>) = channel::<()>();
		tokio::spawn(async move {
			tokio::signal::ctrl_c().await.unwrap();
			trace!("ctrl+c pressed!");
			send.send(())
		});

		match shell.screen_record(Some(options), remote_file, Some(recv.into_future())).await {
			Ok(t) => trace!("Screen Record Ok: {:?}", t),
			Err(e) => {
				error!("{:}", e)
			}
		}

		trace!("need to sleep a bit..");
		tokio::time::sleep(Duration::from_secs(2)).await;

		client.pull(remote_file, local_file.as_path()).await.unwrap();

		if local_file.exists() {
			remove_file(&local_file).unwrap();
		}
	}

	#[tokio::test]
	async fn test_get_events() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let events = client.shell().get_events().await.unwrap();
		assert!(events.len() > 0);

		for event in events {
			trace!("event: {}, {}", event.0, event.1)
		}
	}

	#[tokio::test]
	async fn test_send_events() {
		init_log!();
		let client: AdbClient = client!();
		let shell = client.shell();
		assert_client_connected!(client);
		assert_client_root!(client);

		let events: Vec<_> = shell.get_events().await.unwrap().iter().map(|x| x.0.as_str().to_string()).collect();
		println!("events: {:#?}", events);

		let event = if events.contains(&"/dev/input/event3".to_string()) {
			"/dev/input/event3"
		} else {
			"/dev/input/event0"
		};

		trace!("using event: {:?}", event);

		// KEYCODE_DPAD_RIGHT (action DOWN)
		shell.send_event(event, 0x0001, 0x006a, 0x00000001).await.unwrap();
		shell.send_event(event, 0x0000, 0x0000, 0x00000000).await.unwrap();

		// KEYCODE_DPAD_RIGHT (action UP)
		shell.send_event(event, 0x0001, 0x006a, 0x00000000).await.unwrap();
		shell.send_event(event, 0x0000, 0x0000, 0x00000000).await.unwrap();
	}

	#[tokio::test]
	async fn test_command() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let local_path = env::current_dir().unwrap().join("output.txt");
		trace!("local_path: {:?}", local_path.as_path());

		if local_path.exists() {
			remove_file(local_path.as_path()).expect("failed to delete local file");
		}

		let output_file = File::create(local_path.as_path()).unwrap();

		let mut builder: CommandBuilder = client.into();
		builder.args(vec!["ls", "-la", "/"]).stdout(Stdio::from(output_file));
		trace!("builder: {:?}", builder);

		let output = builder.output().await.unwrap();
		debug!("output: {:?}", output);

		let file_output = read_to_string(local_path.as_path()).unwrap();
		file_output.lines().into_iter().for_each(|line| {
			trace!("line: {:?}", line);
		});

		remove_file(local_path.as_path()).expect("failed to delete local file");
	}

	#[tokio::test]
	async fn test_clear_logcat() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		client.clear_logcat().await.expect("failed to clear logcat");
	}

	#[tokio::test]
	async fn test_list_devices() {
		init_log!();

		let adb = Adb::default();
		let devices = adb.devices().await.expect("failed to list devices");

		info!("Found {} devices", devices.len());
		for device in devices {
			trace!("Found device {:#?}", device);
		}
	}

	#[tokio::test]
	async fn test_push() {
		init_log!();

		let client: AdbClient = client!();
		let shell = client.shell();

		assert_client_connected!(client);

		let remote_path = PathBuf::from("/sdcard/Download/text.txt");
		if shell.exists(remote_path.as_path()).await.unwrap() {
			shell.rm(remote_path.as_path(), None).await.unwrap();
		}

		let local_path = env::current_dir().unwrap().join("test.txt");
		if local_path.exists() {
			remove_file(local_path.as_path()).unwrap();
		}

		let mut file = File::create(&local_path).unwrap();

		file.write("hello world".as_bytes()).unwrap();
		file.flush().unwrap();

		if shell.exists(remote_path.as_path().to_str().unwrap()).await.unwrap() {
			shell.exec(vec!["rm", remote_path.as_path().to_str().unwrap()], None).await.unwrap();
		}

		let result = client.push(local_path.as_path(), remote_path.as_path().to_str().unwrap()).await.unwrap();
		trace!("{}", result);

		assert!(shell.exists(remote_path.as_path().to_str().unwrap()).await.unwrap());
		shell.exec(vec!["rm", remote_path.as_path().to_str().unwrap()], None).await.unwrap();

		remove_file(local_path.as_path()).unwrap();
	}

	#[tokio::test]
	async fn test_logcat() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let (send, recv): (Sender<()>, Receiver<()>) = channel::<()>();
		tokio::spawn(async move {
			tokio::signal::ctrl_c().await.unwrap();
			trace!("Ctrl+c pressed!");
			send.send(())
		});

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

		let output = client.logcat(options, Some(recv.into_future())).await;

		match output {
			Ok(o) => {
				if o.is_success() || o.is_kill() || o.is_interrupt() {
					let stdout = o.stdout();
					let lines = stdout.lines().map(|l| l.unwrap());
					for line in lines {
						trace!("{}", line);
					}
				} else if o.has_stderr() {
					warn!("{}", o);
				} else {
					error!("{}", o);
				}
			}
			Err(err) => {
				warn!("{}", err);
			}
		}
	}

	#[tokio::test]
	async fn test_client_api_level() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let api_level = client.api_level().await.unwrap();
		assert!(api_level > 0);
		trace!("api level: {:?}", api_level);
	}

	#[tokio::test]
	async fn test_client_name() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let name = client.name().await.unwrap();
		assert!(name.is_some());

		let string = name.unwrap();
		assert!(string.len() > 0);

		debug!("device name: {:?}", string);
	}

	#[tokio::test]
	async fn test_client_version() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		let name = client.version().await.unwrap();
		debug!("client version: {:?}", name);
	}

	#[tokio::test]
	async fn test_client_uuid() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();

		let result = shell.exec(vec!["scmuuid_test"], None).await.unwrap();
		assert!(result.is_success());
		assert!(result.has_stdout());

		let stdout = Arg::as_str(&result.stdout()).unwrap().to_string();
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

	#[tokio::test]
	async fn test_stream() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);

		trace!("ok, connected...");

		let mut cmd: CommandBuilder = client.into();
		cmd.arg("logcat");
		cmd.stdout(Stdio::piped());

		trace!("Now spawning the child...");

		//let output = cmd.output();
		let mut child = cmd.spawn().await.expect("failed to spawn command");

		let stdout = child.stdout.take().expect("child did not have a handle to stdout");
		let mut reader = FramedRead::new(stdout, LinesCodec::new());
		let (tx, rx) = channel::<()>();

		tokio::spawn(async move {
			tokio::signal::ctrl_c().await.unwrap();
			tx.send(())
		});

		tokio::spawn(async move {
			trace!("spawned...");

			let sleep = tokio::time::sleep(Duration::from_secs(5));
			tokio::select! {
				_ = rx => {
					warn!("CTRL+C received!");
					child.kill().await.unwrap();
				},
				_ = child.wait() => {
					warn!("Child exited normally")
				},
				_ = (sleep) => {
					warn!("Timeout expired!");
					child.kill().await.unwrap();
				},
			}
		});

		while let Some(line) = reader.next().await {
			trace!("Line: {}", line.unwrap());
		}

		debug!("Ok. done");
	}

	#[tokio::test]
	async fn test_save_screencap_locally() {
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
		let _result = client.save_screencap(file).await.expect("failed to save screencap");
		debug!("ok. done => {:?}", output);

		remove_file(output).unwrap();
	}

	#[tokio::test]
	async fn test_copy_screencap() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		client.copy_screencap().await.unwrap();
		debug!("screencap copied");
	}

	#[tokio::test]
	async fn test_get_boot_id() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let boot_id = client.get_boot_id().await.expect("failed to get boot id");
		debug!("boot_id: {:#?}", boot_id)
	}

	#[tokio::test]
	async fn test_send_broadcast() {
		init_log!();
		let client: AdbClient = client!();
		let shell = client.shell();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.standalone";
		let mut intent = intent!["swisscom.android.tv.action.PRINT_SESSION_INFO"];
		intent.component = Some(format!["{:}/.receiver.PropertiesReceiver", package_name]);
		intent.extra.put_string_extra("swisscom.android.tv.extra.TAG", "SESSION_INFO");
		intent.wait = true;

		trace!("{:}", intent);
		let _result = shell.broadcast(&intent).await.unwrap();

		let (send, recv): (Sender<()>, Receiver<()>) = channel::<()>();
		tokio::spawn(async move {
			tokio::signal::ctrl_c().await.unwrap();
			trace!("Ctrl+c pressed!");
			send.send(())
		});

		let timeout = Some(Duration::from_secs(5));
		let since = Some(Local::now() - chrono::Duration::seconds(15));

		let options = LogcatOptions {
			expr: None,
			dump: true,
			filename: None,
			tags: Some(vec![LogcatTag {
				name: "SESSION_INFO".to_string(),
				level: LogcatLevel::Info,
			}]),
			format: None,
			since,
			pid: None,
			timeout,
		};

		let output = client.logcat(options, Some(recv.into_future())).await;
		assert!(output.is_ok());

		let o = output.unwrap();

		assert!(o.is_success());
		assert!(!o.is_kill());
		assert!(!o.is_interrupt());

		let stdout = o.stdout();

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

	#[tokio::test]
	async fn test_get_enforce() {
		init_log!();
		let client: AdbClient = client!();
		let shell = client.shell();
		assert_client_connected!(client);
		assert_client_root!(client);

		let enforce = shell.get_enforce().await.unwrap();
		debug!("enforce = {:}", enforce);
	}

	#[tokio::test]
	async fn test_set_enforce() {
		init_log!();
		let client: AdbClient = client!();
		let shell = client.shell();

		assert_client_connected!(client);
		assert_client_root!(client);

		let enforce1 = shell.get_enforce().await.unwrap();
		debug!("enforce = {:}", enforce1);

		let _result = if enforce1 == SELinuxType::Permissive {
			shell.set_enforce(SELinuxType::Enforcing).await.unwrap();
		} else {
			shell.set_enforce(SELinuxType::Permissive).await.unwrap();
		};
		assert_ne!(enforce1, shell.get_enforce().await.unwrap());
	}

	#[tokio::test]
	async fn test_scan() {
		init_log!();

		let scanner = Scanner::new();
		let start = Instant::now();
		let result = scanner.scan().await;
		let elapsed = start.elapsed();

		debug!("Time elapsed for scanning is: {:?}ms", elapsed.whole_milliseconds());
		debug!("Found {:} devices", result.len());

		for device in result.iter() {
			info!("device: {:}", device);
		}
	}

	#[tokio::test]
	async fn test_bugreport() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let p = dirs::desktop_dir().unwrap().join("bugreport.zip");

		if p.exists() {
			remove_file(p.as_path()).unwrap();
		}

		client.bug_report(Some(p.as_path())).await.unwrap();

		assert_client_unroot!(client);
	}

	#[tokio::test]
	async fn test_mount_unmount() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		client.mount("/system").await.unwrap();
		client.unmount("/system").await.unwrap();

		assert_client_unroot!(client);
	}

	#[tokio::test]
	async fn test_wait_for_device() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		client.wait_for_device(Some(Duration::from_secs(5))).await.unwrap();
		assert_client_connected!(client);
		assert_client_unroot!(client);
	}

	#[tokio::test]
	async fn test_reboot_and_wait() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		client.reboot(None).await.unwrap();
		client.wait_for_device(None).await.unwrap();
		assert_client_connected!(client);
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

	#[tokio::test]
	async fn test_pm_list_packages() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let pm: PackageManager = client.pm();
		let result = pm
			.list_packages(
				Some(ListPackageFilter {
					show_only_disabled: false,
					show_only_enabed: false,
					show_only_system: true,
					show_only3rd_party: false,
					apex_only: false,
					uid: None,
					user: None,
				}),
				Some(ListPackageDisplayOptions::default()),
				Some("google"),
			)
			.await
			.unwrap();

		trace!("result: {:#?}", result);
		assert!(result.len() > 0);

		result.iter().for_each(|p| {
			assert!(p.file_name.is_some());
			assert!(p.version_code.is_some());
			assert!(p.uid.is_some());
		});
	}

	#[tokio::test]
	async fn test_pm_path() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let pm: PackageManager = client.pm();
		let path = pm.path("com.swisscom.aot.library.standalone", None).await.unwrap();
		trace!("path: {:?}", path)
	}

	#[tokio::test]
	async fn test_pm_is_system() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let pm: PackageManager = client.pm();
		let result = pm.is_system("com.swisscom.aot.library.standalone").await.unwrap();
		trace!("result: {:#?}", result)
	}

	#[tokio::test]
	async fn test_pm_get_package_flags() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.webclient";

		let pm: PackageManager = client.pm();
		let result = pm.package_flags(package_name).await.unwrap();
		trace!("result: {:#?}", result);

		let path = pm.path(package_name, None).await.unwrap();
		trace!("path: {:#?}", path);

		if result.contains(&PackageFlags::System) && result.contains(&PackageFlags::UpdatedSystemApp) {
			assert!(!path.starts_with("/system/"))
		} else if result.contains(&PackageFlags::System) {
			assert!(path.starts_with("/system/"))
		} else {
			assert!(path.starts_with("/data/"))
		}
	}

	#[tokio::test]
	async fn test_pm_is_installed() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let pm: PackageManager = client.pm();
		let result = pm.is_installed("com.swisscom.aot.library.standalone", None).await.unwrap();
		trace!("path: {:?}", result);
	}

	#[tokio::test]
	async fn test_pm_install() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let apk = Path::new("/Users/alessandro/Documents/git/swisscom/app-service/app-service/build/outputs/apk/ip2300/release/app-service-ip2300-release.apk");
		assert!(apk.exists());

		let remote_path = "/data/local/tmp";
		let remote_file = format!("{:}/{:}", remote_path, apk.file_name().unwrap().to_str().unwrap());
		client.push(apk, remote_path).await.unwrap();

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
			.await
			.unwrap();

		client.pm().is_installed(package_name, None).await.unwrap();

		trace!("path: {:?}", result);
	}

	#[tokio::test]
	async fn test_pm_uninstall() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let installed = client.pm().is_installed(package_name, None).await.unwrap();

		if !installed {
			warn!("package `{:}` not installed", package_name);
			return;
		}

		assert!(installed);

		let package = client.pm().list_packages(None, None, Some(package_name)).await.unwrap().first().unwrap().to_owned();

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
			.await
			.unwrap();

		assert!(!client.pm().is_installed(package_name, None).await.unwrap());
	}

	#[tokio::test]
	async fn test_pm_runtime_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let result = client.pm().dump_runtime_permissions(package_name).await.unwrap();
		assert!(result.len() > 0);
		trace!("result: {:#?}", result);
	}

	#[tokio::test]
	async fn test_pm_grant_runtime_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().grant(package_name, Some("1000"), "android.permission.ACCESS_FINE_LOCATION").await.unwrap();
		assert!(client
			.pm()
			.dump_runtime_permissions(package_name)
			.await
			.unwrap()
			.iter()
			.any(|p| p.name == "android.permission.ACCESS_FINE_LOCATION"));
	}

	#[tokio::test]
	async fn test_pm_revoke_runtime_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().revoke(package_name, Some("1000"), "android.permission.ACCESS_FINE_LOCATION").await.unwrap();
	}

	#[tokio::test]
	async fn test_pm_disable() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().disable(package_name, Some("1000")).await.unwrap();
	}

	#[tokio::test]
	async fn test_pm_enable() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().enable(package_name, Some("1000")).await.unwrap();
	}

	#[tokio::test]
	async fn test_pm_dump_requested_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let requested_permissions = client.pm().requested_permissions(package_name).await.unwrap();
		assert!(requested_permissions.len() > 0);

		trace!("requested permissions: {:#?}", requested_permissions);
	}

	#[tokio::test]
	async fn test_pm_install_permissions() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let result = client.pm().install_permissions(package_name).await.unwrap();
		assert!(result.len() > 0);
		trace!("result: {:#?}", result);
	}

	#[tokio::test]
	async fn test_pm_operations() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		client.pm().clear(package_name, None).await.unwrap();
	}

	#[tokio::test]
	async fn test_pm_package_reader() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.appservice";
		let dump = client.pm().dump(package_name).await.unwrap();
		let reader = SimplePackageReader::new(dump.as_str()).unwrap();

		let mut result = reader.get_version_name().await.unwrap();
		trace!("version_name: {:#?}", result);

		result = reader.get_first_install_time().await.unwrap();
		trace!("first_install_time: {:#?}", result);

		result = reader.get_last_update_time().await.unwrap();
		trace!("last_update_time: {:#?}", result);

		result = reader.get_timestamp().await.unwrap();
		trace!("timestamp: {:#?}", result);

		result = reader.get_data_dir().await.unwrap();
		trace!("dataDir: {:#?}", result);

		result = reader.get_user_id().await.unwrap();
		trace!("userId: {:#?}", result);

		result = reader.get_code_path().await.unwrap();
		trace!("codePath: {:#?}", result);

		result = reader.get_resource_path().await.unwrap();
		trace!("resourcePath: {:#?}", result);

		let version_code = reader.get_version_code().await.unwrap();
		trace!("versionCode: {:#?}", version_code);
	}

	#[tokio::test]
	async fn test_am_broadcast() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let package_name = "com.swisscom.aot.library.standalone";
		let mut intent = intent!["swisscom.android.tv.action.PRINT_SESSION_INFO"];
		intent.component = Some(format!["{:}/.receiver.PropertiesReceiver", package_name]);
		intent.extra.put_string_extra("swisscom.android.tv.extra.TAG", "SESSION_INFO");
		intent.wait = true;

		trace!("{:}", intent);
		let _result = client.am().broadcast(&intent).await.unwrap();
	}

	#[tokio::test]
	async fn test_am_start() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let mut intent = intent!["android.intent.action.VIEW"];
		intent.data = Some("http://www.google.com".to_string());
		intent.wait = true;

		trace!("{:}", intent);
		let _result = client.am().start(&intent).await.unwrap();
	}

	#[tokio::test]
	async fn test_am_force_stop() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);
		client.am().force_stop("com.swisscom.aot.ui").await.expect("unable to force stop package");
	}

	#[tokio::test]
	async fn test_shell_send_key_event() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_keyevent(KeyCode::KEYCODE_1, Some(KeyEventType::DoubleTap), Some(InputSource::dpad)).await.unwrap();
	}

	#[tokio::test]
	async fn test_shell_send_motion() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_motion(None, MotionEvent::DOWN, (1000, 600)).await.unwrap();
		shell.send_motion(None, MotionEvent::MOVE, (1000, 600)).await.unwrap();
		shell.send_motion(None, MotionEvent::MOVE, (1000, 100)).await.unwrap();
		shell.send_motion(None, MotionEvent::UP, (1000, 100)).await.unwrap();
	}

	#[tokio::test]
	async fn test_shell_send_drag_and_drop() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_draganddrop(None, Some(Duration::from_millis(1500)), (1800, 600), (1700, 100)).await.unwrap();
	}

	#[tokio::test]
	async fn test_shell_send_press() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_press(None).await.unwrap();
	}

	#[tokio::test]
	async fn test_shell_send_keycombination() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_keycombination(None, vec![KeyCode::KEYCODE_1, KeyCode::KEYCODE_3]).await.unwrap();
	}

	#[tokio::test]
	async fn test_shell_send_key_events() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		shell.send_keyevents(vec![KeyCode::KEYCODE_1, KeyCode::KEYCODE_9], Some(InputSource::dpad)).await.unwrap();
	}

	#[tokio::test]
	async fn test_shell_dumpsys() {
		init_log!();
		let client: AdbClient = client!();
		assert_client_connected!(client);
		assert_client_root!(client);

		let shell = client.shell();
		let result = shell.dumpsys(Some("adb"), None, None, true, false, false, None).await.unwrap();

		trace!("result: {:}", result.stdout().to_string_lossy().to_string());
	}

	//
	//#[test]
	//fn test_proto() {
	//    protoc_rust::Codegen::new()
	//        .out_dir("/Users/alessandro/Desktop/swisscom/protobuffers/dst")
	//        .inputs(&["/Users/alessandro/Desktop/swisscom/protobuffers/dump.buffer"])
	//        .include("/Users/alessandro/Desktop/swisscom/protobuffers/")
	//        .run()
	//        .expect("Running protoc failed.");
	//}

	enum ScmuuIdType {
		UUID,
		VerimatrixChipId,
		ChipId,
	}
}
