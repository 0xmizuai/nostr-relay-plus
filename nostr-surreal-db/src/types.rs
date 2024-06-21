pub const NOSTR_EVENTS_TABLE: &str = "nostr_events";
pub const NOSTR_TAGS_TABLE: &str = "nostr_tags";

pub type Nonce = u64;
pub type Kind = u64;
pub type Tags = Vec<(String, String)>;
