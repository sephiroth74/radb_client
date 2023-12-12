use crate::intent::Intent;
use crate::types::AdbShell;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityManager<'a> {
	pub(crate) parent: AdbShell<'a>,
}

impl<'a> ActivityManager<'a> {
	pub async fn broadcast(&self, intent: &Intent) -> crate::command::Result<()> {
		self.parent.broadcast(intent).await
	}

	pub async fn start(&self, intent: &Intent) -> crate::command::Result<()> {
		self.parent.start(intent).await
	}

	pub async fn start_service(&self, intent: &Intent) -> crate::command::Result<()> {
		self.parent.start_service(intent).await
	}

	pub async fn force_stop(&self, package_name: &str) -> crate::command::Result<()> {
		self.parent.force_stop(package_name).await
	}
}
