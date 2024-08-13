use anyhow::{anyhow, Result};
use num::traits::ToBytes;
use std::convert::TryFrom;

const QUERY_SUBSCRIPTION: u8 = 0x01;
const REPLY_SUBSCRIPTION: u8 = 0x02;

#[repr(u8)]
#[derive(Debug, PartialEq, Eq)]
pub enum BinaryMessage {
    QuerySubscription = QUERY_SUBSCRIPTION,
    ReplySubscription(u8) = REPLY_SUBSCRIPTION,
}

impl BinaryMessage {
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            BinaryMessage::QuerySubscription => vec![QUERY_SUBSCRIPTION],
            BinaryMessage::ReplySubscription(count) => {
                let mut bytes = vec![REPLY_SUBSCRIPTION];
                bytes.extend_from_slice(&count.to_be_bytes());
                bytes
            }
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            return Err(anyhow!("Empty message"));
        }

        match BinaryMessage::try_from(bytes[0]) {
            Ok(BinaryMessage::QuerySubscription) => Ok(BinaryMessage::QuerySubscription),
            Ok(BinaryMessage::ReplySubscription(_)) => {
                if bytes.len() != 2 {
                    return Err(anyhow!("Payload not 1 byte long"));
                }
                let count = u8::from_be_bytes(bytes[1..2].try_into()?);
                Ok(BinaryMessage::ReplySubscription(count))
            }
            Err(err) => Err(err.into()),
        }
    }
}

impl TryFrom<u8> for BinaryMessage {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            QUERY_SUBSCRIPTION => Ok(BinaryMessage::QuerySubscription),
            REPLY_SUBSCRIPTION => Ok(BinaryMessage::ReplySubscription(0)),
            _ => Err(anyhow!("Invalid binary code")),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::binary_protocol::BinaryMessage;

    #[test]
    fn test_to_bytes() {
        let query = BinaryMessage::QuerySubscription;
        let bytes = query.to_bytes();
        assert_eq!(bytes, [0x01]);

        let reply = BinaryMessage::ReplySubscription(8);
        let bytes = reply.to_bytes();
        assert_eq!(bytes, [0x02, 0x08]);
    }

    #[test]
    fn test_from_bytes() {
        let bytes = [0x01];
        let result = BinaryMessage::from_bytes(&bytes).unwrap();
        assert!(matches!(result, BinaryMessage::QuerySubscription));

        let bytes = [0x02, 0xff];
        let result = BinaryMessage::from_bytes(&bytes).unwrap();
        let expected = BinaryMessage::ReplySubscription(255);
        assert_eq!(result, expected);

        let bytes = [0x03];
        let maybe_result = BinaryMessage::from_bytes(&bytes);
        assert!(maybe_result.is_err());
    }
}
