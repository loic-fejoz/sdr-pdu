# pdu-proxy Developer/Agent Guidelines

Welcome to the `pdu-proxy` crate! This directory contains architectural information and coding conventions tailored to this proxy utility.

Please refer to the documents in `agent_docs/` to ensure your contributions adhere to our standards:
- [Architecture](agent_docs/architecture.md)
- [Conventions](agent_docs/conventions.md)
- **[Global Conventions](../../agent_docs/conventions.md)**

## Verification Mandate
A task is only considered complete when:
1. `cargo test --package pdu-proxy` passes.
2. `cargo clippy --workspace --all-targets -- -D warnings` passes.
3. Code is formatted with `cargo fmt`.
