use crate::media::audio::AudioError as Error;
use crate::media::audio_handler::AudioFrame;
use audiopus::{Application, Channels, SampleRate, coder::Encoder};

pub struct AudioEncoder {
    encoder: Encoder,
}

impl AudioEncoder {
    pub fn new() -> Result<Self, Error> {
        let encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)
            .map_err(|_| Error::EncoderInitializationError)?;

        Ok(Self { encoder })
    }

    pub fn encode(&mut self, audio_frame: AudioFrame) -> Result<Vec<u8>, Error> {
        let input_samples = audio_frame.data;
        let mut buff = vec![0u8; 1500];
        let len = self
            .encoder
            .encode_float(&input_samples, &mut buff)
            .map_err(|e| Error::EncodingError(e.to_string()))?;

        Ok(buff[..len].to_vec())
    }
}
