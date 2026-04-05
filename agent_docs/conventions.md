# Global SDR-PDU Conventions & Standards

## Code Style & Quality
- **Language:** Rust (Nightly Edition 2024).
- **Formatting:** Code MUST be formatted with `cargo fmt`.
- **Linting:** Use `cargo clippy --workspace --all-targets -- -D warnings` for strict linting. This is mandatory before completing any task.
- **Dependencies:** Use workspace dependencies in `Cargo.toml` to keep versions synchronized across crates.

## Async & Concurrency
- **Runtime:** `tokio` is the primary asynchronous runtime.
- **Patterns:** Favor idiomatic `tokio` constructs: `tokio::select!`, `tokio::spawn`, `tokio_util::codec`, and `tokio::sync` channels.
- **Non-blocking:** Ensure all I/O (SDR, TCP, KISS) remains non-blocking to maintain real-time performance.

## Error Handling
- **Application Level:** Use `anyhow::Result` and the `anyhow` crate for top-level logic and error propagation.
- **Context:** Use `.context()` or `map_err` to provide granular, traceable details, especially for hardware operations (e.g., requested vs. actual values).
- **Fail-Safe:** In critical loops (e.g., transmission), use `error!` logging to report failures without crashing the entire service.

## Logging & Observability
- **Crate:** Use the `tracing` crate for logging.
- **Levels**:
  - `info!`: Significant events (server started, frame transmitted).
  - `warn!`: Non-fatal hardware issues or invalid frames.
  - `error!`: Fatal connection drops or hardware failures.
- **Noise reduction:** Avoid repetitive logs in hot paths (modulators, NCOs). Log at the orchestration level (e.g., `engine.rs`).

## Documentation Integrity
- **Context Preservation:** NEVER remove technical comments explaining hardware workarounds, SIMD logic, or critical timing. These are foundational.
- **Additive Updates:** Updates to `README.md` or `agent_docs/` should be additive or corrective.
- **"Why" over "What":** When refactoring, preserve or improve comments explaining the rationale behind specific implementations.
- **Human-Centricity:** Ensure cross-compilation examples, CLI usage, and architectural justifications are preserved.

## Verification Mandate
A task is only considered complete when:
1. `cargo test --workspace` passes (or project-specific `make test`).
2. `cargo fmt` has been run.
3. `cargo clippy --workspace --all-targets -- -D warnings` passes without warnings.
