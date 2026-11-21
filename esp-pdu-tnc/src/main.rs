mod tnc_server;

use crate::tnc_server::{TncKissServer, TncKissWsServer};
use anyhow::Context;
use clap::Parser;
use esp_pdu_protocol::{
    HardwareCommand, KissCommand, MAX_TX_POWER_QDBM, MIN_TX_POWER_QDBM, PhyRate,
    channel_to_freq_hz, freq_hz_to_amateur_channel,
};
use futures::{SinkExt, StreamExt};
use sdr_pdu_utils::cat::CatServer;
use sdr_pdu_utils::kiss::{KissDecoder, KissEncoder};
use std::sync::Arc;
use std::sync::atomic::{AtomicI8, AtomicU8, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::{broadcast, mpsc};
use tokio_serial::{SerialPort, SerialPortBuilderExt};
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Host KISS + CAT server for ESP32 ESP-NOW PDU TNC"
)]
struct Args {
    /// Listen address for KISS and CAT servers (defaults to loopback for local security)
    #[arg(short, long, default_value = "127.0.0.1")]
    listen: String,

    /// TCP port for the KISS server
    #[arg(long, default_value_t = 8001)]
    kiss_port: u16,

    /// TCP port for the KISS WebSocket server
    #[arg(long, default_value_t = 8002)]
    kiss_ws_port: u16,

    /// TCP port for the CAT (rigctld) server
    #[arg(long, default_value_t = 4532)]
    cat_port: u16,

    /// Serial port connected to the ESP32 TNC (e.g. /dev/ttyACM0 or /dev/ttyUSB0)
    #[arg(short, long)]
    serial_port: String,

    /// Serial port baud rate
    #[arg(short, long, default_value_t = 115200)]
    baud_rate: u32,

    /// Initial center frequency in Hz (2400-2450 MHz French amateur band, channels 1-6)
    #[arg(short, long, default_value_t = 2_412_000_000)]
    frequency: u64,

    /// TX attenuation in dB (0.0 = 20 dBm max, up to 18.0 dB = 2 dBm min)
    #[arg(short, long, default_value_t = 0.0)]
    attenuation: f64,

    /// ESP-NOW PHY data rate (1m, 2m, 5.5m, 11m, 6m, 9m, 12m, 18m, 24m, 36m, 48m, 54m, mcs0..mcs7, lr-512k, lr-250k)
    #[arg(long, default_value = "1m")]
    phy_rate: String,

    /// Destination peer MAC address for TX (default: ff:ff:ff:ff:ff:ff for broadcast)
    #[arg(long, default_value = "ff:ff:ff:ff:ff:ff")]
    peer_mac: String,
}

fn parse_phy_rate(s: &str) -> anyhow::Result<PhyRate> {
    match s.to_ascii_lowercase().as_str() {
        "1m" => Ok(PhyRate::Rate1ML),
        "2m" => Ok(PhyRate::Rate2ML),
        "5.5m" => Ok(PhyRate::Rate5_5ML),
        "11m" => Ok(PhyRate::Rate11ML),
        "6m" => Ok(PhyRate::Rate6M),
        "9m" => Ok(PhyRate::Rate9M),
        "12m" => Ok(PhyRate::Rate12M),
        "18m" => Ok(PhyRate::Rate18M),
        "24m" => Ok(PhyRate::Rate24M),
        "36m" => Ok(PhyRate::Rate36M),
        "48m" => Ok(PhyRate::Rate48M),
        "54m" => Ok(PhyRate::Rate54M),
        "mcs0" => Ok(PhyRate::RateMcs0Lgi),
        "mcs1" => Ok(PhyRate::RateMcs1Lgi),
        "mcs2" => Ok(PhyRate::RateMcs2Lgi),
        "mcs3" => Ok(PhyRate::RateMcs3Lgi),
        "mcs4" => Ok(PhyRate::RateMcs4Lgi),
        "mcs5" => Ok(PhyRate::RateMcs5Lgi),
        "mcs6" => Ok(PhyRate::RateMcs6Lgi),
        "mcs7" => Ok(PhyRate::RateMcs7Lgi),
        "lr-512k" | "lr" => Ok(PhyRate::RateLr512K),
        "lr-250k" => Ok(PhyRate::RateLr250K),
        _ => anyhow::bail!("Unsupported PHY rate '{}'", s),
    }
}

fn parse_mac_address(s: &str) -> anyhow::Result<[u8; 6]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        anyhow::bail!("Invalid MAC address '{}' (expected AA:BB:CC:DD:EE:FF)", s);
    }
    let mut mac = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        if part.len() != 2 {
            anyhow::bail!(
                "Invalid hex byte '{}' in MAC address '{}' (expected 2 hex digits)",
                part,
                s
            );
        }
        mac[i] = u8::from_str_radix(part, 16)
            .with_context(|| format!("Invalid hex byte '{}' in MAC address '{}'", part, s))?;
    }
    Ok(mac)
}

fn attenuation_tenths_to_qdbm(tenths_db: u64) -> u8 {
    let att_db = (tenths_db as f64) / 10.0;
    let power_dbm = (20.0 - att_db).clamp(2.0, 20.0);
    let qdbm = (power_dbm * 4.0).round() as u8;
    qdbm.clamp(MIN_TX_POWER_QDBM, MAX_TX_POWER_QDBM)
}

fn qdbm_to_attenuation_tenths(qdbm: u8) -> u64 {
    let clamped = qdbm.clamp(MIN_TX_POWER_QDBM, MAX_TX_POWER_QDBM);
    let power_dbm = f64::from(clamped) / 4.0;
    ((20.0 - power_dbm).max(0.0) * 10.0).round() as u64
}

fn encode_hw_kiss_frame(cmd: HardwareCommand) -> Option<Vec<u8>> {
    let mut payload = [0u8; 16];
    let n = cmd.encode(&mut payload)?;
    let mut frame = Vec::with_capacity(n + 1);
    frame.push(KissCommand::SetHardware as u8);
    frame.extend_from_slice(&payload[..n]);
    Some(frame)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();

    if args.attenuation < 0.0 || !args.attenuation.is_finite() {
        anyhow::bail!("Attenuation must be a non-negative number in dB (0.0 to 18.0)");
    }
    if args.attenuation > 18.0 {
        warn!(
            "Attenuation {:.1} dB exceeds 18.0 dB; TX power will be clamped to 2.00 dBm minimum",
            args.attenuation
        );
    }

    let initial_ch = freq_hz_to_amateur_channel(args.frequency).with_context(|| {
        format!(
            "Initial frequency {} Hz is outside legal French 2.4 GHz amateur Wi-Fi channels 1..=6 (2400-2450 MHz)",
            args.frequency
        )
    })?;
    let phy_rate = parse_phy_rate(&args.phy_rate)?;
    let peer_mac = parse_mac_address(&args.peer_mac)?;
    let initial_att_tenths = (args.attenuation * 10.0).round() as u64;
    let initial_qdbm = attenuation_tenths_to_qdbm(initial_att_tenths);

    info!(
        "Opening serial port {} at {} baud (initial ch={}, {} Hz, {:.2} dBm, {:?}, peer={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X})",
        args.serial_port,
        args.baud_rate,
        initial_ch,
        channel_to_freq_hz(initial_ch).unwrap_or(args.frequency),
        f64::from(initial_qdbm) / 4.0,
        phy_rate,
        peer_mac[0],
        peer_mac[1],
        peer_mac[2],
        peer_mac[3],
        peer_mac[4],
        peer_mac[5]
    );

    let mut serial = tokio_serial::new(&args.serial_port, args.baud_rate)
        .open_native_async()
        .with_context(|| format!("Failed to open serial port {}", args.serial_port))?;

    // Deassert DTR/RTS so ESP32 auto-reset circuits stay in normal run mode.
    let _ = serial.write_data_terminal_ready(false);
    let _ = serial.write_request_to_send(false);

    // Allow time for the ESP32 to finish booting if opening the serial port toggled DTR/RTS.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Drain any bootloader/startup bytes buffered before initial host configuration.
    let mut drain_buf = [0u8; 256];
    while let Ok(Ok(n)) =
        tokio::time::timeout(Duration::from_millis(20), serial.read(&mut drain_buf)).await
    {
        if n == 0 {
            break;
        }
    }

    let (serial_rx, serial_tx) = tokio::io::split(serial);

    let (to_tnc_tx, mut to_tnc_rx) = mpsc::channel::<Vec<u8>>(128);
    let (from_tnc_tx, _) = broadcast::channel::<Vec<u8>>(128);

    // Send initial radio configuration over KISS SetHardware
    for cmd in [
        HardwareCommand::SetChannel(initial_ch),
        HardwareCommand::SetTxPowerQdbm(initial_qdbm),
        HardwareCommand::SetPhyRate(phy_rate),
        HardwareCommand::SetPeerMac(peer_mac),
        HardwareCommand::GetStatus,
    ] {
        if let Some(frame) = encode_hw_kiss_frame(cmd) {
            let _ = to_tnc_tx.send(frame).await;
        }
    }

    let freq_atomic = Arc::new(AtomicU64::new(args.frequency));
    let att_atomic = Arc::new(AtomicU64::new(initial_att_tenths));
    let applied_ch = Arc::new(AtomicU8::new(initial_ch));
    let applied_qdbm = Arc::new(AtomicU8::new(initial_qdbm));
    let cat_pending_ch = Arc::new(AtomicU8::new(0));
    let cat_pending_qdbm = Arc::new(AtomicU8::new(0));
    let last_rssi_atomic = Arc::new(AtomicI8::new(0));

    let serial_tx_fut = async move {
        let mut writer = FramedWrite::new(serial_tx, KissEncoder);
        while let Some(frame) = to_tnc_rx.recv().await {
            writer
                .send(frame)
                .await
                .context("Serial write error (TNC disconnected)")?;
        }
        anyhow::Result::<()>::Err(anyhow::anyhow!("Serial TX command channel closed"))
    };

    let from_tnc_tx_clone = from_tnc_tx.clone();
    let rssi_rx = last_rssi_atomic.clone();
    let freq_rx = freq_atomic.clone();
    let att_rx = att_atomic.clone();
    let applied_ch_rx = applied_ch.clone();
    let applied_qdbm_rx = applied_qdbm.clone();
    let cat_pending_ch_rx = cat_pending_ch.clone();
    let cat_pending_qdbm_rx = cat_pending_qdbm.clone();
    let serial_rx_fut = async move {
        let mut reader = FramedRead::new(serial_rx, KissDecoder);
        while let Some(res) = reader.next().await {
            let frame = res.context("Serial read error (TNC disconnected)")?;
            if frame.is_empty() {
                continue;
            }
            match KissCommand::from_type_byte(frame[0]) {
                Some(KissCommand::DataFrame) => {
                    let rssi = rssi_rx.load(Ordering::Relaxed);
                    info!("RX PDU ({} bytes, last_rssi={} dBm)", frame.len() - 1, rssi);
                    let _ = from_tnc_tx_clone.send(frame);
                }
                Some(KissCommand::SetHardware) => {
                    match HardwareCommand::decode(&frame[1..]) {
                        Some(HardwareCommand::Status {
                            channel,
                            tx_power_qdbm,
                            phy_rate,
                            rx_count,
                            tx_count,
                            last_rssi,
                        }) => {
                            rssi_rx.store(last_rssi, Ordering::Relaxed);
                            applied_ch_rx.store(channel, Ordering::SeqCst);
                            applied_qdbm_rx.store(tx_power_qdbm, Ordering::SeqCst);

                            let pending_ch = cat_pending_ch_rx.load(Ordering::SeqCst);
                            if pending_ch == 0 || pending_ch == channel {
                                cat_pending_ch_rx.store(0, Ordering::SeqCst);
                                if freq_hz_to_amateur_channel(freq_rx.load(Ordering::SeqCst))
                                    != Some(channel)
                                    && let Some(hz) = channel_to_freq_hz(channel)
                                {
                                    freq_rx.store(hz, Ordering::SeqCst);
                                }
                            }

                            let pending_qdbm = cat_pending_qdbm_rx.load(Ordering::SeqCst);
                            if pending_qdbm == 0 || pending_qdbm == tx_power_qdbm {
                                cat_pending_qdbm_rx.store(0, Ordering::SeqCst);
                                if attenuation_tenths_to_qdbm(att_rx.load(Ordering::SeqCst))
                                    != tx_power_qdbm
                                {
                                    att_rx.store(
                                        qdbm_to_attenuation_tenths(tx_power_qdbm),
                                        Ordering::SeqCst,
                                    );
                                }
                            }

                            info!(
                                "ESP32 status: ch={} ({:?} Hz), power={:.2} dBm, rate={:?}, rx={}, tx={}, rssi={} dBm",
                                channel,
                                channel_to_freq_hz(channel),
                                f64::from(tx_power_qdbm) / 4.0,
                                phy_rate,
                                rx_count,
                                tx_count,
                                last_rssi
                            );
                        }
                        Some(HardwareCommand::PeerMac(mac)) => {
                            info!(
                                "ESP32 TX peer MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
                            );
                        }
                        _ => {}
                    }
                    let _ = from_tnc_tx_clone.send(frame);
                }
                _ => {}
            }
        }
        anyhow::Result::<()>::Err(anyhow::anyhow!("Serial RX stream ended (TNC disconnected)"))
    };

    // CAT parameter watcher task: translates CAT frequency & attenuation updates into KISS SetHardware
    let freq_watch = freq_atomic.clone();
    let att_watch = att_atomic.clone();
    let applied_ch_watch = applied_ch.clone();
    let applied_qdbm_watch = applied_qdbm.clone();
    let cat_pending_ch_watch = cat_pending_ch.clone();
    let cat_pending_qdbm_watch = cat_pending_qdbm.clone();
    let cat_to_tnc = to_tnc_tx.clone();
    let cat_watch_fut = async move {
        let mut last_freq = freq_watch.load(Ordering::SeqCst);
        let mut last_att = att_watch.load(Ordering::SeqCst);
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        loop {
            interval.tick().await;

            let cur_freq = freq_watch.load(Ordering::SeqCst);
            if cur_freq != last_freq {
                if let Some(ch) = freq_hz_to_amateur_channel(cur_freq) {
                    if ch != applied_ch_watch.swap(ch, Ordering::SeqCst) {
                        cat_pending_ch_watch.store(ch, Ordering::SeqCst);
                        info!(
                            "CAT frequency update: {} Hz -> Wi-Fi channel {}",
                            cur_freq, ch
                        );
                        if let Some(frame) = encode_hw_kiss_frame(HardwareCommand::SetChannel(ch)) {
                            cat_to_tnc
                                .send(frame)
                                .await
                                .context("Serial TX channel closed")?;
                        }
                    }
                    last_freq = cur_freq;
                } else {
                    warn!(
                        "CAT requested out-of-band frequency {} Hz (must be within French 2.4 GHz amateur channels 1..=6); reverting",
                        cur_freq
                    );
                    freq_watch.store(last_freq, Ordering::SeqCst);
                }
            }

            let cur_att = att_watch.load(Ordering::SeqCst);
            if cur_att != last_att {
                let qdbm = attenuation_tenths_to_qdbm(cur_att);
                if qdbm != applied_qdbm_watch.swap(qdbm, Ordering::SeqCst) {
                    cat_pending_qdbm_watch.store(qdbm, Ordering::SeqCst);
                    info!(
                        "CAT attenuation update: {:.1} dB -> TX power {:.2} dBm",
                        (cur_att as f64) / 10.0,
                        f64::from(qdbm) / 4.0
                    );
                    if let Some(frame) = encode_hw_kiss_frame(HardwareCommand::SetTxPowerQdbm(qdbm))
                    {
                        cat_to_tnc
                            .send(frame)
                            .await
                            .context("Serial TX channel closed")?;
                    }
                }
                last_att = cur_att;
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    let kiss_addr = format!("{}:{}", args.listen, args.kiss_port);
    let kiss_ws_addr = format!("{}:{}", args.listen, args.kiss_ws_port);
    let cat_addr = format!("{}:{}", args.listen, args.cat_port);

    let kiss_server = TncKissServer::new(to_tnc_tx.clone(), from_tnc_tx.clone());
    let kiss_ws_server = TncKissWsServer::new(to_tnc_tx, from_tnc_tx);
    let cat_server = CatServer::new(freq_atomic).with_attenuation(att_atomic);

    if let Err(e) = tokio::try_join!(
        serial_tx_fut,
        serial_rx_fut,
        cat_watch_fut,
        kiss_server.run(&kiss_addr),
        kiss_ws_server.run(&kiss_ws_addr),
        cat_server.run(&cat_addr),
    ) {
        error!("esp-pdu-tnc exiting: {:#}", e);
        return Err(e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_phy_rate() {
        assert_eq!(parse_phy_rate("1m").unwrap(), PhyRate::Rate1ML);
        assert_eq!(parse_phy_rate("LR").unwrap(), PhyRate::RateLr512K);
        assert_eq!(parse_phy_rate("lr-250k").unwrap(), PhyRate::RateLr250K);
        assert_eq!(parse_phy_rate("mcs0").unwrap(), PhyRate::RateMcs0Lgi);
        assert!(parse_phy_rate("invalid").is_err());
    }

    #[test]
    fn test_attenuation_to_qdbm() {
        assert_eq!(attenuation_tenths_to_qdbm(0), 80); // 20.0 dBm
        assert_eq!(attenuation_tenths_to_qdbm(100), 40); // 10.0 dB att -> 10.0 dBm
        assert_eq!(attenuation_tenths_to_qdbm(200), 8); // clamped to 2.0 dBm min
        assert_eq!(qdbm_to_attenuation_tenths(80), 0);
        assert_eq!(qdbm_to_attenuation_tenths(40), 100);
        assert_eq!(qdbm_to_attenuation_tenths(8), 180);
    }

    #[test]
    fn test_encode_hw_kiss_frame() {
        let frame = encode_hw_kiss_frame(HardwareCommand::SetChannel(3)).unwrap();
        assert_eq!(frame, vec![0x06, 0x01, 0x03]);
    }

    #[test]
    fn test_parse_mac_address() {
        assert_eq!(
            parse_mac_address("48:27:e2:e4:74:2c").unwrap(),
            [0x48, 0x27, 0xE2, 0xE4, 0x74, 0x2C]
        );
        assert!(parse_mac_address("48:27:e2").is_err());
        assert!(parse_mac_address("48:27:e2:e4:74:c").is_err());
        assert!(parse_mac_address("0048:27:e2:e4:74:2c").is_err());
        assert!(parse_mac_address("zz:27:e2:e4:74:2c").is_err());
    }
}
