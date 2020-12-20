use hotwatch::Hotwatch;
use serde::export::fmt::Write;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() {
	let config_file = std::env::var("MCR_CONFIG_FILE").unwrap_or_else(|_| String::from("config.toml"));
	let config_path = Path::new(&config_file);
	let config = Arc::new(RwLock::new(Config {
		server: Some(vec![]),
		balancer: Some(vec![]),
	}));
	let router = Arc::new(RwLock::new(Router {
		routes: Default::default(),
	}));

	if !config_path.exists() {
		eprintln!("No config found, creating...");
		crate_config(&config_path).await;
	}

	if let Err(err) = reload_config(config_path, &config, &router).await {
		eprintln!("Failed to load config:\n{}", err);
		std::process::exit(1);
	}

	let (tx, rx) = flume::bounded(16);

	let maybe_hw = match Hotwatch::new() {
		Ok(mut hw) => {
			if let Err(err) = hw.watch(config_path, move |event| {
				tx.send(event).unwrap();
			}) {
				eprintln!("Failed to watch config file, {}", err);
				None
			} else {
				Some(hw)
			}
		}
		Err(err) => {
			eprintln!("Failed to start config watcher, {}", err);
			None
		}
	};
	while let Ok(de) = rx.recv_async().await {
		if let hotwatch::notify::DebouncedEvent::Write(_) = de {
			if let Err(err) = reload_config(config_path, &config, &router).await {
				eprintln!("Failed to reload config: {}", err)
			} else {
				eprintln!("Config reloaded");
			}
		}
	}
	drop(maybe_hw);
}

async fn reload_config(
	path: &Path,
	config: &Arc<RwLock<Config>>,
	router: &Arc<RwLock<Router>>,
) -> Result<(), String> {
	let new_cfg: Config = toml::from_slice(
		tokio::fs::read(path)
			.await
			.map_err(|err| format!("{}", err))?
			.as_slice(),
	)
	.map_err(|err| format!("{}", err))?;
	let new_router = new_cfg.make_router()?;
	Ok(())
}

async fn crate_config(path: &Path) {
	tokio::fs::write(path, include_str!("default_config.toml"))
		.await
		.expect("Failed to create config file");
}

struct Router {
	routes: HashMap<String, Route>,
}

#[derive(PartialEq, Eq, Hash, Clone)]
enum Route {
	Direct {
		origin: String,
		target: SocketAddr,
		private: bool,
	},
	Route {
		origin: String,
		target: Option<Vec<SocketAddr>>,
		private: bool,
	},
}

#[derive(serde::Deserialize)]
struct Config {
	server: Option<Vec<Server>>,
	balancer: Option<Vec<Balancer>>,
}

#[derive(serde::Deserialize)]
struct Server {
	name: String,
	target: SocketAddr,
	aliases: Option<Vec<String>>,
	private: Option<bool>,
}

#[derive(serde::Deserialize)]
struct Balancer {
	name: String,
	targets: Vec<String>,
	aliases: Option<Vec<String>>,
	public: Option<bool>,
}

impl Config {
	pub fn make_router(&self) -> Result<Router, String> {
		let mut routes = HashMap::new();
		let mut conflicts = HashMap::<String, HashSet<Route>>::new();
		if let Some(ref servers) = self.server {
			for server in servers {
				let mut names = vec![server.name.clone()];
				if let Some(mut aliases) = server.aliases.clone() {
					names.append(&mut aliases);
				}
				for name in names {
					let new_route = server.route(&name);
					check_and_insert(&mut routes, &mut conflicts, name, new_route);
				}
			}
		}
		if let Some(ref balancers) = self.balancer {
			for balancer in balancers {
				let mut names = vec![balancer.name.clone()];
				if let Some(mut aliases) = balancer.aliases.clone() {
					names.append(&mut aliases);
				}
				for name in names {
					let new_route = balancer.route(&name);
					check_and_insert(&mut routes, &mut conflicts, name, new_route);
				}
			}
		}
		if !conflicts.is_empty() {
			let mut buf = String::with_capacity(512);
			buf.write_str(format!("Found {} conflicts:\n", conflicts.len()).as_str())
				.unwrap();
			for (k, v) in conflicts {
				buf.write_str(format!("  {}: (Exists twice or more)\n", k).as_str())
					.unwrap();
				for route in v {
					let (origin, target) = match route {
						Route::Direct { origin, target, .. } => (origin, Some(target)),
						Route::Route { origin, .. } => (origin, None),
					};
					buf.write_str(
						format!(
							"  . {} -> {}\n",
							origin,
							target
								.map(|ipaddr| format!("{}", ipaddr))
								.unwrap_or_else(|| String::from("(Resolved in next step)"))
						)
						.as_str(),
					)
					.unwrap();
				}
			}
			return Err(buf);
		}
		Err(String::from("Failed to resolve all routes for the balancers"))
		// Ok(Router { routes })
	}
}

impl Server {
	pub fn route(&self, name: &str) -> Route {
		Route::Direct {
			origin: name.to_string(),
			target: self.target,
			private: self.private.unwrap_or(false),
		}
	}
}

impl Balancer {
	pub fn route(&self, name: &str) -> Route {
		Route::Route {
			origin: name.to_string(),
			target: None,
			private: false,
		}
	}
}

fn check_and_insert(
	routes: &mut HashMap<String, Route>,
	conflicts: &mut HashMap<String, HashSet<Route>>,
	name: String,
	new_route: Route,
) {
	if let Some(route) = routes.insert(name.clone(), new_route.clone()) {
		if let Some(hs) = conflicts.get_mut(&name) {
			let _ = hs.insert(route);
			let _ = hs.insert(new_route);
		} else {
			let mut hs = HashSet::new();
			let _ = hs.insert(route);
			let _ = hs.insert(new_route);
			conflicts.insert(name.clone(), hs);
		}
	}
}
