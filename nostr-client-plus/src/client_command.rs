use crate::event::Event;
use nostr_rust::req::Req;

#[derive(Debug)]
pub enum ClientCommand {
    Req(Req),
    Event(Event),
}
