use dashmap::DashMap;
use notify::{DebouncedEvent, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, info_span, Instrument};

pub struct Config {
	pub path: String,
	pub map: DashMap<String, String>,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
	#[error("io error: {0}")]
	Io(#[from] std::io::Error),
	#[error("config error: {0}")]
	InvalidConfig(#[from] serde_json::Error),
}

impl Config {
	pub async fn new(path: String) -> Result<Self, ConfigError> {
		let cfg = tokio::fs::read_to_string(&path).await?;
		let cfg: HashMap<String, String> = serde_json::from_str(&cfg)?;

		Ok(Self {
			path,
			map: DashMap::from_iter(cfg.into_iter()),
		})
	}
}

pub async fn watcher(config: Arc<Config>) {
	let (tx, rx) = std::sync::mpsc::channel();

	let mut watcher = match notify::watcher(tx, Duration::from_secs(1)) {
		Ok(watcher) => watcher,
		Err(err) => {
			error!("Failed to start watcher: {}", err);
			if std::env::var("IGNORE_WATCHER")
				.map(|x| x.trim() == "1")
				.unwrap_or(false)
			{
				error!("This is a fail condition, use env var IGNORE_WATCHER=1 to continue");
				std::process::exit(1);
			}
			return;
		}
	};

	let mut rx = bridge(rx).await;

	if let Err(err) = watcher.watch(&config.path, RecursiveMode::NonRecursive) {
		error!("Failed to watch config: {}", err);
		if std::env::var("IGNORE_WATCHER")
			.map(|x| x.trim() == "1")
			.unwrap_or(false)
		{
			error!("This is a fail condition, use env var IGNORE_WATCHER=1 to continue");
			std::process::exit(1);
		}
	}

	while let Some(event) = rx.recv().await {
		match event {
			DebouncedEvent::Write(_) => {
				rebuild_config(&*config)
					.instrument(info_span!("Config rebuild"))
					.await
			}
			DebouncedEvent::Error(err, _) => {
				error!("Watch error: {}", err);
			}
			_ => (),
		}
	}
}

pub async fn rebuild_config(config: &Config) {
	info!("Config changed!");
	let cfg = match tokio::fs::read_to_string(&config.path).await {
		Ok(cfg) => cfg,
		Err(err) => {
			error!("Failed to read config: {}", err);
			return;
		}
	};
	let cfg: HashMap<String, String> = match serde_json::from_str(&cfg) {
		Ok(cfg) => cfg,
		Err(err) => {
			error!("Failed to parse config: {}", err);
			return;
		}
	};

	info!("Rebuilding...");

	let mut additions = 0;
	let mut updates = 0;
	let mut deletions = 0;

	let mut old = config
		.map
		.iter()
		.map(|ref_| ref_.key().clone())
		.collect::<HashSet<_>>();

	for (k, v) in cfg {
		if old.contains(&k) {
			old.remove(&k);
		}
		let old = config.map.insert(k, v.clone());
		match old {
			None => additions += 1,
			Some(old) if old != v => updates += 1,
			_ => (),
		}
	}
	for k in old.into_iter() {
		let _ = config.map.remove(&k);
		deletions += 1;
	}

	info!("Done...\n\tAdditions: {}\n\tUpdates: {}\n\tDeletions: {}", additions, updates, deletions);
}

pub async fn bridge<T: 'static + Send>(
	rx: std::sync::mpsc::Receiver<T>,
) -> tokio::sync::mpsc::UnboundedReceiver<T> {
	let (tx, nrx) = tokio::sync::mpsc::unbounded_channel();

	tokio::task::spawn_blocking(move || while let Ok(Ok(())) = rx.recv().map(|msg| tx.send(msg)) {});

	nrx
}
