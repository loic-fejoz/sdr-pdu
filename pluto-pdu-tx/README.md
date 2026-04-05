# PlutoSDR 2-FSK Gateway

A Rust binary for the **PlutoSDR (ADALM-PLUTO)** that bridges standard KISS and CAT protocols with the onboard RF transceiver to enable real-time **2FSK** satellite or terrestrial transmissions.
This project is specialized for SDR enthusiasts and satellite communication research.

## Features

- **Multi-client KISS server:** Listens on TCP port `8001` for AX.25/KISS frames.
- **Doppler management (CAT):** Includes a `rigctld`-compatible server on port `4532` for frequency correction.
- **Optimized DSP:** Fixed-point NCO with **ARM NEON SIMD** acceleration.
- **Hardware coexistence:** Manages frequency and rate so you can receive (e.g., with GQRX) while transmitting.
- **Direct hardware integration:** Uses `libiio` for low-level DMA access and tracks the hardware's actual sample rate for NCO precision.
- **Configurable Framing:** Built-in support for hardware preambles and syncwords via the modulator.

## Architecture

1.  **Ingest:** Receives KISS frames, validates them, and strips command bytes.
2.  **Frequency control:** Updates transmission frequency via the CAT server using atomic variables.
3.  **Modulation:** Converts data to I/Q samples at baseband using a continuous-phase 2FSK algorithm.
4.  **RF Output:** Pushes samples to the PlutoSDR's TX DMA. PLL parameters are set only on the TX channel to avoid disrupting concurrent RX applications.

## Installation & Build

### Prerequisites

- **Rust toolchain:** Install via [rustup](https://rustup.rs/). You'll need the `arm-unknown-linux-gnueabihf` target.
- **Cross-compiler:** `arm-linux-gnueabihf-gcc`.
- **PlutoSDR sysroot:** A folder with the Pluto's `libiio` and `glibc`.

### Compiling

Use the `Makefile` to cross-compile for PlutoSDR (ARMv7-A):

```bash
make release-pluto SYSROOT=/path/to/your/pluto-sysroot
```

The binary will be at `target/arm-unknown-linux-gnueabihf/release/pluto-pdu-tx`.

## Usage

### Command line options

```text
Usage: pluto-pdu-tx [OPTIONS]

Options:
  -l, --listen <ADDR>                 Listen address [default: 0.0.0.0]
      --kiss-port <PORT>              TCP port for KISS [default: 8001]
      --cat-port <PORT>               TCP port for CAT (rigctld) [default: 4532]
  -f, --frequency <HZ>                Initial frequency in Hz [default: 144000000]
      --offset <HZ>                   Compensate for PPM offset [default: 0]
  -b, --baud-rate <RATE>              Baud rate [default: 9600]
  -d, --deviation <HZ>                FSK deviation in Hz [default: 2400]
      --attenuation <DB>              TX attenuation in dB (0-89) [default: 10.0]
      --bandwidth <BANDWIDTH>         SDR analog bandwidth [default: 1000000]
      --attenuation <ATTENUATION>     TX attenuation in dB (0-89) [default: 10.0]
      --preamble <PREAMBLE>           Preamble byte (hex) [default: 0x55]
      --preamble-repetition <REP>     Preamble repetitions [default: 8]
      --syncword <SYNCWORD>           Syncword (hex) [default: 0x7E]
  -h, --help                          Print help
```

### Important: Unlocking PlutoSDR frequency range

The PlutoSDR (AD9363) is restricted by default. To operate on the 2m or 70cm bands, you must unlock the extended frequency range:

1. SSH into the PlutoSDR (`ssh root@192.168.2.1`).
2. Run these commands:
   ```bash
   fw_setenv attr_name compatible
   fw_setenv attr_val ad9364  # Or ad9361
   reboot
   ```

For detailed instructions, refer to the [Analog Devices Wiki: Customizing PlutoSDR](https://wiki.analog.com/university/tools/pluto/users/customizing).

### Note on Hardware Coexistence

This application is designed to be "GQRX-friendly". By setting the sample rate and bandwidth strictly on the TX physical channel (rather than the device level), it avoids forcing a global BBPLL retune. This allows you to monitor your own transmission or receive on a different frequency without GQRX losing its lock or experiencing frequency shifts when `pluto-tx-2fsk` starts or changes parameters.

## License

MIT or Apache 2.0.
