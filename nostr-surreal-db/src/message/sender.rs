use alloy_primitives::Address;
use num::traits::ToBytes;
use serde::{Deserialize, Serialize};

use crate::types::Bytes32;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sender {
    SchnorrPubKey(Bytes32),
    EoaAddress(Address),
    ContractAddress((u32, Address)), // chainid + address
    Ens(Bytes32),
}

impl Sender {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();
        match self {
            Sender::SchnorrPubKey(bytes) => {
                result.extend_from_slice(&0u32.to_le_bytes());
                result.extend_from_slice(bytes);
            },
            Sender::EoaAddress(address) => {
                result.extend_from_slice(&1u32.to_le_bytes());
                result.extend_from_slice(address.0.as_slice());
            },
            Sender::ContractAddress((chain_id, address)) => {
                result.extend_from_slice(&2u32.to_le_bytes());
                result.extend_from_slice(&chain_id.to_le_bytes());
                result.extend_from_slice(address.0.as_slice());
            },
            Sender::Ens(bytes) => {
                result.extend_from_slice(&3u32.to_le_bytes());
                result.extend_from_slice(bytes);
            },
        }

        result
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let type_prefix = u32::from_le_bytes(bytes[0..4].try_into().expect("length should match"));
        let pure_content = &bytes[4..];

        match type_prefix {
            0 => Some(Sender::SchnorrPubKey(pure_content[0..32].try_into().expect("length should match"))),
            1 => Some(Sender::EoaAddress(Address::from_slice(&pure_content[0..20]))),
            2 => {
                let chain_id = u32::from_le_bytes(pure_content[0..4].try_into().expect("length should match"));
                Some(Sender::ContractAddress((chain_id, Address::from_slice(&pure_content[4..24]))))
            },
            3 => Some(Sender::Ens(pure_content[0..32].try_into().expect("length should match"))),
            _ => None,
        }
    }

    pub fn validate_signature(&self, _hash: Bytes32, _signature: &[u8]) -> bool {
        match self {
            Sender::SchnorrPubKey(_) => true,
            Sender::EoaAddress(_) => true,
            Sender::ContractAddress((_, _)) => {
                // ?
                

                true
            },
            Sender::Ens(_) => unimplemented!(),
        }
    }
}

impl Serialize for Sender {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.to_bytes();
        serializer.serialize_str(&hex::encode(bytes))
    }
}

impl<'de> Deserialize<'de> for Sender {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex = String::deserialize(deserializer)?;
        let bytes = hex::decode(hex).map_err(|_| serde::de::Error::custom("Invalid hex string"))?;
        Sender::from_bytes(&bytes).ok_or_else(|| serde::de::Error::custom("invalid sender"))
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::address;

    use super::*;

    #[test]
    fn test_sender_serialization() {
        let sender = Sender::SchnorrPubKey([0; 32]);
        let serialized = serde_json::to_string(&sender).unwrap();
        assert_eq!(serialized, "\"000000000000000000000000000000000000000000000000000000000000000000000000\"");

        let sender = Sender::EoaAddress(address!("0000000000000000000000000000000000000000"));
        let serialized = serde_json::to_string(&sender).unwrap();
        assert_eq!(serialized, "\"010000000000000000000000000000000000000000000000\"");

        let sender = Sender::ContractAddress((1, address!("0000000000000000000000000000000000000000")));
        let serialized = serde_json::to_string(&sender).unwrap();
        assert_eq!(serialized, "\"02000000000000010000000000000000000000000000000000000000\"");

        let sender = Sender::Ens([0; 32]);
        let serialized = serde_json::to_string(&sender).unwrap();
        assert_eq!(serialized, "\"030000000000000000000000000000000000000000000000000000000000000000000000\"");
    }

    #[test]
    fn test_sender_deserialization() {
        let sender = Sender::SchnorrPubKey([0; 32]);
        let deserialized: Sender = serde_json::from_str("\"000000000000000000000000000000000000000000000000000000000000000000000000\"").unwrap();
        assert_eq!(deserialized, sender);

        let sender = Sender::EoaAddress(address!("4838B106FCe9647Bdf1E7877BF73cE8B0BAD5f97"));
        let deserialized: Sender = serde_json::from_str("\"010000004838B106FCe9647Bdf1E7877BF73cE8B0BAD5f97\"").unwrap();
        assert_eq!(deserialized, sender);

        let sender = Sender::ContractAddress((1, address!("0000000000000000000000000000000000000000")));
        let deserialized: Sender = serde_json::from_str("\"02000000010000000000000000000000000000000000000000000000\"").unwrap();
        assert_eq!(deserialized, sender);

        let sender = Sender::Ens([0; 32]);
        let deserialized: Sender = serde_json::from_str("\"030000000000000000000000000000000000000000000000000000000000000000000000\"").unwrap();
        assert_eq!(deserialized, sender);
    }
}