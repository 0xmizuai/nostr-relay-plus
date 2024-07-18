use anyhow::Result;
use futures::StreamExt;
use mongodb::{bson::Document, options::FindOptions, Collection};
use serde::{Deserialize, Serialize};

use crate::crypto::CryptoHash;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawDataEntry {
    pub _id: CryptoHash,

    pub source_url: String, // source data locator
    pub line_number: usize,
    pub content_checksum: CryptoHash, // data content checksum
    pub bytes_size: usize,            // source data metadata
    pub r2_key: String,               // Key to retrieve the content from R2
}

pub async fn select_many(
    col: &Collection<RawDataEntry>,
    filter: Document,
    limit: Option<i64>,
    skip: Option<u64>,
) -> Result<Vec<RawDataEntry>> {
    let options = FindOptions::builder().limit(limit).skip(skip).build();

    let mut docs = col.find(filter, Some(options)).await?;
    let mut vec = Vec::new();
    while let Some(doc) = docs.next().await {
        vec.push(doc?);
    }
    Ok(vec)
}
