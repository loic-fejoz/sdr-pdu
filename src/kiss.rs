use bytes::{Buf, BytesMut};
use std::io;
use tokio_util::codec::Decoder;

const FEND: u8 = 0xC0;
const FESC: u8 = 0xDB;
const TFEND: u8 = 0xDC;
const TFESC: u8 = 0xDD;

pub struct KissDecoder;

impl Decoder for KissDecoder {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Find the first FEND
        let start = src.iter().position(|&b| b == FEND);

        if let Some(start_idx) = start {
            // Discard everything before first FEND
            src.advance(start_idx);

            // Find the NEXT FEND (end of frame)
            let end = src.iter().skip(1).position(|&b| b == FEND);

            if let Some(end_idx_relative) = end {
                let end_idx = end_idx_relative + 1;
                let frame_raw = src.split_to(end_idx + 1);

                // Process frame (skip FEND at start and end)
                let mut decoded = Vec::with_capacity(frame_raw.len());
                let mut i = 1;
                while i < frame_raw.len() - 1 {
                    let b = frame_raw[i];
                    if b == FESC {
                        i += 1;
                        if i < frame_raw.len() - 1 {
                            match frame_raw[i] {
                                TFEND => decoded.push(FEND),
                                TFESC => decoded.push(FESC),
                                _ => decoded.push(frame_raw[i]),
                            }
                        }
                    } else if b == FEND {
                        // Double FEND
                    } else {
                        decoded.push(b);
                    }
                    i += 1;
                }

                if decoded.is_empty() {
                    return self.decode(src); // Try next frame
                }

                return Ok(Some(decoded));
            }
        } else {
            // No FEND found, clear if too large to avoid OOM
            if src.len() > 8192 {
                src.clear();
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_kiss_decode_simple() {
        let mut decoder = KissDecoder;
        let mut buf = BytesMut::from(&[FEND, 0x00, 0x01, 0x02, 0x03, FEND][..]);
        let res = decoder.decode(&mut buf).unwrap();
        assert_eq!(res, Some(vec![0x00, 0x01, 0x02, 0x03]));
    }

    #[test]
    fn test_kiss_decode_escaped() {
        let mut decoder = KissDecoder;
        let mut buf = BytesMut::from(&[FEND, 0x00, 0xF0, FESC, TFEND, FESC, TFESC, 0x05, FEND][..]);
        let res = decoder.decode(&mut buf).unwrap();
        assert_eq!(res, Some(vec![0x00, 0xF0, FEND, FESC, 0x05]));
    }

    #[test]
    fn test_kiss_decode_partial() {
        let mut decoder = KissDecoder;
        let mut buf = BytesMut::from(&[FEND, 0x01, 0x02][..]);
        let res = decoder.decode(&mut buf).unwrap();
        assert_eq!(res, None);

        buf.extend_from_slice(&[0x03, FEND]);
        let res = decoder.decode(&mut buf).unwrap();
        assert_eq!(res, Some(vec![0x01, 0x02, 0x03]));
    }

    #[test]
    fn test_kiss_multiple_frames() {
        let mut decoder = KissDecoder;
        let mut buf = BytesMut::from(&[FEND, 0x01, FEND, FEND, 0x02, FEND][..]);

        let res1 = decoder.decode(&mut buf).unwrap();
        assert_eq!(res1, Some(vec![0x01]));

        let res2 = decoder.decode(&mut buf).unwrap();
        assert_eq!(res2, Some(vec![0x02]));
    }
}
