use crate::config::Config;
use crate::media::camera::{Camera, FrameSource};
use crate::media::error::MediaPipelineError as Error;
use crate::media::frame_handler::{Decoder, EncodedFrame, Encoder, Frame};
use crate::transport::rtp::RtpPacket;
use crate::transport::jitter_buffer::JitterBuffer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, mpsc};
use std::thread;
use chrono::Local;
use crate::controller::AppEvent;

const JITTER_BUFF_SIZE: usize = 4096;

pub struct MediaPipeline {
    camera: Camera,
    config: Arc<Config>,
    ssrc: u32,
}

impl MediaPipeline {
    #[must_use]
    pub fn new(config: &Arc<Config>, ssrc: u32) -> Self {
        Self {
            camera: Camera::new(config),
            config: Arc::clone(config),
            ssrc,
        }
    }

    pub fn start(
        &mut self,
        rtp_tx: Sender<RtpPacket>,
        rtp_rx: Receiver<RtpPacket>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
    ) -> Result<(Receiver<Frame>, Receiver<Frame>), Error> {
        let local_frame_rx = self.start_local_frame_pipeline(rtp_tx, event_tx.clone(), connected.clone())?;
        let remote_frame_rx = self.start_remote_frames_pipeline(rtp_rx, event_tx.clone(), connected.clone())?;

        Ok((local_frame_rx, remote_frame_rx))
    }

    pub fn stop(&self) -> Result<(), Error> {
        self.camera.stop().map_err(|e| Error::MapError(e.to_string()))
    }

    fn start_remote_frames_pipeline(
        &self,
        rtp_rx: Receiver<RtpPacket>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
    ) -> Result<Receiver<Frame>, Error> {
        let (remote_frame_tx, remote_frame_rx) = mpsc::channel();

        let mut jitter_buffer = JitterBuffer::<JITTER_BUFF_SIZE>::new();

        thread::spawn({
            move || {
                let mut decoder = match Decoder::new()
                    .map_err(|e| Error::MapError(e.to_string())) {
                    Ok(d) => d,
                    Err(_) => {
                        send_message_to_ui(event_tx, AppEvent::Error("Failed to create decoder".into()));
                        return;
                    }
                };


                loop {
                    if !connected.load(Ordering::SeqCst) {
                        break;
                    }

                    match rtp_rx.recv() {
                        Ok(packet) => jitter_buffer.add(packet),
                        Err(_) => break,
                    }

                    if let Some(chunks) = jitter_buffer.pop()
                        && let Some(frame_data) = generate_frame_from_chunks(&chunks, &mut decoder)
                        && let Err(_) = remote_frame_tx.send(frame_data.clone())
                    {
                        break;
                    }
                }
                if connected.load(Ordering::SeqCst) {
                    connected.store(false, Ordering::SeqCst);
                    send_message_to_ui(event_tx.clone(), AppEvent::CallEnded)
                };
            }
        });
        Ok(remote_frame_rx)
    }

    fn start_local_frame_pipeline(
        &mut self,
        rtp_tx: Sender<RtpPacket>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
    ) -> Result<Receiver<Frame>, Error> {
        let (local_frame_tx, local_frame_rx) = mpsc::channel();

        let camera_frame_rx = self
            .camera
            .start()
            .map_err(|e| Error::MapError(e.to_string()))?;
        let mut encoder = match Encoder::new(&self.config.media) {
            Ok(d) => d,
            Err(e) => {
                send_message_to_ui(event_tx.clone(), AppEvent::Error(e.to_string()));
                return Err(Error::MapError(e.to_string()));
            }
        };

        let ssrc = self.ssrc;
        let config = Arc::clone(&self.config);
        thread::spawn(move || {
            for frame in camera_frame_rx {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                if local_frame_tx.send(frame.clone()).is_err() {
                    break;
                }

                let encoded_frame = match encoder.encode_frame(&frame) {
                    Ok(f) => f,
                    Err(_) => break,
                };

                if send_encoded_frame(encoded_frame, &rtp_tx, ssrc, connected.clone(), &config).is_err() {
                    break;
                }
            }

            if connected.load(Ordering::SeqCst) {
                connected.store(false, Ordering::SeqCst);
                send_message_to_ui(event_tx.clone(), AppEvent::CallEnded);
            }
        });

        Ok(local_frame_rx)
    }
}
fn send_encoded_frame(
    encoded_frame: EncodedFrame,
    rtp_tx: &Sender<RtpPacket>,
    ssrc: u32,
    connected: Arc<AtomicBool>,
    config: &Arc<Config>,
) -> Result<(), Error> {
    if !connected.load(Ordering::SeqCst) {
        return Err(Error::SendError("Media pipeline turned off".into()));
    }

    let total_chunks = encoded_frame.chunks.len();
    for (sequence_number, payload) in encoded_frame.chunks.iter().enumerate() {
        let packet = RtpPacket {
            version: config.media.rtp_version,
            marker: 0,
            total_chunks: total_chunks as u8,
            is_i_frame: encoded_frame.is_i_frame,
            sequence_number: sequence_number as u64,
            payload_type: config.media.rtp_payload_type,
            timestamp: encoded_frame.frame_time,
            ssrc,
            payload: payload.clone(),
        };
        rtp_tx
            .send(packet)
            .map_err(|e| Error::SendError(e.to_string()))?;
    }
    Ok(())
}

fn generate_frame_from_chunks(data: &Vec<u8>, decoder: &mut Decoder) -> Option<Frame> {
    let (decoded_data, width, height) = match decoder.decode_frame(&data) {
        Ok(data) => data,
        Err(_) => return None,
    };

    Some(Frame {
        data: decoded_data,
        width,
        height,
        frame_time: Local::now().timestamp_millis(),
    })
}

fn send_message_to_ui(event_tx: Sender<AppEvent>, event: AppEvent) {
    if let Err(e) = event_tx.send(event) {
        eprintln!("Error sending event: {:?}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn test_generate_frame_from_chunks_h264() {
        // ------- Setup -------
        let config = Arc::new(Config::load(Path::new("./room_rtc.conf")).unwrap());
        let mut encoder = Encoder::new(&config.media).expect("Encoder no pudo inicializarse");
        let mut decoder = Decoder::new().expect("Decoder no pudo inicializarse");

        // Creamos un frame crudo sintético RGB 320x240
        let width = 320;
        let height = 240;
        let raw_data = vec![128u8; width * height * 3];

        let raw_frame = Frame {
            data: raw_data.clone(),
            width,
            height,
            frame_time: Local::now().timestamp_millis(),
        };

        // ------- Encode -------
        let encoded = encoder
            .encode_frame(&raw_frame)
            .expect("Fallo encodear el frame");

        assert!(!encoded.chunks.is_empty(), "El encoder debe generar chunks");

        let mut rtp_chunks = Vec::new();
        let total_chunks = encoded.chunks.len() as u8;

        for (i, chunk) in encoded.chunks.iter().enumerate() {
            rtp_chunks.push(RtpPacket {
                version: config.media.rtp_version,
                marker: 0,
                total_chunks,
                is_i_frame: encoded.is_i_frame,
                payload_type: config.media.rtp_payload_type,
                sequence_number: i as u64,
                timestamp: raw_frame.frame_time,
                ssrc: 55,
                payload: chunk.clone(),
            });
        }

        rtp_chunks.sort_by_key(|c| c.sequence_number);
        let mut data = Vec::new();
        for c in rtp_chunks.iter() {
            data.extend_from_slice(&c.payload);
        }

        let decoded = generate_frame_from_chunks(&data, &mut decoder)
            .expect("generate_frame_from_chunks no devolvió frame");
        assert_eq!(decoded, raw_frame);
    }
}
