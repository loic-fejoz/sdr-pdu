# PlutoSDR 2FSK Gateway

A Rust-native binary designed for the **PlutoSDR (ADALM-PLUTO)**. This application bridges standard network protocols (KISS/CAT) with the onboard AD9361/AD9363 RF transceiver to enable real-time **2FSK** satellite or terrestrial transmissions.

## Features

- **Multi-Client KISS Server:** Listens on TCP port `8001` for AX.25/KISS frames.
- **Doppler Management (CAT):** Integrated `rigctld`-compatible server on TCP port `4532` for real-time frequency correction.
- **Optimized DSP:** Fixed-point NCO (Numerically Controlled Oscillator) designed for the ARM Cortex-A9 architecture.
- **Hardware Integration:** Direct DMA access via `libiio` for low-latency RF output.
- **Robustness:** Asynchronous networking powered by `tokio`, ensuring the transmission engine remains fed even under network jitter.

## Architecture

1.  **Network Ingest:** KISS frames are received, validated, and stripped of command bytes.
2.  **Frequency Control:** The CAT server updates a shared frequency variable (AtomicU64) used by the transmission engine.
3.  **Modulation:** Data is modulated into I/Q samples at baseband using a continuous-phase 2FSK algorithm.
4.  **RF Output:** Samples are pushed to the PlutoSDR's DAC via the `industrial-io` bindings.

## Installation & Build

### Prerequisites

- **Rust Toolchain:** `rustup target add arm-unknown-linux-gnueabihf`
- **Cross-Compiler:** `arm-linux-gnueabihf-gcc`
- **PlutoSDR Sysroot:** A directory containing the Pluto's `libiio` and `glibc` (e.g., `~/pluto-0.30.sysroot/`).

### Compiling

Use the provided `Makefile` to cross-compile for the PlutoSDR hardware:

```bash
# Build for PlutoSDR (ARMv7-A)
make release-pluto SYSROOT=/path/to/your/pluto-sysroot
```

The resulting binary will be located at `target/arm-unknown-linux-gnueabihf/release/pluto-tx-2fsk`.

## Usage

1.  **Deploy:** Copy the binary to your PlutoSDR via `scp`.
2.  **Run:**
    ```bash
    ./pluto-tx-2fsk
    ```
3.  **Connect KISS:** Configure your TNC client (e.g., Direwolf, Gpredict, or custom script) to connect to `<pluto-ip>:8001`.
4.  **Connect CAT:** Configure Gpredict or Hamlib to update frequency via `<pluto-ip>:4532`.

## Development

- **Run Tests:** `make test` (Uses a Mock device for host-side validation).
- **Check Code:** `make check` (Validates cross-compilation environment).
- **Lint:** `cargo clippy`.

## License

This project is specialized for SDR enthusiasts and satellite communication research.
