use nostr_plus_common::types::SubscriptionId;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use std::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub struct Close {
    pub subscription_id: String,
}

impl Close {
    pub fn new(subscription_id: SubscriptionId) -> Self {
        Self { subscription_id }
    }
}

impl Serialize for Close {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element("CLOSE")?;
        seq.serialize_element(&self.subscription_id)?;
        seq.end()
    }
}

impl fmt::Display for Close {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string(&self).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use crate::close::Close;

    #[test]
    fn test_display() {
        let close_req = Close::new("my_sub_id".to_string());
        assert_eq!(format!("{close_req}"), r#"["CLOSE","my_sub_id"]"#);
    }

    #[test]
    fn test_serialize() {
        let close_req = Close::new("my_sub_id".to_string());
        assert_eq!(
            serde_json::to_string(&close_req).unwrap(),
            r#"["CLOSE","my_sub_id"]"#
        );
    }
}
