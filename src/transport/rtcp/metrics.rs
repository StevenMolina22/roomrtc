#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SenderStats {
    /// Total amount of packets sent
    pub packets_sent: u32,
    /// Total amount of bytes sent
    pub bytes_sent: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReceiverStats {
    /// Total amount of valid received packets
    pub packets_received: u32,
    /// Total amount of packets lost
    pub packets_lost: u32,
    /// Calculated jitter
    pub jitter: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallStats {
    pub local_sender: SenderStats,
    pub local_receiver: ReceiverStats,
    pub remote_sender: SenderStats,
    pub remote_receiver: ReceiverStats,
}