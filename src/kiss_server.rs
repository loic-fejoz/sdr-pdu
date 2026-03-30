use crate::kiss::KissDecoder;
use futures::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_util::codec::FramedRead;
use tracing::{error, info};

pub struct KissServer {
    sender: mpsc::Sender<Vec<u8>>,
}

impl KissServer {
    pub fn new(sender: mpsc::Sender<Vec<u8>>) -> Self {
        Self { sender }
    }

    pub async fn run(&self, addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("KISS server listening on {}", addr);

        loop {
            let (socket, _) = listener.accept().await?;
            let sender = self.sender.clone();

            tokio::spawn(async move {
                let mut reader = FramedRead::new(socket, KissDecoder);

                while let Some(frame_res) = reader.next().await {
                    match frame_res {
                        Ok(mut frame) => {
                            if frame.is_empty() {
                                continue;
                            }

                            // First byte is command byte
                            let cmd = frame[0] & 0x0F; // Extract command, ignore port bits
                            if cmd == 0x00 {
                                // Data frame, remove command byte before sending to modulator
                                frame.remove(0);
                                if let Err(e) = sender.send(frame).await {
                                    error!("Failed to send frame to engine: {}", e);
                                    return;
                                }
                            } else {
                                info!("Ignoring non-data KISS frame (cmd={:02X})", cmd);
                            }
                        }
                        Err(e) => {
                            error!("KISS client error: {}", e);
                            return;
                        }
                    }
                }
            });
        }
    }
}
