# DSP & Math Implementation

## NCO (Numerically Controlled Oscillator)
- **Phase Representation:** 32-bit unsigned integer (`u32`).
- **Phase Wrapping:** Native 32-bit integer overflow maps to $2\pi$ wrap-around.
- **LUT (Lookup Table):** Pre-computed `i16` Sin/Cos values.
- **Phase Shift:** Indexing into LUT requires bit-shifting the 32-bit phase based on the table size (e.g., `32 - lut_size_bits`).

## FSK Modulation
- **Baseband FSK:** Modulates relative to DC (Center Frequency).
- **Symbol Timing:** Accumulate fractional samples to maintain phase continuity and baud rate precision.
- **Frequency Deviation:** $\pm \Delta f$ converted to phase increment (`phase_inc`).

## SIMD / NEON Optimization (PlutoSDR Cortex-A9)
- **Strategy:** Process 4 or 8 samples in parallel using 128-bit NEON registers (`q0-q15`).
- **Autovectorization:** Structure loops to allow `rustc` to emit NEON instructions (use `target-cpu=cortex-a9`).
- **Vectorized Phase:** Accumulate phase across a lane of 4 `u32` values.

### Refactoring Safety
- **Intrinsics/Portable SIMD:** Any changes to the NCO loop (`fill_buffer`) must be verified with `objdump` to ensure NEON instructions (`vadd.i32`, `vld1`, `vst1`) are still being generated.
- **Comments:** Keep comments explaining phase wrapping and LUT packing; they are critical for maintaining the fixed-point math integrity.
