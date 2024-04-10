use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::get_current_timstamp;
use crate::types::{Bytes32, Tags, Timestamp};

use super::filter::Filter;
use super::sender::Sender;
use super::wire::EventOnWire;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(with = "hex::serde")]
    pub id: Bytes32,
    pub sender: Sender,
    #[serde(with = "hex::serde")]
    pub sig: Vec<u8>,

    pub created_at: Timestamp,
    pub kind: u16,
    pub tags: Tags,

    pub expiration: Option<Timestamp>,

    pub content: String,
}

impl TryFrom<EventOnWire> for Event {
    type Error = anyhow::Error;

    fn try_from(raw_event: EventOnWire) -> Result<Self, Self::Error> {
        
        // assert the id
        let expected_id = raw_event.to_id_hash();
        if raw_event.id != expected_id {
            return Err(anyhow!(
                "invalid event id, expected {}, got {}", 
                hex::encode(expected_id), 
                hex::encode(raw_event.id)
            ));
        }

        // sanity check
        let now = get_current_timstamp();
        if raw_event.created_at > now {
            return Err(anyhow!("invalid event created_at"));
        }

        // parse the tags
        let mut t = vec![];
        let mut expiration = None;
        for tag in &raw_event.tags {
            if tag.len() > 1 {
                if tag[0] == "expiration" {
                    expiration = Some(
                        u64::from_str_radix(&tag[1], 10)
                            .map_err(|_| anyhow!("Invalid expiration time"))?,
                    );

                    continue;
                }

                if tag[0] == "e" || tag[0] == "p" {
                    let h = hex::decode(&tag[1])?;
                    if h.len() != 32 {
                        return Err(anyhow!("invalid e or p tag value"));
                    }
                }

                if tag[0].len() > 1 {
                    continue;
                }
                
                t.push((tag[0].clone(), tag[1].clone()));
            }
        }

        Ok(Event {
            id: raw_event.id, 

            sender: raw_event.sender,
            sig: raw_event.sig,

            created_at: raw_event.created_at,
            kind: raw_event.kind,
            tags: t,

            expiration,

            content: raw_event.content,
        })
    }
}

impl Event {
    pub fn match_filter(&self, filter: &Filter) -> bool {
        if filter.match_id(&self.id) &&
            filter.match_author(&self.sender) &&
            filter.match_kind(self.kind) &&
            filter.match_tag(&self.tags)
        {
            return true;
        }

        false
    }

    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn index_event() {
        let note = r#"
        {
            "content": "Good morning everyone 😃",
            "created_at": 1680690006,
            "id": "eb91d677875863c5719e54c8f2841930616464d06f9c791e808bf000d398906d",
            "kind": 1,
            "sender": "010000000000000000000000000000000000000000000000",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f",
            "tags": [["t", "nostr"], ["t", ""], ["expiration", "1"], ["delegation", "8e0d3d3eb2881ec137a11debe736a9086715a8c8beeeda615780064d68bc25dd"]]
          }
        "#;
        let raw_event: EventOnWire = serde_json::from_str(note).unwrap();
        let event: Event = raw_event.try_into().unwrap();

        assert_eq!(event.tags.len(), 3);
        assert!(&event.expiration.is_some());

        let note = r#"
        {
            "content": "Good morning everyone",
            "created_at": 1680690006,
            "id": "358cf658eeebe5ff34929853fe9bf19756bb03bf948541f9c0bfee2cff3f9c8e",
            "kind": 1,
            "sender": "010000000000000000000000000000000000000000000000",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f",
            "tags": []
          }
        "#;
        
        let raw_event: EventOnWire = serde_json::from_str(note).unwrap();
        let event: Event = raw_event.try_into().unwrap();
        assert_eq!(event.tags.len(), 0);
        assert!(&event.expiration.is_none());
    }

    #[test]
    fn string() -> Result<()> {
        let note = r#"
        {
            "content": "Good morning everyone",
            "created_at": 1680690006,
            "id": "f98efa15d9d2c8243f0ddf5dbcc6ca94006fe788cdf0cb1c2547d11efcef551d",
            "kind": 1,
            "sender": "010000000000000000000000000000000000000000000000",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f",
            "tags": [["t", "nostr"]]
          }
        "#;

        let raw_event: EventOnWire = serde_json::from_str(note).unwrap();
        
        assert_eq!(
            hex::encode(raw_event.to_id_hash()),
            hex::encode(raw_event.id)
        );

        let json: String = serde_json::to_string(&raw_event).unwrap();
        let val: Value = serde_json::from_str(&json)?;
        assert_eq!(
            val["id"],
            Value::String(
                "f98efa15d9d2c8243f0ddf5dbcc6ca94006fe788cdf0cb1c2547d11efcef551d".to_string()
            )
        );
        Ok(())
    }

    #[test]
    fn default() -> Result<()> {
        let note = r#"
        {
            "created_at": 1680690006,
            "id": "67ea1eab7c58da50a2487c0e80f005ec927aca6d929e90cbb91df4b0878c5ffc",
            "kind": 1,
            "sender": "010000000000000000000000000000000000000000000000",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f"
          }
        "#;
        let raw_event: EventOnWire = serde_json::from_str(note)?;
        assert_eq!(&raw_event.content, "");
        assert_eq!(&raw_event.tags, &Vec::<Vec<String>>::new());
        Ok(())
    }
}

