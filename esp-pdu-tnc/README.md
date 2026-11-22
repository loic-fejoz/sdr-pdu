# esp-pdu-tnc & esp-pdu-firmware

A KISS + CAT (rigctld) gateway and ESP32 / ESP32-S3 firmware for transmitting and receiving Protocol Data Units (PDUs) over **unencrypted ESP-NOW** in the **French 2.4 GHz amateur radio band (2400–2450 MHz, Wi-Fi channels 1–6)**.

## Architecture

- **`esp-pdu-protocol`**: Shared `#![no_std]` crate defining KISS framing, `SetHardware (0x06)` subcommands (channel, frequency, TX power, PHY rate, status), and French 2.4 GHz amateur band channel validation (channels 1–6, 2412–2437 MHz).
- **`esp-pdu-firmware`**: Embassy + `esp-hal` + `esp-wifi` (ESP-NOW) firmware supporting both:
  - `esp32` (ESP-WROOM-32, UART0 GPIO1/GPIO3)
  - `esp32s3` (Heltec WiFi Kit 32 V3.1, UART0 GPIO43/GPIO44)
- **`esp-pdu-tnc`**: Host Tokio service exposing bidirectional TCP KISS (`--kiss-port`), WebSocket KISS (`--kiss-ws-port`), and CAT (`--cat-port`) servers over the ESP32 USB serial port.

See [OVER_THE_AIR.md](./OVER_THE_AIR.md) for the full RF, PHY modulation, preamble, and ESP-NOW MPDU specification (for building an independent SDR decoder).

## Toolchain Setup (First Time)

Run the non-root installer/updater script:

```bash
./scripts/install-esp-toolchain.sh
source ~/export-esp.sh
```

## Building & Flashing Firmware

### Heltec WiFi Kit 32 V3.1 (ESP32-S3, e.g. `/dev/ttyUSB0`)

```bash
source ~/export-esp.sh
cd esp-pdu-firmware
cargo +esp build --release --target xtensa-esp32s3-none-elf --features esp32s3 --no-default-features
espflash flash --port /dev/ttyUSB0 target/xtensa-esp32s3-none-elf/release/esp-pdu-firmware
```

### ESP-WROOM-32 (ESP32, e.g. `/dev/ttyACM0`)

```bash
source ~/export-esp.sh
cd esp-pdu-firmware
cargo +esp build --release --target xtensa-esp32-none-elf --features esp32 --no-default-features
espflash flash --port /dev/ttyACM0 target/xtensa-esp32-none-elf/release/esp-pdu-firmware
```

## Features & Operations

- **Dynamic Baud Rate Negotiation (`--baud-rate`):** Firmware boots at `115200` baud; `esp-pdu-tnc` automatically negotiates higher rates (e.g. `--baud-rate 921600`) via `SetUartBaud`.
- **Automatic Serial Reconnect & Reboot Recovery:** TCP/WS KISS and CAT servers stay alive across USB disconnects or ESP32 watchdog/panic resets, automatically reconnecting and re-applying active radio configuration.
- **Fail-Safe Callsign ID Beacon (`--callsign`, `--beacon-interval-secs`):** Optionally transmits `DE <CALLSIGN> ESP-PDU-TNC` periodically whenever TX activity occurred during the interval (default `600` seconds).
- **CAT Signal Strength:** Supports `l STRENGTH` (S9-normalized `rssi_dbm + 73`) and `l RAWSTR` (raw `rssi_dbm`) over Hamlib `rigctld`.

## Running the Host KISS + CAT Servers

Terminal 1 (ESP-WROOM-32):

```bash
cargo run --release -p esp-pdu-tnc -- --serial-port /dev/ttyACM0 --kiss-port 8001 --kiss-ws-port 8002 --cat-port 4532 -f 2412000000
```

Terminal 2 (Heltec V3.1):

```bash
cargo run --release -p esp-pdu-tnc -- --serial-port /dev/ttyUSB0 --kiss-port 8011 --kiss-ws-port 8012 --cat-port 4542 -f 2412000000
```
