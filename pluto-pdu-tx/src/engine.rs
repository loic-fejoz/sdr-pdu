use crate::modulator::FskModulator;
use crate::pluto::SdrDevice;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct TransmissionEngine<D: SdrDevice> {
    device: D,
    modulator: FskModulator,
    freq_shared: Arc<AtomicU64>,
    receiver: mpsc::Receiver<Vec<u8>>,
}

impl<D: SdrDevice> TransmissionEngine<D> {
    pub fn new(
        device: D,
        modulator: FskModulator,
        freq_shared: Arc<AtomicU64>,
        receiver: mpsc::Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            device,
            modulator,
            freq_shared,
            receiver,
        }
    }

    pub async fn run(mut self) {
        info!("Transmission engine started");

        // Start in silenced state
        if let Err(e) = self.device.disable_tx() {
            error!("Failed to initially silence TX: {}", e);
        }

        while let Some(frame) = self.receiver.recv().await {
            let freq = self.freq_shared.load(Ordering::SeqCst);

            // Modulate first
            let frame_iq = self.modulator.modulate(&frame);
            let preamble_iq = self.modulator.get_preamble_syncword_iq();
            let total_samples = (preamble_iq.len() + frame_iq.len()) / 2;

            info!(
                "Transmitting frame (len={}) at {} Hz. Samples: {}",
                frame.len(),
                freq,
                total_samples
            );

            if let Err(e) = self.device.set_frequency(freq) {
                error!("Failed to set frequency: {}", e);
                continue;
            }

            let mut full_iq = Vec::with_capacity(preamble_iq.len() + frame_iq.len());
            full_iq.extend_from_slice(preamble_iq);
            full_iq.extend_from_slice(&frame_iq);

            if let Err(e) = self.device.enable_tx() {
                error!("Failed to enable TX: {}", e);
            }

            if let Err(e) = self.device.push_samples(&full_iq) {
                error!("Failed to push samples: {}", e);
            }

            if let Err(e) = self.device.disable_tx() {
                error!("Failed to silence TX after transmission: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modulator::FskModulator;
    use crate::pluto::MockDevice;

    #[tokio::test]
    async fn test_engine_run() {
        let (tx, rx) = mpsc::channel(10);
        let freq = Arc::new(AtomicU64::new(144000000));
        let device = MockDevice::new(1000000, 200000, 10.0, 0).unwrap();
        let mut modulat = FskModulator::new(1000000, 9600, 2400);
        modulat
            .set_preamble_and_syncword("0x55", 8, "0x7E")
            .unwrap();

        let engine = TransmissionEngine::new(device, modulat, freq.clone(), rx);

        tx.send(vec![0xAA]).await.unwrap();
        drop(tx); // Close channel to terminate run loop

        engine.run().await;
    }
}
