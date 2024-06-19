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

#[cfg(test)]
mod tests {
    use crate::hash::sha256_hash_digests;
    use crate::schnorr_signer::SchnorrSigner;
    use crate::signer::{Signer, Verifier};

    #[test]
    fn sign_and_verify() {
        let signer = SchnorrSigner::from_bytes(&[199; 32]).unwrap();
        let mut message_hash = sha256_hash_digests("Some message".as_bytes());
        let signature = signer.try_sign(&message_hash).unwrap();

        // Get public_key and verify it works
        let pub_key = signer.private.verifying_key().to_bytes();
        match SchnorrSigner::verify(message_hash, &signature[..], &pub_key) {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }

        // Modify message_hash and check it fails
        message_hash[1] = 123;
        match SchnorrSigner::verify(message_hash, &signature[..], &pub_key) {
            Ok(_) => assert!(false),
            Err(_) => assert!(true),
        }
    }
}
