#!/usr/bin/env python3
import sys
import numpy as np
from gnuradio import gr, blocks, digital, analog

def test_qpsk_demodulation():
    print("=== GNU Radio QPSK Demodulation Test ===")
    
    # 1. Load generated QPSK signal
    samples = np.fromfile("/workspace/scripts/qpsk_signal.fc32", dtype=np.complex64)
    expected_payload = open("/workspace/scripts/expected_payload.bin", "rb").read()
    
    sample_rate = 1_000_000
    baud_rate = 50_000
    sps = sample_rate / baud_rate  # 20 samples per symbol
    
    print(f"Loaded {len(samples)} samples. Samples per symbol: {sps}")
    print(f"Expected payload ({len(expected_payload)} bytes): {expected_payload}")
    
    # 2. Define the matching QPSK constellation object in GNU Radio
    # Symbol mapping matching Rust QPSK Gray code:
    # 00 -> ( 1 + 1j)
    # 01 -> ( 1 - 1j)
    # 10 -> (-1 + 1j)
    # 11 -> (-1 - 1j)
    points = [complex(1, 1), complex(1, -1), complex(-1, 1), complex(-1, -1)]
    constellation = digital.constellation_calcdist(points, [0, 1, 2, 3], 4, 1).base()
    
    # 3. Build GNU Radio Demodulation Flowgraph
    tb = gr.top_block()
    
    src = blocks.vector_source_c(samples.tolist(), False)
    
    # Channel: Add AWGN noise channel using GNU Radio analog.noise_source_c
    noise = analog.noise_source_c(analog.GR_GAUSSIAN, 0.05, 42)
    adder = blocks.add_cc()
    
    # Symbol timing synchronization: sample at the center of each symbol period
    # delay by (sps/2) to align to the symbol center
    delay = blocks.delay(gr.sizeof_gr_complex, int(sps // 2))
    decim = blocks.keep_one_in_n(gr.sizeof_gr_complex, int(sps))
    
    # GNU Radio Demodulation Block: digital.constellation_decoder_cb
    demod = digital.constellation_decoder_cb(constellation)
    
    # Bit repacker: unpack 2-bit symbol indices into bytes (MSB first)
    repack = blocks.repack_bits_bb(2, 8, '', False, gr.GR_MSB_FIRST)
    
    sink = blocks.vector_sink_b()
    
    tb.connect(src, (adder, 0))
    tb.connect(noise, (adder, 1))
    tb.connect(adder, delay, decim, demod, repack, sink)
    
    tb.run()
    
    recovered_bytes = bytes(sink.data())
    print(f"GNU Radio Demodulated ({len(recovered_bytes)} bytes): {recovered_bytes}")
    assert recovered_bytes == expected_payload, f"Expected {expected_payload}, got {recovered_bytes}"
    print(">>> QPSK GNU Radio Demodulation (digital.constellation_decoder_cb) SUCCESS! <<<")

def test_oqpsk_demodulation():
    print("\n=== GNU Radio OQPSK Demodulation Test ===")
    
    samples = np.fromfile("/workspace/scripts/oqpsk_signal.fc32", dtype=np.complex64)
    expected_payload = open("/workspace/scripts/expected_payload.bin", "rb").read()
    
    sample_rate = 1_000_000
    baud_rate = 50_000
    sps = sample_rate / baud_rate  # 20 samples per symbol
    q_delay = int(round(sps * 0.5))  # 10 samples delay
    
    print(f"Loaded {len(samples)} OQPSK samples. Samples per symbol: {sps}, Q delay: {q_delay}")
    
    # 1. Flowgraph to pass through AWGN channel
    tb_channel = gr.top_block()
    src = blocks.vector_source_c(samples.tolist(), False)
    noise = analog.noise_source_c(analog.GR_GAUSSIAN, 0.05, 123)
    adder = blocks.add_cc()
    sink_c = blocks.vector_sink_c()
    
    tb_channel.connect(src, (adder, 0))
    tb_channel.connect(noise, (adder, 1))
    tb_channel.connect(adder, sink_c)
    tb_channel.run()
    
    noisy_samples = np.array(sink_c.data())
    
    # 2. De-stagger OQPSK (sample I and Q with T_s/2 offset)
    num_symbols = int((len(samples) - q_delay) / sps)
    aligned_symbols = []
    for sym in range(num_symbols):
        idx_i = int((sym + 0.5) * sps)
        idx_q = idx_i + q_delay
        aligned_symbols.append(complex(noisy_samples[idx_i].real, noisy_samples[idx_q].imag))
        
    # 3. Demodulate aligned symbols through GNU Radio constellation_decoder_cb
    points = [complex(1, 1), complex(1, -1), complex(-1, 1), complex(-1, -1)]
    constellation = digital.constellation_calcdist(points, [0, 1, 2, 3], 4, 1).base()
    
    tb_demod = gr.top_block()
    src_syms = blocks.vector_source_c(aligned_symbols, False)
    demod = digital.constellation_decoder_cb(constellation)
    repack = blocks.repack_bits_bb(2, 8, '', False, gr.GR_MSB_FIRST)
    sink_b = blocks.vector_sink_b()
    
    tb_demod.connect(src_syms, demod, repack, sink_b)
    tb_demod.run()
    
    recovered_bytes = bytes(sink_b.data())
    print(f"GNU Radio Demodulated ({len(recovered_bytes)} bytes): {recovered_bytes}")
    assert recovered_bytes == expected_payload, f"Expected {expected_payload}, got {recovered_bytes}"
    print(">>> OQPSK GNU Radio Demodulation (digital.constellation_decoder_cb) SUCCESS! <<<")

def test_gnuradio_full_roundtrip():
    print("\n=== GNU Radio Full Mod/Demod Cross-Verification ===")
    tb = gr.top_block()
    
    constellation_points = [
        complex(1.0, 1.0) / np.sqrt(2),
        complex(1.0, -1.0) / np.sqrt(2),
        complex(-1.0, 1.0) / np.sqrt(2),
        complex(-1.0, -1.0) / np.sqrt(2),
    ]
    
    src = blocks.vector_source_b([0, 1, 2, 3], False)
    mod = digital.chunks_to_symbols_bc(constellation_points, 1)
    
    constellation = digital.constellation_calcdist(constellation_points, [0, 1, 2, 3], 4, 1).base()
    demod = digital.constellation_decoder_cb(constellation)
    sink = blocks.vector_sink_b()
    
    tb.connect(src, mod, demod, sink)
    tb.run()
    
    decoded_symbols = sink.data()
    print(f"GNU Radio Modulator -> Demodulator roundtrip symbols: {decoded_symbols}")
    assert list(decoded_symbols) == [0, 1, 2, 3]
    print(">>> Constellation Cross-Verification SUCCESS! <<<")

if __name__ == "__main__":
    test_qpsk_demodulation()
    test_oqpsk_demodulation()
    test_gnuradio_full_roundtrip()
