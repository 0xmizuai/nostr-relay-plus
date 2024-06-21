use crate::types::SubscriptionId;
use crate::wire::EventOnWire;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RelayEvent {
    pub subscription_id: SubscriptionId,
    pub event: EventOnWire,
}

#[cfg(test)]
mod tests {
    use crate::relay_event::RelayEvent;
    use crate::sender::Sender;
    use crate::wire::EventOnWire;
    use crate::types::Bytes32;
    use nostr_crypto::schnorr_signer::SchnorrSigner;

    #[test]
    fn test_serialize_deserialize() {
        let subscription_id = "sub_id".to_string();
        let event = EventOnWire {
            id: Bytes32::default(),
            sender: Sender::SchnorrPubKey(
                SchnorrSigner::from_bytes(&[1; 32])
                    .unwrap()
                    .private
                    .verifying_key()
                    .to_bytes()
                    .into(),
            ),
            created_at: 0,
            kind: 0,
            tags: vec![],
            content: "".to_string(),
            sig: vec![],
        };

        let relay_event = RelayEvent {
            subscription_id,
            event,
        };

        let wire_str = serde_json::to_string(&relay_event).unwrap();
        println!("GERMANO: {}", wire_str);
        let relay_event_from_str: RelayEvent = serde_json::from_str(&wire_str).unwrap();
        assert_eq!(relay_event, relay_event_from_str);
    }

    #[test]
    fn test_deserialize() {
        // Tuple form
        let wire_str = r#"["sub_id",
            {
                "id":"0000000000000000000000000000000000000000000000000000000000000000",
                "sender":"000000001b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f",
                "created_at":0,
                "kind":0,
                "tags":[],
                "content":"",
                "sig":""
            }
        ]"#;

        let relay_event: RelayEvent = serde_json::from_str(&wire_str).unwrap();
        assert!(true);
    }

    #[test]
    fn test_deserialize_map() {
        // The standard form (map) for the struct is allowed, but it should not be when
        // messages are from the wire (RelayMessage)
        let wire_str = r#"{"subscription_id":"sub_id",
            "event": {
                "id":"0000000000000000000000000000000000000000000000000000000000000000",
                "sender":"000000001b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f",
                "created_at":0,
                "kind":0,
                "tags":[],
                "content":"",
                "sig":""
            }
        }"#;

        let relay_event: RelayEvent = serde_json::from_str(&wire_str).unwrap();
        assert!(true)
    }
}
