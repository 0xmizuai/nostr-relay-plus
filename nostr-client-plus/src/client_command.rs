use crate::event::Event;
use crate::request::Request;
use tokio::sync::oneshot::Sender;
use nostr_plus_common::relay_ok::RelayOk;

#[derive(Debug)]
pub enum ClientCommand {
    Req(Request),
    Event((Event, Sender<RelayOk>)),
    Ack(RelayOk),
}
