use crate::types::Event;
use crate::types::PeerCmd;

pub trait Pupynet {
	fn connect(&mut self, addr: &str) -> anyhow::Result<()>;
	fn bind(&mut self, addr: &str) -> anyhow::Result<()>;
	fn send(&mut self, addr: &str, cmd: PeerCmd) -> anyhow::Result<()>;
	async fn wait(&mut self) -> Option<Event>;	
	fn poll(&mut self, timeout: std::time::Duration) -> Option<Event>;
}

pub struct PupynetImpl {
}

impl PupynetImpl {
	pub fn new() -> Self {
		Self {}
	}
}

impl Pupynet for PupynetImpl {
	fn connect(&mut self, addr: &str) -> anyhow::Result<()> {
		todo!()
	}

	fn bind(&mut self, addr: &str) -> anyhow::Result<()> {
		todo!()
	}

	fn send(&mut self, addr: &str, cmd: PeerCmd) -> anyhow::Result<()> {
		todo!()
	}

	async fn wait(&mut self) -> Option<Event> {
		todo!()
	}

	fn poll(&mut self, timeout: std::time::Duration) -> Option<Event> {
		todo!()
	}
}