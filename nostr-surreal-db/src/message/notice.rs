use serde_json::{json, Value};
use nostr_plus_common::types::SubscriptionId;
use crate::message::events::Event;

#[derive(Debug, Clone)]
pub enum EventResultStatus {
    Saved,
    Duplicate,
    Invalid,
    Blocked,
    RateLimited,
    Error,
    Restricted,
}

#[derive(Debug, Clone)]
pub struct EventResult {
    pub id: String,
    pub msg: String,
    pub status: EventResultStatus,
}

#[derive(Debug, Clone)]
pub enum Notice {
    Message(String),
    EventResult(EventResult),
    AuthChallenge(String),
    Eose(String),
    Event((SubscriptionId, Event)),
}

impl EventResultStatus {
    #[must_use]
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Duplicate | Self::Saved => true,
            Self::Invalid | Self::Blocked | Self::RateLimited | Self::Error | Self::Restricted => false,
        }
    }

    #[must_use]
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::Duplicate => "duplicate",
            Self::Invalid => "invalid",
            Self::Blocked => "blocked",
            Self::RateLimited => "rate-limited",
            Self::Error => "error",
            Self::Restricted => "restricted",
        }
    }
}

impl Notice {
    #[must_use]
    pub fn message(msg: String) -> Notice {
        Notice::Message(msg)
    }

    fn prefixed(id: String, msg: &str, status: EventResultStatus) -> Notice {
        let msg = format!("{}: {}", status.prefix(), msg);
        Notice::EventResult(EventResult { id, msg, status })
    }

    #[must_use]
    pub fn invalid(id: String, msg: &str) -> Notice {
        Notice::prefixed(id, msg, EventResultStatus::Invalid)
    }

    #[must_use]
    pub fn blocked(id: String, msg: &str) -> Notice {
        Notice::prefixed(id, msg, EventResultStatus::Blocked)
    }

    #[must_use]
    pub fn rate_limited(id: String, msg: &str) -> Notice {
        Notice::prefixed(id, msg, EventResultStatus::RateLimited)
    }

    #[must_use]
    pub fn duplicate(id: String) -> Notice {
        Notice::prefixed(id, "", EventResultStatus::Duplicate)
    }

    #[must_use]
    pub fn error(id: String, msg: &str) -> Notice {
        Notice::prefixed(id, msg, EventResultStatus::Error)
    }

    #[must_use]
    pub fn restricted(id: String, msg: &str) -> Notice {
        Notice::prefixed(id, msg, EventResultStatus::Restricted)
    }

    #[must_use]
    pub fn saved(id: String) -> Notice {
        Notice::EventResult(EventResult {
            id,
            msg: "".into(),
            status: EventResultStatus::Saved,
        })
    }

    pub fn event(sub_id: SubscriptionId, event: Event) -> Notice {
        Notice::Event((sub_id, event))
    }

    pub fn get_event_kind(&self) -> Option<u16> {
        match &self {
            Notice::Event((_, ev)) => Some(ev.kind),
            _ => None,
        }
    }

    pub fn eose(id: String) -> Notice {
        Notice::Eose(id)
    }

    pub fn to_json(&self) -> Value {
        match &self {
            Notice::Message(ref msg) => json!(["NOTICE", msg]),
            Notice::EventResult(ref res) => json!(["OK", res.id, res.status.to_bool(), res.msg]),
            Notice::AuthChallenge(ref challenge) => json!(["AUTH", challenge]),
            Notice::Eose(ref id) => json!(["EOSE", id]),
            Notice::Event((sub_id, event)) => json!(["EVENT", sub_id, event]),
        }
    }

    pub fn to_string(&self) -> String {
        self.to_json().to_string()
    }
}
