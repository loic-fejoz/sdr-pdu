# HackRF 2-FSK Transmitter

A high-performance, asynchronous transmitter for the HackRF One SDR. It bridges digital protocols (like AX.25 via KISS) to the HackRF hardware and includes a CAT server for remote frequency control.

## Features

- **2-FSK modulation:** Software-defined modulation with adjustable baud rate and deviation.
- **KISS server:** Listen on TCP (default port 8001) for KISS-framed data from tools or custom scripts.
- **CAT server:** A `rigctld`-compatible server (default port 4532) for changing frequency on the fly.
- **Optimized performance:** Uses a pre-computed NCO lookup table and 8-bit complex IQ samples designed specifically for the HackRF's DAC.
- **Async architecture:** Built with `tokio` for low-latency, non-blocking I/O.

## Installation

### Prerequisites

- **Rust:** Install via [rustup.rs](https://rustup.rs/).
- **HackRF One:** Hardware connected via USB.
- **libusb:** Make sure `libusb-1.0` is installed on your system.

### Build

```bash
cargo build --release
```

## Usage

Start the transmitter with default settings (144.0 MHz at 9600 baud):

```bash
./target/release/hackrf-pdu-tx --frequency 144000000 --baud-rate 9600
```

### Common CLI options

| Option | Description | Default |
| :--- | :--- | :--- |
| `-f, --frequency` | Center frequency in Hz | `144000000` |
| `-b, --baud-rate` | Transmission baud rate | `9600` |
| `-d, --deviation` | FSK frequency deviation in Hz | `2400` |
| `--tx-vga` | HackRF TX VGA gain (0-47 dB) | `20` |
| `--amp-enable` | Enable the 14dB front-end RF amplifier | `false` |
| `--scramble` | Enable G3RUH scrambling | `false` |
| `--poly` | Scrambler polynomial (hex mask) | `0x10800` |
| `--seed` | Scrambler initial seed (hex) | `0x1FFFF` |
| `--kiss-port` | TCP port for KISS frames | `8001` |
| `--cat-port` | TCP port for Rigctld control | `4532` |

### Example: 9600 baud packet radio

```bash
./target/release/hackrf-pdu-tx \
  --frequency 433500000 \
  --baud-rate 9600 \
  --deviation 2400 \
  --tx-vga 35 \
  --amp-enable
```

## Sending data

Once the server is running, send KISS-framed data to the KISS port (8001). 

**Quick test with `socat`:**
```bash
# Send "Hello" wrapped in KISS FEND (0xC0) and Data Command (0x00)
echo -ne "\xc0\x00Hello\xc0" | socat - TCP:127.0.0.1:8001
```

## License

MIT or Apache 2.0.
