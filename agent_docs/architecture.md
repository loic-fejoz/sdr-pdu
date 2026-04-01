# Architecture: Hub-and-Spoke System

## Data Flow (Uplink)
1. **KissServer (`src/kiss_server.rs`)**: Multiple TCP clients on port 8001. Deframes KISS (0xC0), filters Data-only (0x00), removes command byte.
2. **Async Channel**: `tokio::sync::mpsc` (Capacity 100) carries `Vec<u8>` payloads.
3. **TransmissionEngine (`src/engine.rs`)**: Consumes the channel, fetches Doppler-corrected frequency.
4. **FskModulator (`src/modulator.rs`)**: Performs baseband 2FSK modulation using `TableNco`.
5. **SdrDevice (`src/pluto.rs`)**: Pushes raw I16 IQ samples to IIO DMA.

## Shared State (Doppler)
- **CatServer (`src/cat.rs`)**: Listen for `rigctld` commands. Updates an `Arc<AtomicU64>` with the target frequency.
- **Engine**: Reads the `AtomicU64` before each transmission frame to update the local oscillator.

## Abstractions
- **SdrDevice Trait**: Decouples hardware IIO from transmission logic. Allows `MockDevice` for unit testing the engine without hardware.

## Safety & Silence (RF Watchdog)
- **Status:** Mandatory.
- **Mechanism:** The `TransmissionEngine` forces the `SdrDevice` into a "disabled" state (max attenuation + zeroed DMA) during idle periods. This prevents parasitic carriers and ensures regulatory compliance during application crashes or network silence.
