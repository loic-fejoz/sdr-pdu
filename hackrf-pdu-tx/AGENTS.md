# Agent Guidance System: HackRF TX 2-FSK

## Mission
Provide a high-performance, asynchronous 2-FSK transmitter for the HackRF One SDR, supporting TCP KISS framing and CAT frequency control.

## Project Map
- `src/main.rs`: Application entry, CLI parsing, and server orchestration.
- `src/hackrf.rs`: `SdrDevice` trait and `waverave-hackrf` driver implementation.
- `src/engine.rs`: Orchestrates the flow from KISS frames to modulated IQ samples.
- `src/modulator.rs`: FSK modulation logic and preamble/syncword handling.
- `src/nco.rs`: Numerically Controlled Oscillator with `i8` LUT optimization.
- `src/kiss_server.rs` & `src/kiss.rs`: TCP KISS server and AX.25 framing logic.
- `src/cat.rs`: Rigctld-compatible CAT server for frequency control.

## Documentation Index
Read these files in `agent_docs/` before making changes:
1. **[Architecture](agent_docs/architecture.md)**: Read when adding new protocols or SDR backends.
2. **[Testing](agent_docs/testing_guidelines.md)**: Read before adding features or fixing bugs.
3. **[Conventions](agent_docs/conventions.md)**: Read to understand the async and error handling patterns.
   - Also refer to **[Global Conventions](../../agent_docs/conventions.md)**.

## Verification Mandate
ALWAYS verify changes by running `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt`. A task is only complete when all these pass.
