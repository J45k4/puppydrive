use std::collections::VecDeque;

pub const INTRODUCE_CMD: u16 = 1;
pub const WRITE_FILE_CMD: u16 = 2;
pub const READ_FILE_CMD: u16 = 3;
pub const REMOVE_FILE_CMD: u16 = 4;
pub const CREATE_FOLDER_CMD: u16 = 5;
pub const RENAME_FOLDER_CMD: u16 = 6;
pub const LIST_FOLDER_CONTENTS_CMD: u16 = 7;

#[derive(Debug)]
pub struct Introduce {
	pub id: String,
	pub name: String,
	pub owner: String,
}

#[derive(Debug)]
pub enum PeerCmd {
    ReadFile {
        node_id: String,
        path: String,
        offset: u64,
        length: u64,
    },
    WriteFile {
        node_id: String,
        path: String,
        offset: u64,
        data: Vec<u8>,
    },
    RemoveFile {
        node_id: String,
        path: String,
    },
    CreateFolder {
        node_id: String,
        path: String,
    },
    RenameFolder {
        node_id: String,
        path: String,
        new_name: String,
    },
    RemoveFolder {
        node_id: String,
        path: String,
    },
    ListFolderContents {
        node_id: String,
        path: String,
        offset: u64,
        length: u64,
        recursive: bool,
    },
	Introduce(Introduce),
	Hello
}

impl PeerCmd {
	pub fn serialize(&self) -> Vec<u8> {
		match self {
			PeerCmd::Introduce(args) => {
				let id_len_bytes = (args.id.len() as u16).to_le_bytes();
				let id_bytes = args.id.as_bytes();
				let name_len_bytes = (args.name.len() as u16).to_le_bytes();
				let name_bytes = args.name.as_bytes();
				let owner_len_bytes = (args.owner.len() as u16).to_le_bytes();
				let owner_bytes = args.owner.as_bytes();
				let payload_size = 2 + id_bytes.len() + 2 + name_bytes.len() + 2 + owner_bytes.len();
				let mut res = Vec::with_capacity(6 + payload_size);
				res.extend_from_slice(&INTRODUCE_CMD.to_le_bytes());
				res.extend_from_slice(&(payload_size as u32).to_le_bytes());
				res.extend_from_slice(&id_len_bytes);
				res.extend_from_slice(id_bytes);
				res.extend_from_slice(&name_len_bytes);
				res.extend_from_slice(name_bytes);
				res.extend_from_slice(&owner_len_bytes);
				res.extend_from_slice(owner_bytes);
				res
			}
			_ => todo!()
		}
	}
}

struct ByteEater<'a> {
	buffer: &'a [u8]
}

impl<'a> ByteEater<'a> {
	pub fn new(buffer: &'a [u8]) -> Self {
		Self {
			buffer
		}
	}
	
	pub fn get_u16(&mut self) -> u16 {
		u16::from_le_bytes(self.buffer[0..2].try_into().unwrap())
	}

	pub fn get_string(&mut self) -> String {
		let len = self.get_u16();
		let s = self.buffer[2..len as usize + 2].to_vec();
		self.buffer = &self.buffer[len as usize + 2..];
		String::from_utf8(s).unwrap()
	}
}

pub struct PupynetProtocol {
	buffer: Vec<u8>,
	cmds: VecDeque<PeerCmd>,
}

impl PupynetProtocol {
	pub fn new() -> Self {
		Self {
			buffer: Vec::new(),
			cmds: VecDeque::new(),
		}
	}

	pub fn parse(&mut self, data: &[u8]) {
		self.buffer.extend_from_slice(data);
		if self.buffer.len() < 6 {
			return;
		}
		let cmd = u16::from_le_bytes(self.buffer[0..2].try_into().unwrap());
		let len = u32::from_le_bytes(self.buffer[2..6].try_into().unwrap()) as u32;
		log::info!("received cmd: {}, len: {}", cmd, len);
		if self.buffer.len() < len as usize + 6 {
			return;
		}
		let mut eater = ByteEater::new(&self.buffer[6..]);
		match cmd {
			INTRODUCE_CMD => {
				log::info!("INTRODUCE_CMD");
				let id = eater.get_string();
				let name = eater.get_string();
				let owner = eater.get_string();
				self.cmds.push_back(PeerCmd::Introduce(
					Introduce {
						id,
						name,
						owner,
					}
				));
			}
			WRITE_FILE_CMD => {}
			READ_FILE_CMD => {}
			REMOVE_FILE_CMD => {}
			_ => {}
		}
	}

	pub fn next(&mut self) -> Option<PeerCmd> {
		self.cmds.pop_front()
	}
}

