macro_rules! loopback_host_literal {
    () => {
        "127.0.0.1"
    };
}
macro_rules! default_host_ws_port_literal {
    () => {
        8787
    };
}
macro_rules! default_host_ws_path_literal {
    () => {
        "/athanor/v1/ws"
    };
}

// [protocol/network/loopback] [security/bind]
pub const LOOPBACK_HOST: &str = loopback_host_literal!();

// [protocol/host/port] [runtime/topology]
pub const DEFAULT_HOST_WS_PORT: u16 = default_host_ws_port_literal!();

// [protocol/host/path] [runtime/routing]
pub const DEFAULT_HOST_WS_PATH: &str = default_host_ws_path_literal!();
pub const HOST_ROOM_PATH_PREFIX: &str = "/room/";
pub const DEFAULT_HOST_URL: &str = concat!(
    "ws://",
    loopback_host_literal!(),
    ":",
    default_host_ws_port_literal!()
);

// [protocol/room/key] [security/path]
pub fn is_safe_room_key(value: &str) -> bool {
    value != "house"
        && !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'0'..=b'9' => true,
            b'-' => index > 0 && index + 1 < value.len(),
            _ => false,
        })
        && !value.contains("--")
}
