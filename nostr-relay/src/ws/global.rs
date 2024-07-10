use anyhow::Result;
use nostr_surreal_db::{message::events::Event, DB};
use tokio::sync::broadcast;

pub struct GlobalState {
    pub db: DB,
    pub global_events_pub_sender: broadcast::Sender<Event>,
    pub global_events_pub_receiver: broadcast::Receiver<Event>,
}

impl Clone for GlobalState {
    fn clone(&self) -> Self {
        let new_receiver = self.global_events_pub_sender.subscribe();
        Self {
            db: self.db.clone(),
            global_events_pub_sender: self.global_events_pub_sender.clone(),
            global_events_pub_receiver: new_receiver,
        }
    }
}

impl GlobalState {
    pub async fn new_with_local_db() -> Result<Self> {
        let (sender, receiver) = broadcast::channel(100);
        let db = DB::local_connect().await?;
        Ok(Self {
            db,
            global_events_pub_sender: sender,
            global_events_pub_receiver: receiver,
        })
    }

    pub async fn new_with_remote_db() -> Result<Self> {
        let (sender, receiver) = broadcast::channel(100);
        let db = DB::prod_connect().await?;
        Ok(Self {
            db,
            global_events_pub_sender: sender,
            global_events_pub_receiver: receiver,
        })
    }

}