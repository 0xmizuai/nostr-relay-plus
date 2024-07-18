use std::time::SystemTime;

pub fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time before EPOCH")
        .as_secs()
}
