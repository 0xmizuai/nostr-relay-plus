use alloy_primitives::eip191_hash_message;
use anyhow::Result;
use tiny_keccak::{Hasher, Keccak};
use sha2::{Digest, Sha256};

pub fn sha256_hash_digests(msg: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(msg);
    hasher.finalize().into()
}

pub fn keccak_digest(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];

    hasher.update(data);
    hasher.finalize(&mut output);

    output
}

pub fn ethereum_hash_msg(msg: &[u8]) -> [u8; 32] {
    eip191_hash_message(&msg).into()
}

pub fn validate_mizu_name(_name: &str) -> Result<()> {
    Ok(())
}

pub fn mizu_name_hash(name: &str) -> Result<[u8; 32]> {
    validate_mizu_name(name)?;
    let mut result = [0u8; 32];
    let labels = name.split(".").collect::<Vec<_>>();
    let mut index = labels.len() - 1;
    loop {
        let hashed = keccak_digest(labels[index].as_bytes());
        result = keccak_digest(&[result, hashed].concat());

        if index == 0 {break;}
        index -= 1;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::hash::mizu_name_hash;


    #[test]
    fn name_hash_test() {
        assert_eq!(
            hex::encode(mizu_name_hash("wevm.eth").unwrap()), 
            "08c85f2f4059e930c45a6aeff9dcd3bd95dc3c5c1cddef6a0626b31152248560"
        );

    }
}