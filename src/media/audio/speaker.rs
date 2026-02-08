use super::error::AudioError as Error;
use crate::logger::Logger;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Audio playback device handler.
///
/// Manages audio output to the system's default speaker device. Handles sample rate
/// conversion from OPUS standard (48kHz) to the device's native sample rate using
/// linear interpolation, and distributes mono audio to all output channels.
///
/// # Features
///
/// - Automatic resampling to device's native sample rate
/// - Mono to multi-channel distribution
/// - Sample feeding via channel
/// - Real-time audio processing
pub struct Speaker {
    _stream: Arc<Mutex<Option<cpal::Stream>>>,
    tx: Sender<Vec<f32>>,
}

impl Speaker {
    /// Creates a new Speaker instance and starts the audio output stream.
    ///
    /// Initializes the default audio output device with its native configuration,
    /// sets up audio processing pipeline (resampling and channel distribution),
    /// and begins playing audio.
    ///
    /// # Arguments
    ///
    /// * `logger` - Logger instance for diagnostics and error reporting
    ///
    /// # Returns
    ///
    /// A new `Speaker` instance ready to receive and play audio samples,
    /// or an error string if device initialization fails.
    pub fn new(logger: Logger, sample_rate: u32) -> Result<Self, Error> {
        let (device, config, output_channels, output_rate) = setup_output_device()?;

        logger.info(&format!(
            "Native speaker: {}Hz, {} channels.",
            output_rate, output_channels
        ));

        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let rx = Arc::new(Mutex::new(rx));
        let logger_cl = logger.clone();

        let stream = Self::generate_output_stream(
            device,
            config,
            rx,
            sample_rate,
            output_rate,
            output_channels,
            logger_cl,
        )?;

        stream.play().map_err(|e| Error::MapError(e.to_string()))?;

        Ok(Self {
            _stream: Arc::new(Mutex::new(Some(stream))),
            tx,
        })
    }

    /// Sends audio samples to the speaker for playback.
    ///
    /// Queues audio samples (typically 960 mono samples at 48kHz from OPUS decoder)
    /// for playback. Samples are processed and played in real-time.
    ///
    /// # Arguments
    ///
    /// * `samples` - Vector of mono audio samples (f32)
    pub fn play(&self, samples: Vec<f32>) {
        let _ = self.tx.send(samples);
    }

    /// Retrieves all pending audio packets from the channel and adds them to the buffer.
    ///
    /// Operation that drains all available samples from the communication
    /// channel and appends them to the internal buffer for processing.
    ///
    /// # Arguments
    ///
    /// * `rx` - Receiver channel with pending audio samples
    /// * `buffer` - Internal buffer to accumulate samples
    fn fetch_new_samples(rx: &Mutex<Receiver<Vec<f32>>>, buffer: &mut Vec<f32>) {
        if let Ok(rx_lock) = rx.lock() {
            while let Ok(packet) = rx_lock.try_recv() {
                buffer.extend(packet);
            }
        }
    }

    /// Performs resampling and distributes audio to device channels.
    ///
    /// Applies linear interpolation to convert from 48kHz OPUS samples to the
    /// device's native sample rate, then duplicates the mono signal across
    /// all output channels.
    ///
    /// # Arguments
    ///
    /// * `output` - Output buffer to fill with samples for the hardware device
    /// * `input` - Input buffer with mono audio samples at 48kHz
    /// * `phase` - Phase accumulator maintaining interpolation position across calls
    /// * `step` - Resampling step size (OPUS_SAMPLE_RATE / device_rate)
    /// * `channels` - Number of output channels in the device
    fn process_audio_frame(
        output: &mut [f32],
        input: &[f32],
        phase: &mut f32,
        step: f32,
        channels: usize,
    ) {
        output.fill(0.0);

        if input.len() < 2 {
            return;
        }

        let mut write_idx = 0;

        while write_idx < output.len() {
            if *phase >= (input.len() as f32 - 1.0) {
                break;
            }

            let idx = *phase as usize;
            let frac = *phase - idx as f32;

            let sample_a = input[idx];
            let sample_b = input[idx + 1];
            let interpolated_val = sample_a * (1.0 - frac) + sample_b * frac;

            for c in 0..channels {
                if write_idx + c < output.len() {
                    output[write_idx + c] = interpolated_val;
                }
            }

            write_idx += channels;
            *phase += step;
        }
    }

    /// Removes processed samples from the internal buffer and updates the phase.
    ///
    /// Eliminates samples that have already been written to the hardware device
    /// and adjusts the phase accumulator to be relative to the new buffer start.
    /// This maintains memory efficiency by discarding consumed samples.
    ///
    /// # Arguments
    ///
    /// * `buffer` - Internal buffer to prune
    /// * `phase` - Phase accumulator to adjust relative to the new buffer position
    fn prune_buffer(buffer: &mut Vec<f32>, phase: &mut f32) {
        let samples_consumed = *phase as usize;

        if samples_consumed > 0 {
            if samples_consumed < buffer.len() {
                buffer.drain(0..samples_consumed);
                *phase -= samples_consumed as f32;
            } else {
                buffer.clear();
                *phase = 0.0;
            }
        }
    }
    fn generate_output_stream(
        device: cpal::Device,
        config: cpal::StreamConfig,
        rx: Arc<Mutex<Receiver<Vec<f32>>>>,
        sample_rate: u32,
        output_rate: u32,
        output_channels: usize,
        logger: Logger,
    ) -> Result<cpal::Stream, Error> {
        let step = sample_rate as f32 / output_rate as f32;
        let mut internal_buffer = Vec::new();
        let mut phase = 0.0;
        device
            .build_output_stream(
                &config,
                move |output_data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    Self::fetch_new_samples(&rx, &mut internal_buffer);

                    Self::process_audio_frame(
                        output_data,
                        &internal_buffer,
                        &mut phase,
                        step,
                        output_channels,
                    );

                    Self::prune_buffer(&mut internal_buffer, &mut phase);
                },
                move |err| {
                    logger.error(&format!("{}", Error::MapError(err.to_string())));
                },
                None,
            )
            .map_err(|e| Error::MapError(e.to_string()))
    }
}
fn setup_output_device() -> Result<(cpal::Device, cpal::StreamConfig, usize, u32), Error> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(Error::MissingOutputDeviceError)?;
    let default_config = device
        .default_output_config()
        .map_err(|e| Error::MapError(e.to_string()))?;

    let channels = default_config.channels() as usize;
    let rate = default_config.sample_rate();
    let config = default_config.into();

    Ok((device, config, channels, rate))
}
