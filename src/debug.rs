use log::log;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;
use tokio::process::Command;

#[macro_export(local_inner_macros)]
macro_rules! debug_output {
	($output:expr, $elapsed:expr) => {
		if log::log_enabled!(log::Level::Trace) {
			log::trace!("output: {:?} in {:?}", $output, $elapsed);
		}
	};
}

pub trait CommandDebug {
	fn debug(&mut self) -> &mut Self;
}

impl CommandDebug for Command {
	fn debug(&mut self) -> &mut Self {
		let path = Path::new(self.as_std().get_program());
		let s = self.as_std().get_args().fold(vec![], |mut a: Vec<&OsStr>, b: &OsStr| {
			a.push(b);
			a
		});
		log!(
			log::Level::Trace,
			"Executing `{} {}`...",
			path.file_name().unwrap().to_str().unwrap(),
			s.join(OsString::from(" ").as_os_str()).to_str().unwrap().trim()
		);
		self
	}
}

impl CommandDebug for std::process::Command {
	fn debug(&mut self) -> &mut Self {
		let path = Path::new(self.get_program());
		let s = self.get_args().fold(vec![], |mut a: Vec<&OsStr>, b: &OsStr| {
			a.push(b);
			a
		});
		log!(
			log::Level::Debug,
			"Executing `{} {}`...",
			path.file_name().unwrap().to_str().unwrap(),
			s.join(OsString::from(" ").as_os_str()).to_str().unwrap().trim()
		);
		self
	}
}
