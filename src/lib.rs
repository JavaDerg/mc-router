pub mod cprot;

#[cfg(debug_assertions)]
pub const SOCKET_PATH: &str = "mcprox.sock";
#[cfg(not(debug_assertions))]
pub const SOCKET_PATH: &str = "/var/run/mcprox.sock";
