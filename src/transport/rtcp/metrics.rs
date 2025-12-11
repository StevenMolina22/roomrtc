/// Sender statistics containing information about transmitted RTP packets.
///
/// This struct holds metrics for monitoring the quality and quantity of media
/// sent by the local endpoint, suitable for inclusion in RTCP Sender Report blocks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SenderStats {
    /// Total number of RTP packets successfully sent.
    pub packets_sent: u32,
    /// Total number of octets (bytes) sent in RTP packets, excluding headers.
    pub bytes_sent: u64,
}

/// Receiver statistics containing information about received RTP packets and quality metrics.
///
/// This struct holds metrics for monitoring the quality and quantity of media
/// received by the local endpoint, suitable for inclusion in RTCP Receiver Report blocks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReceiverStats {
    /// Total number of RTP packets successfully received from the remote peer.
    pub packets_received: u32,
    /// Cumulative number of RTP packets that were expected but not received.
    pub packets_lost: u32,
    /// Estimated interarrival jitter (in RTP timestamp units).
    /// Calculated using RFC 3550 algorithm: an exponential moving average
    /// of the absolute differences between RTP timestamp spacing and the
    /// observed packet arrival time spacing.
    pub jitter: u32,
}

/// Aggregated call statistics combining both local and remote transmission metrics.
///
/// This struct groups sender and receiver statistics from both the local endpoint
/// and the remote peer, providing a complete view of the RTP session health and
/// media quality for a single call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallStats {
    /// Local sender statistics (packets and bytes transmitted by this endpoint).
    pub local_sender: SenderStats,
    /// Local receiver statistics (packets received and jitter observed by this endpoint).
    pub local_receiver: ReceiverStats,
    /// Remote sender statistics (packets and bytes transmitted by the remote peer).
    pub remote_sender: SenderStats,
    /// Remote receiver statistics (packets received and jitter observed by the remote peer).
    pub remote_receiver: ReceiverStats,
}