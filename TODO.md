# PlutoSDR 2FSK Gateway - Future Enhancements

This document tracks planned features and optimizations to move the gateway from a functional prototype to a production-grade SDR system.

## High Priority: Signal Integrity & Standards

- [ ] **G3RUH Scrambling:** 
  - *Rationale:* Essential for 9600 baud packet radio. Prevents DC offset issues by breaking up long sequences of identical bits using the polynomial $1 + x^{12} + x^{17}$.
  - *Module:* `src/modulator.rs`

- [ ] **Dynamic TX Attenuation:**
  - *Rationale:* Allow users to control output power to match link budgets and avoid interference.
  - *Implementation:* Add `--gain` CLI argument and update `cf-ad9361-lpc` hardware attributes.
  - *Module:* `src/pluto.rs`

- [ ] **TXDELAY & Flag Preamble:**
  - *Rationale:* Provides time for the receiving station's AGC and clock recovery to stabilize. 
  - *Implementation:* Inject a configurable number of AX.25 flags (`0x7E`) or zeros before every KISS frame transmission.
  - *Module:* `src/engine.rs`

## Medium Priority: Spectral Efficiency & Safety

- [ ] **Gaussian Pulse Shaping (GFSK):**
  - *Rationale:* Replaces sharp frequency transitions with smooth Gaussian curves, significantly reducing bandwidth occupied and adjacent channel interference.
  - *Module:* `src/modulator.rs` / `src/dsp/gaussian.rs`

- [ ] **RF Watchdog & Hardware Silence:**
  - *Rationale:* Ensure the Pluto does not transmit a parasitic carrier if the application crashes or the queue is idle.
  - *Implementation:* Zero-fill DMA buffers and potentially toggle the TX chain via `ad9361-phy`.
  - *Module:* `src/pluto.rs`

## Low Priority: Deployment & Dev-Ops

- [ ] **Systemd Integration:**
  - *Rationale:* Ensure the gateway starts automatically on PlutoSDR boot.
  - *Implementation:* Create a `pluto-gateway.service` unit file.

- [ ] **Automated Deployment:**
  - *Rationale:* Speed up the development cycle.
  - *Implementation:* Add a `make deploy` target to the `Makefile` using `scp` and remote `systemctl` commands.
