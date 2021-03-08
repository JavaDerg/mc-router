use std::net::SocketAddr;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum Request {
	Echo,
	List,
	MakeListener(SocketAddr),
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Response {
	Error(ErrKind, String),
	Echo,
	Nil,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum ErrKind {
	InvalidPacket,
	IoError(String),
}
