# Conventions & Standards

## Code Style
- Use `cargo fmt` for formatting.
- Use `cargo clippy` for linting.

## Error Handling
- **Application Level:** Use `anyhow::Result<()>` for top-level results (`src/main.rs`).
- **Module Level:** Prefer specific `io::Error` for codecs or `anyhow::Error` for device control.
- **Granularity:** Hardware operations MUST include context (e.g., `map_err(|e| anyhow!("Failed to set frequency to {}: {}", freq, e))`). Include requested vs. actual values whenever possible.
- **Fail-Safe:** In the transmission loop (`src/engine.rs`), use `error!` logging to report failures without crashing the entire service.

## Documentation & Comments
- **Mandatory Comments:** Any code handling hardware workarounds (e.g., PlutoSDR DMA buffer persistence, DDS disabling) or optimized SIMD loops must be documented with technical rationale.
- **Refactoring:** When refactoring, comments explaining "why" (not just "what") must be preserved or improved, never deleted.

## Async & Concurrency
- **Runtime:** `tokio::main` multi-threaded runtime.
- **Cancellation:** Use `tokio::select!` for managing concurrent tasks (`src/main.rs`).
- **Shared Data:** Use `Arc<AtomicU64>` for Doppler frequency updates; avoid `Mutex` where atomics suffice.

## Verification
- **Unit Tests:** Mandatory for all codec/DSP logic (`src/kiss.rs`, `src/nco.rs`, `src/modulator.rs`).
- **Hardware Mocking:** Use `MockDevice` (`src/pluto.rs`) to test `TransmissionEngine` logic without an actual SDR.
- **Verification Loop:** Always run `make test`, `cargo fmt`, and `cargo clippy` before declaring a task done.
