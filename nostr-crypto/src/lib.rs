use crate::schnorr_signer::SchnorrSigner;
use crate::signer::Signer;

pub mod p256;
pub mod wallet;
pub mod hash;
pub mod encrypt;
pub mod rsa;
pub mod utils;
pub mod jwt;
pub mod schnorr_signer;
pub mod signer;

pub enum SenderSigner {
    Schnorr(SchnorrSigner),
}
impl Signer for SenderSigner {
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        match self {
            SenderSigner::Schnorr(signer) => signer.sign(message),
        }
    }
}
