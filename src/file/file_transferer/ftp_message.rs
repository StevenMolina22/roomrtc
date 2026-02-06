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

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes.get(0)? {
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
