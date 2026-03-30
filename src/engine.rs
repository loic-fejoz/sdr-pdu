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
        while let Some(frame) = self.receiver.recv().await {
            let freq = self.freq_shared.load(Ordering::SeqCst);
            info!("Transmitting frame (len={}) at {} Hz", frame.len(), freq);

            if let Err(e) = self.device.set_frequency(freq) {
                error!("Failed to set frequency: {}", e);
                continue;
            }

            let iq_buffer = self.modulator.modulate(&frame);

            if let Err(e) = self.device.push_samples(&iq_buffer) {
                error!("Failed to push samples: {}", e);
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
        let device = MockDevice::new(1000000, 200000).unwrap();
        let modulat = FskModulator::new(1000000, 9600, 2400);

        let engine = TransmissionEngine::new(device, modulat, freq.clone(), rx);

        tx.send(vec![0xAA]).await.unwrap();
        drop(tx); // Close channel to terminate run loop

        engine.run().await;
        // Check if device received something - we need to get back the device or use Arc/Mutex for mock
    }
}
