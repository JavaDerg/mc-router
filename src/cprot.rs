use std::net::SocketAddr;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum Request {
	Echo,
	SetMapping(String, SocketAddr),
	GetMapping(String),
	RmMapping(String, bool),
	LsMappings,
	MkListener(SocketAddr),
	RmListener(SocketAddr),
	LsListeners,
	LsConns,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Response {
	Error(ErrKind, String),
	Echo,
	Ok(String),
	List(Vec<String>),
	Nil,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum ErrKind {
	InvalidPacket,
	IoError(String),
	NotFound,
}
