use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct RelayAuth {
    challenge: String,
}