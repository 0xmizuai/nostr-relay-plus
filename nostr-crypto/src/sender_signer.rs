use anyhow::Result;
use crate::eoa_signer::EoaSigner;
use crate::schnorr_signer::SchnorrSigner;
use crate::signer::Signer;

pub enum SenderSigner {
    Schnorr(SchnorrSigner),
    Eoa(EoaSigner),
}

impl Signer for SenderSigner {
    fn try_sign(&self, message: &[u8; 32]) -> Result<Vec<u8>> {
        match self {
            SenderSigner::Schnorr(signer) => signer.try_sign(message),
            SenderSigner::Eoa(signer) => signer.try_sign(message),
        }
    }
}
