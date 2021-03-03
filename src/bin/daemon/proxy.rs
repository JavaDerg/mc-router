use crate::manager::Manager;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct Listener(flume::Sender<ListenerRequest>, tokio::task::JoinHandle<()>);

enum ListenerRequest {}

pub async fn mk_listener<S>(manager: Arc<Manager>, addr: SocketAddr) -> tokio::io::Result<Listener> {
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
			tracing::info!("New connection; Local={}; Peer={}", &addr, peer_addr);
			manager.new_client(addr, stream).await;
		}
	});

	Ok(Listener(rx, handler))
}
