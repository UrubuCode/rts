//! Building a `dgram.Socket`: its prototype, `createSocket`, and the shared
//! constructor both it and `new dgram.Socket()` call.

use rts_core::entry::{self, Context};

pub(super) fn prototype(context: &mut Context) -> u64 {
    let parent = entry::make_prototype(context, "EventEmitter", &[]);
    // Registered as `"dgram.Socket"`, not the bare `"Socket"`: `net::socket`
    // registers its OWN, differently-shaped TCP `Socket` prototype under that
    // same bare name, and `make_prototype` is idempotent BY NAME — whichever
    // module's `namespace()` ran first won it. Install order puts `dgram`
    // before `net`, so every `net.Socket` instance was getting `dgram`'s UDP
    // method table (`bind`/`send`/…, no `connect`/`setTimeout`/`setNoDelay`),
    // which read as `sock.setTimeout is not a function`. The property name
    // programs see (`dgram.Socket`, `new dgram.Socket()`... actually just
    // `Socket` off the `dgram` namespace object) is unaffected — that comes
    // from the `put_member(namespace, "Socket", ctor)` below, a different key
    // space from this prototype registry.
    let made = entry::make_prototype(context, "dgram.Socket", super::METHODS);
    entry::set_prototype_in(context, made, parent);
    made
}

/// `dgram.createSocket(type | options, callback?)`.
pub(super) extern "C" fn create_socket(e: u64, _this: u64, a: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    let socket = construct(e, 0, a, 0, 0, 0);
    let absent = entry::undefined_value();
    if callback != absent {
        let on_fn = entry::with_runtime(|context| entry::get_member(context, socket, "on"));
        if on_fn != absent {
            let event = super::common::key("message");
            entry::call(on_fn, socket, event, callback, absent, absent);
        }
    }
    socket
}

/// `new dgram.Socket(type | options)` — not part of Node's public API
/// (`createSocket` is the only constructor a program should reach), kept as
/// the shared build [`create_socket`] and [`super::namespace`]'s registered
/// `Socket` both call, following the pattern every other class in this crate
/// uses.
pub(super) extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::registry::pump();
    // The type first, and a refusal ends the call: Node has no default here —
    // `createSocket()` with nothing, with `1`, with `['udp4']` or with `{}` is
    // `ERR_SOCKET_BAD_TYPE` every time. This defaulted to `udp4` instead, so a
    // program that asked for something impossible got a working IPv4 socket
    // and found out at the first datagram, if ever.
    let Some(kind) = super::args::socket_kind(options) else {
        crate::errors::socket_bad_type();
        return entry::undefined_value();
    };
    let bag = entry::with_runtime(|context| entry::string_in(context, options).is_none());
    let (reuse_raw, recv_buf, send_buf, lookup, receive_block_list, send_block_list) = entry::with_runtime(|context| {
        match bag {
            false => (entry::undefined_in(context), None, None, entry::undefined_in(context), entry::undefined_in(context), entry::undefined_in(context)),
            true => {
                let reuse = super::common::option_value(context, options, "reuseAddr");
                let recv_buf = super::common::option_num(context, options, "recvBufferSize");
                let send_buf = super::common::option_num(context, options, "sendBufferSize");
                let lookup = super::common::option_value(context, options, "lookup");
                let receive_block_list = super::common::option_value(context, options, "receiveBlockList");
                let send_block_list = super::common::option_value(context, options, "sendBlockList");
                (reuse, recv_buf, send_buf, lookup, receive_block_list, send_block_list)
            }
        }
    });
    // `lookup` — a custom resolver — is still not honored (see the module
    // doc's "Not implemented" list); Node itself validates it as a function
    // before ever trying to call it, so a caller supplying one gets told the
    // option is refused rather than seeing it silently ignored.
    if lookup != entry::undefined_value() {
        entry::throw_type_error("ERR_INVALID_ARG_VALUE: The property 'lookup' is not implemented");
        return entry::undefined_value();
    }
    // `receiveBlockList`/`sendBlockList` — validated the same way
    // `net.BlockList.isBlockList` does (an own `rules` array; see that
    // module's own doc for why that stand-in is good enough here): a value
    // with no `rules` array is refused with Node's own
    // `ERR_INVALID_ARG_TYPE` naming `net.BlockList`, matching the class
    // constructor validation Node does before a socket is ever built.
    for (option, name) in [(receive_block_list, "receiveBlockList"), (send_block_list, "sendBlockList")] {
        if option != entry::undefined_value() && !is_block_list_like(option) {
            entry::throw_type_error(&format!(
                "ERR_INVALID_ARG_TYPE: The \"options.{name}\" property must be an instance of net.BlockList. Received an instance that is not one"
            ));
            return entry::undefined_value();
        }
    }
    // Decoded OUTSIDE the borrow above: `to_boolean` is an ambient entry point
    // (it calls `with_current` itself), so decoding it while still inside
    // `with_runtime`'s closure would be the nested borrow this crate aborts on.
    let reuse_addr = entry::to_boolean(reuse_raw);
    let is_udp6 = kind == "udp6";
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = self_or_new(context, this, prototype);
        init_emitter(context, instance);
        super::common::set_bool(context, instance, "__udp6", is_udp6);
        super::common::set_bool(context, instance, "__reuseAddr", reuse_addr);
        super::common::set_bool(context, instance, "__bound", false);
        // Read by `send` to decide whether a destination is an argument or a
        // mistake; kept on the instance rather than in the registry because a
        // socket has one before it has a row there.
        super::common::set_bool(context, instance, "__connected", false);
        if receive_block_list != entry::undefined_in(context) {
            super::common::set_value(context, instance, "__receiveBlockList__", receive_block_list);
        }
        if send_block_list != entry::undefined_in(context) {
            super::common::set_value(context, instance, "__sendBlockList__", send_block_list);
        }
        // Applied by `bind`, once the real socket exists — `createSocket`
        // options carrying `recvBufferSize`/`sendBufferSize` are Node's
        // "set before the first bind" shape, stashed here rather than
        // applied now because there is no socket yet to apply them to.
        if let Some(size) = recv_buf {
            super::common::set_num(context, instance, "__wantRecvBuf", size);
        }
        if let Some(size) = send_buf {
            super::common::set_num(context, instance, "__wantSendBuf", size);
        }
        instance
    })
}

/// `__eventNames__` alongside `__events__` — `node:events`' own
/// `make_emitter` sets both when it builds an `EventEmitter` directly, and
/// `emitter.rs::remember_event_name` silently no-ops onto an absent array,
/// so a `Socket` built without this second field passes every listener
/// check while `eventNames()` always answers `[]`. Never caught before
/// `tests/node_dgram_socket.test.ts` ran this far — the same systemic gap
/// is open in `net`, `fs.watch` and `child_process`'s own instance builders
/// too, which is why the fix is here rather than in `events.rs`: those are
/// outside this module's cluster.
fn init_emitter(context: &mut Context, instance: u64) {
    let events = entry::make_object(context);
    entry::put_member(context, instance, "__events__", events);
    let names = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__eventNames__", names);
}

fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}

/// `net.BlockList.isBlockList`'s own duck-type check (an own `rules` array),
/// duplicated rather than called: `net::blocklist::is_block_list` is
/// `pub(super)` to `net`, the same cross-module visibility wall this
/// module's own doc names for `net::registry`/`socket`/`common`.
fn is_block_list_like(value: u64) -> bool {
    let absent = entry::undefined_value();
    entry::get_indexed(value, super::common::key("rules")) != absent
}
