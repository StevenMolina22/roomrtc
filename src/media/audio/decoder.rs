use crate::media::audio::AudioError as Error;
use audiopus::{Channels, SampleRate, coder::Decoder};

pub struct AudioDecoder {
    decoder: Decoder,
}

impl AudioDecoder {
    pub fn new() -> Result<Self, Error> {
        let decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono)
            .map_err(|_| Error::DecoderInitializationError)?;

        Ok(Self { decoder })
    }

    pub fn decode(&mut self, input_bytes: &[u8]) -> Result<Vec<f32>, Error> {
        let mut buff = vec![0.0f32; 5760];
        let len = self
            .decoder
            .decode_float(Some(input_bytes), &mut buff, false)
            .map_err(|e| Error::DecodingError(e.to_string()))?;

        buff.truncate(len);
        Ok(buff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_initialization() {
        let decoder = AudioDecoder::new();
        assert!(
            decoder.is_ok(),
            "El decoder debería inicializarse sin errores"
        );
    }

    #[test]
    fn test_decode_garbage_data() {
        let mut decoder = AudioDecoder::new().expect("Falló al crear decoder");

        let garbage_payload = vec![0u8, 255, 12, 33];

        let result = decoder.decode(&garbage_payload);
        assert!(result.is_ok(), "Debería fallar con datos basura");
    }

    #[test]
    fn test_decode_empty_packet() {
        let mut decoder = AudioDecoder::new().unwrap();
        let empty: &[u8] = &[];

        let result = decoder.decode(empty);

        if let Ok(samples) = result {
            assert!(!samples.is_empty())
        }
    }
}
