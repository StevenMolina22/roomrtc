use crate::clock::Clock;
use crate::transport::rtcp::ReceiverStats;
use crate::transport::rtp::RtpPacket;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};
use crate::transport::rtp::RtpPacket;
use crate::clock::Clock;
use crate::transport::rtcp::ReceiverStats;
use crate::logger::Logger;

const TOLERANCE_MILLIS: u128 = 120;

/// A ring buffer for managing out-of-order and delayed RTP packets.
///
/// This jitter buffer stores incoming RTP packets and delivers complete video frames
/// in playout order. It handles out-of-order arrival, packet loss, and enforces playout
/// deadlines to maintain real-time streaming quality.
///
/// # Overview
/// - Uses a fixed-size circular buffer with generic capacity `N`.
/// - Maintains separate read and write pointers for thread-safe operation.
/// - Tracks sequence numbers to detect gaps and reordering.
/// - Requires an I-frame before accepting any P-frames (video codec requirement).
/// - Implements tolerance-based playout timing to balance latency and resilience.
/// - Collects receiver statistics (packet loss, jitter) for RTCP reporting.
///
/// # Generic Parameters
/// - `N`: the maximum number of RTP packets that can be buffered.
pub struct JitterBuffer<const N: usize> {
    /// Array of buffered RTP packets; `None` indicates an empty slot.
    packets: [Option<RtpPacket>; N],

    /// Current read position in the circular buffer.
    read_idx: usize,
    /// Current write position in the circular buffer.
    write_idx: usize,

    /// Sequence number of the packet at the read position (tracks read progress).
    read_seq: Option<u64>,
    /// Sequence number of the packet at the write position (tracks write progress).
    write_seq: Option<u64>,

    /// Flag indicating whether an I-frame is required before accepting P-frames.
    i_frame_needed: bool,

    /// RTP timestamp of the last frame that was successfully delivered to the decoder.
    last_frame_completed_timestamp: u128,
    /// Local time of the last frame delivered, used for playout timing synchronization.
    last_deliver_timestamp: u128,

    /// Shared reference to the system clock for timing calculations.
    clock: Arc<Clock>,

    /// Shared receiver statistics (packet counts, loss, jitter) for RTCP reporting.
    metrics: Arc<Mutex<ReceiverStats>>,
    /// Previous transit time (used in jitter calculation).
    last_transit: Option<i64>,
    /// Time of the last received packet (for timeout detection).
    last_arrival: Option<Instant>,
    /// Highest sequence number seen so far (for gap detection).
    max_seq_seen: u64,
    /// Logger instance.
    logger: Logger,
}

impl<const N: usize> JitterBuffer<N> {
    /// Create a new `JitterBuffer` with the specified capacity and shared metrics.
    ///
    /// Initializes an empty circular buffer in the initial state, requiring an I-frame
    /// before any packets are accepted. The buffer uses wrapping arithmetic for sequence
    /// numbers to detect packet loss and reordering.
    ///
    /// # Parameters
    /// - `clock`: shared reference to the system clock for playout timing.
    /// - `metrics`: shared receiver statistics structure for tracking packet loss and jitter.
    /// - `logger`: logger instance.
    ///
    /// # Returns
    /// A new `JitterBuffer` ready to accept incoming RTP packets.
    pub fn new(clock: Arc<Clock>, metrics: Arc<Mutex<ReceiverStats>>, logger: Logger) -> Self {
        Self {
            packets: std::array::from_fn(|_| None),
            read_idx: 0,
            write_idx: 0,
            read_seq: None,
            write_seq: None,
            i_frame_needed: true,
            last_frame_completed_timestamp: 0,
            last_deliver_timestamp: 0,
            clock,
            metrics,
            last_transit: None,
            last_arrival: None,
            max_seq_seen: 0,
            logger,
        }
    }

    /// Add an RTP packet to the jitter buffer.
    ///
    /// This method inserts a packet into the circular buffer if it passes validation checks.
    /// Packets are rejected if:
    /// - An I-frame is required but this is a P-frame (delta frame).
    /// - The packet timestamp is older than the last delivered frame.
    /// - The sequence number is outside the valid acceptance window.
    ///
    /// If the packet is accepted, it is stored at the position determined by its sequence
    /// number modulo the buffer capacity. Out-of-order arrival is handled transparently.
    ///
    /// # Parameters
    /// - `packet`: the RTP packet to add to the buffer.
    pub(crate) fn add(&mut self, packet: RtpPacket) {
        self.update_stats(&packet);

        let seq = packet.sequence_number;
        if self.i_frame_needed && !packet.is_i_frame {
            return;
        }
        if packet.timestamp < self.last_frame_completed_timestamp
        {
            self.logger.warn(&format!("Discarding old packet: timestamp {} < last completed {}", packet.timestamp, self.last_frame_completed_timestamp));
            return
        }

        if !self.valid_packet_seq_num(seq) {
            return;
        }

        let pos = (seq % N as u64) as usize;

        match (self.read_seq, self.write_seq) {
            (Some(read), Some(write)) => {
                if seq < read {
                    self.read_idx = pos;
                    self.read_seq = Some(seq);
                    self.packets[pos] = Some(packet);
                } else if seq > write {
                    let old_write_idx = self.write_idx;
                    self.write_idx = pos;
                    self.write_seq = Some(seq);
                    self.packets[pos] = Some(packet);

                    if (self.read_idx <= old_write_idx
                        && pos >= self.read_idx
                        && pos <= old_write_idx)
                        || (self.read_idx > old_write_idx
                            && (pos >= self.read_idx || pos <= old_write_idx))
                    {
                        self.resync_or_clear();
                    }
                } else if seq > read && seq < write {
                    self.packets[pos] = Some(packet);
                }
            }
            _ => {
                self.read_seq = Some(seq);
                self.write_seq = Some(seq);
                self.read_idx = pos;
                self.write_idx = pos;
                self.packets[pos] = Some(packet);
                self.i_frame_needed = false;
            }
        }
    }

    /// Extract the next complete frame from the buffer for decoding.
    ///
    /// This method attempts to retrieve a complete video frame (all chunks) from the buffer.
    /// If a complete frame is ready and within the playout deadline, it is removed from the
    /// buffer and its payload data is returned. The method also applies playout delay
    /// synchronization to maintain consistent frame delivery timing.
    ///
    /// # Returns
    /// - `Some(data)`: the concatenated payload of a complete frame, ready for decoding.
    /// - `None`: if no complete frame is available or if all frames have expired their
    ///   playout deadline (triggering buffer resynchronization).
    ///
    /// # Playout Timing
    /// The buffer enforces a tolerance window (`TOLERANCE_MILLIS`) to balance latency and
    /// resilience. If frames arrive late (beyond the tolerance), the buffer resyncs and
    /// requires a new I-frame.
    pub(crate) fn pop(&mut self) -> Option<Vec<u8>> {
        let mut ts;
        loop {
            ts = match &self.packets[self.read_idx] {
                Some(p) => p.timestamp,
                None => return None,
            };

            if self.valid_playout_time(ts) {
                break;
            }

            self.resync_or_clear();
            if self.write_seq.is_none() || self.read_seq.is_none() {
                return None;
            }
        }

        let mut idx = self.read_idx;
        let mut frame_data = Vec::new();
        let mut chunks_processed = 0;

        for _ in 0..N {
            let packet = self.packets[idx].clone()?;

            if packet.timestamp != ts {
                return None;
            }

            frame_data.extend_from_slice(&packet.payload);
            chunks_processed += 1;

            idx = (idx + 1) % N;

            if packet.marker == 1 {
                if chunks_processed == packet.total_chunks as usize {
                    while self.read_idx != idx {
                        self.packets[self.read_idx] = None;
                        self.read_idx = (self.read_idx + 1) % N;
                    }

                    let mut found_next = false;
                    for _ in 0..N {
                        if let Some(next_p) = &self.packets[self.read_idx] {
                            self.read_seq = Some(next_p.sequence_number);
                            found_next = true;
                            break;
                        }
                        self.read_idx = (self.read_idx + 1) % N;
                    }

                    if !found_next {
                        self.read_seq = None;
                        self.write_seq = None;
                    }

                    if self.last_deliver_timestamp != 0 {
                        let delta_rtp = packet
                            .timestamp
                            .saturating_sub(self.last_frame_completed_timestamp);
                        let expected_playout_time_local = self.last_deliver_timestamp + delta_rtp;
                        let now = self.clock.now();
                        let sleep_time = expected_playout_time_local.saturating_sub(now);

                        if sleep_time > 0 {
                            sleep(Duration::from_millis(sleep_time as u64));
                        }

                        self.last_deliver_timestamp = expected_playout_time_local;
                    } else {
                        self.last_deliver_timestamp = self.clock.now();
                    }

                    self.last_frame_completed_timestamp = packet.timestamp;
                    if packet.is_i_frame {
                        self.i_frame_needed = false
                    }

                    return Some(frame_data);
                }
                return None;
            }
        }
        None
    }

    /// Resynchronize the buffer to the next I-frame or clear all buffered data.
    ///
    /// This method is called when the playout deadline has been exceeded or when
    /// buffer overflow is detected. It searches forward for the next I-frame starting
    /// from the read position. If found, the read pointer jumps to that I-frame and
    /// the buffer continues. Otherwise, the entire buffer is cleared and reset to
    /// the initial state (requiring a new I-frame).
    ///
    /// # Behavior
    /// - If an I-frame is found: move to it and continue streaming.
    /// - If no I-frame is found: clear all packets and reset read/write pointers.
    fn resync_or_clear(&mut self) {
        self.logger.warn("JitterBuffer resync triggered: playout deadline exceeded or buffer overflow.");
        // NO ESTOY CONSIDERANDO EL CASO DE QUE WRITE ESCRIBA DESPUES DEL READ, CONSIDERAR DESPUES
        let read_timestamp = self.packets[self.read_idx].as_ref().unwrap().timestamp;

        for _ in 0..N {
            if let Some(pkt) = &self.packets[self.read_idx]
                && pkt.is_i_frame
                && pkt.timestamp != read_timestamp
            {
                self.read_seq = Some(pkt.sequence_number);
                self.i_frame_needed = false;
                return;
            }
            self.packets[self.read_idx] = None;

            if self.read_idx == self.write_idx {
                break;
            }

            self.read_idx = (self.read_idx + 1) % N;
        }

        self.read_idx = 0;
        self.write_idx = 0;
        self.read_seq = None;
        self.write_seq = None;
        self.i_frame_needed = true;
    }

    /// Check if a packet's sequence number is within the valid acceptance window.
    ///
    /// This method validates whether a packet should be accepted based on its sequence
    /// number relative to the buffer's read and write positions. It prevents acceptance
    /// of packets that are too far in the past or otherwise outside the valid range.
    ///
    /// # Parameters
    /// - `seq_num`: the RTP sequence number to validate.
    ///
    /// # Returns
    /// `true` if the sequence number is within the acceptable range, `false` otherwise.
    fn valid_packet_seq_num(&self, seq_num: u64) -> bool {
        match &self.packets[self.read_idx] {
            Some(read_packet) => {
                let window = (self.read_idx + N - self.write_idx) as u64;
                if window > seq_num {
                    return true;
                }

                let bound = read_packet.sequence_number.wrapping_sub(window);
                seq_num > bound
            }
            None => true,
        }
    }

    /// Check if a frame is within its playout deadline.
    ///
    /// This method determines whether a frame at the read position has arrived in time
    /// to be played out. It uses the expected playout time (derived from the previous frame's
    /// timing and the RTP timestamp delta) and compares it to the current clock time.
    /// A tolerance window (`TOLERANCE_MILLIS`) is applied to balance latency and resilience.
    ///
    /// # Parameters
    /// - `frame_timestamp`: the RTP timestamp of the frame to check.
    ///
    /// # Returns
    /// `true` if the frame is within the playout deadline, `false` if it has expired.
    fn valid_playout_time(&self, frame_timestamp: u128) -> bool {
        if self.last_deliver_timestamp == 0 {
            return true;
        }

        let delta_rtp = frame_timestamp - self.last_frame_completed_timestamp;
        let expected_playout_time_local = self.last_deliver_timestamp + delta_rtp;
        let expiration_deadline = expected_playout_time_local + TOLERANCE_MILLIS;
        let actual = self.clock.now();

        expiration_deadline >= actual
    }

    /// Update receiver statistics based on a newly received packet.
    ///
    /// This method updates the shared receiver metrics with information about the
    /// incoming packet, including packet count, loss detection, and jitter estimation.
    /// The jitter is calculated using an exponential moving average of the transit time
    /// (arrival time minus RTP timestamp) variations, as specified in RFC 3550.
    ///
    /// # Parameters
    /// - `packet`: the RTP packet whose metrics should be recorded.
    fn update_stats(&mut self, packet: &RtpPacket) {
        let now = Instant::now();
        let arrival_time_ms = self.clock.now();

        let mut metrics = match self.metrics.lock() {
            Ok(m) => m,
            Err(_) => return,
        };

        metrics.packets_received += 1;

        if metrics.packets_received == 1 {
            self.max_seq_seen = packet.sequence_number;
        } else if packet.sequence_number > self.max_seq_seen {
            let gap = packet.sequence_number - self.max_seq_seen - 1;

            if gap > 0 && gap < 1000 {
                metrics.packets_lost += gap as u32;
            }
            self.max_seq_seen = packet.sequence_number;
        } else if metrics.packets_lost > 0 {
            metrics.packets_lost -= 1;
        }

        let transit = arrival_time_ms as i64 - (packet.timestamp as i64);
        if let Some(last_transit) = self.last_transit {
            let d = (transit - last_transit).abs();
            let prev_jitter = metrics.jitter as f64;
            let new_jitter = prev_jitter + ((d as f64 - prev_jitter) / 16.0);
            metrics.jitter = new_jitter as u32;
        }
        self.last_transit = Some(transit);
        self.last_arrival = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn create_packet(
        seq: u64,
        ts: u128,
        is_i_frame: bool,
        marker: u8,
        chunks: u8,
        payload: Vec<u8>,
    ) -> RtpPacket {
        RtpPacket {
            version: 2,
            marker,
            total_chunks: chunks,
            is_i_frame,
            payload_type: 96,
            sequence_number: seq,
            timestamp: ts,
            ssrc: 12345,
            payload,
        }
    }

    fn setup_buffer<const N: usize>() -> JitterBuffer<N> {
        let clock = Arc::new(Clock::new());
        let metrics = Arc::new(Mutex::new(ReceiverStats::default()));
        JitterBuffer::new(clock, metrics)
    }

    #[test]
    fn test_initial_iframe_requirement() {
        let mut buffer = setup_buffer::<10>();

        let p_frame = create_packet(1, 1000, false, 1, 1, vec![0x01]);
        buffer.add(p_frame);

        assert!(buffer.pop().is_none());

        let i_frame = create_packet(2, 2000, true, 1, 1, vec![0x02]);
        buffer.add(i_frame);

        let result = buffer.pop();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), vec![0x02]);
    }

    #[test]
    fn test_ordered_frame_assembly() {
        let mut buffer = setup_buffer::<10>();

        buffer.add(create_packet(1, 1000, true, 1, 1, vec![0xFF]));
        buffer.pop();

        buffer.add(create_packet(2, 3000, false, 0, 2, vec![0xAA]));
        buffer.add(create_packet(3, 3000, false, 1, 2, vec![0xBB]));

        let result = buffer.pop();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), vec![0xAA, 0xBB]);
    }

    #[test]
    fn test_out_of_order_assembly() {
        let mut buffer = setup_buffer::<10>();

        buffer.add(create_packet(1, 1000, true, 1, 1, vec![0xFF]));
        buffer.pop();

        buffer.add(create_packet(3, 3000, false, 1, 2, vec![0xBB]));
        buffer.add(create_packet(2, 3000, false, 0, 2, vec![0xAA]));

        let result = buffer.pop();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), vec![0xAA, 0xBB]);
    }

    #[test]
    fn test_incomplete_frame_returns_none() {
        let mut buffer = setup_buffer::<10>();

        buffer.add(create_packet(1, 1000, true, 1, 1, vec![0xFF]));
        buffer.pop();

        buffer.add(create_packet(2, 3000, false, 0, 3, vec![0xAA]));
        buffer.add(create_packet(4, 3000, false, 1, 3, vec![0xCC]));

        assert!(buffer.pop().is_none());
    }

    #[test]
    fn test_packet_loss_metrics() {
        let clock = Arc::new(Clock::new());
        let metrics = Arc::new(Mutex::new(ReceiverStats::default()));
        let mut buffer = JitterBuffer::<10>::new(clock, metrics.clone());

        buffer.add(create_packet(10, 1000, true, 1, 1, vec![0xA]));

        buffer.add(create_packet(12, 1066, false, 1, 1, vec![0xC]));

        {
            let m = metrics.lock().unwrap();
            assert_eq!(m.packets_lost, 1);
        }

        buffer.add(create_packet(11, 1033, false, 1, 1, vec![0xB]));

        {
            let m = metrics.lock().unwrap();
            assert_eq!(m.packets_lost, 0);
        }
    }

    #[test]
    fn test_buffer_overwrite_logic() {
        let mut buffer = setup_buffer::<4>();

        buffer.add(create_packet(0, 1000, true, 1, 1, vec![0x01]));
        buffer.add(create_packet(1, 1033, false, 1, 1, vec![0x02]));
        buffer.add(create_packet(2, 1066, false, 1, 1, vec![0x03]));
        buffer.add(create_packet(3, 1100, false, 1, 1, vec![0x04]));

        assert_eq!(buffer.read_idx, 0);

        buffer.add(create_packet(4, 1133, false, 1, 1, vec![0x05]));

        assert_eq!(buffer.read_idx, 0);
        assert!(buffer.packets[1].is_some());
    }

    #[test]
    fn test_ignore_old_packets() {
        let mut buffer = setup_buffer::<10>();

        buffer.add(create_packet(10, 1000, true, 1, 1, vec![0xFF]));
        buffer.pop();

        let old_packet = create_packet(5, 500, false, 1, 1, vec![0xEE]);
        buffer.add(old_packet);

        assert!(buffer.pop().is_none());
    }
}
