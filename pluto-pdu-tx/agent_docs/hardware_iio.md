# Hardware & IIO (PlutoSDR Specifics)

## IIO Device Mapping
- **TX DAC:** `cf-ad9361-dds-core-lpc` (or `cf-ad9361-lpc`)
  - Channel 0: `voltage0` (I)
  - Channel 1: `voltage1` (Q)
- **PHY Control:** `ad9361-phy` (RF Parameters)
  - Transmit Local Oscillator (TX LO): `altvoltage1` channel.
  - Receive Local Oscillator (RX LO): `altvoltage0` channel.

## Default RF Parameters
- **Sampling Rate:** 1 MSPS (Mega-samples per second).
- **Bandwidth:** 200 kHz (Analog filtering).

### Refactoring Safety & Hardware Workarounds
- **RX/TX Independence:** ALWAYS set `sampling_frequency` and `rf_bandwidth` on the **TX channel** (`voltage0` output) of the PHY device, not the device level. Writing to device-level attributes forces a global Baseband PLL (BBPLL) retune that shifts the RX frequency, interfering with concurrent applications like GQRX.
- **Surgical Updates:** Only write sample rate or bandwidth if the hardware's current value differs by more than 1%. This prevents unnecessary hardware glitches and PLL relocks during application restarts.
- **Frequency State Tracking:** `PlutoDevice` tracks `last_freq`. Redundant frequency writes are skipped to minimize hardware overhead and transient noise between frames.
- **DMA Stability:** DO NOT perform a "zero-buffer flush" (pushing zeros to DAC) during `disable_tx`. This operation has been found to occasionally hang the DMA controller on the PlutoSDR, preventing subsequent transmissions. Use `hardwaregain` (max attenuation) alone to silence the transmitter.
- **LO Power Management:** Ensure the `powerdown` attribute of the LO channel (`altvoltage1`) is set to `0` before frequency updates to guarantee the oscillator is active.
- **Buffer Persistence:** The `std::thread::sleep` in `push_samples` is mandatory to prevent `libiio` from destroying the buffer before the hardware finishes transmission.
- **DDS Disabling:** Initialization MUST disable internal DDS tones by setting both `scale` (0.0) and `raw` (0) on all `voltage` channels of the DDS device to prevent parasitic carriers.
- **Error Context:** Every IIO attribute write MUST use `map_err` to provide diagnostic context including the value being written and the attribute name.

## Cross-Compilation with Sysroot
To cross-compile for the PlutoSDR using the provided `Makefile`, you must specify the path to your PlutoSDR sysroot (which contains `libiio` and the target libc).

Example usage:
```bash
make release-pluto SYSROOT=/home/loic/pluto-0.30.sysroot
```
The `Makefile` will automatically:
1. Pass `--sysroot` to the linker via `RUSTFLAGS`.
2. Configure `pkg-config` to look into the sysroot (`PKG_CONFIG_PATH`).
3. Set `CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER` to `arm-linux-gnueabihf-gcc`.
