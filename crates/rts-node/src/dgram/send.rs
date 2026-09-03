//! `socket.send(...)` — every overload, and the `sendBlockList` check that
//! guards it.

use rts_core::entry;

use super::args::{self, SendCall};
use super::common::{emit, get_bool, get_value, socket_id};
use super::registry;

/// `socket.send(msg, [offset, length,] [port,] [address,] [callback])` — every
/// overload, resolved and checked by [`args::send_call`].
///
/// Validation runs BEFORE the implicit bind below, which is Node's order and
/// matters: a refused call must not leave a socket bound that the program
/// never asked to bind.
pub(super) extern "C" fn send(_e: u64, this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    let Some(call) = args::send_call(get_bool(this, "__connected"), a, b, c, d) else {
        return absent;
    };
    if !get_bool(this, "__bound") {
        super::socket::bind(0, this, entry::make_number(0.0), absent, absent, 0);
    }
    let SendCall { bytes, port: port_num, address: target_host, callback } = call;
    // A destination refused by `sendBlockList` — Node delivers this as a
    // callback error / `'error'` event, never a synchronous throw (the
    // module doc's own note on why this test can only assert "did not throw
    // synchronously"), so the datagram is dropped here and the failure
    // travels the same two paths as any other send error below.
    let block_list = get_value(this, "__sendBlockList__");
    if block_list != absent {
        let host = target_host.clone().unwrap_or_else(|| "127.0.0.1".to_owned());
        if blocked_by(block_list, &host) {
            let error = entry::with_runtime(|context| {
                let object = entry::make_object(context);
                let message = entry::make_string(context, "ERR_SOCKET_BLOCKLIST: Destination address blocked");
                entry::put_member(context, object, "message", message);
                object
            });
            if callback != absent {
                entry::call(callback, absent, error, absent, absent, absent);
            } else {
                emit(this, "error", error, absent, absent);
            }
            return absent;
        }
    }
    let Some(id) = socket_id(this) else { return absent };
    let result = registry::with_sockets(|table| {
        let Some(entry) = table.get(&id) else { return Err("socket closed".to_owned()) };
        let Some(socket) = &entry.socket else { return Err("socket not bound".to_owned()) };
        match port_num {
            Some(port) => {
                let host = target_host.unwrap_or_else(|| if get_bool_static(&entry.instance) { "::1".to_owned() } else { "127.0.0.1".to_owned() });
                socket.send_to(&bytes, (host.as_str(), port)).map(|_| ()).map_err(|error| error.to_string())
            }
            // No port: the socket must already be connected via `connect()`.
            None => socket.send(&bytes).map(|_| ()).map_err(|error| error.to_string()),
        }
    });
    match result {
        Ok(()) if callback != absent => entry::call(callback, absent, absent, absent, absent, absent),
        Ok(()) => absent,
        Err(message) => {
            let error = entry::with_runtime(|context| {
                let object = entry::make_object(context);
                let message_v = entry::make_string(context, &message);
                entry::put_member(context, object, "message", message_v);
                object
            });
            if callback != absent {
                entry::call(callback, absent, error, absent, absent, absent)
            } else {
                emit(this, "error", error, absent, absent);
                absent
            }
        }
    }
}

// `send`'s host default branches on address family; the socket entry itself
// carries no such flag (only the JS instance does), so this reads it off the
// JS instance passed in — a plain member read, not an ambient-context call.
fn get_bool_static(instance: &u64) -> bool {
    get_bool(*instance, "__udp6")
}

/// Whether `host` is refused by `blocklist` (a stored `net.BlockList`
/// instance's real `check(address)` method — called through, never
/// reimplemented, per this crate's own reuse-check convention).
fn blocked_by(blocklist: u64, host: &str) -> bool {
    let absent = entry::undefined_value();
    let check_fn = entry::with_runtime(|context| entry::get_member(context, blocklist, "check"));
    if check_fn == absent {
        return false;
    }
    let address = entry::with_runtime(|context| entry::make_string(context, host));
    entry::to_boolean(entry::call(check_fn, blocklist, address, absent, absent, absent))
}
