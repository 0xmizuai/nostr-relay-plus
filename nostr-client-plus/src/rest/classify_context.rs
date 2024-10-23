use crate::db::RawDataEntry;
use anyhow::Result;
use mongodb::bson::{from_document, Document};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ClassifyContext {
    pub r2_key: String,
    pub byte_size: usize,
    pub checksum: String,
}

impl ClassifyContext {
    pub fn from_db_entry(entry: Document) -> Result<ClassifyContext> {
        let entry: RawDataEntry = from_document(entry)?;
        Ok(ClassifyContext {
            r2_key: entry.r2_key,
            byte_size: entry.bytes_size,
            checksum: entry.content_checksum.to_string(),
        })
    }
}
