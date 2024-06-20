use nostr_surreal_db::types::Bytes32;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RelayOk {
    #[serde(with = "hex::serde")]
    pub(crate) event_id: Bytes32,
    pub(crate) accepted: bool,
    pub(crate) message: String,
}
