use nostr_surreal_db::message::notice::Notice;

#[derive(Debug, Clone)]
pub struct OutgoingMessage (pub Notice);