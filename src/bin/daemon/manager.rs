use std::net::SocketAddr;
use std::sync::Arc;

pub struct Manager;

impl Manager {
	pub async fn new_client(self: &Arc<Self>, peer: SocketAddr, stream: tokio::net::TcpStream) {
		let handler = tokio::spawn(async move {
			let mut buf = [0u8; 5];
			let mut read = 0;
			loop {
				match stream.peek(&mut buf[read..]).await {
					Ok(r) => {
						read += r;
						if read >= buf.len() {
							break;
						}
					}
					Err(err) => {
						tracing::error!("Failed to peek tcp stream; Error={}; Peer={}", err, &peer);
						return;
					}
				}
			}
			if buf == [0xFE, 0x01, 0xFA, 0x00, 0x0B] {
				// Legacy ping
				todo!();
				return;
			}
		});
	}

	pub async fn handle_legacy(peer: SocketAddr, stream: tokio::net::TcpStream) {}
}
gi