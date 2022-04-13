mod config;

use std::io::ErrorKind;
use std::sync::Arc;
use bytes::BytesMut;
use mimalloc::MiMalloc;
use protocol::codec::DecodeError;
use protocol::packets::{decode, encode, Handshake};
use protocol::packets::login::clientbound::Disconnect;
use protocol::types::{Chat, Prim};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpListener;
use tokio::select;
use tracing::{error, info, info_span, Instrument, warn};
use tracing_subscriber::EnvFilter;
use crate::config::Config;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let interface = std::env::var("MC_PORT").unwrap_or("0.0.0.0:25565".to_string());

    let config_path = std::env::var("MC_CONFIG").unwrap_or("mcr.json".to_string());
    let config = Config::new(config_path).await;
    if let Err(err) = &config {
        error!("Failed to load config: {:?}", err);
        std::process::exit(1);
    };
    let config = Arc::new(config.unwrap());

    info!("Opening at {}", interface);

    let listener = TcpListener::bind(interface).await.expect("Unable to open socket");

    while let Ok((stream, addr)) = listener.accept().await {
        info!("Accepted connection from {}", addr);
        tokio::spawn(
            handle_client(stream, config.clone())
                .instrument(info_span!("{}", client=addr.to_string().as_str())),
        );
    }
}

async fn handle_client(stream: tokio::net::TcpStream, cfg: Arc<Config>) {
    let mut stream = stream;
    let mut buf = [0; 1024];
    let mut read = 0;
    let hs = loop {
        match stream.read(&mut buf).await {
            Ok(n) => {
                if n == 0 {
                    info!("Client lost connection");
                    return;
                }
                read += n;
            }
            Err(e) => {
                warn!("IO Error: {}", e);
                return;
            }
        }
        break (match decode::<Handshake, _>(&buf[0..read]) {
            Ok(hs) => hs,
            Err(DecodeError::ToLittleData) => continue,
            Err(_) => {
                info!("Client send invalid data");
                return;
            },
        });
    };

    let cfg = cfg.map.get(&*hs.address.0);
    if cfg.is_none() {
        warn!("Client tried to connect to unknown server");
        return;
    }
    let target_addr = cfg.unwrap().value().clone();

    info!("Trying to connect with {}", &target_addr);
    let target_stream = tokio::net::TcpStream::connect(target_addr).await;
    if let Err(e) = &target_stream {
        error!("Failed to connect to target: {}", e);

        let reason = match e.kind() {
            ErrorKind::ConnectionRefused => "Gateway refused connection",
            ErrorKind::TimedOut => "Gateway timed out",
            _ => "Unknown error",
        };

        let packet = Disconnect(Chat::Primitive(Prim::Str(reason.to_string())));
        let buf = encode(packet).unwrap();
        if let Ok(_) = stream.write_all(&buf).await {
            let _ = stream.flush().await;
        }

        return;
    }

    let mut target_stream = target_stream.unwrap();

    if let Err(_) = target_stream.write_all(&buf[..read]).await {
        info!("Gateway disconnected");

        let packet = Disconnect(Chat::Primitive(Prim::Str("Gateway disconnected".to_string())));
        let buf = encode(packet).unwrap();
        if let Ok(_) = stream.write_all(&buf).await {
            let _ = stream.flush().await;
        }
        return;
    }

    let (rx1, tx2) = stream.into_split();
    let (rx2, tx1) = target_stream.into_split();

    info!("Piping data");
    select! {
        _ = pipe(rx1, tx1) => (),
        _ = pipe(rx2, tx2) => (),
    };

    info!("Client disconnected");
}

async fn pipe(mut rx: OwnedReadHalf, mut tx: OwnedWriteHalf) -> std::io::Result<()> {
    let mut buf = BytesMut::with_capacity(4096);

    loop {
        if buf.capacity() == buf.len() {
            tx.write_buf(&mut buf).await?;
            continue;
        }
        match rx.read_buf(&mut buf).await {
            Ok(0) => {
                info!("Client lost connection");
                break;
            }
            Ok(_) => {
                tx.write_buf(&mut buf).await?;
            }
            Err(e) => {
                warn!("IO Error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
