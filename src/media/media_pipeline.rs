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

    on: AtomicBool
}

impl MediaPipeline {
    pub fn new(config: &Arc<Config>) -> Self {
        Self {
            camera: Camera::new(config.media),
            config: Arc::clone(config),
            on: AtomicBool::new(false),
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

        thread::spawn({
            move || {
                let mut actual_frame = None;
                let mut chunks = Vec::new();

                loop {
                    if !self.on.load(Ordering::SeqCst) {
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

        thread::spawn(move || {
            for frame in camera_frame_rx {
                if !self.on.load(Ordering::SeqCst) {
                    break;
                }
                if let Err(_) = local_frame_tx.send(frame.clone()) {
                    break;
                }

                let encoded_frame = match encoder.encode_frame(&frame) {
                    Ok(encoded_frame) => encoded_frame,
                    Err(_) => break,
                };

                if self.send_encoded_frame(encoded_frame, &rtp_tx).is_err() {
                    break;
                }
            }
        });

        Ok(local_frame_rx)
    }

    fn send_encoded_frame(&self, encoded_frame: EncodedFrame, rtp_tx: &Sender<RtpPacket>) -> Result<(), Error> {
        if !self.on.load(Ordering::SeqCst) {
            return Err(Error::SendError(String::from("Media pipeline turned off")));
        }

        for (chunk_id, payload) in encoded_frame.chunks.iter().enumerate() {
            let marker = u16::try_from(encoded_frame.chunks.len()).map_err(|e| Error::ParsingError(e.to_string()))?;

            rtp_tx.send(RtpPacket {
                version: self.rtp_version, // En este no estoy seguro q poner
                marker,
                payload_type: 0, // En este no estoy seguro q poner
                frame_id: encoded_frame.id,
                chunk_id: chunk_id as u64,
                timestamp: Local::now().timestamp_millis() as u32,
                ssrc: self.src, // En este no estoy seguro q poner
                payload: payload.to_vec(),
            }).map_err(|e| Error::SendError(e.to_string()))?;
        }
        Ok(())
    }
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