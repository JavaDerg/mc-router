use futures::StreamExt;
use std::io::Error;
use std::path::Path;
use tokio::io::{AsyncWriteExt, Lines};
use tokio::net::{TcpStream, UnixStream};
use tokio_util::codec::{Decoder, Framed, FramedRead, LinesCodec};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(debug_assertions)]
const SOCKET_PATH: &str = "mcprox";
#[cfg(not(debug_assertions))]
const SOCKET_PATH: &str = "/var/run/mcprox";

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt()
		.with_max_level(tracing::Level::TRACE)
		.init();

	let socket = Path::new(SOCKET_PATH);

	// Remove old socket
	let _ = std::fs::remove_file(socket);

	let listener = match tokio::net::UnixListener::bind(socket) {
		Ok(l) => l,
		Err(e) => {
			tracing::error!("Can not create unix socket; Path={}; Reason={}", SOCKET_PATH, e);
			std::process::exit(1);
		}
	};

	loop {
		let (socket, addr) = match listener.accept().await {
			Ok(s) => s,
			Err(e) => {
				tracing::error!("Unable to accept client; Reason={}", e);
				continue;
			}
		};
		tracing::info!("New control connection; Addr={:?}", addr);
		tokio::spawn(handle_controller(socket));
	}
}

async fn handle_controller(mut stream: UnixStream) {
	let (read, mut write) = stream.split();
	let mut framed = FramedRead::new(read, LinesCodec::new());
	while let Some(Ok(line)) = framed.next().await {
		write.write_all(format!("{}\n", line).as_bytes()).await;
	}
}
