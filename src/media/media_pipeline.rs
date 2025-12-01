use crate::config::Config;
use crate::controller::AppEvent;
use crate::media::camera::{Camera, FrameSource};
use crate::media::error::MediaPipelineError as Error;
use crate::media::frame_handler::{Decoder, EncodedFrame, Encoder, Frame};
use crate::session::sdp::DtlsSetupRole;
use crate::srtp::SrtpContext;
use crate::transport::rtp::RtpPacket;
use chrono::Local;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

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
        rtp_tx: Sender<Vec<u8>>,
        rtp_rx: Receiver<Vec<u8>>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
        srtp_ctx: SrtpContext,
        role: DtlsSetupRole,
    ) -> Result<(Receiver<Frame>, Receiver<Frame>), Error> {
        let is_client = matches!(role, DtlsSetupRole::Active);
        let srtp_ctx = Arc::new(Mutex::new(srtp_ctx));

        let local_frame_rx = self.start_local_frame_pipeline(
            rtp_tx,
            event_tx.clone(),
            connected.clone(),
            srtp_ctx.clone(),
            is_client.clone(),
        )?;
        let remote_frame_rx = self.start_remote_frames_pipeline(
            rtp_rx,
            event_tx.clone(),
            connected.clone(),
            srtp_ctx.clone(),
            is_client.clone(),
        )?;

        Ok((local_frame_rx, remote_frame_rx))
    }

    pub fn stop(&self) -> Result<(), Error> {
        self.camera
            .stop()
            .map_err(|e| Error::MapError(e.to_string()))
    }

    fn start_remote_frames_pipeline(
        &self,
        srtp_rx: Receiver<Vec<u8>>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
        srtp_context: Arc<Mutex<SrtpContext>>,
        is_client: bool,
    ) -> Result<Receiver<Frame>, Error> {
        let (remote_frame_tx, remote_frame_rx) = mpsc::channel();

        thread::spawn({
            move || {
                let mut decoder = match Decoder::new().map_err(|e| Error::MapError(e.to_string())) {
                    Ok(d) => d,
                    Err(_) => {
                        send_message_to_ui(
                            event_tx,
                            AppEvent::Error("Failed to create decoder".into()),
                        );
                        return;
                    }
                };
                let mut actual_frame = None;
                let mut chunks = Vec::new();

                loop {
                    if !connected.load(Ordering::SeqCst) {
                        break;
                    }

                    let rtp_packet = match srtp_rx.recv() {
                        Ok(protected_data) => {
                            if protected_data.is_empty() {
                                continue;
                            }
                            let first_byte = protected_data[0];

                            if (20..=63).contains(&first_byte) {
                                continue;
                            } else if (128..=191).contains(&first_byte) {
                                match srtp_context.lock() {
                                    Ok(mut ctx) => {
                                        match ctx.unprotect(&protected_data, is_client) {
                                            Ok(unprotected_packet) => unprotected_packet,
                                            Err(e) => break,
                                        }
                                    }
                                    Err(e) => break,
                                }
                            } else {
                                continue;
                            }
                        }
                        Err(_) => break,
                    };

                    if actual_frame == Some(rtp_packet.frame_id) {
                        chunks.push(rtp_packet.clone());
                    } else {
                        chunks = vec![rtp_packet.clone()];
                        actual_frame = Some(rtp_packet.frame_id);
                    }

                    let expected_marker = rtp_packet.marker;
                    let current_chunk_count = chunks.len() as u16;

                    if current_chunk_count == expected_marker {
                        if let Some(frame_data) =
                            generate_frame_from_chunks(&mut chunks, &mut decoder)
                            && remote_frame_tx.send(frame_data.clone()).is_err()
                        {
                            break;
                        }
                        actual_frame = None;
                        chunks.clear();
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
        srtp_tx: Sender<Vec<u8>>,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
        srtp_ctx: Arc<Mutex<SrtpContext>>,
        is_client: bool,
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
        let srtp_ctx = srtp_ctx.clone();
        let config = self.config.clone();
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

                let rtp_packet = generate_rtp_packet(encoded_frame, config.clone(), ssrc);

                for packet in rtp_packet {
                    send_encripted_rtp_packet(srtp_tx.clone(), packet, srtp_ctx.clone(), is_client);
                }
            }

            while connected.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            // if connected.load(Ordering::SeqCst) {
            //     connected.store(false, Ordering::SeqCst);
            //     send_message_to_ui(event_tx.clone(), AppEvent::CallEnded);
            // }
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

    let marker = encoded_frame.chunks.len() as u16;
    for (chunk_id, payload) in encoded_frame.chunks.iter().enumerate() {
        let packet = RtpPacket {
            version: config.media.rtp_version,
            marker,
            payload_type: config.media.rtp_payload_type,
            frame_id: encoded_frame.id,
            chunk_id: chunk_id as u64,
            timestamp: Local::now().timestamp_millis() as u32,
            ssrc,
            payload: payload.clone(),
        };
        rtp_tx
            .send(packet)
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
    let (decoded_data, width, height) = match decoder.decode_frame(&data) {
        Ok(data) => data,
        Err(_) => return None,
    };

    Some(Frame {
        data: decoded_data,
        width,
        height,
        id: fr_id,
    })
}

fn send_message_to_ui(event_tx: Sender<AppEvent>, event: AppEvent) {
    event_tx.send(event);
}

fn generate_rtp_packet(
    encoded_frame: EncodedFrame,
    config: Arc<Config>,
    ssrc: u32,
) -> Vec<RtpPacket> {
    let mut packets = Vec::new();
    let marker = encoded_frame.chunks.len() as u16;
    for (chunk_id, payload) in encoded_frame.chunks.iter().enumerate() {
        let packet = RtpPacket {
            version: config.media.rtp_version,
            marker,
            payload_type: config.media.rtp_payload_type,
            frame_id: encoded_frame.id,
            chunk_id: chunk_id as u64,
            timestamp: Local::now().timestamp_millis() as u32,
            ssrc,
            payload: payload.clone(),
        };
        packets.push(packet);
    }
    packets
}

fn send_encripted_rtp_packet(
    srtp_tx: Sender<Vec<u8>>,
    packet: RtpPacket,
    srtp_ctx: Arc<Mutex<SrtpContext>>,
    is_client: bool,
) -> Result<(), Error> {
    let protected_data = {
        let mut ctx = match srtp_ctx.lock() {
            Ok(c) => c,
            Err(e) => {
                return Err(Error::MapError(e.to_string()));
            }
        };

        match ctx.protect(&packet, is_client) {
            Ok(data) => data,
            Err(e) => {
                return Err(Error::ProtectionError(e.to_string()));
            }
        }
    };
    if let Err(e) = srtp_tx.send(protected_data) {
        return Err(Error::SendError(e.to_string()));
    };

    Ok(())
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
            id: 7,
        };

        // ------- Encode -------
        let encoded = encoder
            .encode_frame(&raw_frame)
            .expect("Fallo encodear el frame");

        assert!(!encoded.chunks.is_empty(), "El encoder debe generar chunks");

        // ------- Construimos los RtpPacket -------
        let mut rtp_chunks = Vec::new();
        let marker = encoded.chunks.len() as u16;

        for (i, chunk) in encoded.chunks.iter().enumerate() {
            rtp_chunks.push(RtpPacket {
                version: config.media.rtp_version,
                marker,
                payload_type: config.media.rtp_payload_type,
                frame_id: encoded.id,
                chunk_id: i as u64,
                timestamp: 1234,
                ssrc: 55,
                payload: chunk.clone(),
            });
        }

        // ------- Decodificar vía generate_frame_from_chunks -------
        let decoded = generate_frame_from_chunks(&mut rtp_chunks, &mut decoder)
            .expect("generate_frame_from_chunks no devolvió frame");

        // ------- Validaciones -------
        assert_eq!(decoded.width, width);
        assert_eq!(decoded.height, height);
        assert_eq!(decoded.id, raw_frame.id);

        // No comparamos byte a byte porque H.264 es con pérdida,
        // pero verificamos que la salida tenga datos y tamaño correcto
        assert_eq!(
            decoded.data.len(),
            raw_data.len(),
            "El decoded debe tener mismo tamaño que el raw"
        );

        assert!(
            decoded.data.iter().any(|b| *b != 0),
            "Debe haber datos válidos"
        );
    }
}
