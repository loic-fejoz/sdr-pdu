# PlutoSDR & SDR-PDU Gateway - Future Enhancements

This document tracks planned features, completed milestones, and optimizations to move the gateway from a functional prototype to a production-grade SDR system.

## High Priority: Signal Integrity & Standards

- [x] **G3RUH Scrambling:** 
  - *Rationale:* Essential for 9600 baud packet radio. Prevents DC offset issues by breaking up long sequences of identical bits using the polynomial $1 + x^{12} + x^{17}$.
  - *Module:* Implemented in `sdr-pdu-utils::scrambler` and supported via `--g3ruh` in `pluto-pdu-tx` and `--scramble` in `hackrf-pdu-tx`.

- [x] **Dynamic TX Attenuation:**
  - *Rationale:* Allow users to control output power to match link budgets and avoid interference.
  - *Implementation:* Added `--attenuation` CLI flag, dynamic attenuation updates in `PlutoDevice`, and `rigctld` `\set_level ATT` / `L ATT` commands in `CatServer`.
  - *Module:* `src/pluto.rs`, `sdr-pdu-utils/src/cat.rs`

- [x] **QPSK & OQPSK Modulations:**
  - *Rationale:* Modern digital satellite up-links and packet modes require phase-shift keying with spectral efficiency.
  - *Implementation:* Added `sdr-pdu-utils::qpsk::QpskModulator` supporting standard QPSK and half-symbol staggered OQPSK, with CLI flags `--qpsk` and `--oqpsk`.
  - *Module:* `sdr-pdu-utils/src/qpsk.rs`, `pluto-pdu-tx/src/modulator.rs`, `hackrf-pdu-tx/src/modulator.rs`

- [x] **Demodulation Verification & GNU Radio in Docker:**
  - *Rationale:* Ensure transmitted signals conform to digital receiver standards under AWGN channel impairments and cross-validate against reference SDR software.
  - *Implementation:* Implemented `sdr-pdu-utils::demod` with AWGN channel simulator, plus headless GNU Radio in Docker pipeline using `digital.constellation_decoder_cb`.
  - *Module:* `sdr-pdu-utils/src/demod.rs`, `docker/gnuradio/`

- [x] **Root Raised Cosine (RRC) Pulse Shaping:**
  - *Rationale:* Eliminate spectral splatter and adjacent channel interference caused by rectangular NRZ pulses in QPSK/OQPSK.
  - *Implementation:* Implemented `sdr-pdu-utils::rrc::RrcFilter` with configurable roll-off factor ($\alpha$), FIR convolution / pulse shaping, unit energy normalization, matched filtering in demodulator, and CLI flags `--rrc` / `--rrc-alpha` in `pluto-pdu-tx` and `hackrf-pdu-tx`.
  - *Module:* `sdr-pdu-utils/src/rrc.rs`, `sdr-pdu-utils/src/qpsk.rs`, `sdr-pdu-utils/src/demod.rs`

- [ ] **TXDELAY, TXTAIL & Flag Preamble:**
  - *Rationale:* Provides time for the receiving station's AGC and clock recovery to stabilize.
  - *Implementation:* Handle KISS commands `TXDELAY` (`0x01`) and `TXTAIL` (`0x04`), injecting a configurable number of AX.25 flags (`0x7E`) or preamble bytes before and after payload transmission.
  - *Module:* `src/engine.rs`, `sdr-pdu-utils/src/kiss.rs`

- [ ] **RF Power Ramping:**
  - *Rationale:* Smoothly ramp TX gain up and down at frame boundaries to prevent key clicks and spectral transients.
  - *Module:* `src/pluto.rs`, `src/engine.rs`

## Medium Priority: Reception (RX) & Spectral Efficiency

- [ ] **Full-Duplex / Half-Duplex Reception Chain (RX):**
  - *Rationale:* Satellite QSOs and bidirectional packet links require receiving downlink telemetry/frames while transmitting uplink frames.
  - *Implementation:* Add RX DMA streaming, preamble/syncword correlation detector, and push decoded frames to KISS TCP / WebSocket clients.
  - *Module:* `src/pluto.rs` (RX channel `voltage0`), `sdr-pdu-utils/src/demod.rs`

- [ ] **Gaussian Pulse Shaping (GFSK):**
  - *Rationale:* Replaces sharp frequency transitions in 2FSK with smooth Gaussian curves, significantly reducing bandwidth occupied and adjacent channel interference.
  - *Module:* `src/modulator.rs` / `src/dsp/gaussian.rs`

- [ ] **RF Watchdog & Hardware Silence:**
  - *Rationale:* Ensure the Pluto does not transmit a parasitic carrier if the application crashes or the queue is idle.
  - *Implementation:* Zero-fill DMA buffers and toggle the TX chain attenuation via `ad9361-phy`.
  - *Module:* `src/pluto.rs`

## Low Priority: Deployment & Dev-Ops

- [ ] **Systemd Integration:**
  - *Rationale:* Ensure the gateway starts automatically on PlutoSDR boot.
  - *Implementation:* Create a `pluto-gateway.service` unit file.

- [ ] **Automated Deployment (`make deploy`):**
  - *Rationale:* Speed up the development and release cycle.
  - *Implementation:* Add a `make deploy` target to the `Makefile` using `scp` and remote `ssh` / `systemctl` commands.
