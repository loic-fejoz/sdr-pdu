# Software Defined Radio (SDR) - Protocol Data Unit (PDU)

## Mission

Develop a set of utility tools to use SDR radios like plutosdr, hackrf, or MMDVM Hat to send and receive frames in various modes for hamradio operations, like satellites QSO.

## Project Map

- `hackrf-pdu-tx`: a KISS and CAT server to send 2FSK frames over an HackRF
- `pluto-pdu-tx`: a KISS and CAT server to send 2FSK frames over a PlutoSDR

You must read individual `AGENTS.md` files to follow their own guidelines.

## Tech Stack

- **Language:** Rust (Nightly Edition 2024)

## Development Principles

- **Context Preservation:** NEVER remove technical comments explaining hardware workarounds, SIMD logic, or critical timing.
- **Documentation Integrity:** When updating `README.md` or `agent_docs`, ensure that existing technical context, "human-centric" instructions (e.g., cross-compilation examples), and feature descriptions are preserved or refined. Never delete useful documentation to "clean up" unless it is factually incorrect.