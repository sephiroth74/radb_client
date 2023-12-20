use crate::types::Intent;
use crate::ActivityManager;

impl<'a> ActivityManager<'a> {
	pub fn broadcast(&self, intent: &Intent) -> crate::Result<()> {
		self.parent.broadcast(intent)
	}

	pub fn start(&self, intent: &Intent) -> crate::Result<()> {
		self.parent.start(intent)
	}

	pub fn start_service(&self, intent: &Intent) -> crate::Result<()> {
		self.parent.start_service(intent)
	}

	pub fn force_stop(&self, package_name: &str) -> crate::Result<()> {
		self.parent.force_stop(package_name)
	}
}
