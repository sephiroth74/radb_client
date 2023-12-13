#[macro_export]
macro_rules! init_logger {
	() => {
		use chrono::Local;
		use env_logger::fmt::Color;
		use std::io::Write;

		env_logger::builder()
			.default_format()
			.format(|buf, record| {
				let mut buf_style = buf.style();
				let default_styled_level = buf.default_level_style(record.level());

				buf_style.set_color(Color::Ansi256(8)).set_dimmed(true).set_intense(false);

				writeln!(
					buf,
					"{}{} {:>5}{} - {}",
					buf_style.value("["),
					default_styled_level.value(Local::now().format("%H:%M:%S:%3f")),
					buf.default_styled_level(record.level()),
					buf_style.value("]"),
					default_styled_level.value(record.args())
				)
			})
			.init();
	};
}

#[macro_export]
macro_rules! intent {
	($action:expr) => {
		crate::types::Intent::from_action($action)
	};
}

#[macro_export(local_inner_macros)]
macro_rules! debug_output {
	($output:expr, $elapsed:expr) => {
		if log::log_enabled!(log::Level::Trace) {
			log::trace!("output: {:?} in {:?}", $output, $elapsed);
		}
	};
}

#[allow(unused_imports)]
pub use init_logger;

#[allow(unused_imports)]
pub use intent;
