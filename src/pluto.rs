use industrial_io::Device;
use std::time::Duration;

pub trait SdrDevice: Send {
    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()>;
    fn push_samples(&mut self, samples: &[i16]) -> anyhow::Result<()>;
    fn get_actual_sample_rate(&self) -> u32;
    fn enable_tx(&mut self) -> anyhow::Result<()>;
    fn disable_tx(&mut self) -> anyhow::Result<()>;
}

pub struct PlutoDevice {
    _ctx: industrial_io::Context,
    phy: Device,
    tx_dev: Device,
    actual_sample_rate: u32,
    offset: i64,
    attenuation: f64,
}

impl PlutoDevice {
    pub fn new(
        sample_rate: u32,
        bandwidth: u32,
        attenuation: f64,
        offset: i64,
    ) -> anyhow::Result<Self> {
        let ctx = industrial_io::Context::new()?;
        let phy = ctx
            .find_device("ad9361-phy")
            .ok_or_else(|| anyhow::anyhow!("ad9361-phy not found"))?;

        let tx_dev = ctx
            .find_device("cf-ad9361-dds-core-lpc")
            .or_else(|| ctx.find_device("cf-ad9361-lpc"))
            .ok_or_else(|| anyhow::anyhow!("TX DMA device not found"))?;

        tracing::info!("Using TX device: {}", tx_dev.name().unwrap_or_default());

        // Disable internal DDS
        for chan in tx_dev.channels() {
            if chan.id().unwrap_or_default().starts_with("altvoltage") {
                let _ = chan.attr_write_float("raw", 0.0);
            }
        }

        let tx_phy_chan = phy
            .find_channel("voltage0", true)
            .ok_or_else(|| anyhow::anyhow!("PHY TX channel (voltage0) not found"))?;

        tracing::info!(
            "Configuring SDR: Requested {} MSPS, {} MHz BW, {} dB Attenuation, {} Hz Offset",
            sample_rate as f64 / 1e6,
            bandwidth as f64 / 1e6,
            attenuation,
            offset
        );

        // Set attenuation initially
        let atten_val = attenuation.abs();
        let _ = tx_phy_chan.attr_write_float("hardwaregain", -atten_val);

        // Set Sample Rate and Bandwidth
        if let Err(e) = tx_phy_chan.attr_write_int("sampling_frequency", sample_rate as i64) {
            let _ = phy.attr_write_int("sampling_frequency", sample_rate as i64);
            tracing::warn!("PHY sampling_frequency write status: {}", e);
        }

        if let Err(e) = tx_phy_chan.attr_write_int("rf_bandwidth", bandwidth as i64) {
            let _ = phy.attr_write_int("rf_bandwidth", bandwidth as i64);
            tracing::warn!("PHY rf_bandwidth write status: {}", e);
        }

        // Configure Sample Rate on TX DAC
        if let Some(chan) = tx_dev.find_channel("voltage0", true) {
            let _ = chan.attr_write_int("sampling_frequency", sample_rate as i64);
        }

        // Read back actual sample rate
        let actual_sample_rate = tx_phy_chan
            .attr_read_int("sampling_frequency")
            .map(|v| v as u32)
            .unwrap_or(sample_rate);

        tracing::info!(
            "Configured SDR: Actual {} MSPS",
            actual_sample_rate as f64 / 1e6
        );

        Ok(Self {
            _ctx: ctx,
            phy,
            tx_dev,
            actual_sample_rate,
            offset,
            attenuation: atten_val,
        })
    }
}

unsafe impl Send for PlutoDevice {}

impl SdrDevice for PlutoDevice {
    fn get_actual_sample_rate(&self) -> u32 {
        self.actual_sample_rate
    }

    fn enable_tx(&mut self) -> anyhow::Result<()> {
        let tx_phy_chan = self
            .phy
            .find_channel("voltage0", true)
            .ok_or_else(|| anyhow::anyhow!("PHY TX channel not found"))?;
        tx_phy_chan
            .attr_write_float("hardwaregain", -self.attenuation)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to enable TX (set gain to -{}): {}",
                    self.attenuation,
                    e
                )
            })?;
        Ok(())
    }

    fn disable_tx(&mut self) -> anyhow::Result<()> {
        let tx_phy_chan = self
            .phy
            .find_channel("voltage0", true)
            .ok_or_else(|| anyhow::anyhow!("PHY TX channel not found"))?;

        // Max attenuation to mute TX leakage
        tx_phy_chan
            .attr_write_float("hardwaregain", -89.75)
            .map_err(|e| anyhow::anyhow!("Failed to silence TX (set gain to -89.75): {}", e))?;

        // Push a short zero buffer to clear DAC/DMA
        let n_zeros = 1024;

        let mut v0 = self
            .tx_dev
            .find_channel("voltage0", true)
            .ok_or_else(|| anyhow::anyhow!("TX voltage0 not found for flush"))?;
        let mut v1 = self
            .tx_dev
            .find_channel("voltage1", true)
            .ok_or_else(|| anyhow::anyhow!("TX voltage1 not found for flush"))?;

        v0.enable();
        v1.enable();

        let mut buffer = self
            .tx_dev
            .create_buffer(n_zeros, false)
            .map_err(|e| anyhow::anyhow!("Failed to create zero-flush buffer: {}", e))?;

        let zeros = vec![0i16; n_zeros];
        v0.write(&buffer, &zeros)
            .map_err(|e| anyhow::anyhow!("Failed to write zero I samples: {}", e))?;
        v1.write(&buffer, &zeros)
            .map_err(|e| anyhow::anyhow!("Failed to write zero Q samples: {}", e))?;
        buffer
            .push()
            .map_err(|e| anyhow::anyhow!("Failed to push zero-flush buffer: {}", e))?;

        Ok(())
    }

    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()> {
        let chan = self
            .phy
            .find_channel("altvoltage1", true)
            .ok_or_else(|| anyhow::anyhow!("TX LO channel (altvoltage1) not found"))?;

        let actual_freq = (freq as i64 + self.offset) as u64;
        chan.attr_write_int("frequency", actual_freq as i64)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to set TX LO frequency to {} Hz (requested {}): {}",
                    actual_freq,
                    freq,
                    e
                )
            })?;

        Ok(())
    }

    fn push_samples(&mut self, samples: &[i16]) -> anyhow::Result<()> {
        let n_samples = samples.len() / 2;
        if n_samples == 0 {
            return Ok(());
        }

        let mut v0 = self
            .tx_dev
            .find_channel("voltage0", true)
            .ok_or_else(|| anyhow::anyhow!("TX voltage0 not found"))?;
        let mut v1 = self
            .tx_dev
            .find_channel("voltage1", true)
            .ok_or_else(|| anyhow::anyhow!("TX voltage1 not found"))?;

        v0.enable();
        v1.enable();

        let duration_ms = (n_samples as f64 * 1000.0 / self.actual_sample_rate as f64) as u64;

        let mut buffer = self.tx_dev.create_buffer(n_samples, false).map_err(|e| {
            anyhow::anyhow!("Failed to create TX DMA buffer (len={}): {}", n_samples, e)
        })?;

        let mut i_samples = Vec::with_capacity(n_samples);
        let mut q_samples = Vec::with_capacity(n_samples);
        for chunk in samples.chunks_exact(2) {
            i_samples.push(chunk[0]);
            q_samples.push(chunk[1]);
        }

        v0.write(&buffer, &i_samples)
            .map_err(|e| anyhow::anyhow!("Failed to write I samples to DMA buffer: {}", e))?;
        v1.write(&buffer, &q_samples)
            .map_err(|e| anyhow::anyhow!("Failed to write Q samples to DMA buffer: {}", e))?;

        buffer
            .push()
            .map_err(|e| anyhow::anyhow!("Failed to push samples to hardware: {}", e))?;

        std::thread::sleep(Duration::from_millis(duration_ms + 10));

        Ok(())
    }
}

#[cfg(test)]
pub struct MockDevice {
    pub last_freq: u64,
}

#[cfg(test)]
impl MockDevice {
    pub fn new(
        _sample_rate: u32,
        _bandwidth: u32,
        _attenuation: f64,
        _offset: i64,
    ) -> anyhow::Result<Self> {
        Ok(Self { last_freq: 0 })
    }
}

#[cfg(test)]
impl SdrDevice for MockDevice {
    fn get_actual_sample_rate(&self) -> u32 {
        1000000
    }

    fn enable_tx(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn disable_tx(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()> {
        self.last_freq = freq;
        Ok(())
    }

    fn push_samples(&mut self, _samples: &[i16]) -> anyhow::Result<()> {
        Ok(())
    }
}
