#![feature(portable_simd)]

mod cat;
mod engine;
mod kiss;
mod kiss_server;
mod modulator;
mod nco;
mod pluto;

use crate::cat::CatServer;
use crate::engine::TransmissionEngine;
use crate::kiss_server::KissServer;
use crate::modulator::FskModulator;
use crate::pluto::PlutoDevice;
use clap::Parser;
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

    /// Transmission baud rate
    #[arg(short, long, default_value_t = 9600)]
    baud_rate: u32,

    /// FSK frequency deviation in Hz
    #[arg(short, long, default_value_t = 2400)]
    deviation: u32,

    /// SDR sample rate in Sps
    #[arg(short, long, default_value_t = 1000000)]
    sample_rate: u32,

    /// SDR analog bandwidth in Hz
    #[arg(short, long, default_value_t = 200000)]
    bandwidth: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let (tx, rx) = mpsc::channel(100);
    let freq = Arc::new(AtomicU64::new(args.frequency));

    // For now we try to initialize Pluto, but it might fail if libiio not found or no hardware
    // In real deployment on Pluto it should work.
    let device = PlutoDevice::new(args.sample_rate, args.bandwidth)
        .map_err(|e| anyhow::anyhow!("Pluto init failed: {}", e))?;
    let modulator = FskModulator::new(args.sample_rate, args.baud_rate, args.deviation);

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
