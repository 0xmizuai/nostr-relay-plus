pub const NOSTR_EVENTS_TABLE: &str = "nostr_events";
pub const NOSTR_TAGS_TABLE: &str = "nostr_tags";


pub type Timestamp = u64;
pub type Nonce = u64;
pub type Bytes32 = [u8; 32];
pub type Kind = u64;
pub type Tags = Vec<(Vec<u8>, Vec<u8>)>;
