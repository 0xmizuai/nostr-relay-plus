use crate::event::Event;
use crate::request::Request;
use tokio::sync::oneshot::Sender;
use nostr_plus_common::relay_ok::RelayOk;
use crate::close::Close;

#[derive(Debug)]
pub enum ClientCommand {
    Req((Request, Option<u64>)),
    Event((Event, Sender<RelayOk>)),
    Ack(RelayOk),
    Close(Close),
}
