#[cfg(test)]
pub(crate) mod test {
	use std::env::current_exe;
	use std::path::{Path, PathBuf};
	use std::sync::{Arc, Mutex, Once};
	use std::time::Duration;
	use std::{io, thread};

	use cmd_lib::AsOsStr;
	use crossbeam_channel::{bounded, Receiver};
	use lazy_static::lazy_static;
	use once_cell::sync::Lazy;
	use regex::Regex;
	use signal_hook::consts::SIGINT;
	use signal_hook::iterator::Signals;
	use tracing::level_filters::LevelFilter;
	use tracing_appender::non_blocking::WorkerGuard;
	use tracing_subscriber::EnvFilter;

	use crate::types::{Adb, Client, ConnectionType};

	pub(crate) static DEVICE_IP: &'static str = "192.168.1.50:5555";
	pub(crate) static TRANSPORT_ID: u8 = 4;

	pub(crate) static INIT: Once = Once::new();
	pub(crate) static GUARDS: Lazy<Arc<Mutex<Vec<WorkerGuard>>>> = Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

	pub(crate) fn init_log() {
		INIT.call_once(|| {
			use tracing_subscriber::prelude::*;

			let registry = tracing_subscriber::Registry::default();
			let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());
			let layer1 = tracing_subscriber::fmt::layer()
				.with_thread_names(false)
				.with_thread_ids(false)
				.with_line_number(false)
				.with_file(false)
				.with_target(true)
				.with_level(false)
				.without_time()
				.with_writer(non_blocking);

			let filter = EnvFilter::builder()
				.with_default_directive(LevelFilter::TRACE.into())
				.from_env()
				.unwrap()
				.add_directive("simple_cmd=trace".parse().unwrap())
				.add_directive("hyper::proto=warn".parse().unwrap())
				.add_directive("hyper::client=warn".parse().unwrap());

			let subscriber = registry.with(layer1).with(filter);
			tracing::subscriber::set_global_default(subscriber).unwrap();
			GUARDS.lock().unwrap().push(guard);
		})
	}

	#[inline]
	pub(crate) fn new_adb() -> Adb {
		Adb::new().expect("failed to find adb in PATH")
	}

	#[inline]
	pub(crate) fn connection_from_tcpip() -> ConnectionType {
		ConnectionType::try_from_ip(DEVICE_IP).expect("failed to parse ip address")
	}

	#[inline]
	#[allow(dead_code)]
	pub(crate) fn connection_from_transport_id() -> ConnectionType {
		ConnectionType::Transport(TRANSPORT_ID)
	}

	#[inline]
	#[allow(dead_code)]
	pub(crate) fn connection_from_usb() -> ConnectionType {
		ConnectionType::USB
	}

	#[inline]
	pub(crate) fn client_from(connection_type: ConnectionType) -> Client {
		Client::try_from(connection_type)
			.expect("Failed to create Client")
			.with_debug(true)
	}

	#[inline]
	pub(crate) fn connect_emulator() -> Client {
		lazy_static! {
			static ref RE: Regex = Regex::new(r#"^emulator-*.+$"#).unwrap();
		}
		let devices = new_adb().list_devices(true).expect("failed to list devices");
		let device = devices
			.iter()
			.find(|device| {
				println!("Checking {device}...");
				RE.is_match(&device.name)
			})
			.expect("no emulator found");
		Client::try_from(device)
			.expect("failed to create client from device")
			.with_debug(true)
	}

	#[inline]
	pub(crate) fn connect_client(connection_type: ConnectionType) -> Client {
		let client = client_from(connection_type);
		let _result = match connection_type {
			ConnectionType::TcpIp(_) => client.connect(None),
			ConnectionType::Transport(_) => Ok(()),
			ConnectionType::USB => Ok(()),
		}
		.expect("failed to connect to client");
		client
	}

	#[inline]
	pub(crate) fn connect_tcp_ip_client() -> Client {
		connect_client(connection_from_tcpip())
	}

	#[inline]
	pub(crate) fn root_client(client: &Client) {
		client.root().expect("failed to root client");
	}

	#[inline]
	#[allow(dead_code)]
	pub(crate) fn reboot_and_wait_for_client(client: &Client) {
		client.reboot(None).expect("failed to send reboot command");
		client
			.wait_for_device(Some(Duration::from_secs(180)))
			.expect("failed to wait for device");
	}

	// Creates a channel that gets a message every time `SIGINT` is signalled.
	pub(crate) fn sigint_notifier() -> io::Result<Receiver<()>> {
		let (s, r) = bounded(1);
		let mut signals = Signals::new(&[SIGINT])?;

		std::thread::spawn(move || {
			for _ in signals.forever() {
				if s.send(()).is_err() {
					break;
				}
			}
		});
		Ok(r)
	}

	pub(crate) fn ctrl_channel() -> Result<Receiver<()>, ctrlc::Error> {
		let (sender, receiver) = bounded(1);
		ctrlc::set_handler(move || {
			println!("sending CTRL+C to ctrl_channel");
			let _ = sender.send(());
		})?;
		Ok(receiver)
	}

	pub(crate) fn temp_dir() -> PathBuf {
		lazy_static! {
			static ref TEMP_PATH: String = uuid::Uuid::new_v4().to_string();
		}
		let dir = std::env::temp_dir().join(Path::new(&TEMP_PATH.as_os_str()));
		std::fs::create_dir_all(dir.as_path()).expect("failed to mkdirs");
		dir
	}

	pub(crate) fn test_files_dir() -> PathBuf {
		let cur_exe = current_exe().unwrap();
		let cur_dir = cur_exe.parent().unwrap();
		cur_dir
			.parent()
			.unwrap()
			.parent()
			.unwrap()
			.parent()
			.unwrap()
			.join("test_files")
	}

	#[allow(dead_code)]
	pub fn ctrl_c_receiver() -> io::Result<Receiver<()>> {
		let (s, r) = bounded(1);
		let mut signals = Signals::new(&[SIGINT])?;

		thread::spawn(move || {
			for _ in signals.forever() {
				println!("Ctrl+c pressed!");
				let _ = s.send(());
				break;
			}
			drop(s);
		});
		Ok(r)
	}
}
