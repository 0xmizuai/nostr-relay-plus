use crate::schnorr_signer::SchnorrSigner;

pub mod p256;
pub mod wallet;
pub mod hash;
pub mod encrypt;
pub mod rsa;
pub mod utils;
pub mod jwt;
pub mod schnorr_signer;

pub enum Signer {
    Schnorr(SchnorrSigner),
}
