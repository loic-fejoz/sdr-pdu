use std::fs::File;
use std::io::Write;

fn main() -> anyhow::Result<()> {
    let sample_rate = 1_000_000u32;
    let baud_rate = 50_000u32;

    // Test payload: standard packet bytes
    let payload = b"Hello GNURadio!".to_vec();

    let out_dir = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            if std::path::Path::new("docker/gnuradio/scripts").exists() {
                std::path::PathBuf::from("docker/gnuradio/scripts")
            } else {
                std::path::PathBuf::from("../docker/gnuradio/scripts")
            }
        });

    std::fs::create_dir_all(&out_dir)?;

    // 1. Modulate QPSK
    let mut qpsk_mod = sdr_pdu_utils::qpsk::QpskModulator::new(sample_rate, baud_rate, false);
    let qpsk_iq_i16 = qpsk_mod.modulate_i16(&payload);

    // Save as raw complex float32 (interleaved f32 I, Q) for GNU Radio
    let mut qpsk_file = File::create(out_dir.join("qpsk_signal.fc32"))?;
    for &sample in &qpsk_iq_i16 {
        let f = (sample as f32) / 32767.0;
        qpsk_file.write_all(&f.to_le_bytes())?;
    }

    // 2. Modulate OQPSK
    let mut oqpsk_mod = sdr_pdu_utils::qpsk::QpskModulator::new(sample_rate, baud_rate, true);
    let oqpsk_iq_i16 = oqpsk_mod.modulate_i16(&payload);

    let mut oqpsk_file = File::create(out_dir.join("oqpsk_signal.fc32"))?;
    for &sample in &oqpsk_iq_i16 {
        let f = (sample as f32) / 32767.0;
        oqpsk_file.write_all(&f.to_le_bytes())?;
    }

    // 3. Save expected payload
    let mut payload_file = File::create(out_dir.join("expected_payload.bin"))?;
    payload_file.write_all(&payload)?;

    println!(
        "Generated test signals: QPSK ({} samples), OQPSK ({} samples)",
        qpsk_iq_i16.len() / 2,
        oqpsk_iq_i16.len() / 2
    );
    Ok(())
}
