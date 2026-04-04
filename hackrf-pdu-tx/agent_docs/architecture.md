# Architecture: HackRF 2-FSK Transmitter

## Data Flow
1. **KISS Input**: `src/kiss_server.rs` listens for TCP connections, decoding frames via `src/kiss.rs`.
2. **Engine Routing**: `src/engine.rs` receives raw frames, fetches the current frequency, and invokes modulation.
3. **Modulation**: `src/modulator.rs` converts bytes into bits, applying 2-FSK logic via a lookup table from `src/nco.rs`.
4. **SDR Output**: `src/hackrf.rs` feeds `i8` complex samples to the `waverave-hackrf` driver.

## Core Design Patterns
### 1. The `SdrDevice` Trait (`src/hackrf.rs`)
An asynchronous abstraction layer. Any hardware (HackRF, Pluto, RTLSDR) must implement this trait to work with the engine.
- Key methods: `enable_tx`, `disable_tx`, `set_frequency`, `push_samples`.

### 2. Typestate Transceiver State (`src/hackrf.rs`)
The `HackRfDevice` handles state transitions using the `HackRfState` enum. This ensures the hardware only receives samples when it's in the correct `Transmit` mode.

### 3. Asynchronous Backpressure (`src/hackrf.rs`)
To prevent memory bloat, `push_samples` monitors the `pending()` count of the HackRF's internal transfer queue. It uses `next_complete()` to wait for hardware completion if the queue exceeds 32 blocks.

### 4. Zero-Copy Sample Packing (`src/nco.rs`)
The NCO LUT stores packed (I, Q) pairs in `u16` format (two `i8` values) to allow direct byte-aligned writes to the HackRF's DMA-backed buffers.
