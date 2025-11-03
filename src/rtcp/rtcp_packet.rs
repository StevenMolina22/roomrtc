use std::fmt::Display;

/// Compact representation of the RTCP packet types used by the
/// report handler in this project.
///
/// Each variant corresponds to a small signaling message exchanged
/// between peers to check connectivity or indicate session closure.
pub enum RtcpPacket {
    /// Periodic connectivity report.
    ConnectivityReport,

    /// Goodbye message indicating the peer is closing the session.
    Goodbye,
    Hello,
    Ready,
}

impl Display for RtcpPacket {
    /// Format the packet as a short, human-readable string (e.g.
    /// "CR", "BYE").
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::ConnectivityReport => write!(f, "CR"),
            Self::Goodbye => write!(f, "BYE"),
            Self::Hello => write!(f, "HELLO"),
            Self::Ready => write!(f, "READY"),
        }
    }
}

impl RtcpPacket {
    /// Return a byte representation of the packet suitable for sending
    /// over a socket.
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            Self::ConnectivityReport => b"CR",
            Self::Goodbye => b"BY",
            Self::Hello => b"HELLO",
            Self::Ready => b"READY",
        }
    }

    /// Parse a packet from the byte representation produced by
    /// `as_bytes`. Returns `Some(RtcpPacket)` if the bytes match a known
    /// variant, or `None` otherwise.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        match data {
            b"CR" => Some(Self::ConnectivityReport),
            b"BY" => Some(Self::Goodbye),
            b"HELLO" => Some(Self::Hello),
            b"READY" => Some(Self::Ready),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtcp_packet_roundtrip_all_variants() {
        //Create a list of all packet variants
        let all_packets = [
            RtcpPacket::ConnectivityReport,
            RtcpPacket::Goodbye,
            RtcpPacket::Hello,
            RtcpPacket::Ready,
        ];

        // Loop through each one and test its roundtrip
        for original_packet in all_packets {
            let bytes = original_packet.as_bytes();
            let deserialized_option = RtcpPacket::from_bytes(bytes);

            assert!(
                deserialized_option.is_some(),
                "from_bytes returned None for a valid packet: {:?}",
                original_packet
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
        // Create a byte slice that doesn't match any known packet
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
        // This is 'C' which is a prefix of 'CR', but not 'CR'
        let partial_bytes = b"C";

        let result = RtcpPacket::from_bytes(partial_bytes);

        assert!(result.is_none(), "Should return None for a partial match");
    }
}
