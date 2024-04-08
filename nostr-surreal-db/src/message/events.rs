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
    pub raw_event: String,
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
                }

                let key = tag[0].as_bytes().to_vec();
                // only index key length 1
                // 0 will break the index separator, ignore
                if key.len() == 1 && key[0] != 0 {
                    let v;
                    // fixed length 32 e and p
                    if tag[0] == "e" || tag[0] == "p" {
                        let h = hex::decode(&tag[1])?;
                        if h.len() != 32 {
                            return Err(anyhow!("invalid e or p tag value"));
                        }
                        v = h;
                    } else {
                        v = tag[1].as_bytes().to_vec();
                        // 0 will break the index separator, ignore
                        // lmdb max_key_size 511 bytes
                        // we only index tag value length < 255
                        if v.contains(&0) || v.len() > 255 {
                            continue;
                        }
                    };
                    t.push((key, v));
                }
            }
        }

        let raw_event_string = serde_json::to_string(&raw_event)?;
        Ok(Event {
            id: raw_event.id, 

            sender: raw_event.sender,
            sig: raw_event.sig,

            created_at: raw_event.created_at,
            kind: raw_event.kind,
            tags: t,

            expiration,

            content: raw_event.content,
            raw_event: raw_event_string,
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

    pub fn match_many_filters(&self, filters: &[Filter]) -> bool {
        for f in filters {
            if self.match_filter(f) {
                return true;
            }
        }

        false
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
            "id": "54189d6c2c9281501b3da6fbb14fe0b5e3fbcd40c96b5eccb1279a30e6f3b612",
            "kind": 1,
            "sender": "017abf57d516b1ff7308ca3bd5650ea6a4674d469c7c5057b1d005fb13d218bfef",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f",
            "tags": [["t", "nostr"], ["t", ""], ["expiration", "1"], ["delegation", "8e0d3d3eb2881ec137a11debe736a9086715a8c8beeeda615780064d68bc25dd"]]
          }
        "#;
        let raw_event: EventOnWire = serde_json::from_str(note).unwrap();
        let event: Event = raw_event.try_into().unwrap();

        assert_eq!(event.tags.len(), 2);
        assert!(&event.expiration.is_some());

        let note = r#"
        {
            "content": "Good morning everyone",
            "created_at": 1680690006,
            "id": "8de461754e1d3aa11bd3efe00a6807c6754f3d2a4cb7710744857c34fb6eddb2",
            "kind": 1,
            "sender": "017abf57d516b1ff7308ca3bd5650ea6a4674d469c7c5057b1d005fb13d218bfef",
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
            "id": "b022dc994caa8d8708406a38c9f2068b6f4d8a62cbf7842c2ad07e390557c120",
            "kind": 1,
            "sender": "017abf57d516b1ff7308ca3bd5650ea6a4674d469c7c5057b1d005fb13d218bfef",
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
                "b022dc994caa8d8708406a38c9f2068b6f4d8a62cbf7842c2ad07e390557c120".to_string()
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
            "sender": "017abf57d516b1ff7308ca3bd5650ea6a4674d469c7c5057b1d005fb13d218bfef",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f"
          }
        "#;
        let raw_event: EventOnWire = serde_json::from_str(note)?;
        assert_eq!(&raw_event.content, "");
        assert_eq!(&raw_event.tags, &Vec::<Vec<String>>::new());
        Ok(())
    }
}

