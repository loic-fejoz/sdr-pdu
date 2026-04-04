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
- **Mandatory Comments:** Any code handling hardware workarounds (e.g., PlutoSDR DMA buffer persistence, DDS disabling, RX/TX PLL independence) must be documented with technical rationale.
- **State Awareness:** Prefer state tracking (e.g., `last_freq`) and value comparison before hardware writes. Avoid redundant IIO attribute writes that force global PLL retunes or hardware glitches.
- **Refactoring:** When refactoring, comments explaining "why" (not just "what") must be preserved or improved, never deleted.

## Documentation Maintenance
- **Additive Updates:** Updates to `README.md` or `agent_docs/` should be additive or corrective.
- **Preservation of Context:** Never remove existing technical context, "human-centric" examples (e.g., sysroot paths, CLI usage), or architectural justifications. These are critical for onboarding and long-term maintenance.
- **Review for Regression:** Before finalizing documentation changes, verify that no useful feature descriptions or operational instructions have been lost.

## Async & Concurrency
- **Runtime:** `tokio::main` multi-threaded runtime.
- **Cancellation:** Use `tokio::select!` for managing concurrent tasks (`src/main.rs`).
- **Shared Data:** Use `Arc<AtomicU64>` for Doppler frequency updates; avoid `Mutex` where atomics suffice.

## Verification
- **Unit Tests:** Mandatory for all codec/DSP logic (`src/kiss.rs`, `src/nco.rs`, `src/modulator.rs`).
- **Hardware Mocking:** Use `MockDevice` (`src/pluto.rs`) to test `TransmissionEngine` logic without an actual SDR.
- **Verification Loop:** Always run `make test`, `cargo fmt`, and `cargo clippy` before declaring a task done.
