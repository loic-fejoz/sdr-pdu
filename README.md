# HackRF TX 2-FSK

A high-performance, asynchronous 2-FSK transmitter for the HackRF One SDR. This tool acts as a bridge between digital radio protocols (like AX.25 via KISS) and the HackRF hardware, featuring a built-in CAT server for remote frequency control.

## Features

*   **2-FSK Modulation**: Efficient software-defined FSK modulation with configurable baud rate and deviation.
*   **KISS Server**: Listen on a TCP port (default 8001) for KISS-framed data (e.g., from Dire Wolf, Xastir, or custom scripts).
*   **CAT Server**: Rigctld-compatible TCP server (default 4532) for real-time frequency control.
*   **Performance Optimized**: Uses a pre-computed NCO lookup table and 8-bit complex IQ samples tailored for the HackRF's DAC.
*   **Async Architecture**: Built on `tokio` and `waverave-hackrf` for non-blocking I/O and low-latency transmission.

## Installation

### Prerequisites

*   **Rust**: [Install Rust](https://rustup.rs/) (edition 2024 recommended).
*   **HackRF One**: Hardware connected via USB.
*   **libusb**: Ensure `libusb-1.0` is installed on your system.

### Build

```bash
git clone https://github.com/youruser/hackrf-tx-2fsk.git
cd hackrf-tx-2fsk
cargo build --release
```

## Usage

Run the transmitter with default settings (144.0 MHz, 9600 baud):

```bash
./target/release/hackrf-tx-2fsk --frequency 144000000 --baud-rate 9600
```

### Common CLI Options

| Option | Description | Default |
| :--- | :--- | :--- |
| `-f, --frequency` | Center frequency in Hz | `144000000` |
| `-b, --baud-rate` | Transmission baud rate | `9600` |
| `-d, --deviation` | FSK frequency deviation in Hz | `2400` |
| `--tx-vga` | HackRF TX VGA gain (0-47 dB) | `20` |
| `--amp-enable` | Enable 14dB front-end RF amplifier | `false` |
| `--scramble` | Enable G3RUH scrambling | `false` |
| `--poly` | Scrambler polynomial (hex mask) | `0x10800` |
| `--seed` | Scrambler initial seed (hex) | `0x1FFFF` |
| `--kiss-port` | TCP port for KISS frames | `8001` |
| `--cat-port` | TCP port for Rigctld control | `4532` |

### Example: High-Speed Packet Radio (9600 baud)

```bash
./target/release/hackrf-tx-2fsk \
  --frequency 433500000 \
  --baud-rate 9600 \
  --deviation 2400 \
  --tx-vga 35 \
  --amp-enable
```

## How to Send Data

Once the server is running, you can send KISS-framed data to the KISS port (8001). 

**Using `socat` (Simple Test):**
```bash
# Send "Hello" wrapped in KISS FEND (0xC0) and Data Command (0x00)
echo -ne "\xc0\x00Hello\xc0" | socat - TCP:127.0.0.1:8001
```

## Architecture

*   **`src/nco.rs`**: Fast `i8` phase-accumulator for IQ generation.
*   **`src/modulator.rs`**: Bit-to-symbol mapping and preamble/syncword insertion.
*   **`src/hackrf.rs`**: Hardware abstraction using the `waverave-hackrf` pure-Rust driver.
*   **`src/engine.rs`**: The main transmission loop handling timing and state transitions.

## License

MIT or Apache 2.0.
