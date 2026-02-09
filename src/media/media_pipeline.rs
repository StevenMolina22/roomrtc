use crate::clock::Clock;
use crate::config::Config;
use crate::controller::AppEvent;
use crate::logger::Logger;
use crate::media::audio_pipeline::AudioPipeline;
use crate::media::camera::{Camera, FrameSource};
use crate::media::error::MediaPipelineError as Error;
use crate::media::frame_handler::color_space::rgb_to_yuv420;
use crate::media::frame_handler::{Decoder, EncodedFrame, Encoder, Frame};
use crate::transport::rtcp::ReceiverStats;
use crate::transport::{jitter_buffer::JitterBuffer, rtp::RtpPacket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use yuv::YuvPlanarImageMut;

/// Maximum number of RTP packets buffered for jitter compensation.
const JITTER_BUFF_SIZE: usize = 2048;

/// Coordinates camera capture, encoding/decoding, and RTP transport.
///
/// `MediaPipeline` starts the local camera, encodes outgoing frames to RTP,
/// and decodes incoming RTP into displayable frames. It also dispatches UI
/// events for errors and call termination.
pub struct MediaPipeline {
    camera: Camera,
    audio: Option<AudioPipeline>,
    config: Arc<Config>,
    logger: Logger,
    clock: Arc<Clock>,
}

impl MediaPipeline {
    /// Creates a new media pipeline with its own clock and camera.
    #[must_use]
    pub fn new(config: &Arc<Config>, logger: Logger) -> Self {
        let clock = Arc::new(Clock::new());
        let audio = match AudioPipeline::new(config, logger.clone(), clock.clone()) {
            Ok(ap) => Some(ap),
            Err(e) => {
                logger.error(&format!("Audio init failed: {}", e));
                None
            }
        };

        Self {
            camera: Camera::new(clock.clone(), config, logger.clone()),
            audio,
            config: Arc::clone(config),
            logger,
            clock,
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
        receiver_metrics: Arc<Mutex<ReceiverStats>>,
    ) -> Result<(Receiver<Frame>, Receiver<Frame>), Error> {
        // Local preview + encoded sender
        let local_frame_rx =
            self.start_local_frame_pipeline(rtp_tx.clone(), event_tx.clone(), connected.clone())?;

        // Create internal splitter channels for video/audio demuxing
        let (video_tx, video_rx) = mpsc::channel();
        let (audio_tx, audio_rx) = mpsc::channel();

        self.start_remote_pipeline(rtp_rx, connected.clone(), video_tx, audio_tx);

        // Start video receive pipeline using video_rx
        let remote_frame_rx = self.start_remote_frame_pipeline(
            video_rx,
            event_tx.clone(),
            connected.clone(),
            receiver_metrics.clone(),
        )?;

        // Start audio pipeline if available (use audio_rx)
        if let Some(audio) = &mut self.audio {
            audio
                .start(
                    rtp_tx,
                    audio_rx,
                    connected.clone(),
                    receiver_metrics.clone(),
                )
                .map_err(|e| Error::MapError(e.to_string()))?;
        }

        Ok((local_frame_rx, remote_frame_rx))
    }

    /// Stops camera capture and related media components.
    pub fn stop(&mut self) {
        self.logger.info("Stopping MediaPipeline...");
        self.camera.stop();
        if let Some(ref mut ap) = self.audio {
            ap.stop();
        }
    }

    fn start_remote_pipeline(
        &mut self,
        rtp_rx: Receiver<RtpPacket>,
        connected: Arc<AtomicBool>,
        video_tx: Sender<RtpPacket>,
        audio_tx: Sender<RtpPacket>,
    ) {
        let config = Arc::clone(&self.config);
        let logger = self.logger.clone();
        thread::spawn(move || {
            while let Ok(packet) = rtp_rx.recv() {
                if !connected.load(Ordering::Relaxed) {
                    break;
                }
                if packet.payload_type == config.media.video_payload_type {
                    if video_tx.send(packet).is_err() {
                        break;
                    }
                } else if packet.payload_type == config.media.audio_payload_type
                    && audio_tx.send(packet).is_err()
                {
                    break;
                }
            }
            logger.info("Demuxer thread finished");
        });
    }
    // Builds the remote receive pipeline: jitter buffer + decoder.
    fn start_remote_frame_pipeline(
        &self,
        rtp_rx: Receiver<RtpPacket>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
        receiver_metrics: Arc<Mutex<ReceiverStats>>,
    ) -> Result<Receiver<Frame>, Error> {
        let (remote_frame_tx, remote_frame_rx) = mpsc::channel();
        let logger = self.logger.clone();

        let jitter_buffer = JitterBuffer::<JITTER_BUFF_SIZE>::new(
            self.clock.clone(),
            receiver_metrics,
            self.logger.clone(),
        );

        let decoder = Decoder::new().map_err(|e| {
            send_message_to_ui(
                event_tx.clone(),
                AppEvent::Error("Failed to create decoder".into()),
            );
            Error::MapError(e.to_string())
        })?;
        thread::spawn({
            move || {
                run_frame_pipeline_loop(
                    rtp_rx,
                    remote_frame_tx,
                    jitter_buffer,
                    decoder,
                    logger,
                    event_tx,
                    connected,
                );
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
        let (rgb_tx, rgb_rx) = mpsc::channel();

        self.start_frame_sender_thread(rgb_rx, event_tx.clone(), rtp_tx, &connected)?;
        let camera_frame_rx = self.camera.start();

        thread::spawn(move || {
            for frame in camera_frame_rx {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                if local_frame_tx.send(frame.clone()).is_err() {
                    break;
                }

                if rgb_tx.send(frame.clone()).is_err() {
                    break;
                }
            }
        });

        Ok(local_frame_rx)
    }

    // Spawns the encoding thread that turns raw frames into RTP packets.
    fn start_frame_sender_thread(
        &mut self,
        rgb_rx: Receiver<Frame>,
        event_tx: Sender<AppEvent>,
        rtp_tx: Sender<RtpPacket>,
        connected: &Arc<AtomicBool>,
    ) -> Result<(), Error> {
        let yuv_rx = self.start_rgb_to_yuv_thread(rgb_rx)?;
        self.start_encoded_sender_thread(rtp_tx, yuv_rx, event_tx.clone(), connected)?;

        Ok(())
    }

    fn start_rgb_to_yuv_thread(
        &mut self,
        rgb_rx: Receiver<Frame>,
    ) -> Result<Receiver<(YuvPlanarImageMut<'static, u8>, u128)>, Error> {
        let (yuv_tx, yuv_rx) = mpsc::channel();

        thread::spawn(move || {
            for rgb_frame in rgb_rx {
                let yuv = match rgb_to_yuv420(&rgb_frame) {
                    Ok(yuv) => yuv,
                    Err(_) => break,
                };

                if yuv_tx.send((yuv, rgb_frame.frame_time)).is_err() {
                    break;
                }
            }
        });

        Ok(yuv_rx)
    }

    fn start_encoded_sender_thread(
        &mut self,
        rtp_tx: Sender<RtpPacket>,
        yuv_rx: Receiver<(YuvPlanarImageMut<'static, u8>, u128)>,
        event_tx: Sender<AppEvent>,
        connected: &Arc<AtomicBool>,
    ) -> Result<(), Error> {
        let mut encoder = match Encoder::new(&self.config.media) {
            Ok(d) => d,
            Err(e) => {
                self.logger.error(&format!("Failed to create encoder: {e}"));
                send_message_to_ui(event_tx, AppEvent::Error(e.to_string()));
                return Err(Error::MapError(e.to_string()));
            }
        };

        let logger = self.logger.clone();
        let config = self.config.clone();
        let connected = connected.clone();
        let mut seq_num: u64 = 0;

        thread::spawn(move || {
            for (yuv, timestamp) in yuv_rx {
                let encoded = match encoder.encode(&yuv, timestamp) {
                    Ok(f) => f,
                    Err(e) => {
                        logger.error(&format!("Failed to encode frame: {e}"));
                        break;
                    }
                };

                if let Err(e) =
                    send_encoded_frame(encoded, &rtp_tx, &connected, &mut seq_num, &config)
                {
                    logger.error(&Error::SendError(e.to_string()).to_string());
                    break;
                }
            }
        });
        Ok(())
    }

    /// Toggles microphone mute state in the embedded audio pipeline.
    ///
    /// If audio was not initialized for this media pipeline, this method is a no-op.
    pub fn toggle_audio(&self) {
        if let Some(audio) = &self.audio {
            audio.toggle_mute();
        }
    }
}

fn run_frame_pipeline_loop(
    rtp_rx: Receiver<RtpPacket>,
    remote_frame_tx: Sender<Frame>,
    mut jitter_buffer: JitterBuffer<JITTER_BUFF_SIZE>,
    mut decoder: Decoder,
    logger: Logger,
    event_tx: Sender<AppEvent>,
    connected: Arc<AtomicBool>,
) {
    while connected.load(Ordering::SeqCst) {
        match rtp_rx.recv() {
            Ok(packet) => jitter_buffer.add(packet),
            Err(_) => break,
        }

        if let Some(chunks) = jitter_buffer.pop()
            && let Some(frame_data) = generate_frame_from_chunks(&chunks, &mut decoder)
            && remote_frame_tx.send(frame_data.clone()).is_err()
        {
            break;
        }
    }

    if connected.load(Ordering::SeqCst) {
        connected.store(false, Ordering::SeqCst);
        send_message_to_ui(event_tx.clone(), AppEvent::CallEnded);
    }
    logger.info("Remote frames pipeline terminated");
}

/// Sends an encoded frame as RTP packets over the provided channel.
fn send_encoded_frame(
    encoded_frame: EncodedFrame,
    rtp_tx: &Sender<RtpPacket>,
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
            payload_type: config.media.video_payload_type,
            timestamp: encoded_frame.frame_time,
            ssrc: config.media.frame_ssrc,
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
        Ok(data) => data,
        Err(_) => return None,
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
                return;
            }
        };

        let mut encoder = match Encoder::new(&config.media) {
            Ok(enc) => enc,
            Err(_) => {
                return;
            }
        };

        let mut decoder = match Decoder::new() {
            Ok(dec) => dec,
            Err(_) => {
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
        let yuv_frame = match rgb_to_yuv420(&raw_frame) {
            Ok(img) => img,
            Err(_) => {
                return;
            }
        };

        let encoded = match encoder.encode(&yuv_frame, raw_frame.frame_time) {
            Ok(enc) => enc,
            Err(_) => {
                return;
            }
        };

        assert!(!encoded.chunks.is_empty(), "Encoding should not be empty");

        let mut rtp_chunks = Vec::new();
        let total_chunks = encoded.chunks.len() as u8;

        for (i, chunk) in encoded.chunks.iter().enumerate() {
            rtp_chunks.push(RtpPacket {
                version: config.media.rtp_version,
                marker: 0,
                total_chunks,
                is_i_frame: encoded.is_i_frame,
                payload_type: config.media.video_payload_type,
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
                return;
            }
        };

        // ------- Validaciones -------
        assert_eq!(decoded.data, raw_frame.data);
    }
}
