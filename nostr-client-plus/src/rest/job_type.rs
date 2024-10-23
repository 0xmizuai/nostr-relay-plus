use serde_repr::{Deserialize_repr, Serialize_repr};

#[repr(u8)]
#[derive(Serialize_repr, Deserialize_repr)]
pub enum JobType {
    Pow = 0,
    Classify = 1,
}
