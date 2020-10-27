use mimalloc::MiMalloc;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() {
	let logger = create_logger();
	slog::info!(
		&logger,
		concat!(
			"McRouter v",
			env!("CARGO_PKG_VERSION_MAJOR"),
			".",
			env!("CARGO_PKG_VERSION_MINOR")
		)
	);

	let listener = tokio::net::TcpListener::bind("0.0.0.0:25565").await.unwrap();
	while let Ok((mut client_stream, addr)) = listener.accept().await {
		let logger = logger.clone();
		tokio::spawn(async move {
			slog::info!(&logger, "New connection: {}", addr);

			let mut client_bytes = bytes::BytesMut::with_capacity(512);
			let read = client_stream.read_buf(&mut client_bytes).await.unwrap();
			slog::info!(&logger, "Read: {}{:?}", read, &client_bytes[..read]);

			// Parse request

			// Select correct server

			let mut server_stream = tokio::net::TcpStream::connect("127.0.0.1:25566").await.unwrap();
			server_stream.write_buf(&mut client_bytes).await.unwrap();

			let mut server_bytes = bytes::BytesMut::with_capacity(512);

			loop {
				tokio::select! {
					len = server_stream.read_buf(&mut server_bytes) => {
						client_stream.write_buf(&mut server_bytes).await.unwrap();
						if len.unwrap() == 0 {
							break;
						}
					},
					len = client_stream.read_buf(&mut client_bytes) => {
						server_stream.write_buf(&mut client_bytes).await.unwrap();
						if len.unwrap() == 0 {
							break;
						}
					},
				}
			}
		});
	}
}

fn create_logger() -> slog::Logger {
	use slog::info;
	use sloggers::terminal::{Destination, TerminalLoggerBuilder};
	use sloggers::types::Severity;
	use sloggers::Build;

	let mut builder = TerminalLoggerBuilder::new();
	builder.level(Severity::Debug);
	builder.destination(Destination::Stderr);

	builder.build().unwrap()
}
