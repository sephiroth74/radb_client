pub(crate) trait CommandDebug {
	fn debug(&mut self) -> &mut Self;
}
//
//impl CommandDebug for tokio::process::Command {
//	fn debug(&mut self) -> &mut Self {
//		let path = Path::new(self.as_std().get_program());
//		let s = self.as_std().get_args().fold(vec![], |mut a: Vec<&OsStr>, b: &OsStr| {
//			a.push(b);
//			a
//		});
//		log!(
//			log::Level::Debug,
//			"Executing `{} {}`...",
//			path.file_name().unwrap().to_str().unwrap(),
//			s.join(OsString::from(" ").as_os_str()).to_str().unwrap().trim()
//		);
//		self
//	}
//}
