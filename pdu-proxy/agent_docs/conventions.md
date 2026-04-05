# pdu-proxy Coding Conventions

## Rust Guidelines
- **Edition:** Rust 2024.
- **Formatting:** Code MUST be formatted with `cargo fmt`. Use standard Rust naming conventions.
- **Error Handling:** Use the `anyhow` crate to handle and propagate errors (`anyhow::Result`).
- **Dependencies:** Rely heavily on workspace dependencies to keep crate versions in sync (e.g. `tokio = { workspace = true }`).
- **Async Runtime:** `tokio` is the primary runtime. Favor idiomatic `tokio` constructs (e.g., `tokio::select!`, `tokio::spawn`, `tokio::sync` channels). Use `tokio_util::codec` for framing where possible.
- **Web & Routing:** Use `axum` for HTTP and WebSocket implementations. Avoid `warp` or bare `hyper` configurations unless required.

## Structure
- Keep logic related to KISS protocol specific to the `sdr-pdu-utils` crate, where the core definitions live. Do not duplicate KISS implementations unless absolutely necessary for proxying edge-cases.
- Ensure the `main.rs` file does not become overly monolithic. Consider separating logic into distinct files/modules if complexity increases.
- Handle edge-cases regarding disconnections efficiently; specifically ensuring a disconnected target correctly breaks tasks handling that connection without panicking the entire application.