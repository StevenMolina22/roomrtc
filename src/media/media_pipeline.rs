use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use chrono::Local;
use crate::config::Config;
use crate::transport::rtp::RtpPacket;
use crate::media::Camera;
use crate::media::camera::FrameSource;
use crate::media::error::MediaPipelineError as Error;
use crate::media::frame_handler::{Decoder, EncodedFrame, Encoder, Frame};

pub struct MediaPipeline {
    camera: Camera,
    config: Arc<Config>,
    src: u32,
    on: Arc<AtomicBool>
}

impl MediaPipeline {
    pub fn new(config: &Arc<Config>, src: u32) -> Self {
        Self {
            camera: Camera::new(&config),
            config: Arc::clone(config),
            src,
            on: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&mut self, rtp_tx: Sender<RtpPacket>, rtp_rx: Receiver<RtpPacket>) -> Result<(Receiver<Frame>, Receiver<Frame>), Error> {
        let local_frame_rx = self.start_local_frame_pipeline(rtp_tx)?;
        let remote_frame_rx = self.start_remote_frames_pipeline(rtp_rx)?;

        self.on.store(true, Ordering::SeqCst);
        Ok((local_frame_rx, remote_frame_rx))
    }

    pub fn stop(&self) {
        self.on.store(false, Ordering::SeqCst)
    }

    fn start_remote_frames_pipeline(&self, rtp_rx: Receiver<RtpPacket>) -> Result<Receiver<Frame>, Error> {
        let (remote_frame_tx, remote_frame_rx) = mpsc::channel();

        let mut decoder = Decoder::new().map_err(|e| Error::MapError(e.to_string()))?;
        let on = Arc::clone(&self.on);
        
        thread::spawn({
            move || {
                let mut actual_frame = None;
                let mut chunks = Vec::new();

                loop {
                    if !on.load(Ordering::SeqCst) {
                        break;
                    }

                    let rtp_packet = match rtp_rx.recv() {
                        Ok(packet) => packet,
                        Err(_) => break,
                    };

                    if actual_frame != Some(rtp_packet.frame_id) {
                        chunks = vec![rtp_packet.clone()];
                        actual_frame = Some(rtp_packet.frame_id);
                    } else {
                        chunks.push(rtp_packet.clone());
                    }

                    let expected_marker = rtp_packet.marker;
                    let current_chunk_count = match u16::try_from(chunks.len()) {
                        Ok(count) => count,
                        Err(_) => return,
                    };

                    if current_chunk_count == expected_marker {
                        if let Some(frame_data) = generate_frame_from_chunks(&mut chunks, &mut decoder)
                            && let Err(_) = remote_frame_tx.send(frame_data.clone()) {
                            break;
                        }

                        actual_frame = None;
                        chunks.clear();
                    }
                }
            }
        });
        Ok(remote_frame_rx)
    }

    fn start_local_frame_pipeline(&mut self, rtp_tx: Sender<RtpPacket>) -> Result<Receiver<Frame>, Error> {
        let (local_frame_tx, local_frame_rx) = mpsc::channel();

        let camera_frame_rx = self.camera.start().map_err(|e| Error::MapError(e.to_string()))?;
        let mut encoder = Encoder::new(&self.config.media).map_err(|e| Error::MapError(e.to_string()))?;

        let on = Arc::clone(&self.on);
        let src = self.src;
        let config = Arc::clone(&self.config);
        
        thread::spawn(move || {
            for frame in camera_frame_rx {
                if !on.load(Ordering::SeqCst) {
                    break;
                }

                if local_frame_tx.send(frame.clone()).is_err() {
                    break;
                }

                let encoded_frame = match encoder.encode_frame(&frame) {
                    Ok(f) => f,
                    Err(_) => break,
                };

                if send_encoded_frame(encoded_frame, &rtp_tx, src, &on, &config).is_err() {
                    break;
                }
            }
        });

        Ok(local_frame_rx)
    }

}
fn send_encoded_frame(
    encoded_frame: EncodedFrame,
    rtp_tx: &Sender<RtpPacket>,
    src: u32,
    on: &Arc<AtomicBool>,
    config: &Arc<Config>,
) -> Result<(), Error> {
    if !on.load(Ordering::SeqCst) {
        return Err(Error::SendError("Media pipeline turned off".into()));
    }

    for (chunk_id, payload) in encoded_frame.chunks.iter().enumerate() {
        let marker = encoded_frame.chunks.len() as u16;

        rtp_tx.send(RtpPacket {
            version: config.media.rtp_version,
            marker,
            payload_type: config.media.rtp_payload_type,
            frame_id: encoded_frame.id,
            chunk_id: chunk_id as u64,
            timestamp: Local::now().timestamp_millis() as u32,
            ssrc: src,
            payload: payload.clone(),
        })
        .map_err(|e| Error::SendError(e.to_string()))?;
    }
    Ok(())
}

fn generate_frame_from_chunks(chunks: &mut Vec<RtpPacket>, decoder: &mut Decoder) -> Option<Frame> {
    let fr_id = chunks.first()?.frame_id;

    chunks.sort_by_key(|c| c.chunk_id);
    let mut data = Vec::new();
    for c in chunks.iter() {
        data.extend_from_slice(&c.payload);
    }
    let (decoded_data, width, height) = decoder.decode_frame(&data).ok()?;

    Some(Frame {
        data: decoded_data,
        width,
        height,
        id: fr_id,
    })
}