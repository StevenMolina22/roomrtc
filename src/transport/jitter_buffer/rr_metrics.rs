use std::time::Duration;

/// Receiver Report (RR) metrics maintained by the jitter buffer.
///
/// These metrics are used to build RTCP Receiver Report blocks and include
/// packet counts, loss statistics and a simple interarrival jitter estimate.
#[derive(Debug, Clone, Default)]
pub struct RrMetrics {
    /// Highest sequence number received.
    ///
    /// Uses `Option<u16>` so we can represent the uninitialized state
    /// (no packets seen yet) without colliding with sequence number 0.
    pub max_sequence_number: Option<u64>,

    /// Total number of packets successfully received.
    pub packets_received: u32,

    /// Total number of packets expected (used for loss calculation).
    pub packets_expected: u32,

    /// Cumulative number of packets lost.
    pub cumulative_lost: u32,

    /// Estimated interarrival jitter (unsigned integer approximation).
    pub interarrival_jitter: u32,

    /// RTP timestamp of the last received packet.
    pub last_rtp_timestamp: i64,

    /// Arrival time of the last received packet (measured as a Duration
    /// since the jitter buffer's `start_time`). Using `Duration` avoids
    /// introducing `Instant` into the serializable/clonable metric struct.
    pub last_arrival_time: Duration,
}