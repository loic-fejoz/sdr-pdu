#![feature(portable_simd)]

mod engine;
mod modulator;
mod nco;
mod pluto;

use crate::engine::TransmissionEngine;
use crate::modulator::FskModulator;
use crate::pluto::{PlutoDevice, SdrDevice};
use clap::Parser;
use sdr_pdu_utils::cat::CatServer;
use sdr_pdu_utils::kiss_server::KissServer;
use sdr_pdu_utils::kiss_ws_server::KissWsServer;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Listen address for KISS and CAT servers
    #[arg(short, long, default_value = "0.0.0.0")]
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

    /// Initial center frequency in Hz
    #[arg(short, long, default_value_t = 144000000)]
    frequency: u64,

    /// Frequency offset in Hz (to compensate for PPM). Use --offset=-1234
    #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
    offset: i64,

    /// Transmission baud rate
    #[arg(short, long, default_value_t = 9600)]
    baud_rate: u32,

    /// FSK frequency deviation in Hz
    #[arg(short, long, default_value_t = 2400)]
    deviation: u32,

    /// SDR sample rate in Sps (Min ~2.1 MSPS for Pluto)
    #[arg(short, long, default_value_t = 2100000)]
    sample_rate: u32,

    /// SDR analog bandwidth in Hz (Min 200 kHz)
    #[arg(short, long, default_value_t = 1000000)]
    bandwidth: u32,

    /// TX attenuation in dB (0 to 89)
    #[arg(short, long, default_value_t = 10.0)]
    attenuation: f64,

    /// Preamble byte (e.g., 0x55)
    #[arg(long, default_value = "0x55")]
    preamble: String,

    /// Number of times to repeat the preamble byte
    #[arg(long, default_value_t = 8)]
    preamble_repetition: u32,

    /// Syncword (e.g., 0x1ACFFC1D)
    #[arg(long, default_value = "0x7E")]
    syncword: String,

    /// Enable G3RUH-style scrambling
    #[arg(long, default_value_t = false)]
    g3ruh: bool,

    /// Enable QPSK modulation
    #[arg(long, default_value_t = false)]
    qpsk: bool,

    /// Enable Offset QPSK (OQPSK) modulation
    #[arg(long, default_value_t = false)]
    oqpsk: bool,

    /// Enable Root Raised Cosine (RRC) pulse shaping filter
    #[arg(long, default_value_t = false)]
    rrc: bool,

    /// Roll-off factor alpha for RRC filter (e.g. 0.35, 0.5)
    #[arg(long, default_value_t = 0.35)]
    rrc_alpha: f64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let (tx, rx) = mpsc::channel(100);
    let freq = Arc::new(AtomicU64::new(args.frequency));
    let attenuation_tenths = (args.attenuation.abs() * 10.0).round() as u64;
    let att = Arc::new(AtomicU64::new(attenuation_tenths));

    // Initialize Pluto
    let device = PlutoDevice::new(
        args.sample_rate,
        args.bandwidth,
        args.attenuation,
        args.offset,
    )
    .map_err(|e| anyhow::anyhow!("Pluto init failed: {}", e))?;

    // Read back actual sample rate if possible to ensure NCO is correct
    let actual_sample_rate = device.get_actual_sample_rate();

    let modulator = if args.qpsk || args.oqpsk {
        let is_oqpsk = args.oqpsk;
        let mut qpsk_mod =
            sdr_pdu_utils::qpsk::QpskModulator::new(actual_sample_rate, args.baud_rate, is_oqpsk);
        if args.rrc {
            qpsk_mod.set_rrc_filter(args.rrc_alpha, 8);
        }
        if args.g3ruh {
            qpsk_mod.set_scrambler(
                sdr_pdu_utils::scrambler::Scrambler::G3RUH_POLY,
                sdr_pdu_utils::scrambler::Scrambler::G3RUH_SEED,
            );
        }
        qpsk_mod.set_preamble_and_syncword(
            &args.preamble,
            args.preamble_repetition,
            &args.syncword,
        )?;
        crate::modulator::Modulator::Qpsk(qpsk_mod)
    } else {
        let mut fsk_mod = FskModulator::new(actual_sample_rate, args.baud_rate, args.deviation);
        if args.g3ruh {
            fsk_mod.set_scrambler(
                sdr_pdu_utils::scrambler::Scrambler::G3RUH_POLY,
                sdr_pdu_utils::scrambler::Scrambler::G3RUH_SEED,
            );
        }
        fsk_mod.set_preamble_and_syncword(
            &args.preamble,
            args.preamble_repetition,
            &args.syncword,
        )?;
        crate::modulator::Modulator::Fsk(fsk_mod)
    };

    let engine = TransmissionEngine::new(device, modulator, freq.clone(), att.clone(), rx);
    let kiss_server = KissServer::new(tx.clone());
    let kiss_ws_server = KissWsServer::new(tx);
    let cat_server = CatServer::new(freq).with_attenuation(att);

    let kiss_addr = format!("{}:{}", args.listen, args.kiss_port);
    let kiss_ws_addr = format!("{}:{}", args.listen, args.kiss_ws_port);
    let cat_addr = format!("{}:{}", args.listen, args.cat_port);

    tokio::select! {
        _ = engine.run() => {},
        res = kiss_server.run(&kiss_addr) => {
            if let Err(e) = res { anyhow::bail!("KISS server failed: {}", e); }
        },
        res = kiss_ws_server.run(&kiss_ws_addr) => {
            if let Err(e) = res { anyhow::bail!("KISS WebSocket server failed: {}", e); }
        },
        res = cat_server.run(&cat_addr) => {
            if let Err(e) = res { anyhow::bail!("CAT server failed: {}", e); }
        },
    }

    Ok(())
}
