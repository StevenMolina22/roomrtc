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
            RtcpPacket::ConnectivityReport => write!(f, "CR"),
            RtcpPacket::Goodbye => write!(f, "BYE"),
            RtcpPacket::Hello => write!(f, "HELLO"),
            RtcpPacket::Ready => write!(f, "READY"),
        }
    }
}

impl RtcpPacket {
    /// Return a byte representation of the packet suitable for sending
    /// over a socket.
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            RtcpPacket::ConnectivityReport => b"CR",
            RtcpPacket::Goodbye => b"BY",
            RtcpPacket::Hello => b"HELLO",
            RtcpPacket::Ready => b"READY",
        }
    }

    /// Parse a packet from the byte representation produced by
    /// `as_bytes`. Returns `Some(RtcpPacket)` if the bytes match a known
    /// variant, or `None` otherwise.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        match data {
            b"CR" => Some(RtcpPacket::ConnectivityReport),
            b"BY" => Some(RtcpPacket::Goodbye),
            b"HELLO" => Some(RtcpPacket::Hello),
            b"READY" => Some(RtcpPacket::Ready),
            _ => None,
        }
    }
}
