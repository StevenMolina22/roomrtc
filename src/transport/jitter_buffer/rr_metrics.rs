use std::sync::Arc;
use crate::clock::Clock;
use crate::transport::rtp::RtpPacket;

/// Receiver Report (RR) metrics maintained by the jitter buffer.
///
/// These metrics are used to build RTCP Receiver Report blocks and include
/// packet counts, loss statistics and a simple interarrival jitter estimate.
#[derive(Debug, Clone)]
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
    pub last_rtp_timestamp: u128,

    /// Arrival time of the last received packet (measured as a Duration
    /// since the jitter buffer's `start_time`). Using `Duration` avoids
    /// introducing `Instant` into the serializable/clonable metric struct.
    pub last_arrival_time: u128,

    pub clock: Arc<Clock>
}

impl RrMetrics {
    pub(crate) fn new(clock: Arc<Clock>) -> RrMetrics {
        Self {
            max_sequence_number: None,
            packets_received: 0u32,
            packets_expected: 0u32,
            cumulative_lost: 0u32,
            interarrival_jitter: 0u32,
            last_rtp_timestamp: 0u128,
            last_arrival_time: 0u128,
            clock
        }
    }
}

impl RrMetrics {
    pub fn update_rr_metrics(&mut self, packet: &RtpPacket) {
        let seq_num = packet.sequence_number;

        self.packets_received = self.packets_received.wrapping_add(1);
        const MAX_DROPOUT: u64 = 3000;
        const SEQ_MOD: u64 = 1 << 15;

        if self.max_sequence_number.is_none() {
            self.max_sequence_number = Some(seq_num);
        } else {
            let max_seq = self.max_sequence_number.unwrap();
            let delta = seq_num.wrapping_sub(max_seq);

            if delta < SEQ_MOD {
                if delta < MAX_DROPOUT {
                    let gap = delta.wrapping_sub(1);

                    self.cumulative_lost = self.cumulative_lost.wrapping_add(gap as u32);
                    self.packets_expected = self.packets_expected.wrapping_add(delta as u32);
                    self.max_sequence_number = Some(seq_num);
                } else {
                    self.max_sequence_number = Some(seq_num);
                }
            }
        }

        let current_arrival_time = self.clock.now();

        if self.last_arrival_time != 0 && self.last_rtp_timestamp != 0 {
            let d_arrival_ms = current_arrival_time;
            let d_arrival_prev_ms = self.last_arrival_time;

            let diff_arrival = d_arrival_ms as i64 - d_arrival_prev_ms as i64;

            let diff_rtp = packet.timestamp as i64 - self.last_rtp_timestamp as i64;

            let delay_diff = diff_arrival - diff_rtp;
            let abs_delay_diff = delay_diff.unsigned_abs() as u32;

            let current_jitter = self.interarrival_jitter as f64;
            let diff = abs_delay_diff as f64;

            let new_jitter = current_jitter + ((diff - current_jitter) / 16.0);

            self.interarrival_jitter = new_jitter.max(0.0) as u32;
        }

        self.last_arrival_time = current_arrival_time;
        self.last_rtp_timestamp = packet.timestamp;
    }
}

