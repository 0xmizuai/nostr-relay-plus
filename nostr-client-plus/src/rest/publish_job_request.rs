use crate::rest::data_job_payload::DataJobPayload;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PublishJobRequest {
    pub data: Vec<DataJobPayload>,
}
