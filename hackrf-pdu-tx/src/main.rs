mod engine;
mod hackrf;
mod modulator;
mod nco;

use crate::engine::TransmissionEngine;
use crate::hackrf::HackRfDevice;
use crate::modulator::FskModulator;
use clap::Parser;
use sdr_pdu_utils::cat::CatServer;
use sdr_pdu_utils::kiss_server::KissServer;
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

    /// SDR sample rate in Sps (Min 2 MSPS for HackRF)
    #[arg(short, long, default_value_t = 2000000)]
    sample_rate: u32,

    /// HackRF TX VGA gain in dB (0 to 47)
    #[arg(short, long, default_value_t = 20)]
    tx_vga: u16,

    /// Enable HackRF 14dB front-end amp
    #[arg(long, default_value_t = false)]
    amp_enable: bool,

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
    scramble: bool,

    /// Scrambler polynomial (hex bitmask). Default G3RUH: x^17 + x^12 + 1 -> bit 16 and 11 set -> 0x21000 (shifted by 1 for multiplicative logic)
    /// Actually standard G3RUH bits are 12 and 17.
    /// If we use 0-indexed: bits 11 and 16.
    /// 1<<11 | 1<<16 = 0x800 | 0x10000 = 0x10800.
    #[arg(long, default_value = "0x10800")]
    poly: String,

    /// Scrambler initial seed (hex)
    #[arg(long, default_value = "0x1FFFF")]
    seed: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let (tx, rx) = mpsc::channel(100);
    let freq = Arc::new(AtomicU64::new(args.frequency));

    // Initialize HackRF
    let device = HackRfDevice::new(args.sample_rate, args.tx_vga, args.amp_enable, args.offset)
        .await
        .map_err(|e| anyhow::anyhow!("HackRF init failed: {}", e))?;

    let mut modulator = FskModulator::new(args.sample_rate, args.baud_rate, args.deviation);

    if args.scramble {
        let poly = u32::from_str_radix(args.poly.trim_start_matches("0x"), 16)?;
        let seed = u32::from_str_radix(args.seed.trim_start_matches("0x"), 16)?;
        modulator.set_scrambler(poly, seed);
    }

    modulator.set_preamble_and_syncword(
        &args.preamble,
        args.preamble_repetition,
        &args.syncword,
    )?;

    let engine = TransmissionEngine::new(device, modulator, freq.clone(), rx);
    let kiss_server = KissServer::new(tx);
    let cat_server = CatServer::new(freq);

    let kiss_addr = format!("{}:{}", args.listen, args.kiss_port);
    let cat_addr = format!("{}:{}", args.listen, args.cat_port);

    tokio::select! {
        _ = engine.run() => {},
        res = kiss_server.run(&kiss_addr) => {
            if let Err(e) = res { anyhow::bail!("KISS server failed: {}", e); }
        },
        res = cat_server.run(&cat_addr) => {
            if let Err(e) = res { anyhow::bail!("CAT server failed: {}", e); }
        },
    }

    Ok(())
}
