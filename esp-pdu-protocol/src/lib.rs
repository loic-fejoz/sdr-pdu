#![no_std]

/// KISS framing bytes (AX.25 / TNC KISS specification)
pub const FEND: u8 = 0xC0;
pub const FESC: u8 = 0xDB;
pub const TFEND: u8 = 0xDC;
pub const TFESC: u8 = 0xDD;

/// Maximum ESP-NOW payload size in bytes (no fragmentation in firmware).
pub const MAX_ESPNOW_PDU_LEN: usize = 250;

/// Maximum decoded KISS frame size (1 command byte + 250 PDU bytes, with headroom).
pub const MAX_KISS_FRAME_LEN: usize = 256;

/// French 2.4 GHz Amateur Radio band limits (2400 - 2450 MHz).
/// Wi-Fi channels 1..=6 (center 2412..=2437 MHz, 22 MHz BW -> 2401..=2448 MHz)
/// stay strictly within 2400..=2450 MHz.
pub const AMATEUR_24GHZ_MIN_HZ: u64 = 2_400_000_000;
pub const AMATEUR_24GHZ_MAX_HZ: u64 = 2_450_000_000;
pub const MIN_AMATEUR_CHANNEL: u8 = 1;
pub const MAX_AMATEUR_CHANNEL: u8 = 6;

/// Minimum and maximum ESP32 Wi-Fi TX power in quarter-dBm units (2.0 dBm ..= 20.0 dBm).
pub const MIN_TX_POWER_QDBM: u8 = 8;
pub const MAX_TX_POWER_QDBM: u8 = 80;

/// KISS command codes (lower nibble of the KISS type byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KissCommand {
    DataFrame = 0x00,
    TxDelay = 0x01,
    Persistence = 0x02,
    SlotTime = 0x03,
    TxTail = 0x04,
    FullDuplex = 0x05,
    SetHardware = 0x06,
    Return = 0x0F,
}

impl KissCommand {
    /// Decode a KISS type byte (`port 0` commands `0x00..=0x06`, or `0xFF` for `Return`).
    /// Non-zero KISS ports and `0x0F` (unless `0xFF`) are rejected.
    pub fn from_type_byte(byte: u8) -> Option<Self> {
        if byte == 0xFF {
            return Some(Self::Return);
        }
        if (byte & 0xF0) != 0 {
            return None;
        }
        match byte & 0x0F {
            0x00 => Some(Self::DataFrame),
            0x01 => Some(Self::TxDelay),
            0x02 => Some(Self::Persistence),
            0x03 => Some(Self::SlotTime),
            0x04 => Some(Self::TxTail),
            0x05 => Some(Self::FullDuplex),
            0x06 => Some(Self::SetHardware),
            _ => None,
        }
    }

    /// Alias for [`KissCommand::from_type_byte`].
    pub fn from_nibble(byte: u8) -> Option<Self> {
        Self::from_type_byte(byte)
    }
}

/// Supported ESP-NOW PHY data rates and modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PhyRate {
    Rate1ML = 0x00,
    Rate2ML = 0x01,
    Rate5_5ML = 0x02,
    Rate11ML = 0x03,
    Rate6M = 0x04,
    Rate9M = 0x05,
    Rate12M = 0x06,
    Rate18M = 0x07,
    Rate24M = 0x08,
    Rate36M = 0x09,
    Rate48M = 0x0A,
    Rate54M = 0x0B,
    RateMcs0Lgi = 0x10,
    RateMcs1Lgi = 0x11,
    RateMcs2Lgi = 0x12,
    RateMcs3Lgi = 0x13,
    RateMcs4Lgi = 0x14,
    RateMcs5Lgi = 0x15,
    RateMcs6Lgi = 0x16,
    RateMcs7Lgi = 0x17,
    RateLr512K = 0x20,
    RateLr250K = 0x21,
}

impl PhyRate {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x00 => Some(Self::Rate1ML),
            0x01 => Some(Self::Rate2ML),
            0x02 => Some(Self::Rate5_5ML),
            0x03 => Some(Self::Rate11ML),
            0x04 => Some(Self::Rate6M),
            0x05 => Some(Self::Rate9M),
            0x06 => Some(Self::Rate12M),
            0x07 => Some(Self::Rate18M),
            0x08 => Some(Self::Rate24M),
            0x09 => Some(Self::Rate36M),
            0x0A => Some(Self::Rate48M),
            0x0B => Some(Self::Rate54M),
            0x10 => Some(Self::RateMcs0Lgi),
            0x11 => Some(Self::RateMcs1Lgi),
            0x12 => Some(Self::RateMcs2Lgi),
            0x13 => Some(Self::RateMcs3Lgi),
            0x14 => Some(Self::RateMcs4Lgi),
            0x15 => Some(Self::RateMcs5Lgi),
            0x16 => Some(Self::RateMcs6Lgi),
            0x17 => Some(Self::RateMcs7Lgi),
            0x20 => Some(Self::RateLr512K),
            0x21 => Some(Self::RateLr250K),
            _ => None,
        }
    }

    pub fn is_long_range(self) -> bool {
        matches!(self, Self::RateLr512K | Self::RateLr250K)
    }
}

/// Subcommands carried inside KISS `SetHardware (0x06)` frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareCommand {
    /// Set Wi-Fi channel (1..=6 for French 2.4 GHz amateur band).
    SetChannel(u8),
    /// Set frequency in Hz (mapped to nearest channel in 1..=6).
    SetFrequencyHz(u64),
    /// Set TX power in quarter-dBm units (8..=80 => 2.0..=20.0 dBm).
    SetTxPowerQdbm(u8),
    /// Set ESP-NOW PHY data rate and mode.
    SetPhyRate(PhyRate),
    /// Set TX destination peer MAC (`FF:FF:FF:FF:FF:FF` for broadcast, or unicast peer MAC).
    SetPeerMac([u8; 6]),
    /// Query current TX destination peer MAC.
    GetPeerMac,
    /// Peer MAC response from firmware.
    PeerMac([u8; 6]),
    /// Query current firmware radio status.
    GetStatus,
    /// Status response from firmware.
    Status {
        channel: u8,
        tx_power_qdbm: u8,
        phy_rate: PhyRate,
        rx_count: u32,
        tx_count: u32,
        last_rssi: i8,
    },
}

impl HardwareCommand {
    pub const SUB_SET_CHANNEL: u8 = 0x01;
    pub const SUB_SET_FREQ_HZ: u8 = 0x02;
    pub const SUB_SET_TX_POWER: u8 = 0x03;
    pub const SUB_SET_PHY_RATE: u8 = 0x04;
    pub const SUB_GET_STATUS: u8 = 0x05;
    pub const SUB_SET_PEER_MAC: u8 = 0x07;
    pub const SUB_GET_PEER_MAC: u8 = 0x08;
    pub const SUB_STATUS_RESP: u8 = 0x85;
    pub const SUB_PEER_MAC_RESP: u8 = 0x88;

    /// Encode this hardware command (without the leading KISS `0x06` byte) into `out`.
    /// Returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        match *self {
            Self::SetChannel(ch) => {
                if out.len() < 2 {
                    return None;
                }
                out[0] = Self::SUB_SET_CHANNEL;
                out[1] = ch;
                Some(2)
            }
            Self::SetFrequencyHz(hz) => {
                if out.len() < 9 {
                    return None;
                }
                out[0] = Self::SUB_SET_FREQ_HZ;
                out[1..9].copy_from_slice(&hz.to_be_bytes());
                Some(9)
            }
            Self::SetTxPowerQdbm(qdbm) => {
                if out.len() < 2 {
                    return None;
                }
                out[0] = Self::SUB_SET_TX_POWER;
                out[1] = qdbm;
                Some(2)
            }
            Self::SetPhyRate(rate) => {
                if out.len() < 2 {
                    return None;
                }
                out[0] = Self::SUB_SET_PHY_RATE;
                out[1] = rate as u8;
                Some(2)
            }
            Self::SetPeerMac(mac) => {
                if out.len() < 7 {
                    return None;
                }
                out[0] = Self::SUB_SET_PEER_MAC;
                out[1..7].copy_from_slice(&mac);
                Some(7)
            }
            Self::GetPeerMac => {
                if out.is_empty() {
                    return None;
                }
                out[0] = Self::SUB_GET_PEER_MAC;
                Some(1)
            }
            Self::PeerMac(mac) => {
                if out.len() < 7 {
                    return None;
                }
                out[0] = Self::SUB_PEER_MAC_RESP;
                out[1..7].copy_from_slice(&mac);
                Some(7)
            }
            Self::GetStatus => {
                if out.is_empty() {
                    return None;
                }
                out[0] = Self::SUB_GET_STATUS;
                Some(1)
            }
            Self::Status {
                channel,
                tx_power_qdbm,
                phy_rate,
                rx_count,
                tx_count,
                last_rssi,
            } => {
                if out.len() < 13 {
                    return None;
                }
                out[0] = Self::SUB_STATUS_RESP;
                out[1] = channel;
                out[2] = tx_power_qdbm;
                out[3] = phy_rate as u8;
                out[4..8].copy_from_slice(&rx_count.to_be_bytes());
                out[8..12].copy_from_slice(&tx_count.to_be_bytes());
                out[12] = last_rssi as u8;
                Some(13)
            }
        }
    }

    /// Decode a hardware command from the payload following the KISS `0x06` byte.
    pub fn decode(payload: &[u8]) -> Option<Self> {
        let &sub = payload.first()?;
        match sub {
            Self::SUB_SET_CHANNEL if payload.len() >= 2 => Some(Self::SetChannel(payload[1])),
            Self::SUB_SET_FREQ_HZ if payload.len() >= 9 => {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&payload[1..9]);
                Some(Self::SetFrequencyHz(u64::from_be_bytes(bytes)))
            }
            Self::SUB_SET_TX_POWER if payload.len() >= 2 => Some(Self::SetTxPowerQdbm(payload[1])),
            Self::SUB_SET_PHY_RATE if payload.len() >= 2 => {
                PhyRate::from_u8(payload[1]).map(Self::SetPhyRate)
            }
            Self::SUB_SET_PEER_MAC if payload.len() >= 7 => {
                let mut mac = [0u8; 6];
                mac.copy_from_slice(&payload[1..7]);
                Some(Self::SetPeerMac(mac))
            }
            Self::SUB_GET_PEER_MAC => Some(Self::GetPeerMac),
            Self::SUB_PEER_MAC_RESP if payload.len() >= 7 => {
                let mut mac = [0u8; 6];
                mac.copy_from_slice(&payload[1..7]);
                Some(Self::PeerMac(mac))
            }
            Self::SUB_GET_STATUS => Some(Self::GetStatus),
            Self::SUB_STATUS_RESP if payload.len() >= 13 => {
                let channel = payload[1];
                let tx_power_qdbm = payload[2];
                let phy_rate = PhyRate::from_u8(payload[3])?;
                let rx_count = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                let tx_count =
                    u32::from_be_bytes([payload[8], payload[9], payload[10], payload[11]]);
                let last_rssi = payload[12] as i8;
                Some(Self::Status {
                    channel,
                    tx_power_qdbm,
                    phy_rate,
                    rx_count,
                    tx_count,
                    last_rssi,
                })
            }
            _ => None,
        }
    }
}

/// Convert a Wi-Fi channel (1..=6) to its center frequency in Hz.
pub const fn channel_to_freq_hz(channel: u8) -> Option<u64> {
    if channel >= MIN_AMATEUR_CHANNEL && channel <= MAX_AMATEUR_CHANNEL {
        Some(2_407_000_000 + (channel as u64) * 5_000_000)
    } else {
        None
    }
}

/// Map a frequency in Hz to the nearest legal French 2.4 GHz amateur Wi-Fi channel (1..=6).
/// Returns `None` if the frequency is outside 2400..=2450 MHz or if the resulting channel
/// would exceed channel 6 (2437 MHz center, 2448 MHz upper edge).
pub fn freq_hz_to_amateur_channel(freq_hz: u64) -> Option<u8> {
    if !(AMATEUR_24GHZ_MIN_HZ..=AMATEUR_24GHZ_MAX_HZ).contains(&freq_hz) {
        return None;
    }
    // Channel 1 = 2412 MHz, Channel 6 = 2437 MHz.
    // Reject frequencies whose 22 MHz Wi-Fi channel would spill above 2450 MHz (i.e. > 2439.5 MHz -> Ch 7).
    if freq_hz > 2_439_500_000 {
        return None;
    }
    if freq_hz <= 2_412_000_000 {
        return Some(1);
    }
    let offset_mhz = (freq_hz - 2_407_000_000 + 2_500_000) / 5_000_000;
    let ch = offset_mhz as u8;
    if (MIN_AMATEUR_CHANNEL..=MAX_AMATEUR_CHANNEL).contains(&ch) {
        Some(ch)
    } else {
        None
    }
}

/// Encode a raw KISS frame (command byte + payload) with FEND/FESC escaping into `out`.
/// Returns `None` if `out` is too small.
pub fn encode_kiss_frame(frame: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut pos = 0;
    if pos >= out.len() {
        return None;
    }
    out[pos] = FEND;
    pos += 1;

    for &b in frame {
        match b {
            FEND => {
                if pos + 2 > out.len() {
                    return None;
                }
                out[pos] = FESC;
                out[pos + 1] = TFEND;
                pos += 2;
            }
            FESC => {
                if pos + 2 > out.len() {
                    return None;
                }
                out[pos] = FESC;
                out[pos + 1] = TFESC;
                pos += 2;
            }
            other => {
                if pos + 1 > out.len() {
                    return None;
                }
                out[pos] = other;
                pos += 1;
            }
        }
    }

    if pos >= out.len() {
        return None;
    }
    out[pos] = FEND;
    pos += 1;
    Some(pos)
}

/// Incremental `no_std` KISS frame decoder for byte streams (e.g., UART).
pub struct KissStreamDecoder<const N: usize> {
    buf: [u8; N],
    len: usize,
    in_frame: bool,
    escaped: bool,
}

impl<const N: usize> Default for KissStreamDecoder<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> KissStreamDecoder<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0u8; N],
            len: 0,
            in_frame: false,
            escaped: false,
        }
    }

    /// Feed a single byte into the decoder.
    /// Returns `Some(&[u8])` when a complete non-empty KISS frame is decoded.
    pub fn push_byte(&mut self, byte: u8) -> Option<&[u8]> {
        if byte == FEND {
            if self.in_frame && self.len > 0 {
                let frame_len = self.len;
                self.len = 0;
                self.escaped = false;
                // Keep in_frame = true because a single FEND can delimit consecutive frames.
                return Some(&self.buf[..frame_len]);
            }
            self.in_frame = true;
            self.len = 0;
            self.escaped = false;
            return None;
        }

        if !self.in_frame {
            return None;
        }

        let decoded_byte = if self.escaped {
            self.escaped = false;
            match byte {
                TFEND => FEND,
                TFESC => FESC,
                other => other,
            }
        } else if byte == FESC {
            self.escaped = true;
            return None;
        } else {
            byte
        };

        if self.len < N {
            self.buf[self.len] = decoded_byte;
            self.len += 1;
        } else {
            // Frame exceeded buffer capacity; drop and wait for next FEND.
            self.in_frame = false;
            self.len = 0;
            self.escaped = false;
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_amateur_channel_mapping() {
        assert_eq!(channel_to_freq_hz(1), Some(2_412_000_000));
        assert_eq!(channel_to_freq_hz(6), Some(2_437_000_000));
        assert_eq!(channel_to_freq_hz(0), None);
        assert_eq!(channel_to_freq_hz(7), None);

        assert_eq!(freq_hz_to_amateur_channel(2_412_000_000), Some(1));
        assert_eq!(freq_hz_to_amateur_channel(2_437_000_000), Some(6));
        assert_eq!(freq_hz_to_amateur_channel(2_442_000_000), None); // Ch 7 spills > 2450 MHz
        assert_eq!(freq_hz_to_amateur_channel(144_000_000), None);
    }

    #[test]
    fn test_kiss_roundtrip_with_escapes() {
        let input = [0x00, FEND, 0x41, FESC, 0x42, TFEND, TFESC];
        let mut encoded = [0u8; 32];
        let enc_len = encode_kiss_frame(&input, &mut encoded).unwrap();

        let mut decoder = KissStreamDecoder::<64>::new();
        let mut decoded = None;
        for &b in &encoded[..enc_len] {
            if let Some(frame) = decoder.push_byte(b) {
                decoded = Some(frame.to_vec());
            }
        }
        assert_eq!(decoded.as_deref(), Some(&input[..]));
    }

    #[test]
    fn test_hardware_command_roundtrip() {
        let cmds = [
            HardwareCommand::SetChannel(3),
            HardwareCommand::SetFrequencyHz(2_422_000_000),
            HardwareCommand::SetTxPowerQdbm(60),
            HardwareCommand::SetPhyRate(PhyRate::RateLr512K),
            HardwareCommand::SetPeerMac([0x48, 0x27, 0xE2, 0xE4, 0x74, 0x2C]),
            HardwareCommand::GetPeerMac,
            HardwareCommand::PeerMac([0x48, 0x27, 0xE2, 0xE4, 0x74, 0x2C]),
            HardwareCommand::GetStatus,
            HardwareCommand::Status {
                channel: 3,
                tx_power_qdbm: 60,
                phy_rate: PhyRate::RateLr512K,
                rx_count: 12,
                tx_count: 34,
                last_rssi: -55,
            },
        ];

        let mut buf = [0u8; 32];
        for cmd in cmds {
            let n = cmd.encode(&mut buf).unwrap();
            let decoded = HardwareCommand::decode(&buf[..n]).unwrap();
            assert_eq!(decoded, cmd);
        }
    }

    #[test]
    fn test_kiss_command_type_byte() {
        assert_eq!(
            KissCommand::from_type_byte(0x00),
            Some(KissCommand::DataFrame)
        );
        assert_eq!(
            KissCommand::from_type_byte(0x01),
            Some(KissCommand::TxDelay)
        );
        assert_eq!(
            KissCommand::from_type_byte(0x02),
            Some(KissCommand::Persistence)
        );
        assert_eq!(
            KissCommand::from_type_byte(0x03),
            Some(KissCommand::SlotTime)
        );
        assert_eq!(KissCommand::from_type_byte(0x04), Some(KissCommand::TxTail));
        assert_eq!(
            KissCommand::from_type_byte(0x05),
            Some(KissCommand::FullDuplex)
        );
        assert_eq!(
            KissCommand::from_type_byte(0x06),
            Some(KissCommand::SetHardware)
        );
        assert_eq!(KissCommand::from_type_byte(0xFF), Some(KissCommand::Return));
        assert_eq!(KissCommand::from_type_byte(0x0F), None);
        assert_eq!(KissCommand::from_type_byte(0x10), None);
        assert_eq!(KissCommand::from_type_byte(0x26), None);
    }
}
