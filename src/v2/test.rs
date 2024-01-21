#[cfg(test)]
pub(crate) mod test {
	use std::sync::{Arc, Mutex, Once};

	use once_cell::sync::Lazy;
	use tracing_appender::non_blocking::WorkerGuard;

	use crate::v2::types::{Client, ConnectionType};

	pub(crate) static DEVICE_IP: &'static str = "192.168.1.42:5555";
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
				.with_target(false)
				.with_level(false)
				.without_time()
				.with_writer(non_blocking);

			let subscriber = registry.with(layer1);
			tracing::subscriber::set_global_default(subscriber).unwrap();
			GUARDS.lock().unwrap().push(guard);
		})
	}

	#[inline]
	pub(crate) fn connection_from_tcpip() -> ConnectionType {
		ConnectionType::try_from_ip(DEVICE_IP).expect("failed to parse ip address")
	}

	#[inline]
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
}
