# PlutoSDR 2FSK Gateway - Agent Hub

## Mission
High-performance 2FSK gateway for PlutoSDR (ARMv7-A), interfacing TCP KISS/CAT network protocols with the AD9361 RF transceiver.

## Tech Stack
- **Language:** Rust (Edition 2024)
- **Runtime:** Tokio (Async Networking)
- **SDR:** Libiio / industrial-io (Hardware DMA)
- **DSP:** Fixed-point NCO with ARM NEON optimization
- **CLI:** Clap v4 (Argument Parsing)

## Development Principles
- **Context Preservation:** NEVER remove technical comments explaining hardware workarounds, SIMD logic, or critical timing.
- **Documentation Integrity:** When updating `README.md` or `agent_docs`, ensure that existing technical context, "human-centric" instructions (e.g., cross-compilation examples), and feature descriptions are preserved or refined. Never delete useful documentation to "clean up" unless it is factually incorrect.
- **Error Integrity:** Always use `anyhow::Context` or `map_err` to provide granular details.

## Critical Commands
- **Install:** `cargo fetch`
- **Test:** `make test` (MANDATORY before task completion)
- **Lint:** `cargo fmt && cargo clippy`
- **Build:** `make release-pluto` (Cross-compilation for PlutoSDR)

## Documentation Index
- `agent_docs/architecture.md`
  - **Trigger:** When modifying the data flow between network servers and the SDR engine.
- `agent_docs/conventions.md`
  - **Trigger:** Before creating new modules or implementing error handling.
- `agent_docs/dsp_implementation.md`
  - **Trigger:** When optimizing or modifying the NCO, modulation, or SIMD code.
- `agent_docs/hardware_iio.md`
  - **Trigger:** When changing RF parameters (LO, Gain, Sample Rate), IIO device paths, or cross-compilation settings (SYSROOT).

**Verification Loop:** You MUST run `make test` and ensure all unit tests pass, AND run `cargo fmt` and `cargo clippy` to ensure code quality, before declaring any implementation or fix as "done".
