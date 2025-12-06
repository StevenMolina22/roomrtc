use crate::transport::rtp::RtpPacket;
#[derive(Default)]
pub struct Slot {
    valid: bool,
    packet: Option<RtpPacket>,
}


pub struct JitterBuffer<const N: usize> {
    packets: [Slot; N],

    read_indx: usize,
    write_indx: usize,

    last_frame_completed_timestamp: u64,
    last_deliver_timestamp: u64,
}

impl<const N: usize> JitterBuffer<N>  {
    pub fn new() -> Self {
        Self {
            packets: std::array::from_fn(|_| Slot::default()),
            read_indx: 0,
            write_indx: 0,
            last_frame_completed_timestamp: 0,
            last_deliver_timestamp: 0,
        }
    }

    fn add(&mut self, packet: RtpPacket) {
        if packet.timestamp < self.last_frame_completed_timestamp {
            return
        }
        if let Some(read_idx_packet) = self.packets[self.read_indx].packet
            && packet.sequence_number <= self.packets[self.read_indx].packet - read_idx - (M - write_idx) {

        }
    }

    pub fn get_frame_packets() -> Option<Vec<RtpPacket>> {

    }

}