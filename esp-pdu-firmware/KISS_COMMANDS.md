# KISS Commands & Subcommands (`esp-pdu-firmware`)

`esp-pdu-firmware` implements the KISS protocol over UART0 (`115200` baud by default) and `esp-pdu-tnc` proxies all supported KISS commands bidirectionally between TCP/WebSocket KISS clients and the ESP32/ESP32-S3 firmware.

## 1. KISS Framing

Standard KISS framing (`esp-pdu-protocol`):

- `FEND = 0xC0`: Frame delimiter
- `FESC = 0xDB`: Escape byte
- `TFEND = 0xDC`: Escaped `FEND` (`0xDB 0xDC` → `0xC0`)
- `TFESC = 0xDD`: Escaped `FESC` (`0xDB 0xDD` → `0xDB`)

Each decoded frame has the form:
```text
[ TYPE_BYTE ] [ PAYLOAD ... ]
```
Only KISS port `0` (`0x00`–`0x06`) and `Return` (`0xFF`) are accepted; non-zero KISS ports and `0x0F` are ignored.

---

## 2. Standard KISS Commands

| Command | Code (`TYPE_BYTE`) | Direction | Payload | Firmware Behavior |
|---------|--------------------|-----------|---------|-------------------|
| `DataFrame` | `0x00` | Host ↔ ESP32 | `1..=250` bytes (raw PDU, no fragmentation) | TX: waits `TxDelay`, sends via ESP-NOW to `tx_peer` (broadcast `FF:FF:FF:FF:FF:FF` by default, or unicast peer), waits `TxTail`. RX: pushes a `Status` (`0x06 0x85`) frame followed by the received PDU so the host has fresh RSSI. |
| `TxDelay` | `0x01` | Host → ESP32 | 1 byte (`u8`, units of 10 ms) | Sets pre-TX delay (`tx_delay_10ms`). |
| `Persistence` | `0x02` | Host → ESP32 | 1 byte (`u8`, `0..=255`) | Stores KISS persistence parameter. |
| `SlotTime` | `0x03` | Host → ESP32 | 1 byte (`u8`, units of 10 ms) | Stores KISS slot time parameter. |
| `TxTail` | `0x04` | Host → ESP32 | 1 byte (`u8`, units of 10 ms) | Sets post-TX delay (`tx_tail_10ms`). |
| `FullDuplex` | `0x05` | Host → ESP32 | 1 byte (`0` = half, nonzero = full) | Stores KISS full-duplex flag. |
| `SetHardware` | `0x06` | Host ↔ ESP32 | `[ SUBCMD, ... ]` (see §3) | Configures radio parameters or reports status. |
| `Return` | `0xFF` | Host → ESP32 | Empty | Accepted as a no-op (stays in KISS mode). |

---

## 3. `SetHardware (0x06)` Subcommands

All `SetHardware` frames start with `0x06` followed by a 1-byte subcommand ID:

| Subcommand | ID | Direction | Payload after ID | Description |
|------------|----|-----------|------------------|-------------|
| `SetChannel` | `0x01` | Host → ESP32 | `ch: u8` (`1..=6`) | Sets Wi-Fi channel (2412–2437 MHz, French 2.4 GHz amateur band). Channels outside `1..=6` are ignored. Replies with `Status`. |
| `SetFrequencyHz` | `0x02` | Host → ESP32 | `hz: u64` (8 B big-endian) | Maps frequency in Hz (`2400_000_000..=2439_500_000`) to nearest legal channel `1..=6`. Out-of-band frequencies are ignored. Replies with `Status`. |
| `SetTxPowerQdbm` | `0x03` | Host → ESP32 | `qdbm: u8` (`8..=80`) | Sets max Wi-Fi TX power in 0.25 dBm steps (clamped to `8..=80`, i.e. `2.0..=20.0` dBm). Replies with `Status`. |
| `SetPhyRate` | `0x04` | Host → ESP32 | `rate: u8` (see §4) | Sets ESP-NOW PHY rate and protocol (`802.11b/g/n` or `802.11LR`). Replies with `Status`. |
| `GetStatus` | `0x05` | Host → ESP32 | None | Requests current radio status. Replies with `Status`. |
| `SetPeerMac` | `0x07` | Host → ESP32 | `mac: [u8; 6]` | Sets TX destination MAC (`FF:FF:FF:FF:FF:FF` for broadcast, or a unicast MAC registered as an unencrypted peer). RX always accepts both broadcast and unicast-to-us. Replies with `Status`. |
| `GetPeerMac` | `0x08` | Host → ESP32 | None | Requests the active TX destination peer MAC. Replies with `PeerMac (0x88)`. |
| `Status` | `0x85` | ESP32 → Host | 12 bytes (see below) | Emitted on boot, after any `SetHardware` command, and before each RX `DataFrame`. |
| `PeerMac` | `0x88` | ESP32 → Host | `mac: [u8; 6]` | Response to `GetPeerMac (0x08)` containing the active 6-byte TX peer MAC. |

### `Status (0x85)` Payload Layout (12 bytes after `0x06 0x85`)

| Offset | Field | Type | Description |
|--------|-------|------|-------------|
| 0 | `channel` | `u8` | Active Wi-Fi channel (`1..=6`) |
| 1 | `tx_power_qdbm` | `u8` | Active TX power in 0.25 dBm units (`8..=80`) |
| 2 | `phy_rate` | `u8` | Active `PhyRate` code (see §4) |
| 3..7 | `rx_count` | `u32` BE | Total ESP-NOW PDUs received |
| 7..11 | `tx_count` | `u32` BE | Total ESP-NOW PDUs transmitted |
| 11 | `last_rssi` | `i8` | RSSI in dBm of the last received ESP-NOW frame |

---

## 4. `PhyRate` Codes (`SetPhyRate 0x04`)

| Code | Name | CLI (`--phy-rate`) | Mode |
|------|------|--------------------|------|
| `0x00` | `Rate1ML` | `1m` | 802.11b 1 Mbps (Long Preamble, default) |
| `0x01` | `Rate2ML` | `2m` | 802.11b 2 Mbps |
| `0x02` | `Rate5_5ML` | `5.5m` | 802.11b 5.5 Mbps |
| `0x03` | `Rate11ML` | `11m` | 802.11b 11 Mbps |
| `0x04`–`0x0B` | `Rate6M`..`Rate54M` | `6m`, `9m`, `12m`, `18m`, `24m`, `36m`, `48m`, `54m` | 802.11g ERP-OFDM |
| `0x10`–`0x17` | `RateMcs0Lgi`..`RateMcs7Lgi` | `mcs0`..`mcs7` | 802.11n HT-OFDM (Long GI) |
| `0x20` | `RateLr512K` | `lr` / `lr-512k` | Espressif 802.11LR 512 kbps |
| `0x21` | `RateLr250K` | `lr-250k` | Espressif 802.11LR 250 kbps |

> **Note on Long-Range (`802.11LR`) mode:** Selecting `lr-512k` or `lr-250k` switches the Wi-Fi interface exclusively to `Protocol::P802D11LR`. Both endpoints must configure an `802.11LR` rate to communicate with each other.
