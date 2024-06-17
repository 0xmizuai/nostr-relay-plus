use anyhow::Result;

pub trait Signer {
    fn sign(&self, message: &[u8]) -> Vec<u8>;
}

pub trait Verifier {
    fn verify(message: [u8; 32], signature: &[u8], pub_key: &[u8]) -> Result<()>;
}
