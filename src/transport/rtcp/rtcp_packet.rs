use std::fmt::Display;

/// Compact representation of the RTCP packet types used by the
/// report handler in this project.
///
/// Each variant corresponds to a small signaling message exchanged
/// between peers to check connectivity or indicate session closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcpPacket {
    /// Periodic connectivity report.
    ConnectivityReport(u32),

    /// Goodbye message indicating the peer is closing the session.
    Goodbye(u32),

    ///Hello message for starting handshake
    Hello(u32),

    ///Ready message to let the other peer know you are ready to join call
    Ready(u32),
}

impl Display for RtcpPacket {
    /// Format the packet as a short, human-readable string (e.g.
    /// "CR", "BYE").
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::ConnectivityReport(_) => write!(f, "CR"),
            Self::Goodbye(_) => write!(f, "BYE"),
            Self::Hello(_) => write!(f, "HELLO"),
            Self::Ready(_) => write!(f, "READY"),
        }
    }
}

impl RtcpPacket {
    /// Return a standard 8-byte RTCP header.
    /// Byte 0: Version (2) + Padding (0) + RC (0) = 10000000 = 0x80
    /// Byte 1: Packet Type (PT)
    /// Bytes 2-3: Length (in 32-bit words - 1). 8 bytes = 2 words. Length = 1.
    /// Bytes 4-7: SSRC
    #[must_use] pub fn to_bytes(&self) -> Vec<u8> {
        // We map our custom types to arbitrary PT numbers or keep using the ASCII bytes
        // 'C'=67, 'B'=66, 'H'=72, 'R'=82 are all valid byte values for PT.
        let (pt, ssrc) = match self {
            Self::ConnectivityReport(s) => (b'C', s),
            Self::Goodbye(s) => (b'B', s),
            Self::Hello(s) => (b'H', s),
            Self::Ready(s) => (b'R', s),
        };

        let mut buf = Vec::with_capacity(8);
        buf.push(0x80); // Byte 0: V=2
        buf.push(pt); // Byte 1: PT
        buf.extend_from_slice(&1u16.to_be_bytes()); // Byte 2-3: Length = 1
        buf.extend_from_slice(&ssrc.to_be_bytes()); // Byte 4-7: SSRC
        buf
    }

    /// Parse a packet from the byte representation produced by
    /// `as_bytes`. Returns `Some(RtcpPacket)` if the bytes match a known
    /// variant, or `None` otherwise.
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }

        // Version must be 2 (top 2 bits of byte 0)
        if (data[0] >> 6) != 2 {
            return None;
        }

        let pt = data[1];
        let ssrc_bytes: [u8; 4] = data[4..8].try_into().ok()?;
        let ssrc = u32::from_be_bytes(ssrc_bytes);

        match pt {
            b'C' => Some(Self::ConnectivityReport(ssrc)),
            b'B' => Some(Self::Goodbye(ssrc)),
            b'H' => Some(Self::Hello(ssrc)),
            b'R' => Some(Self::Ready(ssrc)),
            _ => None,
        }
    }

    /// Helper to extract just the SSRC from raw bytes without fully
    /// parsing the packet type. Useful for SRTCP context lookup.
    #[must_use] pub fn ssrc_from_bytes(data: &[u8]) -> Option<u32> {
        if data.len() < 8 {
            return None;
        }
        let ssrc_bytes: [u8; 4] = data[4..8].try_into().ok()?;
        Some(u32::from_be_bytes(ssrc_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtcp_packet_roundtrip_all_variants() {
        let ssrc = 0x1234_5678;

        let all_packets = [
            RtcpPacket::ConnectivityReport(ssrc),
            RtcpPacket::Goodbye(ssrc),
            RtcpPacket::Hello(ssrc),
            RtcpPacket::Ready(ssrc),
        ];

        for original_packet in all_packets {
            let bytes = original_packet.to_bytes();

            // Validate the raw byte structure (Tag + Big Endian SSRC)
            assert_eq!(bytes.len(), 8, "Serialized packet must be 8 bytes");
            assert_eq!(bytes[0], 0x80, "Invalid RTCP Version byte");

            // Verify SSRC is encoded in Big Endian at bytes 4..8
            let expected_ssrc_bytes = ssrc.to_be_bytes();
            assert_eq!(&bytes[4..8], &expected_ssrc_bytes, "SSRC encoding mismatch");

            let deserialized_option = RtcpPacket::from_bytes(&bytes);

            assert!(
                deserialized_option.is_some(),
                "from_bytes returned None for a valid packet: {original_packet:?}"
            );

            assert_eq!(
                original_packet,
                deserialized_option.unwrap(),
                "Packet mismatch after roundtrip"
            );
        }
    }

    #[test]
    fn test_from_bytes_invalid_data() {
        let invalid_bytes = b"INVALID PACKET";

        let result = RtcpPacket::from_bytes(invalid_bytes);

        assert!(result.is_none(), "Should return None for invalid data");
    }

    #[test]
    fn test_from_bytes_empty_slice() {
        let empty_bytes: &[u8] = &[];

        let result = RtcpPacket::from_bytes(empty_bytes);

        assert!(result.is_none(), "Should return None for an empty slice");
    }

    #[test]
    fn test_from_bytes_partial_match() {
        let partial_bytes = b"C";

        let result = RtcpPacket::from_bytes(partial_bytes);

        assert!(result.is_none(), "Should return None for a partial match");
    }
}
