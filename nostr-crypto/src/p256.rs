use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use base64::STANDARD;
use p256::{ecdsa::{signature::hazmat::PrehashVerifier, Signature, VerifyingKey}, EncodedPoint};
use serde::{Deserialize, Serialize};

use crate::hash::sha256_hash_digests;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct P256PublicKey { pub x: [u8; 32], pub y: [u8; 32] }

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PasskeySignature {
    pub address: Address,
    pub client_data: String,
    pub auth_data: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PasskeyClientData {
    r#type: String,
    challenge: String,
    origin: String,
    #[serde(rename = "crossOrigin")]
    cross_origin: Option<bool>,
}

impl PasskeyClientData {
    pub fn new(
        challenge: String, 
        cross_origin: bool,
        origin: String, 
        r#type: String
    ) -> Self {
        Self { challenge, cross_origin: Some(cross_origin), origin, r#type }
    }
}

impl PasskeySignature {
    pub fn validate(&self, server_nonce: u64, passkey_origin: &[String]) -> Result<()> {
        let client_data: PasskeyClientData = serde_json::from_str(&self.client_data)?;
        let challenge = base64::decode_config(&client_data.challenge, STANDARD)?;
        let challenge = String::from_utf8(challenge)?;
        let expected_nonce = format!("{}", server_nonce);

        if challenge != expected_nonce {
            return Err(anyhow!("nonce error"));
        }

        let is_valid_origin = passkey_origin
            .iter()
            .position(|origin| &client_data.origin == origin);

        if is_valid_origin.is_some() {
            Ok(())
        } else {
            Err(anyhow!("origin error"))
        }
    }
}


impl P256PublicKey {
    pub fn to_public_key(&self) -> VerifyingKey {
        let point = EncodedPoint::from_bytes(&[
            vec![4], self.x.to_vec(), self.y.to_vec()].concat()).unwrap();
        
        VerifyingKey::from_encoded_point(&point).unwrap()
    }

    pub fn verify_signature(&self, signature: &PasskeySignature, server_nonce: u64, passkey_origin: &[String]) -> Result<()> {
        signature.validate(server_nonce, passkey_origin)?;
        let public_key = self.to_public_key();
        let client_data_hash = sha256_hash_digests(signature.client_data.as_bytes());
        let sig_hash_preimage = [signature.auth_data.clone(), client_data_hash.to_vec()].concat();
        let sig_hash = sha256_hash_digests(&sig_hash_preimage);
        let sig = Signature::from_der(&signature.signature[..])?;
        public_key.verify_prehash(&sig_hash, &sig)?;
        Ok(())
    }
}

impl TryFrom<(&str, &str)> for P256PublicKey {
    type Error = anyhow::Error;

    fn try_from(value: (&str, &str)) -> Result<Self, Self::Error> {
        let x: U256 = value.0.parse()?;
        let y: U256 = value.1.parse()?;
        Ok(P256PublicKey { x: x.to_be_bytes(), y: y.to_be_bytes() })
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::address;

    use super::*;

    #[test]
    fn verify_passkey() {
        let pub_key: P256PublicKey = (
            "0x9e2c79fc9be2333225835eb0e782c9d8f7fdda130338b18c73049c7551f77b25",
            "0x29f2c5f35f0718285170cd96fa4267f45d7bbffce3536d5c5a37fbac19343219"
        ).try_into().unwrap();

        let sig = PasskeySignature {
            address: address!("0000000000000000000000000000000000000000"),
            client_data: String::from("{\"type\":\"webauthn.get\",\"challenge\":\"VkZSMDJFVFl3ZnI0ZUx6MEE4ZS8rczJidHVVbE9sRmxSLzFCOWszVmViQT0\",\"origin\":\"http://localhost:3000\",\"crossOrigin\":false}"),
            auth_data: hex::decode("49960de5880e8c687434170f6476605b8fe4aeb9a28632c7995cf3ba831d97631d00000000").unwrap(),
            signature: hex::decode("3046022100fe3a2db69c1a345838d4a57b319d6ad6e53d924d9a5897f8050d2f582b493fb1022100800f47ed39e41f5cf90b8cabe3a5fa234ffd72e7636c926901c40009d41be576").unwrap().try_into().unwrap()
        };
        
        println!("{:?}", pub_key.verify_signature(&sig, 0, &["http://localhost:3000".to_string()]));
    }
}
