# Testing Guidelines: HackRF TX 2-FSK

## Unit Testing
Focus on pure logic in the following modules:
- **`src/nco.rs`**: Verify the NCO's phase accumulation and quadrant accuracy for `i8` packing.
- **`src/modulator.rs`**: Ensure symbols produce the expected number of samples given a specific sample rate and baud rate. Check hex parsing for preamble/syncword.
- **`src/kiss.rs`**: Exhaustively test KISS framing, including FEND/FESC escaping and partial buffer decoding.

## Integration Testing Strategy
Since hardware access is restricted in CI/CD:
- **`src/cat.rs`**: Test frequency updates via TCP socket simulation (see `test_cat_freq_update`).
- **`src/engine.rs`**: Use a `MockDevice` (implementing `SdrDevice`) to verify that the transmission engine correctly calls hardware methods in the right sequence (enable -> set freq -> push samples -> disable).

## Mocking Hardware
New hardware implementations should follow the pattern of the `MockDevice` found in the reference `pluto-tx-2fsk` project:
- Track call counts (e.g., how many frames were "transmitted").
- Assert on the expected frequency value.
- Verify total sample counts.

## Manual Verification
1. Run the application with a dummy interface.
2. Send KISS frames via `socat`: `echo -ne "\xc0\x00Hello\xc0" | socat - TCP:127.0.0.1:8001`.
3. Monitor logs for "Transmitting frame" events with correct sample counts.
