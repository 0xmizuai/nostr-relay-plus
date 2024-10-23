use crate::crypto::CryptoHash;
use rand::random;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PowContext {
    difficulty: u16,
    seed: String,
}

impl Default for PowContext {
    fn default() -> Self {
        Self {
            difficulty: 5,
            seed: CryptoHash::new(random()).to_string(),
        }
    }
}
