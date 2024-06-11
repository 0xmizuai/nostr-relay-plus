use k256::ecdsa::signature::Signer;
use k256::schnorr::SigningKey;

pub struct SchnorrSigner {
    pub private: SigningKey,
}

impl SchnorrSigner {
    pub fn new(sk: SigningKey) -> Self {
        Self { private: sk }
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let private = SigningKey::from_bytes(bytes)?;
        Ok(Self { private })
    }

    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.private.sign(msg).to_bytes()
    }
}
