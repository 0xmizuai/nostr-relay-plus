use anyhow::Result;
use nostr_crypto::signer::Signer;
use nostr_plus_common::sender::Sender;
use nostr_plus_common::wire::EventOnWire;
use nostr_plus_common::types::{Bytes32, Timestamp};
use serde_json::json;
use std::fmt;
use std::fmt::Formatter;

pub struct UnsignedEvent(Event);

impl UnsignedEvent {
    pub fn new(
        sender: Sender,
        created_at: Timestamp,
        kind: u16,
        tags: Vec<Vec<String>>,
        content: String,
    ) -> Self {
        let mut event = EventOnWire {
            id: Bytes32::default(),
            sender,
            created_at,
            kind,
            tags,
            content,
            sig: Vec::new(),
        };
        event.id = event.to_id_hash();
        Self { 0: Event(event) }
    }
}

impl UnsignedEvent {
    pub fn id(&self) -> Bytes32 {
        self.0.id()
    }

    pub fn sign<S: Signer>(mut self, signer: &S) -> Result<Event> {
        let signature = signer.try_sign(&self.id())?;
        self.0.set_sig(signature);
        Ok(self.0)
    }
}

#[derive(Debug)]
pub struct Event(EventOnWire);

impl Event {
    pub fn id(&self) -> Bytes32 {
        self.0.id
    }

    pub fn set_sig(&mut self, signature: Vec<u8>) {
        self.0.sig = signature;
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let req_str = json!(["EVENT", self.0]).to_string();
        write!(f, "{}", req_str)
    }
}
