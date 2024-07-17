use std::time::SystemTime;

pub fn get_mins_before_now_timestamp(mins: u64) -> u64 {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("time before EPOCH").as_secs();
    now.checked_sub(mins * 60).expect("Before UNIX_EPOCH")
}