use bytes::BytesMut;
use esp_pdu_protocol::{
    HardwareCommand, KissCommand, MAX_ESPNOW_PDU_LEN, channel_to_freq_hz,
    freq_hz_to_amateur_channel,
};
use futures::{SinkExt, StreamExt};
use sdr_pdu_utils::kiss::{KissDecoder, KissEncoder};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::codec::{Decoder, Encoder, FramedRead, FramedWrite};
use tracing::{error, info, warn};

/// Bidirectional TCP KISS server.
/// Forwards incoming KISS frames (data and hardware/timing commands) from TCP clients to the TNC,
/// and broadcasts received KISS frames from the TNC to all connected TCP clients.
pub struct TncKissServer {
    to_tnc: mpsc::Sender<Vec<u8>>,
    from_tnc: broadcast::Sender<Vec<u8>>,
}

impl TncKissServer {
    pub fn new(to_tnc: mpsc::Sender<Vec<u8>>, from_tnc: broadcast::Sender<Vec<u8>>) -> Self {
        Self { to_tnc, from_tnc }
    }

    pub async fn run(&self, addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("KISS server listening on {}", addr);
        self.run_on_listener(listener).await
    }

    pub async fn run_on_listener(&self, listener: TcpListener) -> anyhow::Result<()> {
        loop {
            let (socket, peer_addr) = match listener.accept().await {
                Ok(res) => res,
                Err(e) => {
                    error!("Failed to accept KISS client connection: {}", e);
                    continue;
                }
            };
            info!("New KISS client connected: {}", peer_addr);

            let to_tnc = self.to_tnc.clone();
            let mut rx_sub = self.from_tnc.subscribe();

            tokio::spawn(async move {
                let (read_half, write_half) = tokio::io::split(socket);
                let mut reader = FramedRead::new(read_half, KissDecoder);
                let mut writer = FramedWrite::new(write_half, KissEncoder);

                loop {
                    tokio::select! {
                        frame_opt = reader.next() => {
                            match frame_opt {
                                Some(Ok(frame)) => {
                                    if !forward_client_frame(frame, &to_tnc).await {
                                        return;
                                    }
                                }
                                Some(Err(e)) => {
                                    error!("KISS client {} error: {}", peer_addr, e);
                                    return;
                                }
                                None => {
                                    info!("KISS client {} disconnected.", peer_addr);
                                    return;
                                }
                            }
                        }
                        rx_res = recv_broadcast_frame(&mut rx_sub, peer_addr, "KISS") => {
                            let Some(frame) = rx_res else {
                                return;
                            };
                            if let Err(e) = writer.send(frame).await {
                                error!("Failed to send RX KISS frame to {}: {}", peer_addr, e);
                                return;
                            }
                        }
                    }
                }
            });
        }
    }
}

/// Bidirectional WebSocket KISS server.
pub struct TncKissWsServer {
    to_tnc: mpsc::Sender<Vec<u8>>,
    from_tnc: broadcast::Sender<Vec<u8>>,
}

impl TncKissWsServer {
    pub fn new(to_tnc: mpsc::Sender<Vec<u8>>, from_tnc: broadcast::Sender<Vec<u8>>) -> Self {
        Self { to_tnc, from_tnc }
    }

    pub async fn run(&self, addr: &str) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("KISS WebSocket server listening on ws://{}", addr);
        self.run_on_listener(listener).await
    }

    pub async fn run_on_listener(&self, listener: TcpListener) -> anyhow::Result<()> {
        loop {
            let (socket, peer) = match listener.accept().await {
                Ok(res) => res,
                Err(e) => {
                    error!("Failed to accept KISS WS client connection: {}", e);
                    continue;
                }
            };

            let to_tnc = self.to_tnc.clone();
            let mut rx_sub = self.from_tnc.subscribe();

            tokio::spawn(async move {
                let ws_stream = match accept_async(socket).await {
                    Ok(ws) => ws,
                    Err(e) => {
                        error!("WebSocket handshake error with {}: {}", peer, e);
                        return;
                    }
                };
                info!("New WebSocket connection from {}", peer);

                let (mut ws_write, mut ws_read) = ws_stream.split();
                let mut buf = BytesMut::new();
                let mut decoder = KissDecoder;
                let mut encoder = KissEncoder;

                loop {
                    tokio::select! {
                        msg_opt = ws_read.next() => {
                            match msg_opt {
                                Some(Ok(Message::Binary(data))) => {
                                    buf.extend_from_slice(&data);
                                    loop {
                                        match decoder.decode(&mut buf) {
                                            Ok(Some(frame)) => {
                                                if !forward_client_frame(frame, &to_tnc).await {
                                                    return;
                                                }
                                            }
                                            Ok(None) => break,
                                            Err(e) => {
                                                error!("WS KISS decode error from {}: {}", peer, e);
                                                return;
                                            }
                                        }
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None => {
                                    info!("WebSocket closed by {}", peer);
                                    return;
                                }
                                Some(Ok(_)) => {}
                                Some(Err(e)) => {
                                    error!("WebSocket error from {}: {}", peer, e);
                                    return;
                                }
                            }
                        }
                        rx_res = recv_broadcast_frame(&mut rx_sub, peer, "WS") => {
                            let Some(frame) = rx_res else {
                                return;
                            };
                            let mut out = BytesMut::new();
                            if encoder.encode(frame, &mut out).is_ok()
                                && ws_write.send(Message::Binary(out.freeze())).await.is_err()
                            {
                                return;
                            }
                        }
                    }
                }
            });
        }
    }
}

async fn recv_broadcast_frame(
    rx_sub: &mut broadcast::Receiver<Vec<u8>>,
    peer: std::net::SocketAddr,
    label: &str,
) -> Option<Vec<u8>> {
    loop {
        match rx_sub.recv().await {
            Ok(frame) => return Some(frame),
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("{} client {} lagged behind by {} frames", label, peer, n);
            }
            Err(broadcast::error::RecvError::Closed) => return None,
        }
    }
}

async fn forward_client_frame(frame: Vec<u8>, to_tnc: &mpsc::Sender<Vec<u8>>) -> bool {
    if frame.is_empty() {
        return true;
    }

    let Some(cmd) = KissCommand::from_type_byte(frame[0]) else {
        warn!("Ignoring unsupported KISS command byte: 0x{:02X}", frame[0]);
        return true;
    };

    if cmd == KissCommand::DataFrame {
        let pdu_len = frame.len() - 1;
        if pdu_len == 0 || pdu_len > MAX_ESPNOW_PDU_LEN {
            warn!(
                "Dropping KISS DataFrame with invalid PDU length {} (max {} bytes for ESP-NOW)",
                pdu_len, MAX_ESPNOW_PDU_LEN
            );
            return true;
        }
    } else if cmd == KissCommand::SetHardware {
        match HardwareCommand::decode(&frame[1..]) {
            Some(HardwareCommand::SetChannel(ch)) if channel_to_freq_hz(ch).is_none() => {
                warn!("Dropping out-of-band KISS SetChannel({})", ch);
                return true;
            }
            Some(HardwareCommand::SetFrequencyHz(hz))
                if freq_hz_to_amateur_channel(hz).is_none() =>
            {
                warn!("Dropping out-of-band KISS SetFrequencyHz({})", hz);
                return true;
            }
            Some(HardwareCommand::SetUartBaud(baud))
                if !esp_pdu_protocol::is_valid_uart_baud(baud) =>
            {
                warn!("Dropping unsupported KISS SetUartBaud({})", baud);
                return true;
            }
            None => {
                warn!("Dropping malformed KISS SetHardware frame");
                return true;
            }
            _ => {}
        }
    }

    if let Err(e) = to_tnc.send(frame).await {
        error!("Failed to forward KISS frame to serial task: {}", e);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_forward_all_supported_kiss_commands() {
        let (tx, mut rx) = mpsc::channel(16);

        let frames = vec![
            vec![0x00, 0x41, 0x42],                               // DataFrame
            vec![0x01, 20],                                       // TxDelay
            vec![0x02, 63],                                       // Persistence
            vec![0x03, 10],                                       // SlotTime
            vec![0x04, 5],                                        // TxTail
            vec![0x05, 1],                                        // FullDuplex
            vec![0x06, 0x01, 3],                                  // SetHardware: SetChannel
            vec![0x06, 0x05],                                     // SetHardware: GetStatus
            vec![0x06, 0x08],                                     // SetHardware: GetPeerMac
            vec![0x06, 0x07, 0x48, 0x27, 0xE2, 0xE4, 0x74, 0x2C], // SetHardware: SetPeerMac
            vec![0xFF],                                           // Return
        ];

        for frame in &frames {
            assert!(forward_client_frame(frame.clone(), &tx).await);
            assert_eq!(rx.recv().await.as_ref(), Some(frame));
        }

        // Empty or oversized (>250B) DataFrames and malformed/out-of-band SetHardware frames must be dropped
        assert!(forward_client_frame(vec![0x00], &tx).await);
        assert!(forward_client_frame(vec![0x00; MAX_ESPNOW_PDU_LEN + 2], &tx).await);
        assert!(forward_client_frame(vec![0x06], &tx).await);
        assert!(forward_client_frame(vec![0x06, 0x01, 11], &tx).await);
        assert!(forward_client_frame(vec![0x06, 0x09, 0x00, 0x00, 0x30, 0x39], &tx).await);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_tcp_kiss_server_bidirectional_loopback() {
        let (to_tnc_tx, mut to_tnc_rx) = mpsc::channel(16);
        let (from_tnc_tx, _) = broadcast::channel(16);
        let server = TncKissServer::new(to_tnc_tx, from_tnc_tx.clone());

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move { server.run_on_listener(listener).await });

        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (read_half, write_half) = tokio::io::split(stream);
        let mut client_rx = FramedRead::new(read_half, KissDecoder);
        let mut client_tx = FramedWrite::new(write_half, KissEncoder);

        // Client -> TNC: DataFrame + SetHardware
        let tx_data = vec![0x00, b'H', b'I'];
        let tx_hw = vec![0x06, 0x01, 0x03];
        client_tx.send(tx_data.clone()).await.unwrap();
        client_tx.send(tx_hw.clone()).await.unwrap();
        assert_eq!(to_tnc_rx.recv().await.as_ref(), Some(&tx_data));
        assert_eq!(to_tnc_rx.recv().await.as_ref(), Some(&tx_hw));

        // TNC -> Client: Status + DataFrame
        let rx_status = vec![0x06, 0x85, 3, 80, 0x00, 0, 0, 0, 1, 0, 0, 0, 1, 0xDE];
        let rx_data = vec![0x00, b'O', b'K'];
        from_tnc_tx.send(rx_status.clone()).unwrap();
        from_tnc_tx.send(rx_data.clone()).unwrap();

        assert_eq!(client_rx.next().await.unwrap().unwrap(), rx_status);
        assert_eq!(client_rx.next().await.unwrap().unwrap(), rx_data);

        server_task.abort();
    }

    #[tokio::test]
    async fn test_ws_kiss_server_bidirectional_loopback() {
        let (to_tnc_tx, mut to_tnc_rx) = mpsc::channel(16);
        let (from_tnc_tx, _) = broadcast::channel(16);
        let server = TncKissWsServer::new(to_tnc_tx, from_tnc_tx.clone());

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move { server.run_on_listener(listener).await });

        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("ws://{}", addr))
            .await
            .unwrap();
        let (mut ws_tx, mut ws_rx) = ws_stream.split();
        let mut encoder = KissEncoder;
        let mut decoder = KissDecoder;

        // Client -> TNC over WebSocket: DataFrame + SetHardware
        let tx_data = vec![0x00, b'W', b'S'];
        let tx_hw = vec![0x06, 0x05];
        let mut buf = BytesMut::new();
        encoder.encode(tx_data.clone(), &mut buf).unwrap();
        encoder.encode(tx_hw.clone(), &mut buf).unwrap();
        ws_tx.send(Message::Binary(buf.freeze())).await.unwrap();

        assert_eq!(to_tnc_rx.recv().await.as_ref(), Some(&tx_data));
        assert_eq!(to_tnc_rx.recv().await.as_ref(), Some(&tx_hw));

        // TNC -> Client over WebSocket: Status + DataFrame
        let rx_status = vec![0x06, 0x85, 1, 80, 0x00, 0, 0, 0, 1, 0, 0, 0, 1, 0xE0];
        let rx_data = vec![0x00, b'R', b'X'];
        from_tnc_tx.send(rx_status.clone()).unwrap();
        from_tnc_tx.send(rx_data.clone()).unwrap();

        let mut rx_buf = BytesMut::new();
        let mut received = Vec::new();
        while received.len() < 2 {
            if let Some(Ok(Message::Binary(data))) = ws_rx.next().await {
                rx_buf.extend_from_slice(&data);
                while let Some(frame) = decoder.decode(&mut rx_buf).unwrap() {
                    received.push(frame);
                }
            }
        }
        assert_eq!(received, vec![rx_status, rx_data]);

        server_task.abort();
    }
}
