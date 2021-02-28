use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::io::Error;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

pub enum Target {
	Direct(SocketAddr),
	Lookup {
		origin: String,
		target: String,
		last: Arc<RwLock<SocketAddr>>,
		next_lookup: Instant,
	},
}

impl Target {
	pub async fn new(addr: &str, origin: impl Into<String>) -> Result<Self, crate::error::Error> {
		let target = addr;
		if let Ok(addr) = SocketAddr::from_str(target) {
			return Ok(Self::Direct(addr));
		}
		if let Some(addr) = tokio::net::lookup_host(target).await?.next() {
			return Ok(Self::Lookup {
				origin: origin.into(),
				target: target.into(),
				last: Arc::new(RwLock::new(addr)),
				next_lookup: Instant::now() + Duration::from_secs(60 * 30),
			});
		}
		unreachable!()
	}

	pub async fn resolve(&self) -> Result<SocketAddr, crate::error::Error> {
		match self {
			Target::Direct(addr) => Ok(*addr),
			Target::Lookup {
				origin,
				target,
				last,
				next_lookup,
			} => {
				if Instant::now() > *next_lookup {
					let (origin, target) = (origin.clone(), target.clone());
					let last = last.clone();
					tokio::spawn(async move {
						let addr = match tokio::net::lookup_host(target).await {
							Ok(mut addr_iter) => match addr_iter.next() {
								Some(addr) => addr,
								None => todo!(origin),
							},
							Err(_) => todo!(origin),
						};
						*last.write().await = addr;
					});
				}
				Ok(*last.read().await)
			}
		}
	}
}
