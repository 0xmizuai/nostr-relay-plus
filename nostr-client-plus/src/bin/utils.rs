use anyhow::{anyhow, Result};

// Just make rust shut-up and let me use this as a private lib
fn main() {}

pub fn get_private_key_from_name(input: &str) -> Result<[u8; 32]> {
    let bytes = input.as_bytes();
    if bytes.len() > 32 {
        return Err(anyhow!("private key string too long"));
    }

    let mut private_key = [0_u8; 32];
    private_key[..bytes.len()].copy_from_slice(bytes);

    Ok(private_key)
}
