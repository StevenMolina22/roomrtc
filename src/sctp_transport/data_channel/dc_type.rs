use sctp_proto::ReliabilityType;
use std::convert::TryFrom;

#[derive(Copy, Clone)]
pub enum DataChannelType {
    Reliable = 0x00,
    ReliableUnordered = 0x80,
    // PartialReliableRexmit = 0x01,
    // PartialReliableRexmitUnordered = 0x81,
    // PartialReliableTimed = 0x02,
    // PartialReliableTimedUnordered = 0x82,
}

impl TryFrom<u8> for DataChannelType {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::Reliable),
            0x80 => Ok(Self::ReliableUnordered),
            // 0x01 => Ok(Self::PartialReliableRexmit),
            // 0x81 => Ok(Self::PartialReliableRexmitUnordered),
            // 0x02 => Ok(Self::PartialReliableTimed),
            // 0x82 => Ok(Self::PartialReliableTimedUnordered),
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
            DataChannelType::ReliableUnordered => false,
        }
    }

    pub fn reliability_type(&self) -> ReliabilityType {
        match self {
            DataChannelType::Reliable => ReliabilityType::Reliable,
            DataChannelType::ReliableUnordered => ReliabilityType::Reliable,
        }
    }
}
