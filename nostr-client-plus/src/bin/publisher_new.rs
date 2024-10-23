use anyhow::{anyhow, Context as _anyhowContext, Result};
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::db::{left_anti_join, ClassifierPublished, RawDataEntry};
use nostr_client_plus::rest::classify_context::ClassifyContext;
use nostr_client_plus::rest::data_job_payload::DataJobPayload;
use nostr_client_plus::rest::job_type::JobType;
use nostr_client_plus::rest::pow_context::PowContext;
use nostr_client_plus::rest::publish_job_request::PublishJobRequest;
use serde_json::Value;

mod utils;

const LOW_VAL_JOBS: usize = 5_000;

struct Context {
    metrics_server: Option<String>,
    low_val_jobs: usize,
    db_url: String,
    db_name: String,
    node_url: String,
    limit_publish: i64,
    classification_job_percentage: u8,
    api_key: String,
}

#[tokio::main]
async fn main() {
    println!("Starting Publisher (new)");
    if let Err(err) = run().await {
        eprintln!("{}", err);
        std::process::exit(1);
    }
    println!("Stopping Publisher (new)");
}

async fn run() -> Result<()> {
    let ctx = init_from_env()?;

    if ctx.metrics_server.is_some() {
        // Check if jobs are low and, if not, return
        let queued_jobs = get_queued_jobs(ctx.metrics_server.unwrap().as_str()).await?; // unwrap is safe
        if queued_jobs > ctx.low_val_jobs {
            println!("Enough jobs in the queue");
            return Ok(());
        } else {
            println!("Not enough jobs in the queue: publishing some more");
        }
    } else {
        println!("Running without checking cache_jobs");
    }

    // Configure DB from args
    let db = DbClient::with_uri_str(ctx.db_url)
        .await
        .context("Cannot connect to db")?
        .database(ctx.db_name.as_str());
    let collection: Collection<RawDataEntry> = db.collection("raw_data");
    let published_collection: Collection<ClassifierPublished> =
        db.collection("classifier_published");

    let timestamp_now = chrono::Utc::now().timestamp() as u64;

    // Let's figure out number of classifications and pow to send out
    let classification_count =
        (ctx.limit_publish * ctx.classification_job_percentage as i64) / 100_i64;
    println!(
        "Plan to publish {} classification jobs",
        classification_count
    );

    // Now let's fetch all classification entries needed.
    // ToDo: probably if we have fewer classification jobs and fill the rest with PoW is not
    //  what we want. Review it later.
    let mut entries = if classification_count > 0 {
        // this function return an error if classification_count not > 0
        left_anti_join(
            &collection,
            published_collection.name(),
            classification_count,
        )
        .await?
    } else {
        Vec::new()
    };
    if (entries.len() as i64) < classification_count {
        eprintln!("Not enough classification jobs to publish");
    };
    println!(
        "Actually fetched {} classification jobs to publish: PoW will fill the rest",
        entries.len()
    );
    let pow_entries = ctx.limit_publish - entries.len() as i64;

    // Define HTTP client and chunk size
    let http_client = reqwest::Client::new();
    let request_chunk_size = 50;
    let publish_endpoint = format!("{}/publish_jobs", ctx.node_url);

    /*
     * Publish classification jobs
     */
    let mut jobs_sent = 0;
    while !entries.is_empty() {
        let classification_jobs: Result<Vec<DataJobPayload>> = entries
            .drain(..std::cmp::min(50, entries.len()))
            .map(|entry| {
                Ok(DataJobPayload {
                    job_type: JobType::Classify,
                    classify_ctx: Some(ClassifyContext::from_db_entry(entry)?),
                    pow_ctx: None,
                })
            })
            .collect();
        // let aa = classification_jobs.unwrap();
        let classification_jobs = match classification_jobs {
            Ok(val) => val,
            Err(_) => return Err(anyhow!("Error converting classification entries")),
        };
        let jobs_to_send = classification_jobs.len();
        let publish_job_request = PublishJobRequest {
            data: classification_jobs,
        };

        let response = http_client
            .post(&publish_endpoint)
            .json(&publish_job_request)
            .bearer_auth(&ctx.api_key)
            .send()
            .await?;
        match response.error_for_status() {
            Ok(_res) => {
                jobs_sent += jobs_to_send;
                println!("Published {} jobs so far", jobs_sent);
                // Create db entries for published jobs
                let db_entries = publish_job_request
                    .data
                    .into_iter()
                    .map(|entry| ClassifierPublished {
                        _id: entry.classify_ctx.unwrap().checksum, // unwrap is safe here
                        timestamp: timestamp_now,
                    })
                    .collect::<Vec<ClassifierPublished>>();
                match published_collection.insert_many(db_entries, None).await {
                    Ok(_) => {}
                    Err(err) => eprintln!("Failed to insert published classifier job: {}", err),
                }
            }
            Err(err) => eprintln!("Failed to publish classify jobs chunk: {:?}", err.status()),
        }
    }

    /*
     * Publish PoW jobs
     */
    for start in (0..pow_entries).step_by(request_chunk_size) {
        let end = std::cmp::min(start + request_chunk_size as i64, pow_entries);
        let pow_jobs: Vec<DataJobPayload> = (start..end)
            .map(|_| DataJobPayload {
                job_type: JobType::Pow,
                classify_ctx: None,
                pow_ctx: Some(PowContext::default()),
            })
            .collect();
        let jobs_to_send = pow_jobs.len();
        let publish_job_request = PublishJobRequest { data: pow_jobs };

        let response = http_client
            .post(&publish_endpoint)
            .json(&publish_job_request)
            .bearer_auth(&ctx.api_key)
            .send()
            .await?;
        match response.error_for_status() {
            Ok(_) => {
                jobs_sent += jobs_to_send;
                println!("Published {} jobs so far", jobs_sent);
            }
            Err(err) => eprintln!("Failed to publish PoW jobs chunk: {:?}", err),
        }
    }

    println!("Done: {} jobs sent", jobs_sent);
    Ok(())
}

fn init_from_env() -> Result<Context> {
    // Define needed env variables
    dotenv::dotenv().ok();
    let db_url = std::env::var("MONGO_URL")?;
    let db_name = std::env::var("MONGO_DB_NAME")?;
    let node_url = std::env::var("NODE_URL")?;
    let metrics_server = std::env::var("METRICS_URL").ok();
    let api_key = std::env::var("API_KEY")?;
    let low_val_jobs = std::env::var("JOBS_THRESHOLD").unwrap_or(LOW_VAL_JOBS.to_string());
    let low_val_jobs: usize = low_val_jobs.parse()?;
    // Percentage (0-100) of classification jobs. Remainder is PoW jobs.
    let classification_job_percentage = match std::env::var("CLASSIFICATION_PERCENT")
        .unwrap_or("100".to_string())
        .parse::<u8>()
    {
        Ok(val) => {
            // check range
            match val {
                0..=100 => val,
                _ => panic!("CLASSIFICATION_PERCENT must be in range [0, 100]"),
            }
        }
        Err(err) => {
            panic!("Failed to parse CLASSIFICATION_PERCENT to u8: {err}");
        }
    };
    println!(
        "{}% of all jobs published should be classification job",
        classification_job_percentage
    );

    // Command line parsing
    let args: Vec<String> = std::env::args().collect();
    let limit_publish: i64 = match args.len() {
        1 => 1000_i64,
        2 => args[1].parse().context("Invalid number")?,
        _ => {
            return Err(anyhow!("Too many arguments"));
        }
    };

    Ok(Context {
        metrics_server,
        low_val_jobs,
        db_url,
        db_name,
        node_url,
        limit_publish,
        classification_job_percentage,
        api_key,
    })
}

pub async fn get_queued_jobs(url: &str) -> Result<usize> {
    let query_url = format!("{}/queue_len", url);
    let response = reqwest::get(&query_url).await?;
    if !response.status().is_success() {
        return Err(anyhow!("Failed to fetch metric"));
    }
    let body: Value = response.json().await?;
    let value = body
        .get("data")
        .and_then(|data| data.get("length"))
        .and_then(|length| length.as_u64())
        .ok_or_else(|| anyhow!("Failed to parse data"))?;
    Ok(value as usize)
}
