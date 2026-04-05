use bytes::{Buf, BufMut, BytesMut};
use std::io;
use tokio_util::codec::{Decoder, Encoder};

const FEND: u8 = 0xC0;
const FESC: u8 = 0xDB;
const TFEND: u8 = 0xDC;
const TFESC: u8 = 0xDD;

pub struct KissDecoder;

impl Decoder for KissDecoder {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let start = src.iter().position(|&b| b == FEND);

        if let Some(start_idx) = start {
            src.advance(start_idx);
            let end = src.iter().skip(1).position(|&b| b == FEND);

            if let Some(end_idx_relative) = end {
                let end_idx = end_idx_relative + 1;
                let frame_raw = src.split_to(end_idx + 1);

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
                        // Double FEND skip
                    } else {
                        decoded.push(b);
                    }
                    i += 1;
                }

                if decoded.is_empty() {
                    return self.decode(src);
                }

                return Ok(Some(decoded));
            }
        } else {
            if src.len() > 8192 {
                src.clear();
            }
        }

        Ok(None)
    }
}

pub struct KissEncoder;

impl Encoder<Vec<u8>> for KissEncoder {
    type Error = io::Error;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.reserve(item.len() * 2 + 2);
        dst.put_u8(FEND);
        for byte in item {
            match byte {
                FEND => {
                    dst.put_u8(FESC);
                    dst.put_u8(TFEND);
                }
                FESC => {
                    dst.put_u8(FESC);
                    dst.put_u8(TFESC);
                }
                b => dst.put_u8(b),
            }
        }
        dst.put_u8(FEND);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_kiss_decoder_no_panic(data: Vec<u8>) -> bool {
        let mut decoder = KissDecoder;
        let mut buf = BytesMut::from(&data[..]);
        let _ = decoder.decode(&mut buf);
        true
    }

    #[quickcheck]
    fn prop_kiss_roundtrip(data: Vec<u8>) -> bool {
        if data.is_empty() {
            return true;
        }

        let mut encoder = KissEncoder;
        let mut decoder = KissDecoder;
        let mut buf = BytesMut::new();

        if encoder.encode(data.clone(), &mut buf).is_err() {
            return false;
        }

        match decoder.decode(&mut buf) {
            Ok(Some(decoded)) => decoded == data,
            _ => false,
        }
    }

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

    #[test]
    fn test_kiss_encode_simple() {
        let mut encoder = KissEncoder;
        let mut buf = BytesMut::new();
        encoder
            .encode(vec![0x00, 0x01, 0x02, 0x03], &mut buf)
            .unwrap();
        assert_eq!(buf, &[FEND, 0x00, 0x01, 0x02, 0x03, FEND][..]);
    }

    #[test]
    fn test_kiss_encode_escaped() {
        let mut encoder = KissEncoder;
        let mut buf = BytesMut::new();
        encoder
            .encode(vec![0x00, FEND, FESC, 0x05], &mut buf)
            .unwrap();
        assert_eq!(buf, &[FEND, 0x00, FESC, TFEND, FESC, TFESC, 0x05, FEND][..]);
    }

    #[test]
    fn test_kiss_roundtrip() {
        let mut encoder = KissEncoder;
        let mut decoder = KissDecoder;
        let mut buf = BytesMut::new();
        let original = vec![0x00, 0xF0, FEND, FESC, 0x05];

        encoder.encode(original.clone(), &mut buf).unwrap();
        let decoded = decoder.decode(&mut buf).unwrap().unwrap();

        assert_eq!(decoded, original);
    }
}
