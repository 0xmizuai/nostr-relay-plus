use anyhow::{anyhow, Result};
use std::time::SystemTime;

// Just make rust shut-up and let me use this as a private lib
#[allow(dead_code)]
fn main() {}

#[allow(dead_code)]
pub fn get_private_key_from_name(input: &str) -> Result<[u8; 32]> {
    let bytes = input.as_bytes();
    if bytes.len() > 32 {
        return Err(anyhow!("private key string too long"));
    }

    let mut private_key = [0_u8; 32];
    private_key[..bytes.len()].copy_from_slice(bytes);

    Ok(private_key)
}

#[allow(dead_code)]
pub fn get_mins_before_now_timestamp(mins: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time before EPOCH")
        .as_secs();
    now.checked_sub(mins * 60).expect("Before UNIX_EPOCH")
}

/// Get value for a single letter tag. Error if more than one.
#[allow(dead_code)]
pub fn get_single_tag_entry(tag: char, tags: &Vec<Vec<String>>) -> Result<Option<String>> {
    let mut found = false;
    let match_tag = String::from(tag);
    let mut result: Option<String> = None;

    for entry in tags {
        if entry.len() < 2 {
            return Err(anyhow!("Malformed tags"));
        }
        if match_tag == entry[0] {
            if found == true {
                return Err(anyhow!("Non unique tag"));
            }
            result = Some(entry[1].to_string());
            found = true;
        }
    }
    Ok(result)
}
