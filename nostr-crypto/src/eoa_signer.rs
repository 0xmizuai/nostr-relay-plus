use crate::signer::{Signer, Verifier};
use crate::wallet::AlloyWallet;
use alloy_primitives::{Address, Signature};
use anyhow::{anyhow, Result};

pub struct EoaSigner(AlloyWallet);

impl EoaSigner {
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            0: AlloyWallet::new(bytes),
        }
    }

    pub fn address(&self) -> Address {
        self.0.address()
    }
}

impl Signer for EoaSigner {
    fn try_sign(&self, message: &[u8; 32]) -> Result<Vec<u8>> {
        Ok(self.0.sign_message(message)?.into())
    }
}

impl Verifier for EoaSigner {
    fn verify(message: [u8; 32], signature_bytes: &[u8], pub_key: &[u8]) -> Result<()> {
        let signature = Signature::try_from(signature_bytes)?;
        let address_recovered = AlloyWallet::ec_recover_message(&message, &signature)?;
        let address_real = Address::try_from(pub_key)?;
        if address_real == address_recovered {
            Ok(())
        } else {
            Err(anyhow!("Address do not match"))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::eoa_signer::EoaSigner;
    use crate::hash::sha256_hash_digests;
    use crate::signer::{Signer, Verifier};
    use alloy_primitives::Address;

    #[test]
    fn sign_and_verify() {
        let signer = EoaSigner::from_bytes(&[111; 32]);
        let message_hash = sha256_hash_digests("A nice message".as_bytes());
        let signature = signer.try_sign(&message_hash).unwrap();

        // Get address and check everything is OK
        let address = signer.address();
        match EoaSigner::verify(message_hash, &signature[..], address.as_ref()) {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }

        // Tamper with address and check again
        let mut addr = address.to_vec();
        addr[3] = 134;
        let address = Address::from_slice(&addr);
        match EoaSigner::verify(message_hash, &signature[..], address.as_ref()) {
            Ok(_) => assert!(false),
            Err(_) => assert!(true),
        }
    }
}
