use crate::crypto::CryptoHash;
use crate::job_protocol::ResultPayload;
use anyhow::Result;
use futures::StreamExt;
use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, Document},
    options::FindOptions,
    Collection,
};
use nostr_plus_common::sender::Sender;
use nostr_plus_common::types::Timestamp;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawDataEntry {
    pub _id: CryptoHash,

    pub source_url: String, // source data locator
    pub line_number: usize,
    pub content_checksum: CryptoHash, // data content checksum
    pub bytes_size: usize,            // source data metadata
    pub r2_key: String,               // Key to retrieve the content from R2
}

#[derive(Serialize, Deserialize)]
pub struct FinishedJobs {
    pub _id: CryptoHash,

    pub workers: Vec<Sender>,
    pub timestamp: Timestamp,
    pub result: ResultPayload,
    pub job_type: u16, // TODO:
    pub assign_event_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct ClassifierResult {
    pub _id: CryptoHash,
    pub kv_key:String,
    pub tag_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct ClassifierPublished {
    pub _id: CryptoHash,
    pub timestamp: Timestamp,
}

pub async fn select_many<T>(
    col: &Collection<T>,
    filter: Document,
    limit: Option<i64>,
    skip: Option<u64>,
) -> Result<Vec<T>>
where
    T: DeserializeOwned,
{
    let options = FindOptions::builder().limit(limit).skip(skip).build();

    let mut docs = col.find(filter, Some(options)).await?;
    let mut vec = Vec::new();
    while let Some(doc) = docs.next().await {
        vec.push(doc?);
    }
    Ok(vec)
}

// ToDo: this is a very ad-hoc function, but it's useful so make it more generic
pub async fn left_anti_join(
    collection_left: &Collection<RawDataEntry>,
    collection_right_name: &str,
    limit: i64,
) -> Result<Vec<Document>> {
    let pipeline = vec![
        // Lookup to find matches in collection_2
        doc! {
            "$lookup": {
                "from": collection_right_name,
                "localField": "_id",
                "foreignField": "_id",
                "as": "matches"
            }
        },
        // Filter out documents where there are matches
        doc! {
            "$match": {
                "matches": { "$eq": [] }
            }
        },
        // Limit the results to the first N entries
        doc! {
            "$limit": limit  // Replace 10 with your desired limit N
        },
    ];

    let cursor = collection_left.aggregate(pipeline, None).await?;
    let results: Vec<_> = cursor.try_collect().await?;
    Ok(results)
}
