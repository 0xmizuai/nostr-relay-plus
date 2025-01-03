use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct CryptoHash {
    #[serde(with = "hex::serde")]
    hash: [u8; 32],
}

impl CryptoHash {
    pub fn new(hash: [u8; 32]) -> Self {
        Self { hash }
    }

    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }

    pub fn to_string(&self) -> String {
        hex::encode(self.hash())
    }
}

impl TryFrom<String> for CryptoHash {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: value.into_bytes().as_slice().try_into()?,
        })
    }
}
