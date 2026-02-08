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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sctp_transport::data_channel::DataChannelType;

    #[test]
    fn test_ack_message_serialization() {
        let msg = DCEPMessage::DataChannelAck;
        let bytes = msg.to_bytes();
        assert_eq!(bytes, vec![0x02]);
    }

    #[test]
    fn test_ack_message_deserialization() {
        let bytes = vec![0x02];
        let msg = DCEPMessage::from_bytes(&bytes).expect("Should deserialize Ack");

        match msg {
            DCEPMessage::DataChannelAck => {}
            _ => unreachable!("Wrong message type"),
        }
    }

    #[test]
    fn test_open_message_serialization() {
        let msg = DCEPMessage::DataChannelOpen {
            channel_type: DataChannelType::Reliable,
            priority: 0,
            reliability_parameter: 0,
            label: "test".to_string(),
            protocol: "chat".to_string(),
        };

        let bytes = msg.to_bytes();

        assert_eq!(bytes[0], 0x03);
        assert_eq!(bytes[1], 0x00);
        assert_eq!(bytes[2..4], [0, 0]);
        assert_eq!(bytes[4..8], [0, 0, 0, 0]);
        assert_eq!(bytes[8..10], [0, 4]);
        assert_eq!(bytes[10..12], [0, 4]);
    }

    #[test]
    fn test_open_message_deserialization() {
        let mut bytes = vec![
            0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00,
        ];
        bytes.extend_from_slice(b"Label");

        let msg = DCEPMessage::from_bytes(&bytes).expect("Should deserialize Open");

        match msg {
            DCEPMessage::DataChannelOpen {
                label, protocol, ..
            } => {
                assert_eq!(label, "Label");
                assert_eq!(protocol, "");
            }
            _ => unreachable!("Wrong message type"),
        }
    }

    #[test]
    fn test_deserialization_invalid_length() {
        let bytes = vec![0x03, 0x00];
        assert!(DCEPMessage::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_deserialization_wrong_type() {
        let bytes = vec![0xFF];
        assert!(DCEPMessage::from_bytes(&bytes).is_none());
    }
}
