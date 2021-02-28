pub enum Error {
	Own(Box<Self>),
	Custom(String),
	Toml(toml::de::Error),
	Io(std::io::Error),
}

impl From<String> for Error {
	fn from(str: String) -> Self {
		Self::Custom(str)
	}
}

impl From<toml::de::Error> for Error {
	fn from(err: toml::de::Error) -> Self {
		Self::Toml(err)
	}
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Self {
		Self::Io(err)
	}
}
