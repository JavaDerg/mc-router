use bytes::{Buf, BufMut, BytesMut};
use mimalloc::MiMalloc;
use pretty_hex::PrettyHex;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Default)]
struct VarInt(i32, u8);

struct HandshakeFrame {
	version: VarInt,
	hostname_len: VarInt,
	hostname: BytesMut,
	port: Option<u16>,
	next: VarInt,
}

#[derive(Debug)]
struct Handshake {
	version: i32,
	hostname: String,
	port: u16,
	next: i32,
}

impl HandshakeFrame {
	pub fn new() -> Self {
		Self {
			version: VarInt::new(),
			hostname_len: VarInt::new(),
			hostname: BytesMut::with_capacity(128),
			port: None,
			next: VarInt::new(),
		}
	}

	pub fn read<B: Buf>(&mut self, buf: &mut B) -> Result<Option<Handshake>, ()> {
		while buf.has_remaining() {
			if !self.version.complete() {
				if let Some(version) = self.version.read(buf)? {
					if version < 28 {
						// No hostname here
						return Err(());
					}
				}
				continue;
			}
			if !self.hostname_len.complete() {
				if let Some(_) = self.hostname_len.read(buf)? {
					// Sanity check
				}
				continue;
			}
			while self.hostname.len() < self.hostname_len.0 as usize {
				if !buf.has_remaining() {
					continue;
				}
				self.hostname.put_u8(buf.get_u8());
				continue;
			}
			if let None = self.port {
				if buf.remaining() < 2 {
					break;
				}
				self.port = Some(buf.get_u16());
				continue;
			}
			if !self.next.complete() {
				if let Some(next) = self.next.read(buf)? {
					match next {
						1 | 2 => (),
						_ => return Err(()),
					}
				}
				continue;
			}

			return Ok(Some(todo!()));
		}
		Ok(None)
	}
}

impl VarInt {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn read<B: Buf>(&mut self, buf: &mut B) -> Result<Option<i32>, ()> {
		while buf.has_remaining() && self.1 < 5 {
			let read = buf.get_i8() as i8;
			let (read, flag) = (read & 0b0111_1111, read >> 7 == 1);
			self.0 |= (read as i32) << (7 * self.1);
			if flag {
				self.1 = 5;
				return Ok(Some(self.0));
			} else {
				self.1 += 1;
				if self.1 > 5 {
					return Err(());
				}
			}
		}
		Ok(None)
	}

	#[inline]
	pub fn complete(&self) -> bool {
		self.1 >= 5
	}
}

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
			let mut frame = HandshakeFrame::new();
			let handshake;
			loop {
				let read = client_stream.read_buf(&mut client_bytes).await.unwrap();
				slog::info!(
					&logger,
					"[{}] Read: {}{:?}",
					&addr,
					read,
					(&client_bytes[..read]).hex_dump()
				);
				match frame.read(&mut client_bytes) {
					Ok(Some(hs)) => {
						handshake = hs;
						break;
					}
					Ok(None) => (),
					Err(_) => {
						slog::error!(&logger, "Protocol error {}", &addr);
						return;
					}
				}
			}

			slog::info!(&logger, "{:?}", &handshake);

			// Select correct server

			return;

			let mut server_stream = tokio::net::TcpStream::connect("127.0.0.1:25566").await.unwrap();
			server_stream.write_buf(&mut client_bytes).await.unwrap();

			let mut server_bytes = bytes::BytesMut::with_capacity(512);

			loop {
				tokio::select! {
					len = server_stream.read_buf(&mut server_bytes) => {
						client_stream.write_buf(&mut server_bytes).await.unwrap();
						match len {
							Ok(len) if len == 0 => break,
							Err(_) => break,
							_ => ()
						}
					},
					len = client_stream.read_buf(&mut client_bytes) => {
						server_stream.write_buf(&mut client_bytes).await.unwrap();
						match len {
							Ok(len) if len == 0 => break,
							Err(_) => break,
							_ => ()
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
