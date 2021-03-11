mod manager;
mod prot;
mod proxy;

use crate::manager::Manager;
use futures::StreamExt;
use itertools::Itertools;
use mc_router::cprot::{ErrKind, Request, Response};
use mc_router::{cprot, SOCKET_PATH};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::net::unix::WriteHalf;
use tokio::net::UnixStream;
use tokio_util::codec::{FramedRead, LinesCodec};

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

	let manager = &*Box::leak(Box::new(manager::Manager::new()));

	loop {
		let (socket, addr) = match listener.accept().await {
			Ok(s) => s,
			Err(e) => {
				tracing::error!("Unable to accept client; Reason={}", e);
				continue;
			}
		};
		tracing::info!("New control connection; Addr={:?}", addr);
		tokio::spawn(handle_controller(socket, manager));
	}
}

async fn handle_controller(mut stream: UnixStream, manager: &'static Manager) {
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
		if line.is_empty() {
			continue;
		}
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
				continue;
			}
		};
		tracing::info!("Received Packet; Parsed={:?}", ron);
		if let Err(err) = write_res(&mut write, process_packet(ron, manager).await).await {
			tracing::error!(
				"Unable to write response; State=Closing connection; Error={}",
				err
			);
			return;
		}
	}
}

async fn process_packet(packet: cprot::Request, manager: &'static Manager) -> cprot::Response {
	match packet {
		cprot::Request::Echo => cprot::Response::Echo,
		cprot::Request::MkListener(socket) => match proxy::mk_listener(manager, socket).await {
			Ok(listener) => {
				manager.register_listener(socket, listener).await;
				cprot::Response::Ok(format!("Created new listener {}", socket))
			}
			Err(err) => cprot::Response::Error(
				cprot::ErrKind::IoError(format!("{}", err)),
				format!("Unable to listen on '{:?}'", socket),
			),
		},
		Request::RmListener(socket) => match manager.kill_delete_listener(socket).await {
			true => Response::Ok(format!("Listener {} closed", socket)),
			false => Response::Error(ErrKind::NotFound, format!("Listener {} does not exist", socket)),
		},
		Request::LsListeners => Response::List(
			manager
				.get_listeners()
				.await
				.into_iter()
				.map(|addr| addr.to_string())
				.collect_vec(),
		),
		Request::SetMapping(domain, socket) => match manager.set_mapping(domain.clone(), socket).await {
			Some(old) => Response::Ok(format!("Set {} to {}, replaced {}", &domain, socket, old)),
			None => Response::Ok(format!("Set {} to {}", &domain, socket)),
		},
		Request::GetMapping(domain) => match manager.get_mapping(&domain).await {
			Some(addr) => Response::Ok(format!("{} => {}", &domain, addr)),
			None => Response::Error(ErrKind::NotFound, format!("Unable to find {}", &domain)),
		},
		Request::RmMapping(domain, rm_clients) => match manager.del_mapping(&domain, rm_clients).await {
			0 => Response::Error(ErrKind::NotFound, format!("Unable to find {}", &domain)),
			x => Response::Ok(format!(
				"Deleted mapping for {}; Disconnected {} players",
				&domain,
				x - 1
			)),
		},
		Request::LsMappings => Response::List(
			manager
				.list_mappings()
				.await
				.into_iter()
				.map(|(domain, addr)| format!("{} => {}", domain, addr))
				.collect_vec(),
		),
		Request::LsConns => Response::List(
			manager
				.list_connections()
				.await
				.into_iter()
				.map(|(addr, domain)| format!("{} => {}", addr, domain))
				.collect_vec(),
		),
	}
}

async fn write_res(wh: &mut WriteHalf<'_>, res: cprot::Response) -> tokio::io::Result<()> {
	wh.write_all(ron::to_string(&res).unwrap().as_bytes()).await?;
	wh.write_all(b"\n").await?;
	wh.flush().await
}
