pub mod message;
pub mod types;
pub mod sql_builder;
pub mod db;

use anyhow::Result;
use surrealdb::engine::remote::ws::{Client, Ws, Wss};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

use crate::types::NOSTR_EVENTS_TABLE;

pub fn get_current_timstamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[derive(Debug, Clone)]
pub struct DB {
    db: Surreal<Client>,
}

impl DB {
    pub async fn connect(
        address: &str, username: &str,  password: &str,
    ) -> Result<Self> {
        let db = Surreal::new::<Wss>(address).await?;

        db.signin(Root { username, password }).await?;

        db.use_ns("nostr").use_db("nostr").await?;

        println!("DB Connected");

        Ok(Self { db })
    }
    
    pub async fn prod_connect() -> Result<Self> {
        Self::connect(
            &std::env::var("SURREAL_URL").unwrap(),
            &std::env::var("SURREAL_USER").unwrap(),
            &std::env::var("SURREAL_PASS").unwrap(),
        ).await
    }
    
    pub async fn local_connect() -> Result<Self> {
        let db = Surreal::new::<Ws>("localhost:8087").await?;

        db.signin(Root { username: "root", password: "root" }).await?;

        db.use_ns("nostr").use_db("nostr").await?;
        println!("LOCAL DB Connected");

        Ok(Self { db })
    }

    pub async fn clear_db(&self) -> Result<()> {
        self.db.query(format!("
            DELETE {NOSTR_EVENTS_TABLE};
        ")).await?;
        Ok(())
    }
}
