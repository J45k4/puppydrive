use std::cell::RefCell;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::rc::Rc;

pub type SharedState = Rc<RefCell<State>>;

#[derive(Debug, Default)]
pub struct State {
	pub me: Peer,
	pub peers: Vec<Peer>,
	pub binds: Vec<SocketAddr>,
}

impl State {
	pub fn new_shared() -> Rc<RefCell<State>> {
		Rc::new(RefCell::new(State::default()))
	}

    pub fn get_peer_with_addr(&mut self, addr: &str) -> &mut Peer {
		let pos = self.peers.iter().position(|peer| peer.addr.as_deref() == Some(addr));
		match pos {
			Some(pos) => self.peers.get_mut(pos).unwrap(),
			None => {
				let peer = Peer {
					addr: Some(addr.to_string()),
					..Default::default()
				};
				self.peers.push(peer);
				self.peers.last_mut().unwrap()
			}
		}
	}
}

#[derive(Debug, Default)]
pub struct Peer {
	pub id: Option<String>,
	pub name: Option<String>,
	pub owner: Option<String>,
    pub addr: Option<String>
}

#[derive(Debug)]
pub enum NodeStatus {
	Online,
	Offline,
}

impl ToString for NodeStatus {
	fn to_string(&self) -> String {
		match self {
			NodeStatus::Online => "Online".to_string(),
			NodeStatus::Offline => "Offline".to_string(),
		}
	}
}

#[derive(Debug)]
pub struct Node {
	pub id: String,
	pub name: String,
	pub traffic: u32,
	pub status: NodeStatus,
	pub addr: Option<String>
}

pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub hash: Option<String>,
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
	Introduce {
		name: String,
		owner: String,
	}
}

#[derive(Debug)]
pub struct PeerReq {
    pub id: String,
    pub cmd: PeerCmd,
}

#[derive(Debug)]
pub enum NodeCmdRes {
    ReadFile {
        data: Vec<u8>,
    },
    WriteFile {
        success: bool,
    },
    RemoveFile {
        success: bool,
    },
    CreateFolder {
        success: bool,
    },
    RenameFolder {
        success: bool,
    },
    RemoveFolder {
        success: bool,
    },
    ListFolderContents {
        contents: Vec<String>,
    }
}

#[derive(Debug)]
pub struct PeerRes {
    pub id: String,
    pub res: NodeCmdRes,
}

#[derive(Debug)]
pub enum PeerMsg {
    Cmd(PeerCmd),
    Res(NodeCmdRes)
}

pub enum Event {
	PeerConnected{
		addr: String
	},
	PeerDisconnected {
		addr: String
	},
	PeerData {
		addr: String,
		data: Vec<u8>
	},
	ConnectFailed {
		addr: String,
		err: anyhow::Error,
	},
}

pub enum PeerConnCmd {
	Close,
	Send(Vec<u8>)
}