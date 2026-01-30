use crate::config::Config;
use crate::logger::Logger;
use crate::media::audio::{AudioEncoder, AudioDecoder, AudioError as Error};
use crate::media::audio::microphone::Microphone;
use crate::media::audio::speaker::Speaker;
use crate::transport::rtcp::ReceiverStats;
use crate::transport::rtp::RtpPacket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use crate::clock::Clock;

/// Complete audio capture, encoding, transmission, reception, and playback pipeline.
/// 
/// Manages bidirectional audio streaming by coordinating microphone capture,
/// OPUS encoding, RTP transmission, RTP reception, OPUS decoding, and speaker playback.
/// 
/// # Threading Model
/// 
/// Runs two independent worker threads:
/// - **Send thread**: Captures audio from microphone, encodes to OPUS, sends via RTP
/// - **Receive thread**: Receives RTP packets, decodes and plays via speaker (no buffering)
/// 
/// # Features
/// 
/// - Real-time microphone to speaker audio streaming
/// - OPUS codec encoding and decoding
/// - RTP packet formation and processing
/// - Direct playback without jitter buffer
/// - Connection state monitoring
/// - Receiver statistics tracking
pub struct AudioPipeline {
    microphone: Microphone,
    config: Arc<Config>,
    logger: Logger,
}

impl AudioPipeline {
    /// Creates a new AudioPipeline instance.
    /// 
    /// Initializes microphone, speaker, and codec components. Does not start streaming;
    /// use `start()` to begin audio capture and playback.
    /// 
    /// # Arguments
    /// 
    /// * `config` - Application configuration containing audio parameters
    /// * `ssrc` - Synchronization source identifier for RTP packets
    /// * `logger` - Logger instance for diagnostics
    /// * `clock` - Shared clock for timestamp generation
    /// 
    /// # Returns
    /// 
    /// A new `AudioPipeline` instance, or an error string if initialization fails.
    pub fn new(config: &Arc<Config>, logger: Logger, clock: Arc<Clock>) -> Result<Self, Error> {
        let microphone = Microphone::new(clock.clone(), config.clone(), logger.clone());

        Ok(Self {
            microphone,
            config: Arc::clone(config),
            logger,
        })
    }

    /// Starts the audio pipeline with bidirectional streaming.
    /// 
    /// Launches two worker threads for sending and receiving audio streams.
    /// The send thread captures from microphone and encodes to RTP packets.
    /// The receive thread decodes RTP packets and plays through speaker.
    /// 
    /// # Arguments
    /// 
    /// * `rtp_tx` - Sender for outgoing RTP packets (to network transport)
    /// * `rtp_rx` - Receiver for incoming RTP packets (from network transport)
    /// * `connected` - Atomic flag to monitor connection state; threads exit when set to false
    /// * `receiver_metrics` - Reference to receiver statistics for network diagnostics
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if threads were spawned successfully, or an `Error` if microphone startup fails.
    /// 
    /// # Note
    /// 
    /// Worker threads run indefinitely until the `connected` flag is set to false,
    /// at which point they gracefully shut down.
    pub fn start(
        &mut self,
        rtp_tx: Sender<RtpPacket>,
        rtp_rx: Receiver<RtpPacket>,
        connected: Arc<AtomicBool>,
        _receiver_metrics: Arc<Mutex<ReceiverStats>>,
    ) -> Result<(), Error> {
        self.start_audio_sender_pipeline(connected.clone(), rtp_tx)?;
        self.start_audio_receiver_pipeline(connected.clone(), rtp_rx)?;
        Ok(())
    }

    fn start_audio_sender_pipeline(
        &mut self,
        connected: Arc<AtomicBool>,
        rtp_tx: Sender<RtpPacket>,
    ) -> Result<(), Error>{
        let mic_rx = self.microphone.start().map_err(|e| Error::MapError(e.to_string()))?;
        let logger = self.logger.clone();
        let conn = connected.clone();
        let mut encoder = AudioEncoder::new().map_err(|e| Error::MapError(e.to_string()))?;
        let config = self.config.clone();
        thread::spawn(move || {
            let mut seq_num = 0u64;
            for frame in mic_rx {
                if !conn.load(Ordering::Relaxed) { break; }
                let timestamp = frame.timestamp;
                if let Ok(encoded_bytes) = encoder.encode(frame) {
                    let packet = RtpPacket {
                        version: config.media.rtp_version,
                        marker: 1,
                        total_chunks: 1,
                        is_i_frame: false,
                        payload_type: config.media.audio_payload_type,
                        sequence_number: seq_num,
                        timestamp,
                        ssrc: config.media.audio_ssrc,
                        payload: encoded_bytes,
                    };
                    seq_num = seq_num.saturating_add(1);
                    if rtp_tx.send(packet).is_err() { break; }
                }
            }
            logger.info("Audio send thread ended");
        });
        Ok(())
    }

    fn start_audio_receiver_pipeline(
        &mut self,
        connected: Arc<AtomicBool>,
        rtp_rx: Receiver<RtpPacket>,
    ) -> Result<(), Error>{
        let conn = connected.clone();
        let mut decoder = AudioDecoder::new().map_err(|e| Error::MapError(e.to_string()))?;
        let logger = self.logger.clone();
        let speaker = Speaker::new(logger.clone(), self.config.media.audio_sample_rate).map_err(|e| Error::MapError(e.to_string()))?;
        thread::spawn(move || {
            loop {
                if !conn.load(Ordering::Relaxed) { break; }

                match rtp_rx.recv() {
                    Ok(pkt) => {
                        if let Ok(samples) = decoder.decode(&pkt.payload) {
                            speaker.play(samples);
                        }
                    },
                    Err(_) => break,
                }
            }
            logger.info("Audio receive thread ended");
        });
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Err(_) = self.microphone.stop() {
            return;
        }
    }
    pub fn toggle_mute(&self) {
        self.microphone.toggle_mute();
    }
}