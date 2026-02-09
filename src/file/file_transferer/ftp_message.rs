use crate::file::file_metadata::FileMetadata;

pub enum FTPMessage {
    FileOffer {
        offer_id: u32,
        file_metadata: FileMetadata,
    },
    AcceptFile,
    RejectFile,
    FileChunk {
        payload: Vec<u8>,
    },
    EndOfFile,
}

impl FTPMessage {
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::FileOffer {
                offer_id,
                file_metadata,
            } => {
                let mut bytes = vec![0x01];
                bytes.extend_from_slice(&offer_id.to_be_bytes());
                bytes.extend_from_slice(&file_metadata.to_bytes());
                bytes
            }
            Self::AcceptFile => vec![0x02],
            Self::RejectFile => vec![0x03],
            Self::FileChunk { payload } => {
                let mut bytes = vec![0x04];
                bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                bytes.extend_from_slice(payload);
                bytes
            }
            Self::EndOfFile => vec![0x05],
        }
    }

    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes.first()? {
            0x01 if bytes.len() >= 14 => Some(Self::FileOffer {
                offer_id: u32::from_be_bytes(bytes[1..5].try_into().ok()?),
                file_metadata: FileMetadata::from_bytes(&bytes[5..])?,
            }),
            0x02 => Some(Self::AcceptFile),
            0x03 => Some(Self::RejectFile),
            0x04 if bytes.len() >= 5 => {
                let payload_len = u32::from_be_bytes(bytes[1..5].try_into().ok()?) as usize;
                if bytes.len() < 5 + payload_len {
                    return None;
                }
                let payload = bytes[5..5 + payload_len].to_vec();
                Some(Self::FileChunk { payload })
            }
            0x05 => Some(Self::EndOfFile),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FTPMessage;
    use crate::file::file_metadata::FileMetadata;

    #[test]
    fn file_offer_roundtrip() {
        let meta = FileMetadata {
            size: 12345,
            name: "example.txt".to_string(),
        };

        let msg = FTPMessage::FileOffer {
            offer_id: 0xDEAD_BEEF_u32,
            file_metadata: meta.clone(),
        };

        let bytes = msg.to_bytes();
        let parsed = FTPMessage::from_bytes(&bytes).expect("should parse FileOffer");

        match parsed {
            FTPMessage::FileOffer {
                offer_id,
                file_metadata,
            } => {
                assert_eq!(offer_id, 0xDEAD_BEEF_u32);
                assert_eq!(file_metadata.size, meta.size);
                assert_eq!(file_metadata.name, meta.name);
            }
            _ => panic!("expected FileOffer variant"),
        }
    }

    #[test]
    fn accept_reject_eof_variants() {
        let accept = FTPMessage::AcceptFile;
        let reject = FTPMessage::RejectFile;
        let eof = FTPMessage::EndOfFile;

        assert!(matches!(
            FTPMessage::from_bytes(&accept.to_bytes()),
            Some(FTPMessage::AcceptFile)
        ));
        assert!(matches!(
            FTPMessage::from_bytes(&reject.to_bytes()),
            Some(FTPMessage::RejectFile)
        ));
        assert!(matches!(
            FTPMessage::from_bytes(&eof.to_bytes()),
            Some(FTPMessage::EndOfFile)
        ));
    }

    #[test]
    fn file_chunk_roundtrip_and_malformed() {
        let payload = vec![1u8, 2, 3, 4, 5];
        let chunk = FTPMessage::FileChunk { payload };

        let bytes = chunk.to_bytes();
        let parsed = FTPMessage::from_bytes(&bytes).expect("should parse FileChunk");

        match parsed {
            FTPMessage::FileChunk { payload } => assert_eq!(payload, payload),
            _ => panic!("expected FileChunk variant"),
        }

        let mut bad = vec![0x04];
        bad.extend_from_slice(&(10u32.to_be_bytes()));
        bad.extend_from_slice(&[1u8, 2, 3]);
        assert!(FTPMessage::from_bytes(&bad).is_none());
    }

    #[test]
    fn malformed_file_offer() {
        let bytes = vec![0x01, 0x00, 0x00, 0x00];
        assert!(FTPMessage::from_bytes(&bytes).is_none());
    }
}
