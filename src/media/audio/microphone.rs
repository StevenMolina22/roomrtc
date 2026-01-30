use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crate::logger::Logger;
use crate::config::Config;
use crate::clock::Clock;
use crate::media::audio_handler::AudioFrame;
use crate::media::audio::error::AudioError as Error;
use crate::media::audio::ring_buffer::AudioRingBuffer;

/// Microphone capture and audio processing component.
/// 
/// Handles audio input from the system's default microphone device, performing
/// real-time resampling to 48kHz (OPUS standard), mono conversion, and frame
/// buffering for codec processing.
/// 
/// # Features
/// - Automatic sample rate conversion to 48kHz
/// - Multi-channel to mono conversion
/// - Linear interpolation resampling
/// - Mute/unmute functionality
/// - Thread-safe mute state management
pub struct Microphone {
    clock: Arc<Clock>,
    config: Arc<Config>,
    logger: Logger,
    pub muted: Arc<AtomicBool>, 
    stream: Arc<Mutex<Option<cpal::Stream>>>,
}

impl Microphone {
    /// Creates a new Microphone instance.
    /// 
    /// # Arguments
    /// 
    /// * `clock` - Shared clock for timestamping audio frames
    /// * `config` - Application configuration
    /// * `logger` - Logger instance for diagnostics
    /// 
    /// # Returns
    /// 
    /// A new `Microphone` instance in stopped state
    pub fn new(clock: Arc<Clock>, config: Arc<Config>, logger: Logger) -> Self {
        Self {
            clock,
            config,
            logger,
            muted: Arc::new(AtomicBool::new(false)),
            stream: Arc::new(Mutex::new(None)),
        }
    }

    /// Toggles the microphone mute state.
    /// 
    /// When muted, audio capture continues but frames are not sent to the output channel.
    pub fn toggle_mute(&self) {
        let current = self.muted.load(Ordering::Relaxed);
        self.muted.store(!current, Ordering::Relaxed);
        self.logger.info(&format!("Microphone muted: {}", !current));
    }
    
    /// Checks whether the microphone is currently muted.
    /// 
    /// # Returns
    /// 
    /// `true` if muted, `false` otherwise
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    /// Starts the microphone capture stream.
    /// 
    /// Initializes the audio input device, configures resampling to 48kHz mono,
    /// and begins capturing audio frames. Captured audio is processed in real-time
    /// and sent through the returned channel.
    /// 
    /// # Returns
    /// 
    /// A `Receiver<AudioFrame>` channel that will receive processed audio frames,
    /// or an `Error` if the device cannot be initialized.
    /// 
    /// # Errors
    /// 
    /// - `Error::InputDeviceError` - No input device available
    /// - `Error::MapError` - Stream configuration or initialization failed
    pub fn start(&mut self) -> Result<Receiver<AudioFrame>, Error> {
        let (tx, rx) = mpsc::channel();
        let clock = self.clock.clone();
        let logger = self.logger.clone();
        let muted_flag = self.muted.clone();

        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(Error::InputDeviceError)?;
        let default_config = device.default_input_config().map_err(|e| Error::MapError(e.to_string()))?;
        
        let input_channels = default_config.channels() as usize;
        let input_rate = default_config.sample_rate();

        let sample_rate = self.config.media.audio_sample_rate;
        let opus_frame_size = self.config.media.audio_frame_size;
        
        logger.info(&format!("Micrófono nativo: {}Hz, {} canales. Resampleando a {}Hz.", 
            input_rate, input_channels, sample_rate));

        let config: cpal::StreamConfig = default_config.into();

        let mut ring_buffer = AudioRingBuffer::new(opus_frame_size * 10);
        let mut phase = 0.0;
        let step = input_rate as f32 / sample_rate as f32;

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if muted_flag.load(Ordering::Relaxed) { return; }

                let mono_data = Self::convert_to_mono(data, input_channels);
                let resampled_data = Self::resample_linear(&mono_data, &mut phase, step);
                ring_buffer.push(&resampled_data);

                while let Some(chunk) = ring_buffer.pop_chunk(opus_frame_size) {
                    let timestamp = clock.now();
                    let frame = AudioFrame::new(chunk, timestamp);
                    if tx.send(frame).is_err() { return; }
                }
            },
            move |err| {
                logger.error(&format!("Error crítico Micrófono: {}", err));
            },
            None
        ).map_err(|e| Error::MapError(e.to_string()))?;

        stream.play().map_err(|e| Error::MapError(e.to_string()))?;
        *self.stream.lock().map_err(|e| Error::MapError(e.to_string()))? = Some(stream);

        Ok(rx)
    }

    /// Stops the microphone capture stream.
    /// 
    /// Gracefully shuts down the audio input stream and releases system resources.
    /// 
    /// # Errors
    /// 
    /// - `Error::MapError` - Failed to acquire stream lock
    pub fn stop(&self) -> Result<(), Error> {
        let mut stream_lock = self.stream.lock().map_err(|e| Error::MapError(e.to_string()))?;
        if let Some(stream) = stream_lock.take() {
            drop(stream); 
            self.logger.info("Microphone stream stopped.");
        }
        Ok(())
    }

    /// Converts multi-channel audio (stereo) to mono.
    /// 
    /// # Arguments
    /// 
    /// * `data` - Interleaved multi-channel audio samples
    /// * `channels` - Number of channels in the input data
    /// 
    /// # Returns
    /// 
    /// A mono audio buffer (single channel)
    /// 
    /// # Channel Conversion Strategy
    /// 
    /// - 1 channel: Returns input as-is
    /// - 2 channels (stereo): Averages left and right channels
    /// - 3+ channels: Takes only the first channel
    fn convert_to_mono(data: &[f32], channels: usize) -> Vec<f32> {
        match channels {
            1 => data.to_vec(),
            2 => data.chunks(2)
                     .map(|chunk| (chunk[0] + chunk[1]) * 0.5)
                     .collect(),
            _ => data.chunks(channels)
                     .map(|c| c[0])
                     .collect(),
        }
    }

    /// Performs linear interpolation resampling to change the sample rate.
    /// 
    /// # Arguments
    /// 
    /// * `source` - Input audio samples at the original sample rate
    /// * `phase` - Current phase accumulator (maintains continuity between chunks)
    /// * `step` - Resampling step size (input_rate / target_rate)
    /// 
    /// # Returns
    /// 
    /// Resampled audio buffer at the target sample rate
    /// 
    /// # Note
    /// 
    /// The `phase` parameter is modified to maintain phase continuity across
    /// consecutive audio chunks, ensuring smooth resampling without discontinuities.
    fn resample_linear(source: &[f32], phase: &mut f32, step: f32) -> Vec<f32> {
        let mut resampled = Vec::with_capacity(source.len());
        let source_len = source.len() as f32;

        while *phase < (source_len - 1.0) {
            let idx = *phase as usize;
            let frac = *phase - idx as f32;

            let sample_a = source[idx];
            let sample_b = source[idx + 1];
            let new_sample = sample_a * (1.0 - frac) + sample_b * frac;

            resampled.push(new_sample);
            *phase += step;
        }

        *phase -= source_len;
        if *phase < 0.0 { *phase = 0.0; }

        resampled
    }
}