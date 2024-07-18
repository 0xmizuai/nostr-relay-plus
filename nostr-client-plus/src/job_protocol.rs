use nostr_plus_common::types::Timestamp;
use serde::{Deserialize, Serialize};

use crate::crypto::CryptoHash;

#[repr(u16)]
pub enum Kind {
    NewJob = 6000,
    Alive,
    Assign,
    Result,
    Agg,
    Challenge,
    Resolution,
}

impl Kind {
    pub const NEW_JOB: u16 = Kind::NewJob as u16;
    pub const ALIVE: u16 = Kind::Alive as u16;
    pub const ASSIGN: u16 = Kind::Assign as u16;
    pub const RESULT: u16 = Kind::Result as u16;
    pub const AGG: u16 = Kind::Agg as u16;
    pub const CHALLENGE: u16 = Kind::Challenge as u16;
    pub const RESOLUTION: u16 = Kind::Resolution as u16;
}

#[derive(Serialize, Deserialize)]
pub struct PayloadHeader {
    pub job_type: u16,        // ToDo: job types need to be codified
    pub job_hash: CryptoHash, // ToDo: we cannot use the job hash because the payload is part of the calculation. So which one?
    pub time: Timestamp,
}

#[derive(Serialize, Deserialize)]
pub struct NewJobPayload {
    pub header: PayloadHeader,
    pub kv_key: String,
    //    config: Option<Config>, // ?!
}
