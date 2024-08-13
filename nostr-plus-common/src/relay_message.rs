use crate::relay_event::RelayEvent;
use crate::relay_ok::RelayOk;
use serde::{
    de::{self, SeqAccess, Visitor},
    ser::{Error, SerializeSeq},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt::Formatter;
use crate::relay_auth::RelayAuth;

pub enum RelayMessage {
    Event(RelayEvent),
    Ok(RelayOk),
    EOSE,
    Closed,
    Notice,
    Auth(RelayAuth),
    // Internal/Non-protocol messages
    Disconnected,
    Binary(Vec<u8>),
}

impl Serialize for RelayMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            RelayMessage::Event(event) => {
                let mut seq = serializer.serialize_seq(Some(3))?;
                seq.serialize_element("EVENT")?;
                seq.serialize_element(&event.subscription_id)?;
                seq.serialize_element(&event.event)?;
                seq.end()
            }
            RelayMessage::Ok(ok_msg) => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                seq.serialize_element("OK")?;
                seq.serialize_element(&hex::encode(&ok_msg.event_id))?;
                seq.serialize_element(&ok_msg.accepted)?;
                seq.serialize_element(&ok_msg.message)?;
                seq.end()
            }
            RelayMessage::Auth(_)
            | RelayMessage::EOSE
            | RelayMessage::Closed
            | RelayMessage::Notice
            | RelayMessage::Disconnected
            | RelayMessage::Binary(_) => Err(Error::custom("Unsupported enum for serialization")),
        }
    }
}

struct RelayMessageVisitor;

impl<'de> Visitor<'de> for RelayMessageVisitor {
    type Value = RelayMessage;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("a sequence starting with an identifier string")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let identifier: &str = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;

        match identifier {
            "EVENT" => {
                let remaining: RelayEvent =
                    Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(RelayMessage::Event(remaining))
            }
            "OK" => {
                let remaining: RelayOk =
                    Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(RelayMessage::Ok(remaining))
            }
            "AUTH" => {
                let remaining: RelayAuth =
                    Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(RelayMessage::Auth(remaining))
            }
            _ => Err(de::Error::unknown_variant(identifier, &["EVENT", "OK"])),
        }
    }
}

impl<'de> Deserialize<'de> for RelayMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(RelayMessageVisitor)
    }
}

#[cfg(test)]
mod tests {
    use crate::relay_message::RelayMessage;

    #[test]
    fn test_deserialize_serialize_event() {
        let event = r#"["EVENT", "sub_id", {
            "id": "332747c0fab8a1a92def4b0937e177be6df4382ce6dd7724f86dc4710b7d4d7d",
            "sender": "000000000000000000000000000000000000000000000000000000000000000000000000",
            "created_at": 1680690006,
            "kind": 1,
            "tags": [["t", "nostr"], ["t", ""], ["expiration", "1"], ["delegation", "8e0d3d3eb2881ec137a11debe736a9086715a8c8beeeda615780064d68bc25dd"]],
            "content": "Hello!",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f"
        }]"#;

        // deserialize and check it's an EVENT message
        let message: RelayMessage = serde_json::from_str(event).unwrap();
        assert!(matches!(message, RelayMessage::Event(..)));

        // serialize and check it matches the original
        let event_ser = serde_json::to_string(&message).unwrap();
        let orig_event_no_spc: String = event.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(orig_event_no_spc, event_ser);
    }

    #[test]
    fn test_deserialize_serialize_ok() {
        let ok = r#"["OK","332747c0fab8a1a92def4b0937e177be6df4382ce6dd7724f86dc4710b7d4d7d",true,"msg"]"#;

        // deserialize and check it's an OK message
        let message: RelayMessage = serde_json::from_str(&ok).unwrap();
        assert!(matches!(message, RelayMessage::Ok(..)));

        // serialize and check it matches the original
        let ok_ser = serde_json::to_string(&message).unwrap();
        assert_eq!(ok, ok_ser);
    }
}
