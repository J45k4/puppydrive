use crate::types::PeerCmd;

pub struct CmdParser {

}

impl CmdParser {
	pub fn new() -> Self {
		Self {}
	}

	pub fn parse(&self, data: &[u8]) -> anyhow::Result<PeerCmd> {
		todo!()
	}
}