use crate::hackrf::SdrDevice;
use crate::modulator::FskModulator;
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

        // We assume SDR starts in a disabled TX state.
        if let Err(e) = self.device.disable_tx().await {
            error!("Failed to initially disable TX: {}", e);
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

            if let Err(e) = self.device.set_frequency(freq).await {
                error!("Failed to set frequency: {}", e);
                continue;
            }

            let mut full_iq = Vec::with_capacity(preamble_iq.len() + frame_iq.len());
            full_iq.extend_from_slice(preamble_iq);
            full_iq.extend_from_slice(&frame_iq);

            if let Err(e) = self.device.enable_tx().await {
                error!("Failed to enable TX: {}", e);
            }

            if let Err(e) = self.device.push_samples(&full_iq).await {
                error!("Failed to push samples: {}", e);
            }

            if let Err(e) = self.device.disable_tx().await {
                error!("Failed to disable TX after transmission: {}", e);
            }
        }
    }
}
