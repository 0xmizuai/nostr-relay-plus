use crate::rest::classify_context::ClassifyContext;
use crate::rest::job_type::JobType;
use crate::rest::pow_context::PowContext;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DataJobPayload {
    pub job_type: JobType,
    pub classify_ctx: Option<ClassifyContext>,
    pub pow_ctx: Option<PowContext>,
}
