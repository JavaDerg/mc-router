use futures::StreamExt;
use mc_router::{cprot, SOCKET_PATH};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::net::unix::WriteHalf;
use tokio::net::UnixStream;
use tokio_util::codec::{FramedRead, LinesCodec};
use bytes::BufMut;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt()
		.with_max_level(tracing::Level::INFO)
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
	let mut framed = FramedRead::new(read, LinesCodec::new_with_max_length(8192));
	while let Some(line) = framed.next().await {
		let line = match line {
			Ok(line) => line,
			Err(err) => {
				tracing::warn!("Failed to read packet; State=Closing connection; Error={}", err);
				tracing::info!("Closing control connection");
				return;
			}
		};
		let ron: cprot::Request = match ron::from_str(&line) {
			Ok(obj) => obj,
			Err(err) => {
				tracing::warn!(
					"Received Invalid Packet; State=Closing connection; Error={}; Data={}",
					err,
					line
				);
				let _ = write_res(
					&mut write,
					cprot::Response::Error(
						cprot::ErrKind::InvalidPacket,
						String::from("Unable to decode packet"),
					),
				)
				.await;
				return;
			}
		};
		tracing::info!("Received Packet; Parsed={:?}", ron);
		if let Err(err) = write_res(&mut write, process_packet(ron).await).await {
			tracing::error!("Unable to write response; State=Closing connection; Error={}", err);
			return;
		}
	}
}

async fn process_packet(packet: cprot::Request) -> cprot::Response {
	match packet {
		cprot::Request::Echo => cprot::Response::Echo,
		_ => cprot::Response::Nil,
	}
}

async fn write_res(wh: &mut WriteHalf<'_>, res: cprot::Response) -> tokio::io::Result<()> {
	let ron = ron::to_string(&res)
		.expect("If you can read this, email <post-rex@pm.me> about hot glue + stacktrace");
	let ron = ron.as_bytes();

	let mut buf = bytes::BytesMut::with_capacity(ron.len() + 1);
	buf.put_slice(ron);
	buf.put_u8(b'\n');
	let buf = buf.freeze();

	wh.write_all(buf.as_ref()).await
}
