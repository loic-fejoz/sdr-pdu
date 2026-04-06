use crate::kiss::KissDecoder;
use bytes::BytesMut;
use futures::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::codec::Decoder;
use tracing::{error, info};

pub struct KissWsServer {
    sender: mpsc::Sender<Vec<u8>>,
}

impl KissWsServer {
    pub fn new(sender: mpsc::Sender<Vec<u8>>) -> Self {
        Self { sender }
    }

    pub async fn run(&self, addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("KISS WebSocket server listening on ws://{}", addr);

        loop {
            let (socket, peer) = match listener.accept().await {
                Ok(res) => res,
                Err(e) => {
                    error!("Failed to accept KISS WS client connection: {}", e);
                    continue;
                }
            };
            let sender = self.sender.clone();

            tokio::spawn(async move {
                let ws_stream = match accept_async(socket).await {
                    Ok(ws) => ws,
                    Err(e) => {
                        error!("Error during the websocket handshake with {}: {}", peer, e);
                        return;
                    }
                };

                info!("New WebSocket connection from {}", peer);
                let (_, mut read) = ws_stream.split();
                let mut buf = BytesMut::new();
                let mut decoder = KissDecoder;

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Binary(data)) => {
                            buf.extend_from_slice(&data);

                            loop {
                                match decoder.decode(&mut buf) {
                                    Ok(Some(mut frame)) => {
                                        if frame.is_empty() {
                                            continue;
                                        }

                                        let cmd = frame[0] & 0x0F;
                                        if cmd == 0x00 {
                                            frame.remove(0);
                                            if let Err(e) = sender.send(frame).await {
                                                error!("Failed to send frame to engine: {}", e);
                                                return;
                                            }
                                        } else {
                                            info!("Ignoring non-data KISS frame (cmd={:02X})", cmd);
                                        }
                                    }
                                    Ok(None) => {
                                        // Need more data
                                        break;
                                    }
                                    Err(e) => {
                                        error!("KISS decode error: {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            info!("WebSocket closed by {}", peer);
                            break;
                        }
                        Ok(_) => {
                            // Ignore other message types (Text, Ping, Pong)
                        }
                        Err(e) => {
                            error!("WebSocket error from {}: {}", peer, e);
                            break;
                        }
                    }
                }
            });
        }
    }
}
