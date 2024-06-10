use crate::event::Event;
use crate::request::Request;

#[derive(Debug)]
pub enum ClientCommand {
    Req(Request),
    Event(Event),
}
