use sctp_proto::ReliabilityType;
use std::convert::TryFrom;

#[derive(Copy, Clone)]
pub enum DataChannelType {
    Reliable = 0x00,
}

impl TryFrom<u8> for DataChannelType {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::Reliable),
            _ => Err(()),
        }
    }
}

impl DataChannelType {
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    pub fn ordered(&self) -> bool {
        match self {
            DataChannelType::Reliable => true,
        }
    }

    pub fn reliability_type(&self) -> ReliabilityType {
        match self {
            DataChannelType::Reliable => ReliabilityType::Reliable,
        }
    }
}
