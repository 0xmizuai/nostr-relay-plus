use crate::signer::{Signer, Verifier};
use anyhow::Result;
use k256::schnorr::signature::{Signer as K256Signer, Verifier as K256Verifier};
use k256::schnorr::{Signature, SigningKey, VerifyingKey};

pub struct SchnorrSigner {
    pub private: SigningKey,
}

impl SchnorrSigner {
    pub fn new(sk: SigningKey) -> Self {
        Self { private: sk }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let private = SigningKey::from_bytes(bytes)?;
        Ok(Self { private })
    }
}

impl Signer for SchnorrSigner {
    fn try_sign(&self, message: &[u8; 32]) -> Result<Vec<u8>> {
        Ok(self.private.try_sign(message)?.to_bytes().into())
    }
}

impl Verifier for SchnorrSigner {
    fn verify(message: [u8; 32], signature_bytes: &[u8], pub_key: &[u8]) -> Result<()> {
        let signature = Signature::try_from(&signature_bytes[..])?;
        let verifying_key = VerifyingKey::from_bytes(&pub_key)?;
        Ok(verifying_key.verify(&message, &signature)?)
    }
}
