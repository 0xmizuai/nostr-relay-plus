use anyhow::{anyhow, Result};
use nostr_plus_common::binary_protocol::BinaryMessage;
use nostr_surreal_db::message::{events::Event, filter::Filter, notice::Notice};
use nostr_plus_common::wire::EventOnWire;

use crate::{local::hooks::LocalStateHooks, message::IncomingMessage, util::wrap_ws_message};

use super::LocalState;

impl LocalState {

    pub async fn start_auhentication(&mut self) {
        // send out the challenge
        self.outgoing_sender
            .send(Notice::AuthChallenge(hex::encode(self.auth_challenge)))
            .expect("outgoing receiver not to be dropped");
    }


    pub async fn handle_incoming_message(&mut self, incoming_message: IncomingMessage) -> Result<()> {
        match incoming_message {
            IncomingMessage::Event(event) => {
                let sender_hex = hex::encode(event.sender.to_bytes());
                tracing::debug!(
                    "Received event of kind {} from sender {}",
                    event.kind,
                    sender_hex
                );
                let event_id_hex = hex::encode(event.id);
                let reply = match self.handle_event(event).await {
                    Ok(event) => {
                        if self.auth_on_send_global_broadcast_event(&event) {
                            if self.global_state.global_events_pub_sender.send(event).is_err() {
                                // ToDo: if send is broken is logging enough?
                                tracing::error!("Cannot send global event {}", event_id_hex);
                            } else {
                                tracing::debug!("Sent global event {}", event_id_hex);
                            }
                        }
                        Notice::saved(event_id_hex)
                    }
                    Err(err) => {
                        tracing::error!("handle_event {event_id_hex}: {err}");
                        Notice::error(event_id_hex, err.to_string().as_str())
                    },
                };

                // We reply even if error, because EVENT needs OK message
                self.outgoing_sender.send(reply)?;
                tracing::debug!("Reply to sender {sender_hex} queued");
            },
            IncomingMessage::Req(sub) => {
                tracing::debug!("Received Subscription with id {}", sub.id);
                let filters = sub.parse_filters()?;
                let messages = if self.auth_on_db_read() {
                    self.global_state.db.query_by_filters(&filters).await?
                        .iter()
                        .map(|e| {  
                            Notice::event(sub.id.clone(), e.clone())
                        })
                        .collect::<Vec<_>>()
                } else { Vec::new() };

                let messages_len = messages.len();
                for msg in messages {
                    self.outgoing_sender.send(msg)?;
                }
                tracing::debug!(
                    "All messages {} for subscription {} sent",
                    messages_len,
                    sub.id
                );

                self.subscribe(sub)?;
            },
            IncomingMessage::Auth(auth) => {
                // 1. validate the challenge
                // let event: Event = auth.try_into()?;
                // let content = hex::decode(event.content)?;
            }
            IncomingMessage::Close(sub_id) => {
                tracing::info!("Cancelling subscription {}", sub_id);
                self.unsubscribe(sub_id.as_str());
            },
            _ => tracing::warn!("Unhandled message"),
        }
        Ok(())
    }

    async fn handle_event(&mut self, event: EventOnWire) -> Result<Event> {
        event.verify()?;
        let e: Event = event.try_into()?;
        e.validate()?; // ToDo: `verify` on EventOnWire or `validate` on Event?

        // ToDo: Remove. Temporary hack to treat heartbeats as ephemeral
        if e.kind == 6_001 {
            return Ok(e);
        }

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
        tracing::info!(
            "Sending event of type {} for subscription {}",
            event.kind,
            id
        );
        let notice = Notice::event(id, event);
        self.outgoing_sender.send(notice)?;

        Ok(())
    }

    pub fn handle_binary_message(&self, msg: &[u8]) -> Result<Vec<u8>> {
        let binary_message = BinaryMessage::from_bytes(msg)?;
        match binary_message {
            BinaryMessage::QuerySubscription => {
                Ok(BinaryMessage::ReplySubscription(self.subscriptions.len() as u8).to_bytes())
            }
            BinaryMessage::ReplySubscription(_) => Err(anyhow!("Unexpected ReplySubscription")),
        }
    }
}
