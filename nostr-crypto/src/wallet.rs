use alloy_primitives::{Address, Signature};
use alloy_signer::SignerSync;
use alloy_signer_wallet::LocalWallet;
use anyhow::Result;

use crate::hash::keccak_digest;

pub struct AlloyWallet {
    pub wallet: LocalWallet
}

impl AlloyWallet {
    pub fn new(private_key: &[u8; 32]) -> Self {
        let mut try_private_key = private_key.clone();
        let mut salt = 0;
        loop {
            if let Ok(wallet) = LocalWallet::from_slice(&try_private_key) {
                return Self { wallet };
            } else {
                try_private_key = keccak_digest(&[try_private_key.to_vec(), [salt].to_vec()].concat());
                salt += 1;
            }
        };
    }

    pub fn address(&self) -> Address {
        self.wallet.address()
    }

    pub fn sign_digest(&self, digest: &[u8; 32]) -> Result<Signature> {
        Ok(self.wallet.sign_hash_sync(digest.into())?)
    }

    pub fn sign_message(&self, message: &[u8]) -> Result<Signature> {
        Ok(self.wallet.sign_message_sync(message)?)
    }

    pub fn ec_recover_hash(digest: &[u8; 32], signature: &Signature) -> Result<Address> {
        Ok(signature.recover_address_from_prehash(digest.into())?)
    }

    pub fn ec_recover_message(message: &[u8], signature: &Signature) -> Result<Address> {
        Ok(signature.recover_address_from_msg(message)?)
    }
}


#[cfg(test)]
mod tests {
    use alloy_primitives::{address, hex};
    use crate::hash::ethereum_hash_msg;

    use super::*;

    #[test]
    fn test_wallet() {
        let private_key = hex!("9b210836f04d98ca233d2346275ea95be441373d2f0abdaffea05ed150edb913");
        let wallet = AlloyWallet::new(&private_key);
        assert_eq!(wallet.address(), address!("cb010713f7b95e38a49577d31bdd0f9a3cac9ac7"));
        
        let message = b"hello world";
        let hash = ethereum_hash_msg(message);
        let sig = wallet.sign_message(message).unwrap();
        let sig_digest = wallet.sign_digest(&hash).unwrap();

        assert_eq!(sig, sig_digest);
        assert_eq!(AlloyWallet::ec_recover_message(message, &sig).unwrap(), wallet.address());
        assert_eq!(AlloyWallet::ec_recover_hash(&hash, &sig).unwrap(), wallet.address());
    }

}