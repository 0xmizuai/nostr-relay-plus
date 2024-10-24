use crate::rest::classify_context::ClassifyContext;
use crate::rest::job_type::JobType;
use crate::rest::pow_context::PowContext;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DataJobPayload {
    #[serde(rename = "jobType")]
    pub job_type: JobType,
    #[serde(rename = "classifyCtx")]
    pub classify_ctx: Option<ClassifyContext>,
    #[serde(rename = "powCtx")]
    pub pow_ctx: Option<PowContext>,
}
