use audiopus::{coder::Decoder, Channels, SampleRate};
use crate::media::audio::AudioError as Error;

pub struct AudioDecoder {
    decoder: Decoder,
}

impl AudioDecoder {
    pub fn new() -> Result<Self, Error> {
        let decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).map_err(|_| Error::DecoderInitializationError)?;

        Ok(Self { decoder })
    }

    pub fn decode (&mut self, input_bytes: &[u8]) -> Result<Vec<f32>, Error> {
        let mut buff = vec![0.0f32; 5760];
        let len = self.decoder.decode_float(Some(input_bytes), &mut buff, false)
            .map_err(|e| Error::DecodingError(e.to_string()))?;

        buff.truncate(len);
        Ok(buff)
    }
}