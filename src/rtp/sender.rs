use crate::rtp::error::RtpError;
use crate::rtp::rtp_packet::RtpPacket;
use crate::tools::Socket;

pub struct RtpSender<S: Socket + Send + Sync + 'static> {
    rtp_socket: S,
    ssrc: u32,
}

impl<S: Socket + Send + Sync + 'static> RtpSender<S> {
    pub fn new(rtp_socket: S, ssrc: u32) -> Result<Self, RtpError> {
        Ok(Self { rtp_socket, ssrc })
    }

    pub fn send(
        &mut self,
        payload: &[u8],
        payload_type: u8,
        timestamp: u32,
        frame_id: u64,
        chunk_id: u64,
        marker: u16,
    ) -> Result<(), RtpError> {
        let rtp_package = RtpPacket::new(
            marker,
            payload_type,
            payload.to_vec(),
            timestamp,
            frame_id,
            chunk_id,
            self.ssrc,
        );

        let data = rtp_package.to_bytes();

        if self.rtp_socket.send(&data).is_err() {
            // don't close connection
            return Err(RtpError::SendFailed);
        }

        Ok(())
    }

    pub fn terminate(&mut self) -> Result<(), RtpError> {
        Ok(())
    }
}
