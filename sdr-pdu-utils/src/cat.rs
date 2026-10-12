use futures::StreamExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::codec::{FramedRead, LinesCodec};
use tracing::{error, info};

pub struct CatServer {
    frequency: Arc<AtomicU64>,
    attenuation: Option<Arc<AtomicU64>>, // Attenuation stored as tenths of dB (e.g. 100 for 10.0 dB)
}

impl CatServer {
    pub fn new(frequency: Arc<AtomicU64>) -> Self {
        Self {
            frequency,
            attenuation: None,
        }
    }

    pub fn with_attenuation(mut self, attenuation: Arc<AtomicU64>) -> Self {
        self.attenuation = Some(attenuation);
        self
    }

    pub async fn run(&self, addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("CAT server listening on {}", addr);

        loop {
            let (socket, peer_addr) = match listener.accept().await {
                Ok(res) => res,
                Err(e) => {
                    error!("Failed to accept CAT client connection: {}", e);
                    continue;
                }
            };
            info!("New CAT client connected: {}", peer_addr);

            let frequency = self.frequency.clone();
            let attenuation = self.attenuation.clone();

            tokio::spawn(async move {
                let (reader, mut writer) = tokio::io::split(socket);
                let mut lines = FramedRead::new(reader, LinesCodec::new());

                while let Some(line_res) = lines.next().await {
                    let line = match line_res {
                        Ok(l) => l,
                        Err(e) => {
                            error!("CAT client {} error: {}", peer_addr, e);
                            return;
                        }
                    };

                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.is_empty() {
                        continue;
                    }

                    match parts[0] {
                        "F" | "\\set_freq" => {
                            if parts.len() > 1 {
                                if let Ok(f) = parts[1].parse::<u64>() {
                                    frequency.store(f, Ordering::SeqCst);
                                    let _ = writer.write_all(b"RPRT 0\n").await;
                                } else {
                                    let _ = writer.write_all(b"RPRT 1\n").await;
                                }
                            } else {
                                let _ = writer.write_all(b"RPRT 1\n").await;
                            }
                        }
                        "f" | "\\get_freq" => {
                            let f = frequency.load(Ordering::SeqCst);
                            let _ = writer.write_all(format!("{}\n", f).as_bytes()).await;
                        }
                        // rigctld attenuation level: \set_level ATT <val_dB> or L ATT <val_dB>
                        "\\set_level" | "L" => {
                            if parts.len() > 2 && parts[1].eq_ignore_ascii_case("ATT") {
                                if let Some(ref att_atomic) = attenuation {
                                    if let Ok(val) = parts[2].parse::<f64>() {
                                        // Store as tenths of dB
                                        let tenths = (val.abs() * 10.0).round() as u64;
                                        att_atomic.store(tenths, Ordering::SeqCst);
                                        let _ = writer.write_all(b"RPRT 0\n").await;
                                    } else {
                                        let _ = writer.write_all(b"RPRT 1\n").await;
                                    }
                                } else {
                                    let _ = writer.write_all(b"RPRT 1\n").await;
                                }
                            } else {
                                let _ = writer.write_all(b"RPRT 0\n").await;
                            }
                        }
                        // rigctld get attenuation level: \get_level ATT or l ATT
                        "\\get_level" | "l" => {
                            if parts.len() > 1 && parts[1].eq_ignore_ascii_case("ATT") {
                                if let Some(ref att_atomic) = attenuation {
                                    let tenths = att_atomic.load(Ordering::SeqCst);
                                    let val = tenths as f64 / 10.0;
                                    let _ =
                                        writer.write_all(format!("{:.1}\n", val).as_bytes()).await;
                                } else {
                                    let _ = writer.write_all(b"0\n").await;
                                }
                            } else {
                                let _ = writer.write_all(b"RPRT 0\n").await;
                            }
                        }
                        _ => {
                            // Ignore unknown commands for now
                            let _ = writer.write_all(b"RPRT 0\n").await;
                        }
                    }
                }
                info!("CAT client {} disconnected.", peer_addr);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;

    #[tokio::test]
    async fn test_cat_freq_update() {
        let freq = Arc::new(AtomicU64::new(144000000));
        let server_freq = freq.clone();

        server_freq.store(145000000, Ordering::SeqCst);
        assert_eq!(freq.load(Ordering::SeqCst), 145000000);
    }

    #[tokio::test]
    async fn test_cat_server_network_commands() {
        let freq = Arc::new(AtomicU64::new(144000000));
        let att = Arc::new(AtomicU64::new(100)); // 10.0 dB

        let cat_server = CatServer::new(freq.clone()).with_attenuation(att.clone());
        let addr = "127.0.0.1:24532";

        tokio::spawn(async move {
            let _ = cat_server.run(addr).await;
        });

        // Give server a moment to bind
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut stream = TcpStream::connect(addr).await.expect("Failed to connect");
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        // 1. Get freq
        writer.write_all(b"f\n").await.unwrap();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(line.trim(), "144000000");
        line.clear();

        // 2. Set freq
        writer.write_all(b"F 145000000\n").await.unwrap();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(line.trim(), "RPRT 0");
        assert_eq!(freq.load(Ordering::SeqCst), 145000000);
        line.clear();

        // 3. Get attenuation
        writer.write_all(b"\\get_level ATT\n").await.unwrap();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(line.trim(), "10.0");
        line.clear();

        // 4. Set attenuation
        writer.write_all(b"\\set_level ATT 15.5\n").await.unwrap();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(line.trim(), "RPRT 0");
        assert_eq!(att.load(Ordering::SeqCst), 155);
        line.clear();

        // 5. Short form L ATT / l ATT
        writer.write_all(b"L ATT 20.0\n").await.unwrap();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(line.trim(), "RPRT 0");
        assert_eq!(att.load(Ordering::SeqCst), 200);
        line.clear();

        writer.write_all(b"l ATT\n").await.unwrap();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(line.trim(), "20.0");
    }
}
