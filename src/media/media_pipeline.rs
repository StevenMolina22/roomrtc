use crate::config::Config;
use crate::logger::Logger;
use crate::media::camera::{Camera, FrameSource};
use crate::media::error::MediaPipelineError as Error;
use crate::media::frame_handler::{Decoder, EncodedFrame, Encoder, Frame};
use crate::transport::{rtp::RtpPacket, jitter_buffer::JitterBuffer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, mpsc, Mutex};
use std::thread;
use crate::controller::AppEvent;
use crate::clock::Clock;
use crate::transport::rtcp::ReceiverStats;

/// Maximum number of RTP packets buffered for jitter compensation.
const JITTER_BUFF_SIZE: usize = 1024;

/// Coordinates camera capture, encoding/decoding, and RTP transport.
///
/// `MediaPipeline` starts the local camera, encodes outgoing frames to RTP,
/// and decodes incoming RTP into displayable frames. It also dispatches UI
/// events for errors and call termination.
pub struct MediaPipeline {
    camera: Camera,
    config: Arc<Config>,
    ssrc: u32,
    logger: Logger,
    clock: Arc<Clock>
}

impl MediaPipeline {
    /// Creates a new media pipeline with its own clock and camera.
    #[must_use]
    pub fn new(config: &Arc<Config>, ssrc: u32, logger: Logger) -> Self {
        let clock = Arc::new(Clock::new());
        Self {
            camera: Camera::new(clock.clone(), config),
            config: Arc::clone(config),
            ssrc,
            logger,
            clock
        }
    }

    /// Starts local capture/encoding and remote decoding pipelines.
    ///
    /// Spawns threads for sending local camera frames as RTP and for receiving
    /// rendering remote frames. Returns receivers for local (preview) and
    /// remote frames.
    pub fn start(
        &mut self,
        rtp_tx: Sender<RtpPacket>,
        rtp_rx: Receiver<RtpPacket>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
        receiver_metrics: Arc<Mutex<ReceiverStats>>
    ) -> Result<(Receiver<Frame>, Receiver<Frame>), Error> {
        let local_frame_rx = self.start_local_frame_pipeline(rtp_tx, event_tx.clone(), connected.clone())?;
        let remote_frame_rx = self.start_remote_frame_pipeline(rtp_rx, event_tx.clone(), connected.clone(), receiver_metrics)?;

        Ok((local_frame_rx, remote_frame_rx))
    }

    /// Stops camera capture and related media components.
    pub fn stop(&self) {
        self.logger.info("Stopping MediaPipeline...");
        self.camera.stop();
    }

    // Builds the remote receive pipeline: jitter buffer + decoder.
    fn start_remote_frame_pipeline(
        &self,
        rtp_rx: Receiver<RtpPacket>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
        receiver_metrics: Arc<Mutex<ReceiverStats>>
    ) -> Result<Receiver<Frame>, Error> {
        let (remote_frame_tx, remote_frame_rx) = mpsc::channel();
        let logger = self.logger.clone();

        let mut jitter_buffer = JitterBuffer::<JITTER_BUFF_SIZE>::new(self.clock.clone(), receiver_metrics);

        thread::spawn({
            move || {
                let mut decoder = match Decoder::new().map_err(|e| Error::MapError(e.to_string())) {
                    Ok(d) => d,
                    Err(e) => {
                        logger.error(&format!("Failed to create decoder: {e}"));
                        send_message_to_ui(
                            event_tx,
                            AppEvent::Error("Failed to create decoder".into()),
                        );
                        return;
                    }
                };

                loop {
                    if !connected.load(Ordering::SeqCst) {
                        break;
                    }

                    match rtp_rx.recv() {
                        Ok(packet) => {
                            jitter_buffer.add(packet)
                        },
                        Err(_) => break,
                    }

                    if let Some(chunks) = jitter_buffer.pop()
                        && let Some(frame_data) = generate_frame_from_chunks(&chunks, &mut decoder)
                        && remote_frame_tx.send(frame_data.clone()).is_err() {
                            break;
                        }
                }
                if connected.load(Ordering::SeqCst) {
                    connected.store(false, Ordering::SeqCst);
                    send_message_to_ui(event_tx.clone(), AppEvent::CallEnded);
                }
                logger.info("Remote frames pipeline terminated");
            }
        });
        Ok(remote_frame_rx)
    }

    // Builds the local capture pipeline and returns preview frames.
    fn start_local_frame_pipeline(
        &mut self,
        rtp_tx: Sender<RtpPacket>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
    ) -> Result<Receiver<Frame>, Error> {
        let (local_frame_tx, local_frame_rx) = mpsc::channel();
        let (raw_tx, raw_rx) = mpsc::channel();

        self.start_encoding_thread(raw_rx, event_tx.clone(), rtp_tx, connected.clone())?;
        let camera_frame_rx = self
            .camera
            .start()
            .map_err(|e| Error::MapError(e.to_string()))?;

        thread::spawn(move || {
            for frame in camera_frame_rx {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                if local_frame_tx.send(frame.clone()).is_err() {
                    break;
                }

                if raw_tx.send(frame.clone()).is_err() {
                    break;
                }
            }
        });

        Ok(local_frame_rx)
    }

    // Spawns the encoding thread that turns raw frames into RTP packets.
    fn start_encoding_thread(&mut self, raw_rx: Receiver<Frame>, event_tx: Sender<AppEvent>, rtp_tx: Sender<RtpPacket>, connected: Arc<AtomicBool>) -> Result<(), Error> {
        let mut encoder = match Encoder::new(&self.config.media) {
            Ok(d) => d,
            Err(e) => {
                self.logger
                    .error(&format!("Failed to create encoder: {e}"));
                send_message_to_ui(event_tx, AppEvent::Error(e.to_string()));
                return Err(Error::MapError(e.to_string()));
            }
        };

        let ssrc = self.ssrc;
        let config = Arc::clone(&self.config);
        let logger = self.logger.clone();

        thread::spawn(move || {
            let mut seq_num: u64 = 0;
            for frame in raw_rx {
                let encoded_frame = match encoder.encode_frame(&frame) {
                    Ok(f) => f,
                    Err(e) => {
                        logger.error(&format!("Failed to encode frame: {e}"));
                        break;
                    }
                };

                if let Err(e) = send_encoded_frame(encoded_frame, &rtp_tx, ssrc, &connected, &mut seq_num, &config) {
                    logger.error(&Error::SendError(e.to_string()).to_string());
                    break
                }
            };
        });

        Ok(())
    }
}

/// Sends an encoded frame as RTP packets over the provided channel.
fn send_encoded_frame(
    encoded_frame: EncodedFrame,
    rtp_tx: &Sender<RtpPacket>,
    ssrc: u32,
    connected: &Arc<AtomicBool>,
    sequence_number: &mut u64,
    config: &Arc<Config>,
) -> Result<(), Error> {
    if !connected.load(Ordering::SeqCst) {
        return Err(Error::SendError("Media pipeline turned off".into()));
    }

    let total_chunks = encoded_frame.chunks.len();

    for (i, payload) in encoded_frame.chunks.iter().enumerate() {
        let packet = RtpPacket {
            version: config.media.rtp_version,
            marker: (i == total_chunks - 1) as u8,
            total_chunks: total_chunks as u8,
            is_i_frame: encoded_frame.is_i_frame,
            sequence_number: *sequence_number,
            payload_type: config.media.rtp_payload_type,
            timestamp: encoded_frame.frame_time,
            ssrc,
            payload: payload.clone(),
        };
        *sequence_number = sequence_number.saturating_add(1);
        rtp_tx
            .send(packet)
            .map_err(|e| Error::SendError(e.to_string()))?;
    }
    Ok(())
}

/// Reassembles and decodes RTP chunks into a displayable frame.
fn generate_frame_from_chunks(data: &[u8], decoder: &mut Decoder) -> Option<Frame> {
    let (decoded_data, width, height) = match decoder.decode_frame(data) {
        Ok(data) => {
            data
        },
        Err(_) => {
            return None
        },
    };

    Some(Frame {
        data: decoded_data,
        width,
        height,
        frame_time: 0,
    })
}

fn send_message_to_ui(event_tx: Sender<AppEvent>, event: AppEvent) {
    let _ = event_tx.send(event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn test_generate_frame_from_chunks_h264() {
        // ------- Setup -------
        let config = match Config::load(Path::new("./room_rtc.conf")) {
            Ok(cfg) => Arc::new(cfg),
            Err(_) => {
                eprintln!("configuration file not found");
                return;
            }
        };

        let mut encoder = match Encoder::new(&config.media) {
            Ok(enc) => enc,
            Err(e) => {
                eprintln!("Failed to create encoder: {}", e);
                return;
            }
        };

        let mut decoder = match Decoder::new() {
            Ok(dec) => dec,
            Err(e) => {
                eprintln!("Failed to create decoder: {}", e);
                return;
            }
        };

        // Creamos un frame crudo sintético RGB 320x240
        let width = 320;
        let height = 240;
        let raw_data = vec![128u8; width * height * 3];

        let raw_frame = Frame {
            data: raw_data.clone(),
            width,
            height,
            frame_time: 0,
        };

        // ------- Encode -------
        let encoded = match encoder.encode_frame(&raw_frame) {
            Ok(enc) => enc,
            Err(e) => {
                eprintln!("Failed to encode frame: {}", e);
                return;
            }
        };

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

        // ------- Decodificar vía generate_frame_from_chunks -------
        rtp_chunks.sort_by_key(|c| c.sequence_number);
        let mut data = Vec::new();
        for c in rtp_chunks.iter() {
            data.extend_from_slice(&c.payload);
        }

        let decoded = match generate_frame_from_chunks(&data, &mut decoder) {
            Some(frame) => frame,
            None => {
                eprintln!("generate_frame_from_chunks returned None");
                return;
            }
        };

        // ------- Validaciones -------
        assert_eq!(decoded.data, raw_frame.data);
    }
}
