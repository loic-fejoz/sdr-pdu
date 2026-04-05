# Software Defined Radio (SDR) - Protocol Data Unit (PDU) Tools

A collection of utilities for sending and receiving data frames with SDRs like the PlutoSDR, HackRF, or MMDVM hat.

## Why PDU?

In satellite operations, you need a simple, reliable way to handle frames. I use the term **PDU** (Protocol Data Unit) instead of "packet" or "frame" to avoid confusion with traditional AX.25 packet radio. Here, a PDU is just the raw data you want to transmit - whether that is AX.25 or something custom.

## The Tools

- [**`hackrf-pdu-tx`**](./hackrf-pdu-tx/): An async 2-FSK transmitter for the HackRF One. It connects KISS-based digital protocols to the hardware and includes a CAT server for frequency control.
- [**`pluto-pdu-tx`**](./pluto-pdu-tx/): A gateway for the PlutoSDR (AD9361/AD9363) written in Rust. It supports KISS, CAT (for Doppler correction), and uses ARM NEON for faster DSP.
- [**`pdu-proxy`**](./pdu-proxy/): A developer tool that proxies a TCP KISS stream (like from Direwolf) to multiple TCP and WebSocket clients.
- [**`sdr-pdu-utils`**](./sdr-pdu-utils/): A shared Rust crate containing the core KISS and server logic used by the other tools.
- [**`js-client`**](./js-client/): A JavaScript library designed to talk to these tools over WebSockets.

## License

MIT or Apache 2.0.
