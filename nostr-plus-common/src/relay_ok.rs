use crate::types::Bytes32;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RelayOk {
    #[serde(with = "hex::serde")]
    pub event_id: Bytes32,
    pub accepted: bool,
    pub message: String,
}
