# PlutoSDR 2FSK Gateway

A Rust-native binary designed for the **PlutoSDR (ADALM-PLUTO)**. This application bridges standard network protocols (KISS/CAT) with the onboard AD9361/AD9363 RF transceiver to enable real-time **2FSK** satellite or terrestrial transmissions.

## Features

- **Multi-Client KISS Server:** Listens on TCP port `8001` for AX.25/KISS frames.
- **Doppler Management (CAT):** Integrated `rigctld`-compatible server on TCP port `4532` for real-time frequency correction.
- **Optimized DSP:** Fixed-point NCO (Numerically Controlled Oscillator) with **ARM NEON SIMD** acceleration for low CPU overhead.
- **Configurable Framing:** Built-in support for hardware preambles and syncwords.
- **Hardware Integration:** Direct DMA access via `libiio` for low-latency RF output.
- **Robustness:** Asynchronous networking powered by `tokio`, ensuring the transmission engine remains fed even under network jitter.

## Architecture

1.  **Network Ingest:** KISS frames are received, validated, and stripped of command bytes.
2.  **Frequency Control:** The CAT server updates a shared frequency variable (AtomicU64) used by the transmission engine.
3.  **Modulation:** Data is modulated into I/Q samples at baseband using a continuous-phase 2FSK algorithm.
4.  **RF Output:** Samples are pushed to the PlutoSDR's TX DMA via the `industrial-io` bindings.

## Installation & Build

### Prerequisites

- **Rust Toolchain:** `rustup target add arm-unknown-linux-gnueabihf` (Requires Nightly for `portable_simd`).
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

### Command Line Arguments

```text
Usage: pluto-tx-2fsk [OPTIONS]

Options:
  -l, --listen <LISTEN>               Listen address [default: 0.0.0.0]
      --kiss-port <KISS_PORT>         TCP port for KISS [default: 8001]
      --cat-port <CAT_PORT>           TCP port for CAT (rigctld) [default: 4532]
  -f, --frequency <FREQUENCY>         Initial frequency in Hz [default: 144000000]
  -b, --baud-rate <BAUD_RATE>         Baud rate [default: 9600]
  -d, --deviation <DEVIATION>         FSK deviation in Hz [default: 2400]
  -s, --sample-rate <SAMPLE_RATE>     SDR sample rate (Min ~2.1MSPS) [default: 2100000]
      --bandwidth <BANDWIDTH>         SDR analog bandwidth [default: 1000000]
      --attenuation <ATTENUATION>     TX attenuation in dB (0-89) [default: 10.0]
      --preamble <PREAMBLE>           Preamble byte (hex) [default: 0x55]
      --preamble-repetition <REP>     Preamble repetitions [default: 8]
      --syncword <SYNCWORD>           Syncword (hex) [default: 0x7E]
  -h, --help                          Print help
```

### Communicating with Spino Radioboard

To communicate with a Spino radioboard configured for 2400 baud FSK at 145.83 MHz:

```bash
./pluto-tx-2fsk \
    --frequency 145830000 \
    --baud-rate 2400 \
    --deviation 1200 \
    --preamble 0x55 \
    --preamble-repetition 8 \
    --syncword 0x743F19E4 \
    --attenuation 10.0
```

### Important: Unlocking PlutoSDR Frequency Range

By default, the PlutoSDR (AD9363) is restricted to **325 MHz - 3.8 GHz**. To operate on the 2m band (145 MHz) or 70cm band, you must unlock the extended frequency range (70 MHz - 6 GHz) by modifying the environment variables on the PlutoSDR:

1. SSH into the PlutoSDR (`ssh root@192.168.2.1`).
2. Run the following commands:
   ```bash
   fw_setenv attr_name compatible
   fw_setenv attr_val ad9364  # Or ad9361
   reboot
   ```
For detailed instructions, refer to the [Analog Devices Wiki: Customizing PlutoSDR](https://wiki.analog.com/university/tools/pluto/users/customizing).

## Development

- **Run Tests:** `make test` (Uses a Mock device for host-side validation).
- **Check Code:** `make check` (Validates cross-compilation environment).
- **Lint:** `cargo clippy`.

## License

This project is specialized for SDR enthusiasts and satellite communication research.
