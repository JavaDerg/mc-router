use crate::proxy::{ConnInfo, Listener};
use itertools::Itertools;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Weak;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Default)]
pub struct Manager {
	dict: RwLock<HashMap<String, SocketAddr>>,
	connections: RwLock<HashMap<String, Vec<Weak<ConnInfo>>>>,
	listener: RwLock<HashMap<SocketAddr, Listener>>,
	cached_players: RwLock<HashMap<String, Vec<Weak<ConnInfo>>>>,
	con_count: AtomicUsize,
	cleaner_running: AtomicBool,
}

impl Manager {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn start_cleaner(&'static self) {
		if self.cleaner_running.load(Ordering::Acquire) {
			panic!("Cleaner started twice");
		}
		tokio::spawn(async move {
			loop {
				tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
				let cc = self.con_count.swap(0, Ordering::AcqRel);
				if cc == 0 {
					continue;
				}
				info!("Cleaning closed connections from {} total connections", cc);
				self.clean_cache().await;
			}
		});
	}

	async fn clean_cache(&self) {
		let mut cc = self.connections.write().await;
		for vec in cc.values_mut() {
			Manager::clean_vec(vec);
		}
		drop(cc);
		let mut cc = self.cached_players.write().await;
		for vec in cc.values_mut() {
			Manager::clean_vec(vec);
		}
		drop(cc);
	}

	fn clean_vec(vec: &mut Vec<Weak<ConnInfo>>) {
		let indexes = vec
			.iter()
			.zip(0..)
			.filter(|(conn, _)| conn.strong_count() == 0)
			.map(|(_, index)| index)
			.collect_vec();
		for (index, offset) in indexes.iter().zip(0..) {
			let index = *index - offset;
			vec.remove(index);
		}
	}

	pub async fn new_client(
		&'static self,
		local: SocketAddr,
		peer: SocketAddr,
		stream: tokio::net::TcpStream,
	) {
		let _handler = tokio::spawn(async move {
			let host = match crate::prot::seek_handshake(&stream).await {
				Ok(host) => host,
				Err(err) => {
					error!("Unknown Error; Error={}; Peer={}", err, &peer);
					return;
				}
			};
			info!(
				"New Connection; Local={}; Peer={}; Target={{ Host={}; Port={} }}",
				&local, &peer, &host.domain, &host.port
			);
			let target = self.get_socket_addr(&host.domain).await;
			match target {
				Some(target) => {
					let connection = crate::proxy::route(stream, peer, target).await;
					self.register_client(host.domain, connection).await;
				}
				None => warn!(
					"Unknown target; Target={}:{}; Peer={}; Local={}; State=Disconnecting",
					&host.domain, &host.port, &peer, &local
				),
			}
		});
		self.con_count.fetch_add(1, Ordering::AcqRel);
	}

	pub async fn register_client(&self, target: String, conn: Weak<ConnInfo>) {
		self.connections
			.write()
			.await
			.entry(target)
			.or_default()
			.push(conn);
	}

	pub async fn register_listener(&self, addr: SocketAddr, listener: Listener) {
		if let Some(old_listener) = self.listener.write().await.insert(addr, listener) {
			warn!(
				"Stopping old listener in favour of new one (this shouldn't be possible); Local={}",
				addr
			);
			old_listener.0.abort();
		}
	}

	pub async fn kill_delete_listener(&self, addr: SocketAddr) -> bool {
		if let Some(listener) = self.listener.write().await.remove(&addr) {
			listener.0.abort();
			true
		} else {
			false
		}
	}

	pub async fn get_listeners(&self) -> Vec<SocketAddr> {
		self.listener.read().await.keys().copied().collect_vec()
	}

	pub async fn set_mapping(&self, domain: String, socket: SocketAddr) -> Option<SocketAddr> {
		self.dict.write().await.insert(domain, socket)
	}

	pub async fn get_mapping(&self, domain: &str) -> Option<SocketAddr> {
		self.dict.read().await.get(domain).cloned()
	}

	pub async fn del_mapping(&self, domain: &str, dc_players: bool) -> usize {
		let rm_map = self.dict.write().await.remove(domain);
		if let Some(conns) = self.connections.write().await.remove(domain) {
			if dc_players {
				#[allow(clippy::suspicious_map)]
				return conns
					.into_iter()
					.map(|conn| conn.upgrade())
					.filter(|conn| conn.is_some())
					.map(|conn| conn.unwrap().handler.abort())
					.count() as usize + 1;
			} else {
				let mut vec = conns
					.into_iter()
					.filter(|conn| conn.strong_count() > 0)
					.collect_vec();
				let mut cap = self.cached_players.write().await;
				cap.entry(domain.to_string()).or_default().append(&mut vec);
			}
		}
		match rm_map.is_some() {
			true => 1,
			false => 0,
		}
	}

	pub async fn list_connections(&self) -> Vec<(SocketAddr, String)> {
		let mut vec = vec![];
		let ci = self.connections.read().await;
		let cpi = self.cached_players.read().await;
		for mut sub_vec in ci
			.iter()
			.chain(cpi.iter())
			.map(|(domain, conns)| (domain.clone(), conns.clone()))
			.map(|(domain, conns)| {
				conns
					.into_iter()
					.map(|conn| conn.upgrade())
					.filter(|conn| conn.is_some())
					.map(|conn| (conn.unwrap().peer, domain.clone()))
					.collect_vec()
			}) {
			vec.append(&mut sub_vec);
		}
		vec
	}

	pub async fn list_mappings(&self) -> Vec<(String, SocketAddr)> {
		self.dict
			.read()
			.await
			.iter()
			.map(|(domain, addr)| (domain.clone(), *addr))
			.collect_vec()
	}

	pub async fn get_socket_addr(&self, host: &String) -> Option<SocketAddr> {
		self.dict.read().await.get(host).cloned()
	}
}
