use crate::manager::Manager;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::task::JoinHandle;

pub struct Listener(tokio::task::JoinHandle<()>);

pub struct ConnInfo {
	peer: SocketAddr,
	target: SocketAddr,
	handler: JoinHandle<()>,
}

async fn loopback(mut read: OwnedReadHalf, mut write: OwnedWriteHalf) -> tokio::io::Result<()> {
	let mut buffer = [0u8; 2048];
	loop {
		let read = read.read(&mut buffer[..]).await?;
		write.write_all(&buffer[..read]).await?;
	}
}

pub async fn mk_listener(manager: &'static Manager, addr: SocketAddr) -> tokio::io::Result<Listener> {
	let listener = tokio::net::TcpListener::bind(addr).await?;
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

	Ok(Listener(handler))
}

pub async fn route(stream: tokio::net::TcpStream, peer: SocketAddr, target: SocketAddr) -> Weak<ConnInfo> {
	let (tx, rx) = flume::bounded(0);
	let conn = Arc::new(ConnInfo {
		peer,
		target,
		handler: tokio::spawn(async move {
			let res: tokio::io::Result<()> = async move {
				let conn = rx.recv_async().await.unwrap();
				let target_stream = tokio::net::TcpStream::connect(target).await?;
				let (read1, write1) = stream.into_split();
				let (read2, write2) = target_stream.into_split();
				tokio::select! {
					res = loopback(read1, write2) => if let Err(err) = res {
						tracing::error!("Proxy error; Err={}", err);
					},
					res = loopback(read2, write1) => if let Err(err) = res {
						tracing::error!("Proxy error; Err={}", err);
					},
				}
				drop(conn);
				Ok(())
			}
			.await;
			if let Err(err) = res {
				tracing::error!("Proxy error; Err={}", err);
			}
		}),
	});
	let weak = Arc::downgrade(&conn);
	tx.send_async(conn).await.unwrap();
	weak
}
