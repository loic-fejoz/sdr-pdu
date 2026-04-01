# Hardware & IIO (PlutoSDR Specifics)

## IIO Device Mapping
- **TX DAC:** `cf-ad9361-lpc` (Voltage Output)
  - Channel 0: `voltage0` (I)
  - Channel 1: `voltage1` (Q)
- **PHY Control:** `ad9361-phy` (RF Parameters)
  - Local Oscillator (LO): `altvoltage1` channel.
  - Frequency Attribute: `frequency` (u64, Hz).

## Default RF Parameters
- **Sampling Rate:** 1 MSPS (Mega-samples per second).
- **Bandwidth:** 200 kHz (Analog filtering).
- **Local Oscillator (LO):** VHF Uplink band (e.g., 144.XXX MHz).

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

### Refactoring Safety
- **Buffer Persistence:** The `std::thread::sleep` in `push_samples` is mandatory to prevent `libiio` from destroying the buffer before the hardware finishes transmission. DO NOT remove it without implementing an async completion callback.
- **DDS Disabling:** Initialization MUST disable internal DDS tones to prevent parasitic carriers.
- **Error Context:** Every IIO attribute write MUST use `map_err` to provide diagnostic context including the value being written and the attribute name.
