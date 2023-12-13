use crate::errors::AdbError;
use futures::future::IntoFuture;
use std::cell::RefCell;
use std::process::Output;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::oneshot::Receiver;

mod impls;

pub type Result<T> = std::result::Result<T, AdbError>;

#[derive(Debug)]
pub struct ProcessResult {
	pub(crate) output: Box<Output>,
}

#[derive(Debug)]
pub struct CommandBuilder {
	pub(crate) debug: bool,
	pub(crate) command: RefCell<Command>,
	pub(crate) timeout: Option<Duration>,
	pub(crate) signal: Option<IntoFuture<Receiver<()>>>,
}

pub trait OutputResult {
	fn to_result(&self) -> Result<Vec<u8>>;
	fn try_to_result(&self) -> Result<Vec<u8>>;
}
