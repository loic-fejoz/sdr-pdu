use anyhow::Context;
use tracing::{error, info, warn};
use waverave_hackrf::{ComplexI8, HackRf, Transmit};

pub trait SdrDevice: Send {
    async fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()>;
    async fn push_samples(&mut self, samples: &[i8]) -> anyhow::Result<()>;
    fn _get_actual_sample_rate(&self) -> u32;
    async fn enable_tx(&mut self) -> anyhow::Result<()>;
    async fn disable_tx(&mut self) -> anyhow::Result<()>;
}

enum HackRfState {
    Idle(HackRf),
    Tx(Transmit),
    Invalid,
}

pub struct HackRfDevice {
    state: HackRfState,
    _actual_sample_rate: u32,
    _tx_vga: u16,
    _amp_enable: bool,
    offset: i64,
}

impl HackRfDevice {
    pub async fn new(
        sample_rate: u32,
        tx_vga: u16,
        amp_enable: bool,
        offset: i64,
    ) -> anyhow::Result<Self> {
        let hackrf = waverave_hackrf::open_hackrf().context("Failed to open HackRF")?;

        info!("Configuring HackRF SDR...");

        // HackRF wants frequency in Hz as f64 for sample rate
        hackrf
            .set_sample_rate(sample_rate as f64)
            .await
            .context("Failed to set sample rate")?;
        hackrf
            .set_txvga_gain(tx_vga)
            .await
            .context("Failed to set TX VGA gain")?;
        hackrf
            .set_amp_enable(amp_enable)
            .await
            .context("Failed to set AMP enable")?;

        info!(
            "Configured HackRF: {} MSPS, TX VGA: {} dB, Amp: {}",
            sample_rate as f64 / 1e6,
            tx_vga,
            amp_enable
        );

        Ok(Self {
            state: HackRfState::Idle(hackrf),
            _actual_sample_rate: sample_rate,
            _tx_vga: tx_vga,
            _amp_enable: amp_enable,
            offset,
        })
    }
}

impl SdrDevice for HackRfDevice {
    fn _get_actual_sample_rate(&self) -> u32 {
        self._actual_sample_rate
    }

    async fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()> {
        let actual_freq = (freq as i64 + self.offset) as u64;
        match &mut self.state {
            HackRfState::Idle(hackrf) => {
                hackrf
                    .set_freq(actual_freq)
                    .await
                    .context("Failed to set frequency")?;
            }
            HackRfState::Tx(_) => {
                warn!("Cannot set frequency while TX is active for waverave_hackrf (currently).");
            }
            HackRfState::Invalid => return Err(anyhow::anyhow!("Invalid state")),
        }
        Ok(())
    }

    async fn enable_tx(&mut self) -> anyhow::Result<()> {
        let state = std::mem::replace(&mut self.state, HackRfState::Invalid);
        match state {
            HackRfState::Idle(hackrf) => {
                let tx = hackrf.start_tx(16384).await.map_err(|e| {
                    error!("Failed to start TX: {}", e.err);
                    anyhow::anyhow!("Failed to start TX: {}", e.err)
                })?;
                self.state = HackRfState::Tx(tx);
            }
            HackRfState::Tx(tx) => {
                // Already in TX mode
                self.state = HackRfState::Tx(tx);
            }
            HackRfState::Invalid => return Err(anyhow::anyhow!("Invalid state")),
        }
        Ok(())
    }

    async fn push_samples(&mut self, samples: &[i8]) -> anyhow::Result<()> {
        let tx = match &mut self.state {
            HackRfState::Tx(tx) => tx,
            _ => return Err(anyhow::anyhow!("Not in TX state")),
        };

        let mut offset = 0;
        // Cast &[i8] to &[ComplexI8] since memory layout is identical
        let complex_samples = unsafe {
            std::slice::from_raw_parts(samples.as_ptr() as *const ComplexI8, samples.len() / 2)
        };

        while offset < complex_samples.len() {
            let mut buf = tx.get_buffer();
            let chunk_size =
                std::cmp::min(buf.remaining_capacity(), complex_samples.len() - offset);
            buf.extend_from_slice(&complex_samples[offset..offset + chunk_size]);
            tx.submit(buf);
            offset += chunk_size;

            // Simple backpressure: if we have more than 32 pending blocks, wait for one
            if tx.pending() > 32 {
                tx.next_complete()
                    .await
                    .context("Error waiting for TX completion")?;
            }
        }

        Ok(())
    }

    async fn disable_tx(&mut self) -> anyhow::Result<()> {
        let state = std::mem::replace(&mut self.state, HackRfState::Invalid);
        match state {
            HackRfState::Tx(mut tx) => {
                tx.flush();
                while tx.pending() > 0 {
                    if let Err(e) = tx.next_complete().await {
                        warn!("Error waiting for final TX completion: {}", e);
                    }
                }
                let hackrf = tx.stop().await.map_err(|e| {
                    error!("Failed to stop TX: {}", e.err);
                    anyhow::anyhow!("Failed to stop TX: {}", e.err)
                })?;
                self.state = HackRfState::Idle(hackrf);
            }
            HackRfState::Idle(hackrf) => {
                // Already in Idle mode
                self.state = HackRfState::Idle(hackrf);
            }
            HackRfState::Invalid => return Err(anyhow::anyhow!("Invalid state")),
        }
        Ok(())
    }
}
