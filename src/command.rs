use std::cell::RefCell;
use std::ffi::OsStr;
use std::fmt::{Debug, Display, Formatter};
use std::num::ParseIntError;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output, Stdio};
use std::time::Duration;

use futures::future::IntoFuture;
use log::{trace, warn};
use string_builder::ToBytes;
use tokio::process::{Child, ChildStdout, Command};
use tokio::signal::unix::SignalKind;
use tokio::sync::oneshot::Receiver;
use tokio::time::error::Elapsed;

use crate::util::Vec8ToString;
use crate::{Adb, AdbDevice};

use super::debug::CommandDebug;

#[allow(dead_code)]
pub struct Error {
    pub status: Option<ExitStatus>,
    pub msg: Vec<u8>,
}

impl std::error::Error for Error {}

impl Error {
    pub fn from(msg: &str) -> Self {
        Error {
            status: None,
            msg: msg.to_owned().to_bytes(),
        }
    }

    pub fn from_err(status: ExitStatus, msg: Vec<u8>) -> Self {
        Error {
            status: Some(status),
            msg,
        }
    }

    pub fn exit_code(&self) -> Option<i32> {
        match self.status {
            Some(s) => s.code(),
            None => None,
        }
    }

    pub fn exit_signal(&self) -> Option<i32> {
        match self.status {
            None => None,
            Some(s) => s.signal(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.msg.as_str() {
            None => {
                write!(f, "code: {:?}, msg: unknown error", self.status)
            }
            Some(s) => {
                write!(f, "code: {:?}, msg: {:?}", self.status, s)
            }
        }
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.msg.as_str() {
            None => {
                write!(f, "code: {:?}, msg: unknown error", self.status)
            }
            Some(s) => {
                write!(f, "code: {:?}, msg: {:?}", self.status, s)
            }
        }
    }
}

impl From<&which::Error> for Error {
    fn from(value: &which::Error) -> Self {
        Error {
            status: None,
            msg: value.to_string().to_bytes(),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error {
            status: None,
            msg: value.to_string().to_bytes(),
        }
    }
}

impl From<ParseIntError> for Error {
    fn from(value: ParseIntError) -> Self {
        Error {
            status: None,
            msg: value.to_string().to_bytes(),
        }
    }
}

impl From<Elapsed> for Error {
    fn from(value: Elapsed) -> Self {
        Error {
            status: None,
            msg: value.to_string().to_bytes(),
        }
    }
}

impl From<nom::Err<nom::error::Error<&[u8]>>> for Error {
    fn from(value: nom::Err<nom::error::Error<&[u8]>>) -> Self {
        Error {
            status: None,
            msg: value.to_string().to_bytes(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct ProcessResult {
    output: Box<Output>,
}

impl From<Output> for ProcessResult {
    fn from(value: Output) -> Self {
        ProcessResult {
            output: Box::new(value),
        }
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
        self.signal()
            .map(|s| SignalKind::from_raw(s) == SignalKind::interrupt())
            .unwrap_or(false)
    }

    pub fn is_kill(&self) -> bool {
        self.signal().map(|s| s == 9).unwrap_or(false)
    }

    fn try_from(value: std::io::Result<Output>) -> Result<Self> {
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
                        Err(Error::from_err(output.status, output.stderr))
                    }
                } else {
                    Err(Error::from_err(output.status, output.stderr))
                }
            }

            Err(e) => Err(Into::into(e)),
        }
    }
}

pub trait OutputResult {
    fn to_result(&self) -> Result<Vec<u8>>;
    fn try_to_result(&self) -> Result<Vec<u8>>;
}

impl OutputResult for Output {
    fn to_result(&self) -> Result<Vec<u8>> {
        if self.status.success() && self.stderr.is_empty() {
            Ok(self.stdout.to_owned())
        } else {
            Err(Error {
                status: Some(self.status.to_owned()),
                msg: self.stderr.to_owned(),
            })
        }
    }

    fn try_to_result(&self) -> Result<Vec<u8>> {
        warn!("status: {}", self.status);
        warn!("signal: {:?}", self.status.signal());
        warn!("code: {:?}", self.status.code());
        warn!("success: {:?}", self.status.success());

        if self.status.code().is_none() && self.stderr.is_empty() {
            Ok(self.stdout.to_owned())
        } else {
            Err(Error {
                status: Some(self.status.to_owned()),
                msg: self.stderr.to_owned(),
            })
        }
    }
}

#[derive(Debug)]
pub struct CommandBuilder {
    debug: bool,
    command: RefCell<Command>,
    timeout: Option<Duration>,
    signal: Option<IntoFuture<Receiver<()>>>,
}

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
        builder.command.borrow_mut().args(device.into().args());
        builder.command.borrow_mut().arg("shell");
        builder
    }

    pub async fn pipe(cmd1: &mut CommandBuilder, cmd2: &mut CommandBuilder) -> std::io::Result<Child> {
        let child1 = cmd1.spawn().await?;
        let out: ChildStdout = child1
            .stdout
            .ok_or(std::io::Error::new(std::io::ErrorKind::InvalidData, "child stdout unavailable"))?;
        let fd: Stdio = out.try_into()?;
        cmd2.stdin(fd);
        cmd2.spawn().await
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

    pub async fn output(&mut self) -> Result<ProcessResult> {
        let mut child = self.spawn().await?;
        let has_signal = self.signal.is_some();
        let has_timeout = self.timeout.is_some();

        let sleep = self.timeout.map(tokio::time::sleep);
        tokio::select! {
            _ = (conditional_signal(self.signal.as_mut())), if has_signal => {
                trace!("Ctrl+c received");
                child.kill().await?
            },
            _ = child.wait() => {
                //trace!("Child exited normally")
            },
            _ = (conditional_sleeper(sleep)), if has_timeout => {
                trace!("Timeout expired!");
                child.kill().await?
            },
        }

        let output = child.wait_with_output().await;
        ProcessResult::try_from(output)
    }

    pub async fn spawn(&mut self) -> std::io::Result<Child> {
        if self.debug {
            self.command.borrow_mut().debug().spawn()
        } else {
            self.command.borrow_mut().spawn()
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
