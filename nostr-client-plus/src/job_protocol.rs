use crate::crypto::CryptoHash;
use nostr_plus_common::sender::Sender;
use nostr_plus_common::types::Timestamp;
use serde::{Deserialize, Serialize};
use redis_macros::{FromRedisValue, ToRedisArgs};
use strum_macros::{AsRefStr, EnumString};

// ToDo: this just a placeholder struct
#[derive(Serialize, Deserialize)]
pub struct AIRuntimeConfig {}

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

#[derive(PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct PayloadHeader {
    pub job_type: u16,           // ToDo: job types need to be codified
    pub raw_data_id: CryptoHash, // we need in order to cross-reference finished_jobs db entries with raw_data
    pub time: Timestamp,
}

#[derive(Serialize, Deserialize, FromRedisValue, ToRedisArgs, Clone)]
pub struct AssignerTask {
    pub worker: Sender,
    pub event_id: String,
    pub result: ResultPayload,
    // TODO(wangjun.hong): Figure out how to support timeout and retry
}

#[derive(Serialize, Deserialize)]
pub struct NewJobPayload {
    pub header: PayloadHeader,
    pub kv_key: String,
    pub config: Option<AIRuntimeConfig>,
    pub validator: String,
    pub classifier: String,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct ResultPayload {
    pub header: PayloadHeader,
    pub output: String,
    pub version: String,
    pub kv_key: String
}

#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub struct ClassifierJobOutput {
    pub tag_id: u16,
}

#[repr(u16)]
#[derive(AsRefStr, EnumString, PartialEq)]
pub enum JobType {
    #[strum(serialize = "pow")]
    PoW = 0,
    #[strum(serialize = "classification")]
    Classification,
}

impl JobType {
    pub fn job_type(&self) -> u16 {
        match self {
            JobType::PoW => JobType::PoW as u16,
            JobType::Classification => JobType::Classification as u16,
        }
    }

    pub fn workers(&self) -> usize {
        match self {
            JobType::PoW => 1,
            JobType::Classification => 1,
        }
    }
}


#[cfg(test)]
mod tests {
    use crate::job_protocol::JobType;
    use std::str::FromStr;

    #[test]
    fn test_job_type_deserialization() {
        let job_type: JobType = "pow".parse().unwrap();
        assert!(matches!(job_type, JobType::PoW));
        let job_type: JobType = "classification".parse().unwrap();
        assert!(matches!(job_type, JobType::Classification));
        let maybe_job_type = JobType::from_str("PoW");
        assert!(maybe_job_type.is_err());
    }

    #[test]
    fn test_job_type_serialization() {
        let job = JobType::PoW;
        assert_eq!("pow", job.as_ref());
        let job = JobType::Classification;
        assert_eq!("classification", job.as_ref());
    }

    #[test]
    fn test_job_type_u16_repr() {
        let job = JobType::PoW as u16;
        assert_eq!(job, 0_u16);
        let job = JobType::Classification as u16;
        assert_eq!(job, 1_u16);
        let job = JobType::PoW;
        assert_eq!(job.job_type(), 0_u16);
        let job = JobType::Classification;
        assert_eq!(job.job_type(), 1_u16);
    }
}
