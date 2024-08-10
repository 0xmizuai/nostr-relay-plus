use crate::close::Close;
use crate::event::Event;
use crate::request::Request;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug)]
pub enum ClientCommand {
    Req((Request, Option<u64>)),
    Event(Event),
    Close(Close),
    Binary(Message),
}
