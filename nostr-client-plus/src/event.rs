use nostr_surreal_db::message::sender::Sender;
use nostr_surreal_db::message::wire::EventOnWire;
use nostr_surreal_db::types::{Bytes32, Timestamp};
use serde_json::json;
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub struct Event(EventOnWire);

impl Event {
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
            sig: vec![0, 1, 0, 1], // ToDo: implement signature properly
        };
        event.id = event.to_id_hash();
        Self { 0: event }
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let req_str = json!(["EVENT", self.0]).to_string();
        write!(f, "{}", req_str)
    }
}
