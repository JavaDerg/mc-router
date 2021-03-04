use bytes::{Buf, BufMut};
use itertools::Itertools;
use std::borrow::Cow;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;
use tracing::{error, info, trace};

pub struct Manager {
	dict: RwLock<HashMap<String, SocketAddr>>,
}

impl Manager {
	pub fn new() -> Self {
		Self {
			dict: RwLock::default(),
		}
	}

	pub async fn new_client(self: &Arc<Self>, peer: SocketAddr, mut stream: tokio::net::TcpStream) {
		let handler =
			tokio::spawn(async move {
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
							error!("Failed to peek tcp stream; Error={}; Peer={}", err, &peer);
							return;
						}
					}
				}
				if buf == [0xFE, 0x01, 0xFA, 0x00, 0x0B] {
					// Legacy ping
					let mut data = [0u8; 27 + 2]; // Minecraft legacy pings send 28 bytes of irrelevant data https://wiki.vg/Server_List_Ping#1.6 + 2 size
					match stream.read_exact(&mut data).await {
						Ok(len) => len,
						Err(err) => {
							error!("Failed to read tcp stream; Error={}; Peer={}", err, &peer);
							return;
						}
					};
					let len = (data[27] as u16) << 8 | data[28] as u16;
					if len < 7 || len > 256 + 7 {
						error!("Invalid Handshake Packet; Error=len(hostname) wrong size, 7 > {} > 256 + 7; Peer={}", len , &peer);
						return;
					}
					let mut bytes = bytes::BytesMut::with_capacity(len as usize);
					while bytes.len() < len as usize {
						match stream.read_buf(&mut bytes).await {
							Ok(_) => continue,
							Err(err) => {
								error!("Failed to read tcp stream; Error={}; Peer={}", err, &peer);
								return;
							}
						}
					}
					let mut bytes = bytes.freeze();
					let ver = bytes.get_u8();
					let len_str = bytes.get_u16();
					let offset = len_str as usize * 2;
					let string = bytes.slice(0..offset);
					// I should have just use transmute
					let string = String::from_utf16_lossy(
						string
							.as_ref()
							.into_iter()
							.map(|x| *x as u16)
							.tuples()
							.map(|(x, y)| x << 8 | y)
							.collect::<Vec<_>>()
							.as_slice(),
					);
					let port = bytes.slice(offset..offset + 4).get_i32();
					// let response = format!("§1\074\0any\0No route.\0{}\00", port)
					let response = format!("§1\074\01.8.7\0A Minecraft Server\00\020")
						.encode_utf16()
						.map(|x| vec![(x >> 8) as u8, (x & 0xff) as u8])
						.flatten()
						.collect_vec();
					let mut bytes = bytes::BytesMut::with_capacity(response.len() + 3);
					bytes.put_u8(0xFF);
					bytes.put_u16((response.len() / 2) as u16);
					bytes.put_slice(response.as_slice());
					let bytes = bytes.freeze();
					stream.write_all(bytes.as_ref()).await;
					stream.flush().await;
					return;
				}
				let len = match read_varint(&mut stream).await {
					Ok(len) => len,
					Err(err) => {
						error!("Failed to read tcp stream; Error={}; Peer={}", err, &peer);
						return;
					}
				};
				let mut bytes = bytes::BytesMut::with_capacity(len as usize);
				while bytes.len() < len as usize {
					match stream.read_buf(&mut bytes).await {
						Ok(_) => continue,
						Err(err) => {
							error!("Failed to read tcp stream; Error={}; Peer={}", err, &peer);
							return;
						}
					}
				}
				let mut bytes = bytes.freeze();
				let id = match read_varint_buf(&mut bytes) {
					Some(id) => id,
					None => {
						error!(
							"Failed to parse packet; Position=Id; Error=No data left; Peer={}",
							&peer
						);
						return;
					}
				};
				if id != 0 {
					error!("Invalid Handshake Packet; Error={} != 0; Peer={}", id, &peer);
					return;
				}
				let ver = match read_varint_buf(&mut bytes) {
					Some(ver) => ver,
					None => {
						error!(
							"Failed to parse packet; Position=Version; Error=No data left; Peer={}",
							&peer
						);
						return;
					}
				};
				let domain = match read_string_buf(&mut bytes, 256) {
					Some(cow) => cow,
					None => {
						error!(
							"Failed to parse packet; Position=Domain; Error=No data left; Peer={}",
							&peer
						);
						return;
					}
				};
				if bytes.remaining() < 2 {
					error!(
						"Failed to parse packet; Position=Port; Error=No data left; Peer={}",
						&peer
					);
					return;
				}
				let port = bytes.get_u16();
				let state = match read_varint_buf(&mut bytes) {
					Some(ver) => ver,
					None => {
						error!(
							"Failed to parse packet; Position=Version; Error=No data left; Peer={}",
							&peer
						);
						return;
					}
				};
				write_error(&mut stream, ver, "No route.", ver, false).await;
				stream.flush().await;
			});
	}

	pub async fn handle_legacy(peer: SocketAddr, stream: tokio::net::TcpStream) {}
}

pub async fn write_error(
	stream: &mut tokio::net::TcpStream,
	err_code: i32,
	msg: &str,
	version: i32,
	legacy: bool,
) -> tokio::io::Result<()> {
	if legacy {
		todo!()
	}
	let payload = serde_json::json!({
		"version": {
			"name": "Mcprox",
			"protocol": version
		},
		"players": {
			"max": 0,
			"online": err_code,
			"sample": []
		},
		"description": {
			"text": msg // format!("§l§4{}", msg)
		},
		"favicon": include_str!("question.base64")
	})
	.to_string();
	let payload = payload.as_bytes();
	let mut bytes = bytes::BytesMut::with_capacity(4);
	write_varint_buf(&mut bytes, payload.len() as i32);
	bytes.put_u8(0x00);
	let header = bytes.freeze();
	trace!("{:?}", header.as_ref());
	stream.write_all(header.as_ref()).await;
	stream.write_all(payload).await;
	stream.flush().await;
	Ok(())
}

pub fn write_varint_buf(buf: &mut bytes::BytesMut, mut int: i32) {
	loop {
		let mut tmp = (int & 0b0111_1111) as u8;
		int >>= 7;
		if int != 0 {
			tmp |= 0b1000_0000;
		}
		buf.put_u8(tmp);
		if int == 0 {
			break;
		}
	}
}

pub async fn read_varint(stream: &mut tokio::net::TcpStream) -> tokio::io::Result<i32> {
	let mut result = 0i32;
	for i in 0..5 {
		let byte = stream.read_u8().await?;
		result |= (byte as i32 & 0b01111111) << (i * 7);
		if byte >> 7 == 0 {
			break;
		}
	}
	Ok(result)
}

pub fn read_varint_buf(buf: &mut bytes::Bytes) -> Option<i32> {
	let mut result = 0i32;
	for i in 0..5 {
		if !buf.has_remaining() {
			return None;
		}
		let byte = buf.get_u8();
		result |= (byte as i32 & 0b0111_1111) << (i * 7);
		if byte >> 7 == 0 {
			break;
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
