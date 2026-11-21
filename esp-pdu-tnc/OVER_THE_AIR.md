# Over-the-Air Signal & Frame Specification (`esp-pdu-tnc`)

This document describes the exact RF, physical-layer (PHY), MAC, and ESP-NOW frame structure transmitted over the air by `esp-pdu-firmware` so that an independent receiver or SDR decoder can demodulate and extract PDUs.

---

## 1. RF Carrier & Channel Plan (French 2.4 GHz Amateur Band)

In France, the 2.4 GHz amateur radio allocation is **2400–2450 MHz**. `esp-pdu-protocol` and `esp-pdu-firmware` restrict operation to **IEEE 802.11 channels 1–6**:

| Channel | Center Frequency (`f_c`) | Occupied ~22 MHz (802.11b) | Occupied ~20 MHz (OFDM) |
|---------|--------------------------|----------------------------|-------------------------|
| 1       | 2412 MHz                 | 2401 – 2423 MHz            | 2402 – 2422 MHz         |
| 2       | 2417 MHz                 | 2406 – 2428 MHz            | 2407 – 2427 MHz         |
| 3       | 2422 MHz                 | 2411 – 2433 MHz            | 2412 – 2432 MHz         |
| 4       | 2427 MHz                 | 2416 – 2438 MHz            | 2417 – 2437 MHz         |
| 5       | 2432 MHz                 | 2421 – 2443 MHz            | 2422 – 2442 MHz         |
| 6       | 2437 MHz                 | 2426 – 2448 MHz            | 2427 – 2447 MHz         |

- Formula: $f_c = 2407 + 5 \times \text{channel}\text{ MHz}$ (`channel` $\in [1, 6]$).
- Encryption is **disabled** (`encrypt: false`) for amateur radio compliance.
- Upper-layer PDUs are responsible for station callsign identification.

---

## 2. Physical Layer (PHY) Modulation & Preamble

The over-the-air PHY depends on the `PhyRate` configured via `--phy-rate` or KISS `SetHardware (0x06)`.

### 2.1 Default Mode: `1m` (IEEE 802.11b DSSS, 1 Mbps Long Preamble)

This is the default and simplest mode to decode with an SDR.

- **Modulation:** **DBPSK** (Differential Binary Phase Shift Keying) at **1 MSym/s** (1 bit/symbol).
  - Bit `0` → 0° phase change; Bit `1` → 180° phase change.
- **Direct-Sequence Spread Spectrum (DSSS):** Each DBPSK symbol is multiplied by the 11-chip **Barker sequence**:
  ```text
  [+1, -1, +1, +1, -1, +1, +1, +1, -1, -1, -1]
  ```
  - **Chip rate:** 11 Mchip/s (11 chips per 1 µs symbol).
  - **Null-to-null RF bandwidth:** 22 MHz (recommend ≥ 22–25 MSPS complex IQ capture).
- **Scrambler:** All bits of the PLCP preamble, PLCP header, and PSDU are passed through the IEEE 802.11 7-bit self-synchronizing (multiplicative) scrambler:
  $$S(z) = 1 + z^{-4} + z^{-7}$$
  - At the receiver, descramble with the inverse: `out[n] = in[n] ^ in[n-4] ^ in[n-7]`.
- **PLCP Long Preamble (144 µs @ 1 Mbps, LSB-first):**
  1. **SYNC (128 bits):** 128 consecutive `1` bits (scrambled).
  2. **SFD (16 bits):** `0xF3A0` (transmitted LSB-first: `0,0,0,0, 0,1,0,1, 1,1,0,0, 1,1,1,1`).
- **PLCP Header (48 bits @ 1 Mbps, LSB-first):**
  1. **SIGNAL (8 bits):** `0x0A` (1 Mbps), `0x14` (2 Mbps), `0x37` (5.5 Mbps), or `0x6E` (11 Mbps).
  2. **SERVICE (8 bits):** `0x00` (for 1M/2M; bit 7 is length-extension flag for 11M CCK).
  3. **LENGTH (16 bits, little-endian):** Duration in microseconds required to transmit the MPDU (for 1 Mbps, `MPDU_bytes * 8`).
  4. **HEC (16 bits):** CRC-16-CCITT (`x^16 + x^12 + x^5 + 1`, initial `0xFFFF`, final XOR `0xFFFF`) over `SIGNAL || SERVICE || LENGTH`.

### 2.2 Other Supported PHY Modes

- **`2m` (802.11b):** Same 144-bit + 48-bit 1 Mbps DBPSK preamble/header, then **DQPSK** (1 MSym/s, 2 bits/sym) spread by the same 11-chip Barker sequence.
- **`5.5m` / `11m` (802.11b CCK):** Same 1 Mbps DBPSK preamble/header, then Complementary Code Keying (CCK) at 1.375 MSym/s (4 or 8 bits/sym).
- **`6m`–`54m` (802.11g ERP-OFDM):**
  - 20 MHz channel, 64-point FFT, 52 active subcarriers (48 data + 4 pilots at `±7, ±21`), 312.5 kHz subcarrier spacing, 4.0 µs symbol (3.2 µs IFFT + 0.8 µs cyclic prefix).
  - Preamble: 10× Short Training Symbols (8 µs) + 2× Long Training Symbols with 1.6 µs GI (8 µs) + 1× SIGNAL symbol (BPSK, rate 1/2 convolutional code `K=7, G=[133_8, 171_8]`).
  - Data symbols use BPSK (`6m, 9m`), QPSK (`12m, 18m`), 16-QAM (`24m, 36m`), or 64-QAM (`48m, 54m`) with 802.11a/g scrambler `x^7 + x^4 + 1`.
- **`mcs0`–`mcs7` (802.11n HT-OFDM):** Mixed-mode HT preamble (L-STF, L-LTF, L-SIG, HT-SIG, HT-STF, HT-LTF) + 56 active subcarriers (52 data + 4 pilots).
- **`lr-250k` / `lr-512k` (Espressif 802.11LR):** Proprietary Espressif long-range mode (not standard IEEE 802.11; decodable only by ESP32-family receivers).

---

## 3. MAC & ESP-NOW Frame Format (PSDU / MPDU)

Once the PHY PSDU bytes are decoded, the MPDU is a standard **IEEE 802.11 Management Action frame** carrying an **Espressif ESP-NOW Vendor-Specific body**, followed by a 4-byte FCS:

```text
+-------------------+-----------------------------------------------+-----------+
| 802.11 MAC Header |           ESP-NOW Action Frame Body           |    FCS    |
|    (24 bytes)     |           (15 + PDU_LEN bytes)                | (4 bytes) |
+-------------------+-----------------------------------------------+-----------+
```

### 3.1 802.11 MAC Header (24 bytes)

| Offset | Field            | Size | Value / Notes |
|--------|------------------|------|---------------|
| 0..2   | Frame Control    | 2 B  | `0xD0 0x00` (`0x00D0` LE: Version 0, Type 0 = Management, Subtype 13 = Action) |
| 2..4   | Duration / ID    | 2 B  | `0x00 0x00` (broadcast) |
| 4..10  | Address 1 (DA)   | 6 B  | `FF:FF:FF:FF:FF:FF` (Broadcast destination) |
| 10..16 | Address 2 (SA)   | 6 B  | Transmitter STA MAC address (e.g. `fc:b4:67:74:32:84`) |
| 16..22 | Address 3 (BSSID)| 6 B  | `FF:FF:FF:FF:FF:FF` (Wildcard BSSID) |
| 22..24 | Sequence Control | 2 B  | LE: bits 0–3 Fragment (`0`), bits 4–15 Sequence Number |

### 3.2 ESP-NOW Vendor-Specific Action Body (`15 + PDU_LEN` bytes)

| Offset | Field                  | Size       | Value / Notes |
|--------|------------------------|------------|---------------|
| 24     | Category Code          | 1 B        | `0x7F` (Vendor-Specific Action) |
| 25..28 | OUI                    | 3 B        | `0x18 0xFE 0x34` (Espressif Systems) |
| 28..32 | Random Value           | 4 B        | 32-bit random anti-replay value |
| 32     | Element ID             | 1 B        | `0xDD` (Vendor-Specific Information Element) |
| 33     | IE Length              | 1 B        | `5 + PDU_LEN` (bytes following this field) |
| 34..37 | IE OUI                 | 3 B        | `0x18 0xFE 0x34` (Espressif Systems) |
| 37     | Type                   | 1 B        | `0x04` (ESP-NOW) |
| 38     | Version                | 1 B        | `0x01` (ESP-NOW v1.0) |
| 39..   | **PDU Payload**        | `1..=250` B| Raw PDU bytes from KISS `DataFrame (0x00)` (unencrypted) |

### 3.3 Frame Check Sequence (4 bytes)

- **FCS:** Standard IEEE 802.11 **CRC-32** (polynomial `0x04C11DB7` / reflected `0xEDB88320`, init `0xFFFFFFFF`, final XOR `0xFFFFFFFF`, little-endian) computed over all bytes from offset `0` through the end of the PDU payload.

---

## 4. Implementing an SDR Decoder (Recommended `1m` Pipeline)

1. **IQ Capture:** Tune SDR to $f_c = 2407 + 5 \times \text{ch}\text{ MHz}$ at `>= 22 MSPS` (e.g. `22 MSPS` or `44 MSPS`).
2. **Chip Synchronization & Barker Despreading:** Correlate the complex baseband against the 11-chip Barker sequence `[+1, -1, +1, +1, -1, +1, +1, +1, -1, -1, -1]` to recover 1 MSym/s DBPSK symbols.
3. **DBPSK Demodulation:** Compute $\text{Re}(s_k \cdot s_{k-1}^*)$ for differential bit decisions (LSB-first).
4. **Descrambling:** Apply `out[n] = in[n] ^ in[n-4] ^ in[n-7]`.
5. **PLCP Sync & Header Check:** Match the 16-bit SFD (`0xF3A0`), read `SIGNAL` (`0x0A` for 1M), `SERVICE`, `LENGTH`, and verify the 16-bit HEC CRC.
6. **PSDU Extraction & FCS Check:** Read `MPDU_bytes = LENGTH / 8` (for `1m`), verify the trailing 4-byte IEEE 802.11 CRC-32.
7. **ESP-NOW PDU Filter:**
   - Check `frame[0..2] == [0xD0, 0x00]` (Management Action)
   - Check `frame[24..28] == [0x7F, 0x18, 0xFE, 0x34]`
   - Check `frame[32] == 0xDD` and `frame[34..39] == [0x18, 0xFE, 0x34, 0x04, 0x01]`
   - Extract PDU: `frame[39 .. 39 + (frame[33] - 5)]`.

---

## 5. Why Is the Broadcast MAC Peer (`FF:FF:FF:FF:FF:FF`) Required?

In Espressif's ESP-NOW stack (`esp_now_send(dst_addr, data, len)`), the driver **always** looks up `dst_addr` in its internal **peer table** before transmitting:

- If `dst_addr` is not present in the peer table, `esp_now_send()` fails with `ESP_ERR_ESPNOW_NOT_FOUND` (`12393`) — **even when `dst_addr` is the broadcast address `FF:FF:FF:FF:FF:FF`**.
- Registering `FF:FF:FF:FF:FF:FF` as a peer with `encrypt: false` and `channel: None` does **not** create a one-to-one connection; it simply provides the transmit policy (unencrypted, use current Wi-Fi channel, no LMK key) required by `esp_now_send()`.
- On the receiving side, any ESP32 listening on the same channel receives `FF:FF:FF:FF:FF:FF` broadcast frames without needing to register the transmitter's MAC address as a peer. (`esp-wifi`'s `EspNow::new()` registers `BROADCAST_ADDRESS` automatically during initialization.)
