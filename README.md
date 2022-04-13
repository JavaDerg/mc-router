# Mc-Router
A Minecraft reverse proxy based on domains.


## What
If you have multiple Minecraft servers running but only a limited number of ports this is for you.
Mc-Router allows you to listen on a single port while having many local Minecraft servers it will proxy the connection,
to based on the domain used when connecting.


## Why
Was to lazy to set up proper DNS records that allow Minecraft to figure out the correct port :)


## Building
1. Make sure you have the most recent rust and cargo version installed, you can update by using `rustup update`;
   if you don't have `rustup` installed you can get it [here](https://rustup.rs/).
2. Clone the repo and cd into it.
3. Run `cargo build --release`.
4. You will find the compiled binary in `target/release/mc-router` (append `.exe` on Windows);


## Usage
1. Create a config similar to how show in [example.json](example.json).
   The server will try to obtain the config path from your environment variables, specifically `MCR_CONFIG`. (Default is `./mcr.json`)
   
   Additionally, you can provide the interface/port the server should listen on with `MCR_INTERFACE`. (Default is `0.0.0.0:25565`)
2. Run the server


#### Notes:
- The config is automatically hot reloaded, no need to restart the server, doing so would force all current users to lose connection.
