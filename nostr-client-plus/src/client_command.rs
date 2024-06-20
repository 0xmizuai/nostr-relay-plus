use crate::event::Event;
use crate::request::Request;
use tokio::sync::oneshot::Sender;
use crate::wire::relay_ok::RelayOk;

#[derive(Debug)]
pub enum ClientCommand {
    Req(Request),
    Event((Event, Sender<RelayOk>)),
    Ack(RelayOk),
}
