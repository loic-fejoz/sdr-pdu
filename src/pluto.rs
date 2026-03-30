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
        let tx_dev = ctx
            .find_device("cf-ad9361-lpc")
            .ok_or_else(|| anyhow::anyhow!("cf-ad9361-lpc not found"))?;

        // Configure Sample Rate on TX DAC
        if let Some(chan) = tx_dev.find_channel("voltage0", true) {
            chan.attr_write_int("sampling_frequency", sample_rate as i64)?;
        }

        // Configure Bandwidth on PHY
        if let Some(chan) = phy.find_channel("voltage0", true) {
            chan.attr_write_int("rf_bandwidth", bandwidth as i64)?;
        }

        // Enable TX channels for buffering
        if let Some(mut chan) = tx_dev.find_channel("voltage0", true) {
            chan.enable();
        }
        if let Some(mut chan) = tx_dev.find_channel("voltage1", true) {
            chan.enable();
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
            .ok_or_else(|| anyhow::anyhow!("LO channel not found"))?;
        chan.attr_write_int("frequency", freq as i64)?;
        Ok(())
    }

    fn push_samples(&mut self, samples: &[i16]) -> anyhow::Result<()> {
        let n_samples = samples.len() / 2;
        if n_samples == 0 {
            return Ok(());
        }

        // Create a non-cyclic buffer for this burst
        let mut buffer = self.tx_dev.create_buffer(n_samples, false)?;

        let v0 = self
            .tx_dev
            .find_channel("voltage0", true)
            .ok_or_else(|| anyhow::anyhow!("voltage0 not found"))?;
        let v1 = self
            .tx_dev
            .find_channel("voltage1", true)
            .ok_or_else(|| anyhow::anyhow!("voltage1 not found"))?;

        // De-interleave I and Q samples
        let mut i_samples = Vec::with_capacity(n_samples);
        let mut q_samples = Vec::with_capacity(n_samples);
        for chunk in samples.chunks_exact(2) {
            i_samples.push(chunk[0]);
            q_samples.push(chunk[1]);
        }

        // Write to channels (industrial-io will handle multiplexing into the buffer)
        v0.write(&buffer, &i_samples)?;
        v1.write(&buffer, &q_samples)?;

        buffer.push()?;

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
