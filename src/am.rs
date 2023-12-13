use crate::types::Intent;
use crate::ActivityManager;

impl<'a> ActivityManager<'a> {
	pub async fn broadcast(&self, intent: &Intent) -> crate::process::Result<()> {
		self.parent.broadcast(intent).await
	}

	pub async fn start(&self, intent: &Intent) -> crate::process::Result<()> {
		self.parent.start(intent).await
	}

	pub async fn start_service(&self, intent: &Intent) -> crate::process::Result<()> {
		self.parent.start_service(intent).await
	}

	pub async fn force_stop(&self, package_name: &str) -> crate::process::Result<()> {
		self.parent.force_stop(package_name).await
	}
}
