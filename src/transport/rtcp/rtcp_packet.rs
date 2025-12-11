use crate::transport::rtcp::metrics::{ReceiverStats, SenderStats};
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq)]
pub enum RtcpPacket {
    /// SR: Sender Report (Yo informo cuánto envié)
    SenderReport(SenderStats),

    /// RR: Receiver Report (Yo informo cómo recibo lo tuyo)
    ReceiverReport(ReceiverStats),

    Goodbye,
    Hello,
    Ready,
}

impl Display for RtcpPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Self::SenderReport(s) => write!(f, "SR (Sent: {})", s.packets_sent),
            Self::ReceiverReport(r) => write!(f, "RR (Lost: {}, Jitter: {})", r.packets_lost, r.jitter),
            Self::Goodbye => write!(f, "BYE"),
            Self::Hello => write!(f, "HELLO"),
            Self::Ready => write!(f, "READY"),
        }
    }
}

impl RtcpPacket {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            Self::SenderReport(stats) => {
                buf.extend_from_slice(b"SR");
                buf.extend_from_slice(&stats.packets_sent.to_be_bytes());
                buf.extend_from_slice(&stats.bytes_sent.to_be_bytes());
            }
            Self::ReceiverReport(stats) => {
                buf.extend_from_slice(b"RR");
                buf.extend_from_slice(&stats.packets_received.to_be_bytes());
                buf.extend_from_slice(&stats.packets_lost.to_be_bytes());
                buf.extend_from_slice(&stats.jitter.to_be_bytes());
            }
            Self::Goodbye => buf.extend_from_slice(b"BYE"),
            Self::Hello => buf.extend_from_slice(b"HELLO"),
            Self::Ready => buf.extend_from_slice(b"READY"),
        }
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        // Sender Report: Header (2) + u32 (4) + u64 (8) = 14 bytes
        if data.starts_with(b"SR") && data.len() >= 14 {
            let packets_sent = u32::from_be_bytes(data[2..6].try_into().ok()?);
            let bytes_sent = u64::from_be_bytes(data[6..14].try_into().ok()?);
            return Some(Self::SenderReport(SenderStats { packets_sent, bytes_sent }));
        }

        // Receiver Report: Header (2) + u32 (4) + u32 (4) + u32 (4) = 14 bytes
        if data.starts_with(b"RR") && data.len() >= 14 {
            let packets_received = u32::from_be_bytes(data[2..6].try_into().ok()?);
            let packets_lost = u32::from_be_bytes(data[6..10].try_into().ok()?);
            let jitter = u32::from_be_bytes(data[10..14].try_into().ok()?);

            return Some(Self::ReceiverReport(ReceiverStats {
                packets_received,
                packets_lost,
                jitter,
            }));
        }

        match data {
            b"BYE" => Some(Self::Goodbye),
            b"HELLO" => Some(Self::Hello),
            b"READY" => Some(Self::Ready),
            _ => None,
        }
    }
}