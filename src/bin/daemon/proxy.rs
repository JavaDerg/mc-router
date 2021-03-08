use crate::manager::Manager;
use std::net::SocketAddr;

pub struct Listener(flume::Sender<ListenerRequest>, tokio::task::JoinHandle<()>);

enum ListenerRequest {}

pub async fn mk_listener(manager: &'static Manager, addr: SocketAddr) -> tokio::io::Result<Listener> {
	let listener = tokio::net::TcpListener::bind(addr).await?;
	let (rx, tx) = flume::bounded(16);
	let handler = tokio::spawn(async move {
		loop {
			let (stream, peer_addr) = match listener.accept().await {
				Ok(c) => c,
				Err(err) => {
					tracing::error!("Failed to accept clients; Local={}; Error={}", &addr, err);
					continue;
				}
			};
			manager.new_client(addr, peer_addr, stream).await;
		}
	});

	Ok(Listener(rx, handler))
}
