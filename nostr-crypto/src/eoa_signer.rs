use alloy_primitives::{Address, Signature};
use anyhow::{anyhow, Result};
use crate::signer::{Signer, Verifier};
use crate::wallet::AlloyWallet;

pub struct EoaSigner(AlloyWallet);

impl EoaSigner {
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            0: AlloyWallet::new(bytes)
        }
    }

    pub fn address(&self) -> Address {
        self.0.address()
    }
}

impl Signer for EoaSigner {
    fn try_sign(&self, message: &[u8; 32]) -> Result<Vec<u8>> {
        Ok(self.0.sign_digest(message)?.into())
    }
}

impl Verifier for EoaSigner {
    fn verify(message: [u8; 32], signature_bytes: &[u8], pub_key: &[u8]) -> anyhow::Result<()> {
        let signature = Signature::try_from(signature_bytes)?;
        let address_recovered = AlloyWallet::ec_recover_hash(&message, &signature)?;
        let address_real = Address::try_from(pub_key)?;
        if address_real == address_recovered {
            Ok(())
        } else {
            Err(anyhow!("Address do not match"))
        }
    }
}