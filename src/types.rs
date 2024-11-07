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
}

pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub hash: Option<String>,
}

#[derive(Debug)]
pub enum NodeCmd {
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
    }
}

#[derive(Debug)]
pub struct NodeMessageReq {
    pub id: String,
    pub cmd: NodeCmd,
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
pub struct NodeMessageRes {
    pub id: String,
    pub res: NodeCmdRes,
}