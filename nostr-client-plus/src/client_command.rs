use crate::event::Event;
use crate::request::Request;
use nostr_surreal_db::types::Bytes32;
use tokio::sync::oneshot::Sender;

#[derive(Debug)]
pub enum ClientCommand {
    Req(Request),
    Event((Event, Sender<bool>)),
    Ack((Bytes32, bool)),
}
