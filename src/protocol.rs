use crate::types::PeerCmd;

pub const INTRODUCE_CMD: u16 = 1;
pub const WRITE_FILE_CMD: u16 = 2;
pub const READ_FILE_CMD: u16 = 3;

pub struct PupynetProtocol {
	buffer: Vec<u8>,
	cmds: Vec<PeerCmd>,
}

impl PupynetProtocol {
	pub fn new() -> Self {
		Self {
			buffer: Vec::new(),
			cmds: Vec::new(),
		}
	}

	fn parse_cmd(&mut self, cmd: u16, len: u32, data: &[u8]) {
		match cmd {
			INTRODUCE_CMD => {}
			WRITE_FILE_CMD => {}
			READ_FILE_CMD => {}
			_ => {}
		}
	}

	pub fn parse(&mut self, data: &[u8]) {
		if data.len() < 6 {
			self.buffer.extend_from_slice(data);
		}

		let cmd = u16::from_le_bytes(self.buffer[0..2].try_into().unwrap());
		let data_len = u32::from_le_bytes(self.buffer[2..6].try_into().unwrap()) as usize;
		if data.len() < data_len + 6 {
			self.buffer.extend_from_slice(data);
			return;
		}

		self.buffer.extend_from_slice(data);
	}

	pub fn next(&mut self) -> Option<PeerCmd> {
		None
	}
}