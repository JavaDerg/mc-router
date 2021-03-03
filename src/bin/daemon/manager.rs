use bytes::Buf;
use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

pub struct Manager;

impl Manager {
	pub async fn new_client(self: &Arc<Self>, peer: SocketAddr, mut stream: tokio::net::TcpStream) {
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
			let len = match read_varint(&mut stream).await {
				Ok(len) => len,
				Err(err) => {
					tracing::error!("Failed to read tcp stream; Error={}; Peer={}", err, &peer);
					return;
				}
			};
			let mut bytes = bytes::BytesMut::with_capacity(len as usize);
			while bytes.len() < len as usize {
				match stream.read_buf(&mut bytes).await {
					Ok(_) => continue,
					Err(err) => {
						tracing::error!("Failed to read tcp stream; Error={}; Peer={}", err, &peer);
						return;
					}
				}
			}
			let mut bytes = bytes.freeze();
			let id = match read_varint_buf(&mut bytes) {
				Some(id) => id,
				None => {
					tracing::error!(
						"Failed to parse packet; Position=Id; Error=No data left; Peer={}",
						&peer
					);
					return;
				}
			};
			if id != 0 {
				tracing::error!("Invalid Handshake Packet; Error={} != 0; Peer={}", id, &peer);
				return;
			}
			let ver = match read_varint_buf(&mut bytes) {
				Some(ver) => ver,
				None => {
					tracing::error!(
						"Failed to parse packet; Position=Version; Error=No data left; Peer={}",
						&peer
					);
					return;
				}
			};
			let domain = match read_string_buf(&mut bytes, 256) {
				Some(cow) => cow,
				None => {
					tracing::error!(
						"Failed to parse packet; Position=Domain; Error=No data left; Peer={}",
						&peer
					);
					return;
				}
			};
			if bytes.remaining() < 2 {
				tracing::error!(
					"Failed to parse packet; Position=Port; Error=No data left; Peer={}",
					&peer
				);
				return;
			}
		});
	}

	pub async fn handle_legacy(peer: SocketAddr, stream: tokio::net::TcpStream) {}
}

pub async fn read_varint(stream: &mut tokio::net::TcpStream) -> tokio::io::Result<i32> {
	let mut result = 0i32;
	for i in 0..4 {
		let byte = stream.read_u8().await?;
		result |= (byte as i32 & 0b01111111) << (i * 7);
		if byte >> 7 == 0 {
			continue;
		}
	}
	Ok(result)
}

pub fn read_varint_buf(buf: &mut bytes::Bytes) -> Option<i32> {
	let mut result = 0i32;
	for i in 0..4 {
		if !buf.has_remaining() {
			return None;
		}
		let byte = buf.get_u8();
		result |= (byte as i32 & 0b01111111) << (i * 7);
		if byte >> 7 == 0 {
			continue;
		}
	}
	Some(result)
}

pub fn read_string_buf<'a>(buf: &'a mut bytes::Bytes, max: u32) -> Option<String> {
	let len = match read_varint_buf(buf) {
		Some(len) => len,
		None => return None,
	};
	if len.is_negative() || len as u32 > max || buf.remaining() < len as usize {
		return None;
	}
	Some(String::from_utf8_lossy(buf.slice(0usize..len as usize).as_ref()).into_owned())
}
