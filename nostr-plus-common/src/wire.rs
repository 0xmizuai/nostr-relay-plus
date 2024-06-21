use std::collections::HashMap;

use anyhow::{anyhow, Result};
use nostr_crypto::hash::sha256_hash_digests;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::types::Bytes32;

use crate::types::Timestamp;

use super::sender::Sender;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventOnWire {
    #[serde(with = "hex::serde")]
    pub id: Bytes32,
    pub sender: Sender,    
    pub created_at: Timestamp,
    pub kind: u16,
    
    #[serde(default)]
    pub tags: Vec<Vec<String>>,
    
    #[serde(default)]
    pub content: String,
    
    #[serde(with = "hex::serde")]
    pub sig: Vec<u8>,
}

impl EventOnWire {
    pub fn to_id_hash(&self) -> Bytes32 {
        let json = json!([
            0, 
            hex::encode(self.sender.to_bytes()),
            self.created_at,
            self.kind,
            self.tags,
            self.content
        ]);
        sha256_hash_digests(json.to_string().as_bytes())
    }

    pub fn verify(&self) -> Result<()> {
        let expected_hash = self.to_id_hash();
        if expected_hash != self.id {
            return Err(anyhow!("invalid event id"));
        }
        self.sender.validate_signature(expected_hash, &self.sig)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(transparent)]
pub struct _HexString {
    #[serde(with = "hex::serde")]
    hex: [u8; 32],
}

impl Into<Bytes32> for _HexString {
    fn into(self) -> Bytes32 {
        self.hex
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct FilterOnWire {
    pub ids: Vec<_HexString>,
    pub authors: Vec<Sender>,
    pub kinds: Vec<u16>,
    
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<u64>,

    #[serde(flatten)]
    pub tags: HashMap<String, Value>,
}

pub fn parse_filter_tags(raw_tags: HashMap<String, Value>) -> Result<
    HashMap< String, Vec<String> > 
> {
    let mut tags = HashMap::new();
    for (raw_key, raw_value) in raw_tags {
        if let Some(key) = raw_key.strip_prefix('#') {
            // only index key with len == 1
            if key.as_bytes().len() != 1 {
                continue;
            }

            let val = Vec::<String>::deserialize(&raw_value)?;
            let mut list = vec![];
            for s in val {
                if key == "e" || key == "p" {
                    let h = hex::decode(&s)?;
                    if h.len() != 32 {
                        return Err(anyhow!("invalid e or p tag value"));
                    }
                }

                list.push(s);
            }
            if !list.is_empty() {
                list.sort();
                list.dedup();
                tags.insert(key.to_string(), list.into());
            }
        }
    }
    Ok(tags)
}
