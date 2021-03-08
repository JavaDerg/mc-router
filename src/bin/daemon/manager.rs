use crate::prot::Host;
use crate::proxy::ConnInfo;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Weak;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Default)]
pub struct Manager {
	dict: RwLock<HashMap<Host, SocketAddr>>,
	connections: RwLock<HashMap<Host, Vec<Weak<ConnInfo>>>>,
}

impl Manager {
	pub fn new() -> Self {
		Self::default()
	}

	pub async fn new_client(
		&'static self,
		local: SocketAddr,
		peer: SocketAddr,
		mut stream: tokio::net::TcpStream,
	) {
		let _handler = tokio::spawn(async move {
			let host = match crate::prot::seek_handshake(&stream).await {
				Ok(host) => host,
				Err(err) => {
					error!("Unknown Error; Error={}; Peer={}", err, &peer);
					return;
				}
			};
			info!(
				"New Connection; Local={}; Peer={}; Target={{ Host={}; Port={} }}",
				&local, &peer, &host.domain, &host.port
			);
			let target = self.get_socket_addr(&host).await;
			match target {
				Some(target) => {
					tokio::spawn(crate::proxy::route(stream, target));
				}
				None => warn!(
					"Unknown target; Target={}:{}; Peer={}; Local={}; State=Disconnecting",
					&host.domain, &host.port, &peer, &local
				),
			}
		});
	}

	pub async fn get_socket_addr(&self, host: &crate::prot::Host) -> Option<SocketAddr> {
		self.dict.read().await.get(host).cloned()
	}
}
