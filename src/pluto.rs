use industrial_io::Device;

pub trait SdrDevice: Send {
    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()>;
    fn push_samples(&mut self, samples: &[i16]) -> anyhow::Result<()>;
}

pub struct PlutoDevice {
    _ctx: industrial_io::Context,
    phy: Device,
    tx_dev: Device,
}

impl PlutoDevice {
    pub fn new(sample_rate: u32, bandwidth: u32) -> anyhow::Result<Self> {
        let ctx = industrial_io::Context::new()?;
        let phy = ctx
            .find_device("ad9361-phy")
            .ok_or_else(|| anyhow::anyhow!("ad9361-phy not found"))?;
        
        // On PlutoSDR:
        // cf-ad9361-lpc is the RX DMA
        // cf-ad9361-dds-core-lpc is the TX DMA
        let tx_dev = ctx
            .find_device("cf-ad9361-dds-core-lpc")
            .or_else(|| ctx.find_device("cf-ad9361-lpc")) // Fallback just in case
            .ok_or_else(|| anyhow::anyhow!("TX DMA device not found"))?;

        tracing::info!("Using TX device: {}", tx_dev.name().unwrap_or_default());

        // Configure Sample Rate and Bandwidth on PHY TX channel
        let tx_phy_chan = phy.find_channel("voltage0", true)
            .ok_or_else(|| anyhow::anyhow!("PHY TX channel (voltage0) not found"))?;

        tracing::info!("Configuring SDR: {} MSPS, {} MHz BW", sample_rate as f64 / 1e6, bandwidth as f64 / 1e6);

        if let Err(e) = tx_phy_chan.attr_write_int("sampling_frequency", sample_rate as i64) {
            tracing::warn!("Failed to set sampling_frequency on channel: {}. Retrying on device...", e);
            phy.attr_write_int("sampling_frequency", sample_rate as i64)
                .map_err(|e2| anyhow::anyhow!("Failed to set PHY sampling frequency: {}", e2))?;
        }

        if let Err(e) = tx_phy_chan.attr_write_int("rf_bandwidth", bandwidth as i64) {
             tracing::warn!("Failed to set rf_bandwidth on channel: {}. Retrying on device...", e);
             phy.attr_write_int("rf_bandwidth", bandwidth as i64)
                .map_err(|e2| anyhow::anyhow!("Failed to set PHY RF bandwidth: {}", e2))?;
        }

        // Configure Sample Rate on TX DAC (DMA engine)
        if let Some(chan) = tx_dev.find_channel("voltage0", true) {
            let _ = chan.attr_write_int("sampling_frequency", sample_rate as i64);
        }

        Ok(Self {
            _ctx: ctx,
            phy,
            tx_dev,
        })
    }
}

unsafe impl Send for PlutoDevice {}

impl SdrDevice for PlutoDevice {
    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()> {
        let chan = self
            .phy
            .find_channel("altvoltage1", true)
            .ok_or_else(|| anyhow::anyhow!("TX LO channel (altvoltage1) not found"))?;

        chan.attr_write_int("frequency", freq as i64)
            .map_err(|e| anyhow::anyhow!("Failed to set TX LO frequency to {} Hz: {}", freq, e))?;

        Ok(())
    }

    fn push_samples(&mut self, samples: &[i16]) -> anyhow::Result<()> {
        let n_samples = samples.len() / 2;
        if n_samples == 0 {
            return Ok(());
        }

        // Disable all channels first to ensure a clean state
        for mut chan in self.tx_dev.channels() {
            chan.disable();
        }

        let mut v0 = self.tx_dev.find_channel("voltage0", true)
            .ok_or_else(|| {
                let channels: Vec<String> = self.tx_dev.channels().map(|c| format!("{}(out={})", c.id().unwrap_or_default(), c.is_output())).collect();
                anyhow::anyhow!("voltage0 (output) not found on {}. Available: {:?}", self.tx_dev.name().unwrap_or_default(), channels)
            })?;

        let mut v1 = self.tx_dev.find_channel("voltage1", true)
            .ok_or_else(|| anyhow::anyhow!("voltage1 (output) not found"))?;

        v0.enable();
        v1.enable();

        // Create a non-cyclic buffer for this burst
        let mut buffer = self.tx_dev.create_buffer(n_samples, false)
            .map_err(|e| anyhow::anyhow!("Failed to create TX buffer (n_samples={}): {}", n_samples, e))?;

        // De-interleave I and Q samples
        let mut i_samples = Vec::with_capacity(n_samples);
        let mut q_samples = Vec::with_capacity(n_samples);
        for chunk in samples.chunks_exact(2) {
            i_samples.push(chunk[0]);
            q_samples.push(chunk[1]);
        }

        // Write to channels
        v0.write(&buffer, &i_samples)
            .map_err(|e| anyhow::anyhow!("Failed to write I samples: {}", e))?;
        v1.write(&buffer, &q_samples)
            .map_err(|e| anyhow::anyhow!("Failed to write Q samples: {}", e))?;

        buffer.push()
            .map_err(|e| anyhow::anyhow!("Failed to push buffer: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
pub struct MockDevice {
    pub last_freq: u64,
    pub samples_pushed: usize,
}

#[cfg(test)]
impl MockDevice {
    pub fn new(_sample_rate: u32, _bandwidth: u32) -> anyhow::Result<Self> {
        Ok(Self {
            last_freq: 0,
            samples_pushed: 0,
        })
    }
}

#[cfg(test)]
impl SdrDevice for MockDevice {
    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()> {
        self.last_freq = freq;
        Ok(())
    }

    fn push_samples(&mut self, samples: &[i16]) -> anyhow::Result<()> {
        self.samples_pushed += samples.len() / 2;
        Ok(())
    }
}
