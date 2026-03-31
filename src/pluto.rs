use industrial_io::Device;
use std::time::Duration;

pub trait SdrDevice: Send {
    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()>;
    fn push_samples(&mut self, samples: &[i16]) -> anyhow::Result<()>;
    fn get_actual_sample_rate(&self) -> u32;
}

pub struct PlutoDevice {
    _ctx: industrial_io::Context,
    phy: Device,
    tx_dev: Device,
    actual_sample_rate: u32,
    offset: i64,
}

impl PlutoDevice {
    pub fn new(sample_rate: u32, bandwidth: u32, attenuation: f64, offset: i64) -> anyhow::Result<Self> {
        let ctx = industrial_io::Context::new()?;
        let phy = ctx
            .find_device("ad9361-phy")
            .ok_or_else(|| anyhow::anyhow!("ad9361-phy not found"))?;
        
        let tx_dev = ctx
            .find_device("cf-ad9361-dds-core-lpc")
            .or_else(|| ctx.find_device("cf-ad9361-lpc"))
            .ok_or_else(|| anyhow::anyhow!("TX DMA device not found"))?;

        // Disable internal DDS
        for chan in tx_dev.channels() {
            if chan.id().unwrap_or_default().starts_with("altvoltage") {
                let _ = chan.attr_write_float("raw", 0.0);
            }
        }

        let tx_phy_chan = phy.find_channel("voltage0", true)
            .ok_or_else(|| anyhow::anyhow!("PHY TX channel (voltage0) not found"))?;

        // Set attenuation
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
        let actual_sample_rate = tx_phy_chan.attr_read_int("sampling_frequency")
            .map(|v| v as u32)
            .unwrap_or(sample_rate);

        tracing::info!("Configuring SDR: Requested {} MSPS, Actual {} MSPS, {} MHz BW, {} dB Attenuation, {} Hz Offset", 
            sample_rate as f64 / 1e6, actual_sample_rate as f64 / 1e6, bandwidth as f64 / 1e6, attenuation, offset);

        Ok(Self {
            _ctx: ctx,
            phy,
            tx_dev,
            actual_sample_rate,
            offset,
        })
    }
}

unsafe impl Send for PlutoDevice {}

impl SdrDevice for PlutoDevice {
    fn get_actual_sample_rate(&self) -> u32 {
        self.actual_sample_rate
    }

    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()> {
        let chan = self
            .phy
            .find_channel("altvoltage1", true)
            .ok_or_else(|| anyhow::anyhow!("TX LO channel (altvoltage1) not found"))?;

        let actual_freq = (freq as i64 + self.offset) as u64;
        chan.attr_write_int("frequency", actual_freq as i64)
            .map_err(|e| anyhow::anyhow!("Failed to set TX LO frequency to {} Hz: {}", actual_freq, e))?;

        Ok(())
    }

    fn push_samples(&mut self, samples: &[i16]) -> anyhow::Result<()> {
        let n_samples = samples.len() / 2;
        if n_samples == 0 {
            return Ok(());
        }

        let mut v0 = self.tx_dev.find_channel("voltage0", true)
            .ok_or_else(|| anyhow::anyhow!("voltage0 not found"))?;
        let mut v1 = self.tx_dev.find_channel("voltage1", true)
            .ok_or_else(|| anyhow::anyhow!("voltage1 not found"))?;

        v0.enable();
        v1.enable();

        let duration_ms = (n_samples as f64 * 1000.0 / self.actual_sample_rate as f64) as u64;

        let mut buffer = self.tx_dev.create_buffer(n_samples, false)
            .map_err(|e| anyhow::anyhow!("Failed to create TX buffer: {}", e))?;

        let mut i_samples = Vec::with_capacity(n_samples);
        let mut q_samples = Vec::with_capacity(n_samples);
        for chunk in samples.chunks_exact(2) {
            i_samples.push(chunk[0]);
            q_samples.push(chunk[1]);
        }

        v0.write(&buffer, &i_samples)?;
        v1.write(&buffer, &q_samples)?;

        buffer.push()?;

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
    pub fn new(_sample_rate: u32, _bandwidth: u32, _attenuation: f64, _offset: i64) -> anyhow::Result<Self> {
        Ok(Self { last_freq: 0 })
    }
}

#[cfg(test)]
impl SdrDevice for MockDevice {
    fn get_actual_sample_rate(&self) -> u32 {
        1000000
    }

    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()> {
        self.last_freq = freq;
        Ok(())
    }

    fn push_samples(&mut self, _samples: &[i16]) -> anyhow::Result<()> {
        Ok(())
    }
}
