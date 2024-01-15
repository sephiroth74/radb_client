#[macro_export]
macro_rules! intent {
	($action:expr) => {{
		crate::types::Intent::from_action($action)
	}};
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
pub use intent;
