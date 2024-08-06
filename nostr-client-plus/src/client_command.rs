use crate::event::Event;
use crate::request::Request;
use crate::close::Close;

#[derive(Debug)]
pub enum ClientCommand {
    Req((Request, Option<u64>)),
    Event(Event),
    Close(Close),
}
