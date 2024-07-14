use anyhow::{anyhow, Result};
use nostr_surreal_db::message::{events::Event, filter::Filter, notice::Notice};
use nostr_plus_common::wire::EventOnWire;

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
        println!("Handling incoming message");
        match incoming_message {
            IncomingMessage::Event(event) => {
                let event_id_hex = hex::encode(event.id);
                println!("Sending reply back for {}", event_id_hex);
                let reply = match self.handle_event(event).await {
                    Ok(event) => {
                        println!("Sending broadcast for {}", event_id_hex);
                        if self.auth_on_send_global_broadcast_event(&event) {
                            if self.global_state.global_events_pub_sender.send(event).is_err() {
                                // ToDo: if send is broken is logging enough?
                                tracing::error!("Cannot send global event");
                            }
                        }
                        Notice::saved(event_id_hex)
                    }
                    Err(err) => Notice::error(event_id_hex, err.to_string().as_str()),
                };

                // We reply even if error, because EVENT needs OK message
                self.outgoing_sender.send(reply).await?;

            },
            IncomingMessage::Req(sub) => {
                let filters = sub.parse_filters()?;
                let messages = if self.auth_on_db_read() {
                    self.global_state.db.query_by_filters(&filters).await?
                        .iter()
                        .map(|e| {  
                            Notice::event(sub.id.clone(), e.clone())
                        })
                        .collect::<Vec<_>>()
                } else { Vec::new() };

                tracing::warn!("#### I have {} messages stored for this REQ", messages.len());
                for msg in messages {
                    self.outgoing_sender.send(msg).await?;
                }

                self.subscribe(sub)?;
            },
            IncomingMessage::Auth(auth) => {
                println!("{:?}", auth);

                // 1. validate the challenge
                // let event: Event = auth.try_into()?;
                // let content = hex::decode(event.content)?;
            }
            IncomingMessage::Close(sub_id) => {
                tracing::info!("Cancelling subscription {}", sub_id);
                self.unsubscribe(sub_id.as_str());
            },
            _ => { 
                println!("{:?}", incoming_message); 
            }
        }
        Ok(())
    }

    async fn handle_event(&mut self, event: EventOnWire) -> Result<Event> {
        println!("Processing Event {}", hex::encode(event.id));
        event.verify()?;
        eprintln!("verify success");
        let e: Event = event.try_into()?;
        e.validate()?; // ToDo: `verify` on EventOnWire or `validate` on Event?

        if self.auth_on_db_write(&e) {
            tracing::debug!(
                        "Writing Event ID {} to db on IP {:?}",
                        hex::encode(e.id),
                        self.client_ip_addr
                    );
            self.global_state.db.write_event(&e).await?;
            tracing::debug!("Done Writing Event to db");
        }
        Ok(e)
    }

    pub async fn handle_global_incoming_events(&self, event: Event) -> Result<()> {
        let id = self.is_interested(&event)?;
        let notice = Notice::event(id, event);
        self.outgoing_sender.send(notice).await?;

        Ok(())
    }

}