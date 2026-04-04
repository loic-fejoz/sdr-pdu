# Software Defined Radio (SDR) - Protocol Data Unit (PDU)

A set of utility tools to ease sending/receiving frames over SDR radios like PlutoSDR, HackRF, or even MMDVM hat.

## Context

In the context of operating hamradio satellites, the needs has come to have a simple setup to send/receive frame.

Note that I am using the term PDU instead of packet/frame so as to not confuse with packet radio (AX.25 based). The PDU is whatever soft bytes one would like to transmit over the air, incl. AX.25 but not exclusively.

## Tools

- `hackrf-pdu-tx`: a TCP KISS and CAT server to send 2FSK frames over an HackRF
- `pluto-pdu-tx`: a TCP KISS and CAT server to send 2FSK frames over an HackRF
- `sdr-pdu-utils`: a Rust crate for shared code

## License

MIT or Apache 2.0.
