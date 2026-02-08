use crate::sctp_transport::data_channel::DataChannelType;

pub enum DCEPMessage {
    DataChannelOpen {
        channel_type: DataChannelType,
        priority: u16,
        reliability_parameter: u32,
        label: String,
        protocol: String,
    },
    DataChannelAck,
}

impl DCEPMessage {
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::DataChannelOpen {
                channel_type,
                priority,
                reliability_parameter,
                label,
                protocol,
            } => {
                let label_len = label.len() as u16;
                let protocol_len = protocol.len() as u16;

                let mut bytes = Vec::with_capacity(12 + label_len as usize + protocol_len as usize);

                bytes.push(0x03);
                bytes.push(channel_type.to_u8());
                bytes.extend_from_slice(&priority.to_be_bytes());
                bytes.extend_from_slice(&reliability_parameter.to_be_bytes());
                bytes.extend_from_slice(&label_len.to_be_bytes());
                bytes.extend_from_slice(&protocol_len.to_be_bytes());
                bytes.extend_from_slice(label.as_bytes());
                bytes.extend_from_slice(protocol.as_bytes());

                bytes
            }
            Self::DataChannelAck => vec![0x02],
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes.first()? {
            0x03 => {
                if bytes.len() < 12 {
                    return None;
                }

                let label_len = u16::from_be_bytes(bytes[8..10].try_into().ok()?) as usize;
                let protocol_len = u16::from_be_bytes(bytes[10..12].try_into().ok()?) as usize;

                let label_start = 12;
                let label_end = label_start + label_len;
                let protocol_end = label_end + protocol_len;

                if bytes.len() < protocol_end {
                    return None;
                }

                Some(Self::DataChannelOpen {
                    channel_type: DataChannelType::try_from(bytes[1]).ok()?,
                    priority: u16::from_be_bytes(bytes[2..4].try_into().ok()?),
                    reliability_parameter: u32::from_be_bytes(bytes[4..8].try_into().ok()?),
                    label: String::from_utf8(bytes[label_start..label_end].to_vec()).ok()?,
                    protocol: String::from_utf8(bytes[label_end..protocol_end].to_vec()).ok()?,
                })
            }
            0x02 => Some(Self::DataChannelAck),
            _ => None,
        }
    }
}
