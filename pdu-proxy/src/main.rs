use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use clap::Parser;
use futures::{sink::SinkExt, stream::StreamExt};
use sdr_pdu_utils::kiss::{KissDecoder, KissEncoder};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_util::codec::{Decoder, Encoder, FramedRead, FramedWrite};
use tower_http::services::ServeDir;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Address of the TCP KISS source for receiving frames (e.g. 127.0.0.1:8001)
    #[arg(short, long)]
    source: String,

    /// Optional address of the TCP KISS target for transmitting frames.
    /// If absent, --source is used for both RX and TX.
    #[arg(long)]
    target: Option<String>,

    /// Address to expose TCP KISS (e.g. 0.0.0.0:8002)
    #[arg(short, long)]
    tcp_listen: String,

    /// Address to expose WebSocket KISS and HTTP (e.g. 0.0.0.0:8003)
    #[arg(short, long)]
    ws_listen: String,

    /// Optional directory to serve over HTTP on the same port as the WebSocket
    #[arg(short = 'd', long)]
    http_dir: Option<PathBuf>,
}

#[derive(Clone)]
struct AppState {
    tx_broadcast: broadcast::Sender<Vec<u8>>,
    tx_mpsc: mpsc::Sender<Vec<u8>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    println!("WARNING: This tool is for development purpose only!");
    warn!("This tool is for development purpose only!");

    // Channel for frames coming FROM the source, to broadcast to all clients
    // It contains the decoded KISS payload (including the command byte).
    let (tx_broadcast, _rx_broadcast) = broadcast::channel::<Vec<u8>>(1024);

    // Channel for frames coming FROM clients, to send to the source
    // It expects the decoded KISS payload (including the command byte).
    let (tx_mpsc, mut rx_mpsc) = mpsc::channel::<Vec<u8>>(1024);

    // --- Task 1a: Source RX (Frames FROM the source) ---
    let tx_bcast_rx = tx_broadcast.clone();
    let source_addr = args.source.clone();
    tokio::spawn(async move {
        loop {
            match TcpStream::connect(&source_addr).await {
                Ok(stream) => {
                    info!("Connected to source TCP KISS (RX) at {}", source_addr);
                    let mut reader = FramedRead::new(stream, KissDecoder);

                    while let Some(result) = reader.next().await {
                        match result {
                            Ok(frame) => {
                                if !frame.is_empty() {
                                    let _ = tx_bcast_rx.send(frame);
                                }
                            }
                            Err(e) => {
                                error!("Error reading from source (RX): {}", e);
                                break;
                            }
                        }
                    }
                    warn!("Source (RX) disconnected");
                }
                Err(e) => {
                    error!("Failed to connect to source (RX) {}: {}", source_addr, e);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            info!("Reconnecting to source (RX)...");
        }
    });

    // --- Task 1b: Target TX (Frames TO the target/source) ---
    let target_addr = args.target.clone().unwrap_or(args.source.clone());
    tokio::spawn(async move {
        loop {
            match TcpStream::connect(&target_addr).await {
                Ok(stream) => {
                    info!("Connected to target TCP KISS (TX) at {}", target_addr);
                    let mut writer = FramedWrite::new(stream, KissEncoder);

                    while let Some(frame) = rx_mpsc.recv().await {
                        if let Err(e) = writer.send(frame).await {
                            error!("Error writing to target (TX): {}", e);
                            break;
                        }
                    }
                    warn!("Target (TX) disconnected");
                }
                Err(e) => {
                    error!("Failed to connect to target (TX) {}: {}", target_addr, e);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            info!("Reconnecting to target (TX)...");
        }
    });

    // --- Task 2: TCP Server ---
    let tcp_listen_addr = args.tcp_listen.clone();
    let tx_mpsc_tcp = tx_mpsc.clone();
    let tx_broadcast_tcp = tx_broadcast.clone();
    tokio::spawn(async move {
        let listener = match TcpListener::bind(&tcp_listen_addr).await {
            Ok(l) => {
                info!("TCP server listening on {}", tcp_listen_addr);
                l
            }
            Err(e) => {
                error!("Failed to bind TCP server to {}: {}", tcp_listen_addr, e);
                return;
            }
        };

        loop {
            if let Ok((stream, peer_addr)) = listener.accept().await {
                info!("New TCP client connected: {}", peer_addr);
                let tx_mpsc_client = tx_mpsc_tcp.clone();
                let mut rx_broadcast_client = tx_broadcast_tcp.subscribe();

                tokio::spawn(async move {
                    let (read_half, write_half) = stream.into_split();
                    let mut reader = FramedRead::new(read_half, KissDecoder);
                    let mut writer = FramedWrite::new(write_half, KissEncoder);

                    loop {
                        tokio::select! {
                            // Read from TCP client, send to MPSC
                            result = reader.next() => {
                                match result {
                                    Some(Ok(frame)) => {
                                        if !frame.is_empty() {
                                            let _ = tx_mpsc_client.send(frame).await;
                                        }
                                    }
                                    Some(Err(e)) => {
                                        error!("TCP client {} error: {}", peer_addr, e);
                                        break;
                                    }
                                    None => {
                                        info!("TCP client {} disconnected", peer_addr);
                                        break;
                                    }
                                }
                            }
                            // Read from broadcast, write to TCP client
                            result = rx_broadcast_client.recv() => {
                                match result {
                                    Ok(frame) => {
                                        if let Err(e) = writer.send(frame).await {
                                            error!("Failed to send to TCP client {}: {}", peer_addr, e);
                                            break;
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(n)) => {
                                        warn!("TCP client {} lagged by {} messages", peer_addr, n);
                                    }
                                    Err(broadcast::error::RecvError::Closed) => {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }
    });

    // --- Task 3: WebSocket & HTTP Server ---
    let ws_listen_addr: SocketAddr = args.ws_listen.parse()?;
    let tx_broadcast_ws = tx_broadcast.clone();
    let tx_mpsc_ws = tx_mpsc.clone();

    let state = AppState {
        tx_broadcast: tx_broadcast_ws,
        tx_mpsc: tx_mpsc_ws,
    };

    let mut router = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    if let Some(dir) = args.http_dir {
        info!("Serving HTTP static files from {:?}", dir);
        router = router.fallback_service(ServeDir::new(dir));
    }

    let listener = TcpListener::bind(&ws_listen_addr).await?;
    info!("WebSocket/HTTP server listening on {}", ws_listen_addr);

    axum::serve(listener, router).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_args_parsing_simple() {
        let args = Args::try_parse_from(&[
            "pdu-proxy",
            "--source", "127.0.0.1:8001",
            "--tcp-listen", "0.0.0.0:8002",
            "--ws-listen", "0.0.0.0:8003",
        ]).unwrap();
        assert_eq!(args.source, "127.0.0.1:8001");
        assert_eq!(args.target, None);
        assert_eq!(args.tcp_listen, "0.0.0.0:8002");
        assert_eq!(args.ws_listen, "0.0.0.0:8003");
        assert_eq!(args.http_dir, None);
    }

    #[test]
    fn test_args_parsing_split() {
        let args = Args::try_parse_from(&[
            "pdu-proxy",
            "--source", "127.0.0.1:8001",
            "--target", "127.0.0.1:8004",
            "--tcp-listen", "0.0.0.0:8002",
            "--ws-listen", "0.0.0.0:8003",
        ]).unwrap();
        assert_eq!(args.source, "127.0.0.1:8001");
        assert_eq!(args.target, Some("127.0.0.1:8004".to_string()));
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx_broadcast = state.tx_broadcast.subscribe();
    let tx_mpsc = state.tx_mpsc;
    let mut decoder = KissDecoder;
    let mut encoder = KissEncoder;

    loop {
        tokio::select! {
            // Read from WebSocket client, send to MPSC
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        let mut buf = bytes::BytesMut::from(&data[..]);
                        // Process potentially multiple KISS frames in one WS message
                        loop {
                            match decoder.decode(&mut buf) {
                                Ok(Some(frame)) => {
                                    if !frame.is_empty() {
                                        let _ = tx_mpsc.send(frame).await;
                                    }
                                }
                                Ok(None) => break, // Need more data
                                Err(e) => {
                                    error!("KISS decode error on WS: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Ok(_)) => {
                        // Ignore non-binary messages
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        break;
                    }
                }
            }
            // Read from broadcast, write to WebSocket client
            result = rx_broadcast.recv() => {
                match result {
                    Ok(frame) => {
                        let mut buf = bytes::BytesMut::new();
                        if let Ok(()) = encoder.encode(frame, &mut buf) {
                            let msg = Message::Binary(buf.freeze().to_vec());
                            if let Err(e) = socket.send(msg).await {
                                error!("Failed to send to WS client: {}", e);
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("WS client lagged by {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }
}