# sdr-pdu-utils

A shared Rust crate for the `sdr-pdu` tools. It contains core logic and utilities to handle KISS framing, network servers, and common DSP functions.

## Features

- **KISS decoding and encoding:** High-performance, memory-safe handling of KISS frames (including FEND/FESC escaping).
- **TCP KISS server:** A generic server for handling multiple KISS clients over TCP.
- **WebSocket KISS server:** A server to expose KISS data over WebSockets, used by `pdu-proxy` and the `js-client`.
- **CAT server:** A `rigctld`-compatible interface for frequency control.
- **DSP utilities:** NCO and modulation logic shared between the HackRF and PlutoSDR implementations.

## Usage

This crate is primarily used as a dependency for other tools in this repository. 

```toml
[dependencies]
sdr-pdu-utils = { path = "../sdr-pdu-utils" }
```

## License

MIT or Apache 2.0.
