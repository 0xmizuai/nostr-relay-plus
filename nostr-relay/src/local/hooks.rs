pub trait LocalStateHooks {
    fn on_wire_read() -> bool;
    fn on_wire_write() -> bool;

    fn on_db_write() -> bool;
    fn on_db_read() -> bool;

    fn on_send_global_broadcast_event() -> bool;
    fn on_receive_global_boradcast_event() -> bool;
}