use crate::error::Error;
use std::path::Path;

mod public {
	#[derive(serde::Deserialize)]
	pub struct Config {
		server: Vec<Server>,
		fallback: Option<Server>,
	}

	#[derive(serde::Deserialize)]
	pub struct Server {
		name: String,
		target: String,
		aliases: Option<Vec<String>>,
	}
}

pub struct Config {
	server: Vec<Server>,
	fallback: Option<Server>,
}

pub struct Server {
	name: String,
	names: Vec<String>,
	target: crate::router::Target,
}

pub async fn load_from_file(path: &Path) -> Result<Config, Error> {
	Ok(toml::from_slice(tokio::fs::read(path).await?.as_slice())?.into())
}
