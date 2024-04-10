use anyhow::{anyhow, Result};
use nostr_surreal_db::message::{events::Event, notice::Notice};

use crate::{local::hooks::LocalStateHooks, message::IncomingMessage, util::wrap_ws_message};

use super::LocalState;

impl LocalState {

    pub async fn start_auhentication(&mut self) {
        // send out the challenge
        self.outgoing_sender
            .send(Notice::AuthChallenge(hex::encode(self.auth_challenge)))
            .await
            .expect("outgoing receiver not to be dropped");
    }


    pub async fn handle_incoming_message(&mut self, incoming_message: IncomingMessage) -> Result<()> {
        match incoming_message {
            IncomingMessage::Event(event) => {
                let e: Event = event.try_into()?;
                e.validate()?;

                if self.auth_on_db_write(&e) {
                    tracing::debug!(
                        "Writing Event ID {} to db on IP {:?}", 
                        hex::encode(e.id), 
                        self.client_ip_addr
                    );
                    self.global_state.db.write_event(&e).await?;
                    tracing::debug!("Done Writing Event to db");
                }

                if self.auth_on_send_global_broadcast_event(&e) {
                    self.global_state.global_events_pub_sender.send(e)?;
                }
            },
            IncomingMessage::Req(sub) => {
                // 1. send the initial filter
                let filter = sub.clone().filter.try_into()?;

                let messages = if self.auth_on_db_read() {
                    self.global_state.db.query_by_filter(&filter).await?
                        .iter()
                        .map(|e| {  
                            Notice::message(hex::encode(e.id))
                        })
                        .collect::<Vec<_>>()
                } else { Vec::new() };

                for msg in messages {
                    self.outgoing_sender.send(msg).await?;
                }

                self.subscribe(sub)?;
            },
            IncomingMessage::Auth(auth) => {
                // 1. validate the challenge
                let event: Event = auth.try_into()?;
                let content = hex::decode(event.content)?;
                

            }

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