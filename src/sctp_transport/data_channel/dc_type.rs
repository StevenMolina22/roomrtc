use sctp_proto::ReliabilityType;
use std::convert::TryFrom;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
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
    /// Returns the DCEP wire value for this data channel type.
    #[must_use]
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    /// Returns whether messages are delivered in order for this type.
    #[must_use]
    pub fn ordered(&self) -> bool {
        match self {
            DataChannelType::Reliable => true,
        }
    }

    /// Returns the SCTP reliability mode associated with this type.
    #[must_use]
    pub fn reliability_type(&self) -> ReliabilityType {
        match self {
            DataChannelType::Reliable => ReliabilityType::Reliable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sctp_proto::ReliabilityType;

    #[test]
    fn test_conversion_from_u8_valid() {
        let val = 0x00;
        let result = DataChannelType::try_from(val);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), DataChannelType::Reliable);
    }

    #[test]
    fn test_conversion_from_u8_invalid() {
        let val = 0x01; // Valor no definido
        let result = DataChannelType::try_from(val);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_u8() {
        let dc_type = DataChannelType::Reliable;
        assert_eq!(dc_type.to_u8(), 0x00);
    }

    #[test]
    fn test_ordered_flag() {
        let dc_type = DataChannelType::Reliable;
        assert!(dc_type.ordered());
    }

    #[test]
    fn test_reliability_type() {
        let dc_type = DataChannelType::Reliable;
        assert_eq!(dc_type.reliability_type(), ReliabilityType::Reliable);
    }
}
