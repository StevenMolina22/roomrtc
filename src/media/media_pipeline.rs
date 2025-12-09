use crate::config::Config;
use crate::media::camera::{Camera, FrameSource};
use crate::media::error::MediaPipelineError as Error;
use crate::media::frame_handler::{Decoder, EncodedFrame, Encoder, Frame};
use crate::transport::{rtp::RtpPacket, jitter_buffer::JitterBuffer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, mpsc};
use std::thread;
use crate::controller::AppEvent;
use crate::clock::Clock;


const JITTER_BUFF_SIZE: usize = 512;

pub struct MediaPipeline {
    camera: Camera,
    config: Arc<Config>,
    ssrc: u32,
    clock: Arc<Clock>
}

impl MediaPipeline {
    #[must_use]
    pub fn new(config: &Arc<Config>, ssrc: u32) -> Self {
        let clock = Arc::new(Clock::new());
        Self {
            camera: Camera::new(clock.clone(), config),
            config: Arc::clone(config),
            ssrc,
            clock
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
        let remote_frame_rx = self.start_remote_frame_pipeline(rtp_rx, event_tx.clone(), connected.clone())?;

        Ok((local_frame_rx, remote_frame_rx))
    }

    pub fn stop(&self) {
        self.camera.stop()
    }

    fn start_remote_frame_pipeline(
        &self,
        rtp_rx: Receiver<RtpPacket>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
    ) -> Result<Receiver<Frame>, Error> {
        let (remote_frame_tx, remote_frame_rx) = mpsc::channel();

        let mut jitter_buffer = JitterBuffer::<JITTER_BUFF_SIZE>::new(self.clock.clone());
        let clock = self.clock.clone();

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
                        Ok(packet) => {
                            println!("[MP] RTP packet received {}", packet.sequence_number);
                            jitter_buffer.add(packet)
                        },
                        Err(_) => break,
                    }

                    if let Some(chunks) = jitter_buffer.pop()
                        && let Some(frame_data) = generate_frame_from_chunks(&chunks, &mut decoder, clock.clone())
                    {
                        if let Err(_) = remote_frame_tx.send(frame_data.clone()) {
                            println!("[MP] Failed to send remote frame");
                            break;
                        }
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

            // if connected.load(Ordering::SeqCst) {
            //     connected.store(false, Ordering::SeqCst);
            //     send_message_to_ui(event_tx.clone(), AppEvent::CallEnded);
            // }
        });

        Ok(local_frame_rx)
    }

    fn start_encoding_thread(&mut self, raw_rx: Receiver<Frame>, event_tx: Sender<AppEvent>, rtp_tx: Sender<RtpPacket>, connected: Arc<AtomicBool>) -> Result<(), Error> {
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
            let mut seq_num: u64 = 0;
            for frame in raw_rx {
                let encoded_frame = match encoder.encode_frame(&frame) {
                    Ok(f) => f,
                    Err(_) => break,
                };

                if send_encoded_frame(encoded_frame, &rtp_tx, ssrc, connected.clone(), &mut seq_num, &config).is_err() {
                    break;
                }
            };
        });

        Ok(())
    }
}
fn send_encoded_frame(
    encoded_frame: EncodedFrame,
    rtp_tx: &Sender<RtpPacket>,
    ssrc: u32,
    connected: Arc<AtomicBool>,
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

fn generate_frame_from_chunks(data: &Vec<u8>, decoder: &mut Decoder, clock: Arc<Clock>) -> Option<Frame> {
    let time_before = clock.now();
    let (decoded_data, width, height) = match decoder.decode_frame(&data) {
        Ok(data) => {
            println!("[MP] data decoded");
            data
        },
        Err(_) => {
            println!("[MP] failed to decode");
            return None
        },
    };
    let time_after = clock.now();
    println!("TIME LASTED TO ENCODE {}", time_after - time_before);

    Some(Frame {
        data: decoded_data,
        width,
        height,
        frame_time: 0,
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
            frame_time: 0,
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

        // ------- Decodificar vía generate_frame_from_chunks -------
        rtp_chunks.sort_by_key(|c| c.sequence_number);
        let mut data = Vec::new();
        for c in rtp_chunks.iter() {
            data.extend_from_slice(&c.payload);
        }

        let decoded = generate_frame_from_chunks(&data, &mut decoder, Arc::new(Clock::new()))
            .expect("generate_frame_from_chunks no devolvió frame");

        // ------- Validaciones -------
        assert_eq!(decoded.data, raw_frame.data);
    }
}
