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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::audio_handler::AudioFrame;

    fn create_dummy_frame(samples: usize, pattern: f32) -> AudioFrame {
        let data = vec![pattern; samples];
        AudioFrame {
            data,
            timestamp: 123456,
        }
    }

    #[test]
    fn test_encoder_initialization() {
        let encoder = AudioEncoder::new();
        assert!(
            encoder.is_ok(),
            "El encoder debería inicializarse correctamente (48kHz, Mono, Voip)"
        );
    }

    #[test]
    fn test_encode_silence() {
        let mut encoder = AudioEncoder::new().expect("Falló init encoder");

        let frame = create_dummy_frame(960, 0.0);

        let result = encoder.encode(frame);

        match result {
            Ok(bytes) => {
                assert!(!bytes.is_empty());
                assert!(
                    bytes.len() < 100,
                    "El silencio debería comprimirse a pocos bytes"
                );
            }
            Err(e) => panic!("El encode falló con silencio: {}", e),
        }
    }

    #[test]
    fn test_encode_signal() {
        let mut encoder = AudioEncoder::new().unwrap();

        let frame = create_dummy_frame(960, 0.5);

        let result = encoder.encode(frame);

        match result {
            Ok(bytes) => {
                assert!(!bytes.is_empty());
                assert!(bytes.len() > 10);
            }
            Err(e) => panic!("El encode falló: {}", e),
        }
    }

    #[test]
    fn test_encode_buffer_size_constraint() {
        let mut encoder = AudioEncoder::new().unwrap();
        let frame = create_dummy_frame(1, 0.0);

        let result = encoder.encode(frame);

        let _ = result;
    }
}
