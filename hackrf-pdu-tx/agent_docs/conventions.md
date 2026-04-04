# Conventions: HackRF TX 2-FSK

## Project Principles
1. **Async-First**: All I/O (SDR, TCP, KISS) must remain non-blocking. Use `tokio` for scheduling.
2. **Deterministic Errors**: Use `anyhow::Result` for application-level logic and `.context()` to provide traceable hardware error messages.
3. **Hardware Safety**: Hardware transitions (e.g., enabling/disabling the SDR amp) should always happen through `SdrDevice` methods.

## Logic Patterns
- **Memory Management**: For high-bandwidth IQ samples, reuse buffers from `waverave-hackrf::Transmit::get_buffer()` to minimize allocation churn.
- **Sample Typing**: IQ samples are strictly `i8`. The `ComplexI8` type from `num-complex` is used for math, but `i8` slices are used for bulk transport to the driver.
- **Backpressure**: Always monitor the `pending()` count when pushing samples to hardware to prevent memory exhaustion (see `src/hackrf.rs`).

## Logging & Observability
- **`tracing` Levels**:
  - `info!`: Significant events (server started, frame transmitted).
  - `warn!`: Non-fatal hardware issues or invalid KISS frames.
  - `error!`: Fatal connection drops or SDR failure.
- Avoid repetitive logs in the modulator or NCO. Log only at the `engine.rs` level for each transmitted frame.

## Code Style
- Follow the standard Rust style. Run `cargo fmt` before every commit.
- Use `#[cfg(test)]` to keep test utilities out of release binaries.
