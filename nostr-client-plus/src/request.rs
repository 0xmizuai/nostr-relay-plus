use nostr_surreal_db::message::wire::FilterOnWire;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub struct Request {
    pub subscription_id: String, // Make it Option so we can generate one if None
    pub filters: Vec<FilterOnWire>,
}

impl Request {
    pub fn new(subscription_id: String, filters: Vec<FilterOnWire>) -> Self {
        Self {
            subscription_id,
            filters,
        }
    }
}

impl Serialize for Request {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.filters.len() + 2))?;
        seq.serialize_element("REQ")?;
        seq.serialize_element(&self.subscription_id)?;
        for el in &self.filters {
            seq.serialize_element(el)?;
        }
        seq.end()
    }
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string(&self).unwrap())
    }
}

pub type Filter = FilterOnWire;
