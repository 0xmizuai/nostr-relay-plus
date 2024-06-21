use core::fmt;
use std::marker::PhantomData;

use serde::{
    de::{self, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::Value;

use nostr_plus_common::wire::{EventOnWire, FilterOnWire};
use nostr_surreal_db::message::subscription::Subscription;

#[derive(Debug, Clone)]
pub enum IncomingMessage {
    Event(EventOnWire),
    Close(String),
    Req(Subscription),
    Auth(EventOnWire),
    Count(Subscription),
    Unknown(String, Vec<Value>),
}

struct MessageVisitor(PhantomData<()>);

impl<'de> Visitor<'de> for MessageVisitor {
    type Value = IncomingMessage;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("sequence")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let t: &str = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        match t {
            "EVENT" => Ok(IncomingMessage::Event(
                seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?,
            )),
            "CLOSE" => Ok(IncomingMessage::Close(
                seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?,
            )),
            "REQ" => {
                let t = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let r = Vec::<FilterOnWire>::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(IncomingMessage::Req(Subscription { id: t, filters: r }))
            }
            "AUTH" => Ok(IncomingMessage::Auth(
                seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?,
            )),
            "COUNT" => {
                let t = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let r = Vec::<FilterOnWire>::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(IncomingMessage::Count(Subscription { id: t, filters: r }))
            }
            _ => Ok(IncomingMessage::Unknown(
                t.to_string(),
                Vec::<Value>::deserialize(de::value::SeqAccessDeserializer::new(seq))?,
            )),
        }
    }
}

impl<'de> Deserialize<'de> for IncomingMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(MessageVisitor(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incoming() {
        let events = r#"["EVENT", {
            "content": "Good morning everyone 😃",
            "created_at": 1680690006,
            "id": "332747c0fab8a1a92def4b0937e177be6df4382ce6dd7724f86dc4710b7d4d7d",
            "kind": 1,
            "sender": "000000000000000000000000000000000000000000000000000000000000000000000000",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f",
            "tags": [["t", "nostr"], ["t", ""], ["expiration", "1"], ["delegation", "8e0d3d3eb2881ec137a11debe736a9086715a8c8beeeda615780064d68bc25dd"]]
        }]"#;

        let m: IncomingMessage = serde_json::from_str(events).unwrap();
        println!("{:?}", m);

        let auth = r#"["AUTH", {
            "content": "Good morning everyone 😃",
            "created_at": 1680690006,
            "id": "332747c0fab8a1a92def4b0937e177be6df4382ce6dd7724f86dc4710b7d4d7d",
            "kind": 1,
            "sender": "000000000000000000000000000000000000000000000000000000000000000000000000",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f",
            "tags": [["t", "nostr"], ["t", ""], ["expiration", "1"], ["delegation", "8e0d3d3eb2881ec137a11debe736a9086715a8c8beeeda615780064d68bc25dd"]]
        }]"#;
        let m: IncomingMessage = serde_json::from_str(auth).unwrap();
        println!("{:?}", m);

        let req = r#"["REQ", "sub_id1", {}]"#;
        let m: IncomingMessage = serde_json::from_str(req).unwrap();
        println!("{:?}", m);

        let count = r#"["COUNT", "sub_id1"]"#;
        let m: IncomingMessage = serde_json::from_str(count).unwrap();
        println!("{:?}", m);

        let close = r#"["CLOSE", "sub_id1"]"#;
        let m: IncomingMessage = serde_json::from_str(close).unwrap();
        println!("{:?}", m);

        let unknown = r#"["REQ1", "sub_id1", {}]"#;
        let m: IncomingMessage = serde_json::from_str(unknown).unwrap();
        println!("{:?}", m);
    }
}
