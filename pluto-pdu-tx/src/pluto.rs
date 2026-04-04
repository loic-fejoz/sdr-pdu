use industrial_io::*;
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
    last_freq: u64,
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

        // Disable internal DDS more thoroughly
        for chan in tx_dev.channels() {
            if chan.id().unwrap_or_default().contains("voltage") {
                let _ = chan.attr_write_float("scale", 0.0);
                let _ = chan.attr_write_int("raw", 0);
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

        // Set TX port to A
        let _ = tx_phy_chan.attr_write_int("rf_port_select", 0); // Some drivers map 'A' to 0 or use strings

        // Set attenuation
        let atten_val = attenuation.abs();
        let _ = tx_phy_chan.attr_write_float("hardwaregain", -atten_val);

        // Set Sample Rate and Bandwidth ONLY on TX channel to avoid affecting RX
        // We check current value first to avoid unnecessary BBPLL retunes
        let current_sr = tx_phy_chan.attr_read_int("sampling_frequency").unwrap_or(0);
        if (current_sr as f64 - sample_rate as f64).abs() > (sample_rate as f64 * 0.01)
            && let Err(e) = tx_phy_chan.attr_write_int("sampling_frequency", sample_rate as i64)
        {
            tracing::warn!("Failed to set TX sampling_frequency: {}", e);
        }

        let current_bw = tx_phy_chan.attr_read_int("rf_bandwidth").unwrap_or(0);
        if (current_bw as f64 - bandwidth as f64).abs() > (bandwidth as f64 * 0.01)
            && let Err(e) = tx_phy_chan.attr_write_int("rf_bandwidth", bandwidth as i64)
        {
            tracing::warn!("Failed to set TX rf_bandwidth: {}", e);
        }

        // Configure Sample Rate on TX DAC if possible
        if let Some(chan) = tx_dev.find_channel("voltage0", true) {
            let _ = chan.attr_write_int("sampling_frequency", sample_rate as i64);
        }

        // Read back actual sample rate from TX channel
        let actual_sample_rate = tx_phy_chan
            .attr_read_int("sampling_frequency")
            .map(|v| v as u32)
            .unwrap_or_else(|_| {
                tracing::warn!("Failed to read back actual TX sampling_frequency, using requested");
                sample_rate
            });

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
            last_freq: 0,
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

        Ok(())
    }

    fn set_frequency(&mut self, freq: u64) -> anyhow::Result<()> {
        if freq == self.last_freq {
            return Ok(());
        }

        let chan = self
            .phy
            .find_channel("altvoltage1", true)
            .ok_or_else(|| anyhow::anyhow!("TX LO channel (altvoltage1) not found"))?;

        let actual_freq = (freq as i64 + self.offset) as u64;

        // Ensure powerdown is 0
        let _ = chan.attr_write_int("powerdown", 0);

        // Try setting frequency on the channel
        if let Err(e) = chan.attr_write_int("frequency", actual_freq as i64) {
            // Fallback to device attribute if channel attribute fails
            self.phy
                .attr_write_int("out_altvoltage1_frequency", actual_freq as i64)
                .map_err(|_| {
                    anyhow::anyhow!(
                        "Failed to set TX LO frequency to {} Hz (requested {}): {}",
                        actual_freq,
                        freq,
                        e
                    )
                })?;
        }

        self.last_freq = freq;
        tracing::info!("TX LO frequency updated to {} Hz", actual_freq);

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
