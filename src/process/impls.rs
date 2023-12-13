use crate::debug::CommandDebug;
use crate::errors::AdbError::CmdError;
use crate::errors::CommandError;
use crate::process::{CommandBuilder, OutputResult, ProcessResult};
use crate::traits::AdbDevice;
use crate::{process, Adb};
use futures::future::IntoFuture;
use log::{trace, warn};
use std::cell::RefCell;
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::os::unix::prelude::ExitStatusExt;
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::process::{Child, ChildStdout, Command};
use tokio::signal::unix::SignalKind;
use tokio::sync::oneshot::Receiver;

impl From<&Adb> for CommandBuilder {
	fn from(adb: &Adb) -> Self {
		let builder = CommandBuilder::new(adb.as_os_str());
		builder
	}
}

impl Display for CommandBuilder {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", self.command.borrow())
	}
}

#[allow(dead_code)]
impl<'a> CommandBuilder {
	pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
		let mut command = Command::new(program);
		command.stdin(Stdio::piped());
		command.stdout(Stdio::piped());
		command.stderr(Stdio::piped());
		CommandBuilder {
			command: RefCell::new(command),
			timeout: None,
			signal: None,
			debug: true,
		}
	}

	pub fn with_debug(&mut self, debug: bool) -> &mut Self {
		self.debug = debug;
		self
	}

	pub fn device<'b, T>(adb: &Adb, device: T) -> Self
	where
		T: Into<&'b dyn AdbDevice>,
	{
		let builder = CommandBuilder::new(adb.as_os_str());
		builder.command.borrow_mut().args(device.into().args());
		builder
	}

	pub fn shell<'b, T>(adb: &Adb, device: T) -> Self
	where
		T: Into<&'b dyn AdbDevice>,
	{
		let builder = CommandBuilder::new(adb);
		builder.command.borrow_mut().args(device.into().args()).arg("shell");
		builder
	}

	pub async fn pipe(cmd1: &mut CommandBuilder, cmd2: &mut CommandBuilder) -> std::io::Result<Child> {
		let child1 = cmd1.spawn().await?;
		let out: ChildStdout = child1.stdout.ok_or(std::io::Error::new(std::io::ErrorKind::InvalidData, "child stdout unavailable"))?;
		let fd: Stdio = out.try_into()?;
		cmd2.stdin(fd);
		cmd2.spawn().await
	}

	pub async fn pipe_with_timeout(cmd1: &mut CommandBuilder, cmd2: &mut CommandBuilder, timeout: Duration) -> std::io::Result<Output> {
		let child1 = cmd1.spawn().await?;
		let out: ChildStdout = child1.stdout.ok_or(std::io::Error::new(std::io::ErrorKind::InvalidData, "child stdout unavailable"))?;
		let fd: Stdio = out.try_into()?;

		cmd2.stdin(fd);
		let mut child2 = cmd2.spawn().await?;

		if let Err(_) = tokio::time::timeout(timeout, child2.wait()).await {
			trace!("Got timeout!");
			let _ = child2.start_kill().unwrap();
		} else {
			trace!("No timeout");
			let _ = child2.wait();
		}

		child2.wait_with_output().await
	}

	pub fn with_timeout(&mut self, duration: Option<Duration>) -> &mut Self {
		self.timeout = duration;
		self
	}

	pub fn with_signal(&mut self, signal: Option<IntoFuture<Receiver<()>>>) -> &mut Self {
		self.signal = signal;
		self
	}

	pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
		self.command.borrow_mut().arg(arg);
		self
	}

	pub fn args<I, S>(&mut self, args: I) -> &mut Self
	where
		I: IntoIterator<Item = S>,
		S: AsRef<OsStr>,
	{
		self.command.borrow_mut().args(args);
		self
	}

	pub fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
		self.command.borrow_mut().stdout(cfg);
		self
	}

	pub fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
		self.command.borrow_mut().stdin(cfg);
		self
	}

	pub async fn output(&mut self) -> process::Result<ProcessResult> {
		let mut child = self.spawn().await?;
		let has_signal = self.signal.is_some();
		let has_timeout = self.timeout.is_some();
		let sleep = self.timeout.map(tokio::time::sleep);

		if has_signal || has_timeout || sleep.is_some() {
			tokio::select! {
				_ = (conditional_signal(self.signal.as_mut())), if has_signal => {
					trace!("Ctrl+c received");
					let _ = child.start_kill();
					//let _ = child.kill().await;
				},
				_ = (conditional_sleeper(sleep)), if has_timeout => {
					trace!("Timeout expired!");
					let _ = child.start_kill();
					//let _ = child.kill().await;
				},
				_ = child.wait() => {
					//trace!("Child exited normally")
				},
			}
		}

		let output = child.wait_with_output().await;
		ProcessResult::try_from(output)
	}

	pub async fn spawn(&mut self) -> std::io::Result<Child> {
		if self.debug {
			self.command.borrow_mut().kill_on_drop(true).debug().spawn()
		} else {
			self.command.borrow_mut().kill_on_drop(true).spawn()
		}
	}

	//
	//pub async fn process(&mut self) {
	//    self.command.borrow_mut().kill_on_drop(true);
	//    //let output = self.command.borrow_mut().debug().output().await.unwrap();
	//    let mut child = self.spawn();
	//    let stdout = child.stdout.take().expect("child did not have a handle to stdout");
	//
	//    let mut reader = FramedRead::new(stdout, LinesCodec::new());
	//
	//    // Ensure the child process is spawned in the runtime so it can
	//    // make progress on its own while we await for any output.
	//    tokio::spawn(async {
	//        let status = child.await
	//            .expect("child process encountered an error");
	//
	//        println!("child status was: {}", status);
	//    });
	//
	//    while let Some(line) = reader.next().await {
	//        println!("Line: {}", line?);
	//    }
	//}
}

impl CommandDebug for CommandBuilder {
	fn debug(&mut self) -> &mut Self {
		self.command.borrow_mut().debug();
		self
	}
}

impl From<Output> for ProcessResult {
	fn from(value: Output) -> Self {
		ProcessResult { output: Box::new(value) }
	}
}

impl Display for ProcessResult {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:#?}", self.output)
	}
}

impl ProcessResult {
	pub fn stdout(&self) -> Vec<u8> {
		self.output.stdout.to_owned()
	}

	pub fn stderr(&self) -> Vec<u8> {
		self.output.stderr.to_owned()
	}

	pub fn has_stderr(&self) -> bool {
		!self.output.stderr.is_empty()
	}

	pub fn has_stdout(&self) -> bool {
		!self.output.stdout.is_empty()
	}

	pub fn is_error(&self) -> bool {
		!self.output.status.success()
	}

	pub fn signal(&self) -> Option<i32> {
		self.output.status.signal()
	}

	pub fn has_signal(&self) -> bool {
		self.output.status.signal().is_some()
	}

	pub fn is_success(&self) -> bool {
		self.output.status.success()
	}

	pub fn is_interrupt(&self) -> bool {
		self.signal().map(|s| SignalKind::from_raw(s) == SignalKind::interrupt()).unwrap_or(false)
	}

	pub fn is_kill(&self) -> bool {
		self.signal().map(|s| s == 9).unwrap_or(false)
	}

	fn try_from(value: std::io::Result<Output>) -> process::Result<Self> {
		match value {
			Ok(output) => {
				if output.status.success() {
					Ok(output.into())
				} else if output.status.signal().is_some() {
					let signal = SignalKind::from_raw(output.status.signal().unwrap());

					if signal == SignalKind::interrupt() {
						trace!("SIGINT(2)");
						Ok(output.into())
					} else if signal == SignalKind::from_raw(9) {
						trace!("SIGKILL(9)");
						Ok(output.into())
					} else {
						Err(CmdError(CommandError::from_err(output.status, output.stdout, output.stderr)))
					}
				} else {
					Err(CmdError(CommandError::from_err(output.status, output.stdout, output.stderr)))
				}
			}

			Err(e) => Err(Into::into(e)),
		}
	}
}

impl OutputResult for Output {
	fn to_result(&self) -> process::Result<Vec<u8>> {
		if self.status.success() && self.stderr.is_empty() {
			Ok(self.stdout.to_owned())
		} else {
			Err(CmdError(CommandError::from_err(self.status, self.stdout.to_owned(), self.stderr.to_owned())))
		}
	}

	fn try_to_result(&self) -> process::Result<Vec<u8>> {
		warn!("status: {}", self.status);
		warn!("signal: {:?}", self.status.signal());
		warn!("code: {:?}", self.status.code());
		warn!("success: {:?}", self.status.success());

		if self.status.code().is_none() && self.stderr.is_empty() {
			Ok(self.stdout.to_owned())
		} else {
			Err(CmdError(CommandError::from_err(self.status, self.stdout.to_owned(), self.stderr.to_owned())))
		}
	}
}

async fn conditional_sleeper(t: Option<tokio::time::Sleep>) -> Option<()> {
	match t {
		Some(timer) => {
			timer.await;
			Some(())
		}
		None => None,
	}
}

async fn conditional_signal(t: Option<&mut IntoFuture<Receiver<()>>>) -> Option<()> {
	match t {
		Some(timer) => {
			timer.await.unwrap();
			Some(())
		}
		None => None,
	}
}
