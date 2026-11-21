#![no_std]
#![no_main]

extern crate alloc;

use core::sync::atomic::{AtomicI8, AtomicU32, AtomicU8, Ordering};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use enumset::EnumSet;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config as UartConfig, Uart, UartRx, UartTx};
use esp_hal::Async;
use esp_pdu_protocol::{
    encode_kiss_frame, freq_hz_to_amateur_channel, HardwareCommand, KissCommand, KissStreamDecoder,
    PhyRate, MAX_AMATEUR_CHANNEL, MAX_ESPNOW_PDU_LEN, MAX_KISS_FRAME_LEN, MAX_TX_POWER_QDBM,
    MIN_AMATEUR_CHANNEL, MIN_TX_POWER_QDBM,
};
use esp_wifi::esp_now::{EspNow, EspNowManager, PeerInfo, WifiPhyRate, BROADCAST_ADDRESS};
use esp_wifi::wifi::Protocol;

/// Maximum encoded KISS frame size: FEND + up to 2*(1 + 250) + FEND
const MAX_ENCODED_KISS_LEN: usize = MAX_KISS_FRAME_LEN * 2 + 2;

struct KissFrameBuf {
    data: [u8; MAX_KISS_FRAME_LEN],
    len: usize,
}

static UART_TX_QUEUE: Channel<CriticalSectionRawMutex, KissFrameBuf, 24> = Channel::new();
static ESPNOW_TX_QUEUE: Channel<CriticalSectionRawMutex, KissFrameBuf, 16> = Channel::new();

struct SharedRadioState {
    channel: AtomicU8,
    tx_power_qdbm: AtomicU8,
    phy_rate: AtomicU8,
    rx_count: AtomicU32,
    tx_count: AtomicU32,
    last_rssi: AtomicI8,
}

impl SharedRadioState {
    const fn new() -> Self {
        Self {
            channel: AtomicU8::new(1),
            tx_power_qdbm: AtomicU8::new(MAX_TX_POWER_QDBM),
            phy_rate: AtomicU8::new(PhyRate::Rate1ML as u8),
            rx_count: AtomicU32::new(0),
            tx_count: AtomicU32::new(0),
            last_rssi: AtomicI8::new(0),
        }
    }
}

struct TxConfig {
    tx_peer: [u8; 6],
    tx_delay_10ms: u8,
    persistence: u8,
    slot_time_10ms: u8,
    tx_tail_10ms: u8,
    full_duplex: bool,
}

impl Default for TxConfig {
    fn default() -> Self {
        Self {
            tx_peer: BROADCAST_ADDRESS,
            tx_delay_10ms: 0,
            persistence: 63,
            slot_time_10ms: 0,
            tx_tail_10ms: 0,
            full_duplex: false,
        }
    }
}

fn phy_rate_to_wifi(rate: PhyRate) -> WifiPhyRate {
    match rate {
        PhyRate::Rate1ML => WifiPhyRate::Rate1mL,
        PhyRate::Rate2ML => WifiPhyRate::Rate2m,
        PhyRate::Rate5_5ML => WifiPhyRate::Rate5mL,
        PhyRate::Rate11ML => WifiPhyRate::Rate11mL,
        PhyRate::Rate6M => WifiPhyRate::Rate6m,
        PhyRate::Rate9M => WifiPhyRate::Rate9m,
        PhyRate::Rate12M => WifiPhyRate::Rate12m,
        PhyRate::Rate18M => WifiPhyRate::Rate18m,
        PhyRate::Rate24M => WifiPhyRate::Rate24m,
        PhyRate::Rate36M => WifiPhyRate::Rate36m,
        PhyRate::Rate48M => WifiPhyRate::Rate48m,
        PhyRate::Rate54M => WifiPhyRate::Rate54m,
        PhyRate::RateMcs0Lgi => WifiPhyRate::RateMcs0Lgi,
        PhyRate::RateMcs1Lgi => WifiPhyRate::RateMcs1Lgi,
        PhyRate::RateMcs2Lgi => WifiPhyRate::RateMcs2Lgi,
        PhyRate::RateMcs3Lgi => WifiPhyRate::RateMcs3Lgi,
        PhyRate::RateMcs4Lgi => WifiPhyRate::RateMcs4Lgi,
        PhyRate::RateMcs5Lgi => WifiPhyRate::RateMcs5Lgi,
        PhyRate::RateMcs6Lgi => WifiPhyRate::RateMcs6Lgi,
        PhyRate::RateMcs7Lgi => WifiPhyRate::RateMcs7Lgi,
        PhyRate::RateLr512K => WifiPhyRate::RateLora500k,
        PhyRate::RateLr250K => WifiPhyRate::RateLora250k,
    }
}

fn apply_channel(manager: &EspNowManager<'_>, channel: u8) -> bool {
    if !(MIN_AMATEUR_CHANNEL..=MAX_AMATEUR_CHANNEL).contains(&channel) {
        return false;
    }
    manager.set_channel(channel).is_ok()
}

fn apply_tx_power(qdbm: u8) -> Option<u8> {
    let clamped = qdbm.clamp(MIN_TX_POWER_QDBM, MAX_TX_POWER_QDBM);
    let rc = unsafe { esp_wifi_sys::include::esp_wifi_set_max_tx_power(clamped as i8) };
    (rc == 0).then_some(clamped)
}

fn apply_phy_rate(manager: &EspNowManager<'_>, rate: PhyRate) -> bool {
    let protocols: EnumSet<Protocol> = if rate.is_long_range() {
        Protocol::P802D11LR.into()
    } else {
        Protocol::P802D11BGN.into()
    };
    manager.set_protocol(protocols).is_ok() && manager.set_rate(phy_rate_to_wifi(rate)).is_ok()
}

#[embassy_executor::task]
async fn uart_rx_task(mut rx: UartRx<'static, Async>) {
    let mut decoder = KissStreamDecoder::<MAX_KISS_FRAME_LEN>::new();
    let mut byte_buf = [0u8; 64];

    loop {
        match rx.read_async(&mut byte_buf).await {
            Ok(n) if n > 0 => {
                for &b in &byte_buf[..n] {
                    if let Some(frame) = decoder.push_byte(b) {
                        if frame.len() <= MAX_KISS_FRAME_LEN {
                            let mut msg = KissFrameBuf {
                                data: [0u8; MAX_KISS_FRAME_LEN],
                                len: frame.len(),
                            };
                            msg.data[..frame.len()].copy_from_slice(frame);
                            let _ = ESPNOW_TX_QUEUE.try_send(msg);
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(_) => {
                Timer::after(Duration::from_millis(5)).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn uart_tx_task(mut tx: UartTx<'static, Async>) {
    let mut encoded = [0u8; MAX_ENCODED_KISS_LEN];
    loop {
        let frame = UART_TX_QUEUE.receive().await;
        if let Some(enc_len) = encode_kiss_frame(&frame.data[..frame.len], &mut encoded) {
            let mut offset = 0;
            while offset < enc_len {
                match tx.write_async(&encoded[offset..enc_len]).await {
                    Ok(written) if written > 0 => offset += written,
                    _ => break,
                }
            }
            let _ = tx.flush_async().await;
        }
    }
}

fn send_status_to_host(shared: &SharedRadioState) {
    let mut msg = KissFrameBuf {
        data: [0u8; MAX_KISS_FRAME_LEN],
        len: 0,
    };
    msg.data[0] = KissCommand::SetHardware as u8;
    let status = HardwareCommand::Status {
        channel: shared.channel.load(Ordering::Relaxed),
        tx_power_qdbm: shared.tx_power_qdbm.load(Ordering::Relaxed),
        phy_rate: PhyRate::from_u8(shared.phy_rate.load(Ordering::Relaxed))
            .unwrap_or(PhyRate::Rate1ML),
        rx_count: shared.rx_count.load(Ordering::Relaxed),
        tx_count: shared.tx_count.load(Ordering::Relaxed),
        last_rssi: shared.last_rssi.load(Ordering::Relaxed),
    };
    if let Some(n) = status.encode(&mut msg.data[1..]) {
        msg.len = 1 + n;
        let _ = UART_TX_QUEUE.try_send(msg);
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let mut config = esp_hal::Config::default();
    config.cpu_clock = CpuClock::max();
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(72 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    // UART0 connected to the USB-UART bridge on both boards:
    // - ESP-WROOM-32: TX=GPIO1, RX=GPIO3
    // - Heltec WiFi Kit 32 V3.1 (ESP32-S3): TX=GPIO43, RX=GPIO44
    let uart_config = UartConfig::default().baudrate(115_200);
    #[cfg(feature = "esp32")]
    let uart0 = Uart::new_with_config(
        peripherals.UART0,
        uart_config,
        peripherals.GPIO3,
        peripherals.GPIO1,
    )
    .unwrap()
    .into_async();
    #[cfg(feature = "esp32s3")]
    let uart0 = Uart::new_with_config(
        peripherals.UART0,
        uart_config,
        peripherals.GPIO44,
        peripherals.GPIO43,
    )
    .unwrap()
    .into_async();

    let (uart_rx, uart_tx) = uart0.split();
    spawner.spawn(uart_rx_task(uart_rx)).ok();
    spawner.spawn(uart_tx_task(uart_tx)).ok();

    let timg1 = TimerGroup::new(peripherals.TIMG1);
    let wifi_init = esp_wifi::init(
        timg1.timer0,
        Rng::new(peripherals.RNG),
        peripherals.RADIO_CLK,
    )
    .unwrap();

    let esp_now = EspNow::new(&wifi_init, peripherals.WIFI).unwrap();
    let (manager, mut sender, mut receiver) = esp_now.split();
    let shared = SharedRadioState::new();

    if apply_channel(&manager, 1) {
        shared.channel.store(1, Ordering::Relaxed);
    }
    if let Some(qdbm) = apply_tx_power(MAX_TX_POWER_QDBM) {
        shared.tx_power_qdbm.store(qdbm, Ordering::Relaxed);
    }
    if apply_phy_rate(&manager, PhyRate::Rate1ML) {
        shared
            .phy_rate
            .store(PhyRate::Rate1ML as u8, Ordering::Relaxed);
    }

    send_status_to_host(&shared);

    let tx_fut = async {
        let mut tx_cfg = TxConfig::default();
        loop {
            let frame = ESPNOW_TX_QUEUE.receive().await;
            if frame.len == 0 {
                continue;
            }
            let Some(cmd) = KissCommand::from_type_byte(frame.data[0]) else {
                continue;
            };
            let payload = &frame.data[1..frame.len];
            match cmd {
                KissCommand::DataFrame => {
                    if !payload.is_empty() && payload.len() <= MAX_ESPNOW_PDU_LEN {
                        if tx_cfg.tx_delay_10ms > 0 {
                            Timer::after(Duration::from_millis(
                                u64::from(tx_cfg.tx_delay_10ms) * 10,
                            ))
                            .await;
                        }
                        if sender.send_async(&tx_cfg.tx_peer, payload).await.is_ok() {
                            shared.tx_count.fetch_add(1, Ordering::Relaxed);
                        }
                        if tx_cfg.tx_tail_10ms > 0 {
                            Timer::after(Duration::from_millis(
                                u64::from(tx_cfg.tx_tail_10ms) * 10,
                            ))
                            .await;
                        }
                    }
                }
                KissCommand::TxDelay => {
                    if let Some(&v) = payload.first() {
                        tx_cfg.tx_delay_10ms = v;
                    }
                }
                KissCommand::Persistence => {
                    if let Some(&v) = payload.first() {
                        tx_cfg.persistence = v;
                    }
                }
                KissCommand::SlotTime => {
                    if let Some(&v) = payload.first() {
                        tx_cfg.slot_time_10ms = v;
                    }
                }
                KissCommand::TxTail => {
                    if let Some(&v) = payload.first() {
                        tx_cfg.tx_tail_10ms = v;
                    }
                }
                KissCommand::FullDuplex => {
                    if let Some(&v) = payload.first() {
                        tx_cfg.full_duplex = v != 0;
                    }
                }
                KissCommand::SetHardware => {
                    if let Some(hw_cmd) = HardwareCommand::decode(payload) {
                        match hw_cmd {
                            HardwareCommand::SetChannel(ch) => {
                                if apply_channel(&manager, ch) {
                                    shared.channel.store(ch, Ordering::Relaxed);
                                }
                                send_status_to_host(&shared);
                            }
                            HardwareCommand::SetFrequencyHz(hz) => {
                                if let Some(ch) = freq_hz_to_amateur_channel(hz) {
                                    if apply_channel(&manager, ch) {
                                        shared.channel.store(ch, Ordering::Relaxed);
                                    }
                                }
                                send_status_to_host(&shared);
                            }
                            HardwareCommand::SetTxPowerQdbm(qdbm) => {
                                if let Some(applied) = apply_tx_power(qdbm) {
                                    shared.tx_power_qdbm.store(applied, Ordering::Relaxed);
                                }
                                send_status_to_host(&shared);
                            }
                            HardwareCommand::SetPhyRate(rate) => {
                                if apply_phy_rate(&manager, rate) {
                                    shared.phy_rate.store(rate as u8, Ordering::Relaxed);
                                }
                                send_status_to_host(&shared);
                            }
                            HardwareCommand::SetPeerMac(mac) => {
                                if tx_cfg.tx_peer != BROADCAST_ADDRESS && tx_cfg.tx_peer != mac {
                                    let _ = manager.remove_peer(&tx_cfg.tx_peer);
                                }
                                if mac == BROADCAST_ADDRESS || manager.peer_exists(&mac) {
                                    tx_cfg.tx_peer = mac;
                                } else if manager
                                    .add_peer(PeerInfo {
                                        peer_address: mac,
                                        lmk: None,
                                        channel: None,
                                        encrypt: false,
                                    })
                                    .is_ok()
                                {
                                    tx_cfg.tx_peer = mac;
                                }
                                send_status_to_host(&shared);
                            }
                            HardwareCommand::GetPeerMac => {
                                let mut msg = KissFrameBuf {
                                    data: [0u8; MAX_KISS_FRAME_LEN],
                                    len: 0,
                                };
                                msg.data[0] = KissCommand::SetHardware as u8;
                                if let Some(n) = HardwareCommand::PeerMac(tx_cfg.tx_peer)
                                    .encode(&mut msg.data[1..])
                                {
                                    msg.len = 1 + n;
                                    let _ = UART_TX_QUEUE.try_send(msg);
                                }
                            }
                            HardwareCommand::GetStatus => {
                                send_status_to_host(&shared);
                            }
                            HardwareCommand::PeerMac(_) | HardwareCommand::Status { .. } => {}
                        }
                    }
                }
                KissCommand::Return => {}
            }
        }
    };

    let rx_fut = async {
        loop {
            let rx_data = receiver.receive_async().await;
            let data = rx_data.data();
            if !data.is_empty() && data.len() <= MAX_ESPNOW_PDU_LEN {
                shared.rx_count.fetch_add(1, Ordering::Relaxed);
                shared
                    .last_rssi
                    .store(rx_data.info.rx_control.rssi as i8, Ordering::Relaxed);
                // Push Status before DataFrame so the host has the fresh RSSI when logging/forwarding the PDU.
                send_status_to_host(&shared);
                let mut msg = KissFrameBuf {
                    data: [0u8; MAX_KISS_FRAME_LEN],
                    len: 1 + data.len(),
                };
                msg.data[0] = KissCommand::DataFrame as u8;
                msg.data[1..msg.len].copy_from_slice(data);
                UART_TX_QUEUE.send(msg).await;
            }
        }
    };

    join(tx_fut, rx_fut).await;
}
