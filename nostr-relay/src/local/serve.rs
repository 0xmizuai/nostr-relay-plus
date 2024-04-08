use anyhow::{anyhow, Result};
use nostr_surreal_db::message::{events::Event, notice::Notice};

use crate::{message::IncomingMessage, util::wrap_ws_message};

use super::LocalState;

impl LocalState {

    pub async fn start_auhentication(&mut self) {
        // send out the challenge
        self.outgoing_sender
            .send(Notice::AuthChallenge(hex::encode(self.auth_challenge)))
            .await
            .expect("outgoing receiver not to be dropped");
    }


    pub async fn handle_incoming_message(&self, incoming_message: IncomingMessage) -> Result<()> {
        match incoming_message {
            IncomingMessage::Event(event) => {
                let e = event.try_into()?;

                // TODO: DBWriteHook
                // validate signature and if valid to be written into db
                println!("Writing Event to db");
                self.global_state.db.write_event(&e).await?;
                println!("Done Writing Event to db");
                // TODO: BroadcastHook
                self.global_state.global_events_pub_sender.send(e)?;

            },
            IncomingMessage::Req(sub) => {
                // 1. send the initial filter
                let filters = sub.filters
                    .iter()
                    .map(|f| f.clone().try_into().unwrap())
                    .collect::<Vec<_>>();

                // TODO: DBReadHook 
                let messages = self.global_state.db.query_by_filter(&filters[0]).await?
                    .iter()
                    .map(|e| {  
                        Notice::message(hex::encode(e.id))
                    })
                    .collect::<Vec<_>>();

                // TODO: SubHook
                for msg in messages {
                    self.outgoing_sender.send(msg).await?;
                }
            },
            _ => { 
                println!("{:?}", incoming_message); 
            }
        }
        Ok(())
    }

    pub async fn handle_global_incoming_events(&self, event: Event) -> Result<()> {
        // 1. send the event to the client
        // TODO: check is interested

        println!("{:?}", event);
        Ok(())
    }

}