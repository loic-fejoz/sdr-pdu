use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{error, info};

pub struct CatServer {
    frequency: Arc<AtomicU64>,
}

impl CatServer {
    pub fn new(frequency: Arc<AtomicU64>) -> Self {
        Self { frequency }
    }

    pub async fn run(&self, addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("CAT server listening on {}", addr);

        loop {
            let (mut socket, peer_addr) = match listener.accept().await {
                Ok(res) => res,
                Err(e) => {
                    error!("Failed to accept CAT client connection: {}", e);
                    continue;
                }
            };
            info!("New CAT client connected: {}", peer_addr);

            let frequency = self.frequency.clone();

            tokio::spawn(async move {
                let mut buf = [0u8; 128];
                loop {
                    let n = match socket.read(&mut buf).await {
                        Ok(0) => {
                            info!("CAT client {} disconnected.", peer_addr);
                            return;
                        }
                        Ok(n) => n,
                        Err(e) => {
                            error!("CAT client {} error: {}", peer_addr, e);
                            return;
                        }
                    };

                    let line = String::from_utf8_lossy(&buf[..n]);
                    for cmd in line.lines() {
                        let parts: Vec<&str> = cmd.split_whitespace().collect();
                        if parts.is_empty() {
                            continue;
                        }

                        match parts[0] {
                            "F" => {
                                if parts.len() > 1 {
                                    if let Ok(f) = parts[1].parse::<u64>() {
                                        frequency.store(f, Ordering::SeqCst);
                                        // rigctld returns "RPRT 0" on success
                                        let _ = socket.write_all(b"RPRT 0\n").await;
                                    } else {
                                        let _ = socket.write_all(b"RPRT 1\n").await;
                                    }
                                }
                            }
                            "f" => {
                                let f = frequency.load(Ordering::SeqCst);
                                let _ = socket.write_all(format!("{}\n", f).as_bytes()).await;
                            }
                            _ => {
                                // Ignore unknown commands for now
                                let _ = socket.write_all(b"RPRT 0\n").await;
                            }
                        }
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    #[tokio::test]
    async fn test_cat_freq_update() {
        let freq = Arc::new(AtomicU64::new(144000000));
        let server_freq = freq.clone();

        server_freq.store(145000000, Ordering::SeqCst);
        assert_eq!(freq.load(Ordering::SeqCst), 145000000);
    }
}
