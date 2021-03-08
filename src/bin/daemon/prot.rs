use bytes::{Buf, Bytes};
use itertools::Itertools;
use tokio::io::{Error, ErrorKind};
use tokio::net::TcpStream;

#[derive(Hash, Eq, PartialEq)]
pub struct Host {
	pub domain: String,
	pub port: u16,
}

struct PeekCursor<'a> {
	stream: &'a TcpStream,
	pos: usize,
}

impl<'a> PeekCursor<'a> {
	pub async fn read(&mut self, len: usize) -> tokio::io::Result<Bytes> {
		let mut buffer = vec![0u8; self.pos + len];
		let read = self.stream.peek(buffer.as_mut()).await?;
		(read..self.pos + len).for_each(|_| drop(buffer.pop()));
		let mut buf = Bytes::from(buffer);
		buf.advance(self.pos);
		self.pos += len;
		Ok(buf)
	}

	pub async fn read_var_int(&mut self) -> tokio::io::Result<i32> {
		let mut buf = vec![0u8; self.pos + 5];
		let read = self.stream.peek(&mut buf).await?;
		if read < self.pos + 1 {
			return Err(Error::from(ErrorKind::UnexpectedEof));
		}
		let mut bytes = Bytes::from(buf);
		bytes.advance(self.pos);
		let mut result = 0;
		for i in 0..bytes.remaining().min(5) {
			let byte = bytes.get_u8();
			result |= (byte as i32 & 0x7F) << (i * 7);
			if byte >> 7 == 0 {
				self.pos += i + 1;
				return Ok(result);
			}
		}
		Err(Error::from(ErrorKind::InvalidData))
	}

	pub fn set_pos(&mut self, pos: usize) {
		self.pos = pos;
	}

	pub fn get_pos(&self) -> usize {
		self.pos
	}
}

pub async fn seek_handshake(stream: &TcpStream) -> tokio::io::Result<Host> {
	let mut buf = [0u8; 5];
	let read = stream.peek(&mut buf).await?;
	if read < 5 {
		return Err(Error::from(ErrorKind::UnexpectedEof));
	}
	// Legacy ping
	if buf == [0xFE, 0x01, 0xFA, 0x00, 0x0B] {
		seek_legacy(stream).await
	} else {
		seek_default(stream).await
	}
}

async fn seek_default(stream: &TcpStream) -> tokio::io::Result<Host> {
	let mut cursor = PeekCursor { stream, pos: 0 };
	let len = cursor.read_var_int().await?;
	if len.is_negative() || len > 19 + 256 {
		return Err(Error::from(ErrorKind::InvalidData));
	}
	let mut payload = cursor.read(len as usize).await?;
	if payload.remaining() < len as usize {
		return Err(Error::from(ErrorKind::InvalidData));
	}
	let _id = read_var_int_buf(&mut payload);
	let _version = read_var_int_buf(&mut payload);
	let domain = read_string_buf(&mut payload, 256)?;
	let port = payload.get_u16();
	let _next_state = read_var_int_buf(&mut payload);
	Ok(Host { domain, port })
}

async fn seek_legacy(stream: &TcpStream) -> tokio::io::Result<Host> {
	let mut cursor = PeekCursor { stream, pos: 27 };
	let len = cursor.read(2).await?.get_u16();
	if len >= 7 + 512 {
		return Err(Error::from(ErrorKind::InvalidData));
	}
	if len < 7 {
		return Err(Error::from(ErrorKind::InvalidData));
	}
	let mut payload = cursor.read(len as usize).await?;
	let _version = payload.get_u8();
	let str_len = payload.get_u16();
	if payload.remaining() < (str_len * 2 + 4) as usize {
		return Err(Error::from(ErrorKind::InvalidData));
	}
	let str_bytes = payload.split_to((str_len * 2) as usize);
	let port = payload.get_u32() as u16;
	let domain = String::from_utf16(
		str_bytes
			.as_ref()
			.iter()
			.map(|x| *x as u16)
			.tuples()
			.map(|(x, y)| x << 8 | y)
			.collect_vec()
			.as_slice(),
	)
	.map_err(|_| Error::from(ErrorKind::InvalidData))?;

	Ok(Host { domain, port })
}

fn read_var_int_buf(buf: &mut bytes::Bytes) -> Option<i32> {
	let mut result = 0i32;
	for i in 0..5 {
		if !buf.has_remaining() {
			return None;
		}
		let byte = buf.get_u8();
		result |= (byte as i32 & 0x7f) << (i * 7);
		if byte >> 7 == 0 {
			break;
		}
	}
	Some(result)
}

fn read_string_buf(buf: &mut bytes::Bytes, max: u32) -> tokio::io::Result<String> {
	let len = match read_var_int_buf(buf) {
		Some(len) => len,
		None => return Err(Error::from(ErrorKind::InvalidData)),
	};
	if len.is_negative() || len as u32 > max || buf.remaining() < len as usize {
		return Err(Error::from(ErrorKind::InvalidData));
	}
	let str_res = String::from_utf8(buf.slice(0..len as usize).to_vec())
		.map_err(|_| Error::from(ErrorKind::InvalidData));
	buf.advance(len as usize);
	str_res
}
