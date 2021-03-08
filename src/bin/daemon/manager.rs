use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub struct Manager {
	dict: RwLock<HashMap<crate::prot::Host, SocketAddr>>,
}

impl Manager {
	pub fn new() -> Self {
		Self {
			dict: RwLock::default(),
		}
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
				Some(target) => {}
				None => {
					warn!(
						"Unknown target; Target={}:{}; Peer={}; Local={}; State=Disconnecting",
						&host.domain, &host.port, &peer, &local
					);
				}
			}
		});
	}

	pub async fn get_socket_addr(&self, host: &crate::prot::Host) -> Option<SocketAddr> {
		self.dict.read().await.get(host).cloned()
	}
}
