mod tnc_server;

use crate::tnc_server::{TncKissServer, TncKissWsServer};
use anyhow::Context;
use clap::Parser;
use esp_pdu_protocol::{
    HardwareCommand, KissCommand, MAX_TX_POWER_QDBM, MIN_TX_POWER_QDBM, PhyRate,
    channel_to_freq_hz, freq_hz_to_amateur_channel, is_valid_uart_baud,
};
use futures::{SinkExt, StreamExt};
use sdr_pdu_utils::cat::CatServer;
use sdr_pdu_utils::kiss::{KissDecoder, KissEncoder};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU8, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::{broadcast, mpsc};
use tokio_serial::{SerialPort, SerialPortBuilderExt, SerialStream};
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

    /// Target serial port baud rate (boots at 115200 then negotiates dynamically via SetUartBaud)
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

    /// Optional amateur radio callsign for periodic fail-safe ID beacon (transmitted only when TX occurred)
    #[arg(long)]
    callsign: Option<String>,

    /// Callsign ID beacon interval in seconds (used when --callsign is set)
    #[arg(long, default_value_t = 600)]
    beacon_interval_secs: u64,
}

#[derive(Clone)]
struct ActiveRadioConfig {
    phy_rate: PhyRate,
    peer_mac: [u8; 6],
    tx_delay: Option<u8>,
    persistence: Option<u8>,
    slot_time: Option<u8>,
    tx_tail: Option<u8>,
    full_duplex: Option<u8>,
    target_baud: u32,
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
    let mut payload = [0u8; 24];
    let n = cmd.encode(&mut payload)?;
    let mut frame = Vec::with_capacity(n + 1);
    frame.push(KissCommand::SetHardware as u8);
    frame.extend_from_slice(&payload[..n]);
    Some(frame)
}

async fn drain_serial(serial: &mut SerialStream) {
    let mut drain_buf = [0u8; 256];
    while let Ok(Ok(n)) =
        tokio::time::timeout(Duration::from_millis(20), serial.read(&mut drain_buf)).await
    {
        if n == 0 {
            break;
        }
    }
}

async fn connect_and_configure_serial(
    port_path: &str,
    target_baud: u32,
) -> anyhow::Result<SerialStream> {
    let mut serial = tokio_serial::new(port_path, 115_200)
        .open_native_async()
        .with_context(|| format!("Failed to open serial port {}", port_path))?;

    // Deassert DTR/RTS so ESP32 auto-reset circuits stay in normal run mode.
    let _ = serial.write_data_terminal_ready(false);
    let _ = serial.write_request_to_send(false);

    // Allow time for the ESP32 to finish booting if opening the serial port toggled DTR/RTS.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    drain_serial(&mut serial).await;

    if target_baud != 115_200 {
        let frame = encode_hw_kiss_frame(HardwareCommand::SetUartBaud(target_baud))
            .context("Failed to encode SetUartBaud")?;
        let mut writer = FramedWrite::new(&mut serial, KissEncoder);
        writer
            .send(frame)
            .await
            .context("Failed to send SetUartBaud at 115200")?;
        drop(writer);
        tokio::time::sleep(Duration::from_millis(60)).await;
        serial
            .set_baud_rate(target_baud)
            .with_context(|| format!("Failed to switch host serial baud to {}", target_baud))?;
        drain_serial(&mut serial).await;
    }

    Ok(serial)
}

async fn push_active_config<W>(
    writer: &mut FramedWrite<W, KissEncoder>,
    ch: u8,
    qdbm: u8,
    cfg: &ActiveRadioConfig,
) -> anyhow::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    for cmd in [
        HardwareCommand::SetChannel(ch),
        HardwareCommand::SetTxPowerQdbm(qdbm),
        HardwareCommand::SetPhyRate(cfg.phy_rate),
        HardwareCommand::SetPeerMac(cfg.peer_mac),
    ] {
        if let Some(frame) = encode_hw_kiss_frame(cmd) {
            writer.send(frame).await?;
        }
    }
    for (cmd_byte, val_opt) in [
        (KissCommand::TxDelay as u8, cfg.tx_delay),
        (KissCommand::Persistence as u8, cfg.persistence),
        (KissCommand::SlotTime as u8, cfg.slot_time),
        (KissCommand::TxTail as u8, cfg.tx_tail),
        (KissCommand::FullDuplex as u8, cfg.full_duplex),
    ] {
        if let Some(val) = val_opt {
            writer.send(vec![cmd_byte, val]).await?;
        }
    }
    if let Some(frame) = encode_hw_kiss_frame(HardwareCommand::GetStatus) {
        writer.send(frame).await?;
    }
    Ok(())
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

    if !is_valid_uart_baud(args.baud_rate) {
        anyhow::bail!(
            "Unsupported --baud-rate {} (allowed: 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600)",
            args.baud_rate
        );
    }

    if args.attenuation < 0.0 || !args.attenuation.is_finite() {
        anyhow::bail!("Attenuation must be a non-negative number in dB (0.0 to 18.0)");
    }
    if args.attenuation > 18.0 {
        warn!(
            "Attenuation {:.1} dB exceeds 18.0 dB; TX power will be clamped to 2.00 dBm minimum",
            args.attenuation
        );
    }

    let callsign = match args.callsign {
        Some(raw) => {
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() || trimmed.len() > 32 {
                anyhow::bail!("--callsign must be 1..=32 non-whitespace characters");
            }
            if args.beacon_interval_secs == 0 {
                anyhow::bail!("--beacon-interval-secs must be greater than 0");
            }
            Some(trimmed)
        }
        None => None,
    };

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
        "Starting esp-pdu-tnc on {} (target {} baud, ch={}, {} Hz, {:.2} dBm, {:?}, peer={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X})",
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

    let (to_tnc_tx, mut to_tnc_rx) = mpsc::channel::<Vec<u8>>(128);
    let (from_tnc_tx, _) = broadcast::channel::<Vec<u8>>(128);

    let freq_atomic = Arc::new(AtomicU64::new(args.frequency));
    let att_atomic = Arc::new(AtomicU64::new(initial_att_tenths));
    let applied_ch = Arc::new(AtomicU8::new(initial_ch));
    let applied_qdbm = Arc::new(AtomicU8::new(initial_qdbm));
    let cat_pending_ch = Arc::new(AtomicU8::new(0));
    let cat_pending_qdbm = Arc::new(AtomicU8::new(0));
    let last_rssi_atomic = Arc::new(AtomicI8::new(0));
    let tx_activity_since_beacon = Arc::new(AtomicBool::new(false));

    let serial_port_path = args.serial_port.clone();
    let from_tnc_tx_clone = from_tnc_tx.clone();
    let rssi_rx = last_rssi_atomic.clone();
    let freq_rx = freq_atomic.clone();
    let att_rx = att_atomic.clone();
    let applied_ch_rx = applied_ch.clone();
    let applied_qdbm_rx = applied_qdbm.clone();
    let cat_pending_ch_rx = cat_pending_ch.clone();
    let cat_pending_qdbm_rx = cat_pending_qdbm.clone();
    let tx_activity_serial = tx_activity_since_beacon.clone();

    let serial_manager_fut = async move {
        let mut active_cfg = ActiveRadioConfig {
            phy_rate,
            peer_mac,
            tx_delay: None,
            persistence: None,
            slot_time: None,
            tx_tail: None,
            full_duplex: None,
            target_baud: args.baud_rate,
        };

        'reconnect: loop {
            let serial =
                match connect_and_configure_serial(&serial_port_path, active_cfg.target_baud).await
                {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(
                            "Serial connection to {} failed ({:#}); retrying in 1s...",
                            serial_port_path, e
                        );
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue 'reconnect;
                    }
                };

            info!(
                "Connected to ESP32 TNC on {} at {} baud",
                serial_port_path, active_cfg.target_baud
            );

            let (serial_rx, serial_tx) = tokio::io::split(serial);
            let mut reader = FramedRead::new(serial_rx, KissDecoder);
            let mut writer = FramedWrite::new(serial_tx, KissEncoder);

            if let Err(e) = push_active_config(
                &mut writer,
                applied_ch_rx.load(Ordering::SeqCst),
                applied_qdbm_rx.load(Ordering::SeqCst),
                &active_cfg,
            )
            .await
            {
                warn!(
                    "Failed to push initial TNC configuration ({}); reconnecting...",
                    e
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue 'reconnect;
            }

            let mut seen_nonzero_counters = false;
            let mut last_src_mac = [0u8; 6];
            let mut missed_heartbeats: u8 = 0;
            let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(3));
            heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    cmd_opt = to_tnc_rx.recv() => {
                        let Some(frame) = cmd_opt else {
                            anyhow::bail!("Internal TX command channel closed");
                        };
                        let mut baud_change_to = None;
                        if !frame.is_empty() {
                            match KissCommand::from_type_byte(frame[0]) {
                                Some(KissCommand::DataFrame) => {
                                    tx_activity_serial.store(true, Ordering::SeqCst);
                                }
                                Some(KissCommand::TxDelay) if frame.len() >= 2 => {
                                    active_cfg.tx_delay = Some(frame[1]);
                                }
                                Some(KissCommand::Persistence) if frame.len() >= 2 => {
                                    active_cfg.persistence = Some(frame[1]);
                                }
                                Some(KissCommand::SlotTime) if frame.len() >= 2 => {
                                    active_cfg.slot_time = Some(frame[1]);
                                }
                                Some(KissCommand::TxTail) if frame.len() >= 2 => {
                                    active_cfg.tx_tail = Some(frame[1]);
                                }
                                Some(KissCommand::FullDuplex) if frame.len() >= 2 => {
                                    active_cfg.full_duplex = Some(frame[1]);
                                }
                                Some(KissCommand::SetHardware) => {
                                    match HardwareCommand::decode(&frame[1..]) {
                                        Some(HardwareCommand::SetChannel(ch)) => {
                                            applied_ch_rx.store(ch, Ordering::SeqCst);
                                        }
                                        Some(HardwareCommand::SetFrequencyHz(hz)) => {
                                            if let Some(ch) = freq_hz_to_amateur_channel(hz) {
                                                applied_ch_rx.store(ch, Ordering::SeqCst);
                                            }
                                        }
                                        Some(HardwareCommand::SetTxPowerQdbm(qdbm)) => {
                                            applied_qdbm_rx.store(qdbm, Ordering::SeqCst);
                                        }
                                        Some(HardwareCommand::SetPhyRate(rate)) => {
                                            active_cfg.phy_rate = rate;
                                        }
                                        Some(HardwareCommand::SetPeerMac(mac)) => {
                                            active_cfg.peer_mac = mac;
                                        }
                                        Some(HardwareCommand::SetUartBaud(baud))
                                            if is_valid_uart_baud(baud)
                                                && baud != active_cfg.target_baud =>
                                        {
                                            active_cfg.target_baud = baud;
                                            baud_change_to = Some(baud);
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }

                        if let Err(e) = writer.send(frame).await {
                            warn!("Serial write error ({}); reconnecting...", e);
                            break;
                        }
                        if let Some(new_baud) = baud_change_to {
                            info!("Switching host serial baud rate to {}...", new_baud);
                            tokio::time::sleep(Duration::from_millis(60)).await;
                            continue 'reconnect;
                        }
                    }
                    res_opt = reader.next() => {
                        match res_opt {
                            Some(Ok(frame)) => {
                                if frame.is_empty() {
                                    continue;
                                }
                                missed_heartbeats = 0;
                                match KissCommand::from_type_byte(frame[0]) {
                                    Some(KissCommand::DataFrame) => {
                                        let rssi = rssi_rx.load(Ordering::Relaxed);
                                        info!(
                                            "RX PDU ({} bytes, src={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}, last_rssi={} dBm)",
                                            frame.len() - 1,
                                            last_src_mac[0],
                                            last_src_mac[1],
                                            last_src_mac[2],
                                            last_src_mac[3],
                                            last_src_mac[4],
                                            last_src_mac[5],
                                            rssi
                                        );
                                        let _ = from_tnc_tx_clone.send(frame);
                                    }
                                    Some(KissCommand::SetHardware) => {
                                        match HardwareCommand::decode(&frame[1..]) {
                                            Some(HardwareCommand::Status {
                                                channel,
                                                tx_power_qdbm,
                                                phy_rate: status_rate,
                                                rx_count,
                                                tx_count,
                                                last_rssi,
                                                last_src_mac: src_mac,
                                            }) => {
                                                if seen_nonzero_counters && rx_count == 0 && tx_count == 0 {
                                                    warn!("Detected ESP32 reboot (counters reset to 0); re-applying radio configuration");
                                                    seen_nonzero_counters = false;
                                                    if active_cfg.target_baud != 115_200 {
                                                        break;
                                                    }
                                                    if let Err(e) = push_active_config(
                                                        &mut writer,
                                                        applied_ch_rx.load(Ordering::SeqCst),
                                                        applied_qdbm_rx.load(Ordering::SeqCst),
                                                        &active_cfg,
                                                    )
                                                    .await
                                                    {
                                                        warn!("Failed to re-push config after ESP32 reboot ({}); reconnecting...", e);
                                                        break;
                                                    }
                                                    continue;
                                                }
                                                if rx_count > 0 || tx_count > 0 {
                                                    seen_nonzero_counters = true;
                                                }

                                                last_src_mac = src_mac;
                                                rssi_rx.store(last_rssi, Ordering::Relaxed);
                                                applied_ch_rx.store(channel, Ordering::SeqCst);
                                                applied_qdbm_rx.store(tx_power_qdbm, Ordering::SeqCst);

                                                let pending_ch = cat_pending_ch_rx.swap(0, Ordering::SeqCst);
                                                if (pending_ch == 0 || pending_ch == channel)
                                                    && freq_hz_to_amateur_channel(freq_rx.load(Ordering::SeqCst))
                                                        != Some(channel)
                                                    && let Some(hz) = channel_to_freq_hz(channel)
                                                {
                                                    freq_rx.store(hz, Ordering::SeqCst);
                                                }

                                                let pending_qdbm = cat_pending_qdbm_rx.swap(0, Ordering::SeqCst);
                                                if (pending_qdbm == 0 || pending_qdbm == tx_power_qdbm)
                                                    && attenuation_tenths_to_qdbm(att_rx.load(Ordering::SeqCst))
                                                        != tx_power_qdbm
                                                {
                                                    att_rx.store(
                                                        qdbm_to_attenuation_tenths(tx_power_qdbm),
                                                        Ordering::SeqCst,
                                                    );
                                                }

                                                info!(
                                                    "ESP32 status: ch={} ({:?} Hz), power={:.2} dBm, rate={:?}, rx={}, tx={}, rssi={} dBm, last_src={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                                                    channel,
                                                    channel_to_freq_hz(channel),
                                                    f64::from(tx_power_qdbm) / 4.0,
                                                    status_rate,
                                                    rx_count,
                                                    tx_count,
                                                    last_rssi,
                                                    src_mac[0],
                                                    src_mac[1],
                                                    src_mac[2],
                                                    src_mac[3],
                                                    src_mac[4],
                                                    src_mac[5]
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
                            Some(Err(e)) => {
                                warn!("Serial read error ({}); reconnecting...", e);
                                break;
                            }
                            None => {
                                warn!("Serial port {} closed; reconnecting...", serial_port_path);
                                break;
                            }
                        }
                    }
                    _ = heartbeat_interval.tick() => {
                        if active_cfg.target_baud != 115_200 {
                            if missed_heartbeats >= 2 {
                                warn!("ESP32 unresponsive at {} baud (possible reboot to 115200); reconnecting...", active_cfg.target_baud);
                                break;
                            }
                            missed_heartbeats = missed_heartbeats.saturating_add(1);
                            if let Some(frame) = encode_hw_kiss_frame(HardwareCommand::GetStatus)
                                && writer.send(frame).await.is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
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

    let beacon_to_tnc = to_tnc_tx.clone();
    let beacon_interval_secs = args.beacon_interval_secs;
    let beacon_fut = async move {
        let Some(cs) = callsign else {
            return std::future::pending::<anyhow::Result<()>>().await;
        };
        let mut interval = tokio::time::interval(Duration::from_secs(beacon_interval_secs));
        interval.tick().await;
        loop {
            interval.tick().await;
            if tx_activity_since_beacon.swap(false, Ordering::SeqCst) {
                let id_text = format!("DE {} ESP-PDU-TNC", cs);
                let mut frame = Vec::with_capacity(1 + id_text.len());
                frame.push(KissCommand::DataFrame as u8);
                frame.extend_from_slice(id_text.as_bytes());
                beacon_to_tnc
                    .send(frame)
                    .await
                    .context("Beacon TX channel closed")?;
                info!("Transmitted amateur radio callsign ID beacon: {}", id_text);
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
    let cat_server = CatServer::new(freq_atomic)
        .with_attenuation(att_atomic)
        .with_rssi_dbm(last_rssi_atomic);

    if let Err(e) = tokio::try_join!(
        serial_manager_fut,
        cat_watch_fut,
        beacon_fut,
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
