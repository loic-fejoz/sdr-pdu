pub fn parse_hex_byte(s: &str) -> anyhow::Result<u8> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    if s.len() > 2 {
        anyhow::bail!("Invalid hex byte: {}", s);
    }
    u8::from_str_radix(s, 16).map_err(|e| anyhow::anyhow!("Hex parse error: {}", e))
}

pub fn parse_hex_bytes(s: &str) -> anyhow::Result<Vec<u8>> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    if s.is_empty() {
        return Ok(Vec::new());
    }
    if !s.len().is_multiple_of(2) {
        let padded = format!("0{}", s);
        parse_hex_bytes_even(&padded)
    } else {
        parse_hex_bytes_even(s)
    }
}

pub fn parse_hex_bytes_even(s: &str) -> anyhow::Result<Vec<u8>> {
    let mut res = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        let byte = u8::from_str_radix(&s[i..i + 2], 16)
            .map_err(|e| anyhow::anyhow!("Hex parse error at {}: {}", &s[i..i + 2], e))?;
        res.push(byte);
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_parsing() {
        assert_eq!(parse_hex_byte("0x55").unwrap(), 0x55);
        assert_eq!(parse_hex_byte("55").unwrap(), 0x55);
        assert_eq!(
            parse_hex_bytes("0x1ACFFC1D").unwrap(),
            vec![0x1A, 0xCF, 0xFC, 0x1D]
        );
        assert_eq!(parse_hex_bytes("7E").unwrap(), vec![0x7E]);
        assert_eq!(parse_hex_bytes("7").unwrap(), vec![0x07]);
    }
}
