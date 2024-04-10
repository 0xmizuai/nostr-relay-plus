use nostr_surreal_db::message::events::Event;

use super::LocalState;

pub trait LocalStateHooks {
    fn auth_on_db_write(&self, e: &Event) -> bool;
    fn auth_on_db_read(&self) -> bool;

    fn auth_on_send_global_broadcast_event(&self, e: &Event) -> bool;
    fn auth_on_receive_global_boradcast_event(&self) -> bool;
}

impl LocalStateHooks for LocalState {
    fn auth_on_db_write(&self, _e: &Event) -> bool {
        true
    }

    fn auth_on_db_read(&self) -> bool {
        true
    }

    fn auth_on_send_global_broadcast_event(&self, _e: &Event) -> bool {
        true
    }

    fn auth_on_receive_global_boradcast_event(&self) -> bool {
        true
    }
}