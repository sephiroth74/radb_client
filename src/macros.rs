#[macro_export]
macro_rules! init_logger {
	($level:expr) => {
		use chrono::Local;
		use env_logger::fmt::Color;
		use log::LevelFilter;
		use std::io::Write;
		use std::path::Path;

		env_logger::builder()
			.filter(Some("radb_client"), $level)
			.filter(Some("stb-utils"), $level)
			.format(|buf, record| {
				let default_style = buf.style();
				let mut dimmed_style = buf.style();

				let mut default_styled_level = buf.default_level_style(record.level());
				let mut level_style = buf.default_level_style(record.level());

				dimmed_style.set_dimmed(true);
				default_styled_level.set_dimmed(true);
				level_style.set_bold(true);

				let msg_style = if record.level() == LevelFilter::Trace { &dimmed_style } else { &default_style };

				//metadata: Metadata<'a>,
				//args: fmt::Arguments<'a>,
				//module_path: Option<MaybeStaticStr<'a>>,
				//file: Option<MaybeStaticStr<'a>>,
				//line: Option<u32>,

				if let (Some(file), Some(line)) = (record.file(), record.line()) {
					writeln!(
						buf,
						"{} {:<5} - [{:}:{:}] {}",
						dimmed_style.value(Local::now().format("%H:%M:%S:%3f")),
						level_style.value(record.level()),
						dimmed_style.value(Path::new(file).file_stem().unwrap().to_str().unwrap()),
						dimmed_style.value(line),
						msg_style.value(record.args())
					)
				} else {
					writeln!(
						buf,
						"{} {:<5} - [{:?}] {}",
						dimmed_style.value(Local::now().format("%H:%M:%S:%3f")),
						level_style.value(record.level()),
						dimmed_style.value(record.module_path()),
						msg_style.value(record.args())
					)
				}
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
