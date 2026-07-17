//! node:dgram — `createSocket(type | options[, listener])`: allocates the real
//! OS socket and the JS-visible `Socket` object.
//!
//! Node has no public `Socket` constructor — the factory is the only entry, and
//! it is overloaded BY TYPE at one arity (`'udp4'` vs `{ type: 'udp4', … }`).
//! The Registry keys overloads by arity, so both forms land on ONE member taking
//! a `PolyValue` and the shape is decided here, off the value's tag (see
//! [`crate::values::Val`]).

use std::sync::Arc;

use socket2::{Domain, Protocol, Socket, Type};

use rts_engine::heap::handles::{alloc_entry, Entry};

use crate::emitter;
use super::errors;
use super::state::{self, SocketState};
use crate::net::blocklist::rules::Rule;
use crate::values::{opt_bool, opt_has, opt_num, opt_str, opt_word, val, Val};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_PIN_HANDLE(handle: u64);
}

/// The options `createSocket` accepts, already normalized.
struct Options {
    v6: bool,
    reuse_addr: bool,
    reuse_port: bool,
    ipv6_only: bool,
    recv_buffer_size: Option<usize>,
    send_buffer_size: Option<usize>,
    /// `receiveBlockList` — inbound datagrams from a matching sender are dropped.
    receive_block_list: Option<Vec<Rule>>,
    /// `sendBlockList` — outbound datagrams to a matching destination are refused.
    send_block_list: Option<Vec<Rule>>,
}

impl Options {
    fn for_type(kind: &str) -> Option<Self> {
        let v6 = match kind {
            "udp4" => false,
            "udp6" => true,
            _ => return None,
        };
        Some(Self {
            v6,
            reuse_addr: false,
            reuse_port: false,
            ipv6_only: false,
            recv_buffer_size: None,
            send_buffer_size: None,
            receive_block_list: None,
            send_block_list: None,
        })
    }

    /// Read a `SocketOptions` object. `type` is required — Node throws
    /// `ERR_SOCKET_BAD_TYPE` otherwise.
    fn from_object(obj: u64) -> Result<Self, Bad> {
        let kind = opt_str(obj, "type").ok_or(Bad::Type)?;
        let mut o = Self::for_type(&kind).ok_or(Bad::Type)?;
        o.reuse_addr = opt_bool(obj, "reuseAddr");
        o.reuse_port = opt_bool(obj, "reusePort");
        o.ipv6_only = opt_bool(obj, "ipv6Only");
        o.recv_buffer_size = opt_num(obj, "recvBufferSize").map(|n| n.max(0.0) as usize);
        o.send_buffer_size = opt_num(obj, "sendBufferSize").map(|n| n.max(0.0) as usize);
        // The block lists are `net.BlockList` instances: take a SNAPSHOT of their
        // rules, matching Node — the option is read once, at socket creation.
        o.receive_block_list = block_list_arg(obj, "receiveBlockList")?;
        o.send_block_list = block_list_arg(obj, "sendBlockList")?;
        for unsupported in ["lookup", "signal"] {
            if opt_has(obj, unsupported) {
                // An honest refusal beats silently ignoring an option the caller
                // set: these are DEFERRED, see the module doc in `mod.rs`.
                return Err(Bad::Unsupported(unsupported));
            }
        }
        Ok(o)
    }
}

/// Read a `receiveBlockList`/`sendBlockList` option: absent → `None`; a real
/// `net.BlockList` → its rules; anything else → Node's arg-type error.
fn block_list_arg(obj: u64, key: &'static str) -> Result<Option<Vec<Rule>>, Bad> {
    let Some(word) = opt_word(obj, key) else {
        return Ok(None);
    };
    if val(word).is_nullish() {
        return Ok(None);
    }
    match val(word) {
        Val::Obj(h) => match crate::net::blocklist::rules_of(h) {
            Some(rules) => Ok(Some(rules)),
            None => Err(Bad::NotABlockList(key)),
        },
        _ => Err(Bad::NotABlockList(key)),
    }
}

/// Why a `createSocket` argument was rejected.
enum Bad {
    /// Not `'udp4'`/`'udp6'` (or no `type` at all).
    Type,
    /// A block-list option that is not a `net.BlockList`.
    NotABlockList(&'static str),
    /// A real Node option RTS has not implemented yet.
    Unsupported(&'static str),
}

impl Bad {
    fn throw(self) {
        match self {
            Bad::Type => errors::throw(
                "ERR_SOCKET_BAD_TYPE",
                "Bad socket type specified. Valid types are: udp4, udp6",
            ),
            Bad::NotABlockList(name) => errors::throw(
                "ERR_INVALID_ARG_TYPE",
                &format!("The \"options.{name}\" property must be an instance of net.BlockList"),
            ),
            Bad::Unsupported(name) => errors::throw(
                "ERR_INVALID_ARG_VALUE",
                &format!(
                    "createSocket: the '{name}' option is not implemented yet in RTS — hostname \
                     resolution uses the system resolver, and socket.close() replaces an \
                     AbortSignal"
                ),
            ),
        }
    }
}

/// Build the OS socket + the JS object for the given options.
fn build(opts: Options) -> u64 {
    let domain = if opts.v6 { Domain::IPV6 } else { Domain::IPV4 };
    let sock = match Socket::new(domain, Type::DGRAM, Some(Protocol::UDP)) {
        Ok(s) => s,
        Err(e) => {
            errors::throw_io(&e, "socket");
            return 0;
        }
    };
    // Options that must be set BEFORE bind (Node applies them at bind time; the
    // OS requires them before the bind syscall).
    if opts.reuse_addr && let Err(e) = sock.set_reuse_address(true) {
        errors::throw_io(&e, "setsockopt(SO_REUSEADDR)");
        return 0;
    }
    if opts.reuse_port && let Err(e) = set_reuse_port(&sock) {
        errors::throw_io(&e, "setsockopt(SO_REUSEPORT)");
        return 0;
    }
    if opts.v6 && opts.ipv6_only && let Err(e) = sock.set_only_v6(true) {
        errors::throw_io(&e, "setsockopt(IPV6_V6ONLY)");
        return 0;
    }
    if let Some(size) = opts.recv_buffer_size && let Err(e) = sock.set_recv_buffer_size(size) {
        errors::throw(errors::BUFFER_SIZE, &format!("Could not set buffer size: {e}"));
        return 0;
    }
    if let Some(size) = opts.send_buffer_size && let Err(e) = sock.set_send_buffer_size(size) {
        errors::throw(errors::BUFFER_SIZE, &format!("Could not set buffer size: {e}"));
        return 0;
    }

    let handle = alloc_object();
    let mut st = SocketState::new(sock, opts.v6);
    st.receive_block_list = opts.receive_block_list;
    st.send_block_list = opts.send_block_list;
    state::insert(handle, Arc::new(st));
    // An open socket (and therefore its object) stays alive until close().
    unsafe { __RTS_FN_NS_GC_PIN_HANDLE(handle) };
    handle
}

/// SO_REUSEPORT where the platform has it. Windows has no equivalent (Node
/// documents `reusePort` as Linux 3.9+/DragonFlyBSD/FreeBSD/Solaris/AIX only), so
/// the request fails with the real "protocol option not supported" errno rather
/// than being silently dropped.
#[cfg(unix)]
fn set_reuse_port(sock: &Socket) -> std::io::Result<()> {
    sock.set_reuse_port(true)
}

#[cfg(not(unix))]
fn set_reuse_port(_sock: &Socket) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "SO_REUSEPORT is not supported on this platform",
    ))
}

/// The JS-visible `Socket`: an object-backed Registry class instance.
fn alloc_object() -> u64 {
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(b"Socket".to_vec())) as i64,
    );
    alloc_entry(Entry::Map(Box::new(m)))
}

/// The `type`-or-`options` first argument → normalized options. Overload
/// resolution BY VALUE: a string is the `type` form, an object the `options`
/// form — exactly the branch Node's own JS does.
fn parse_first(arg: u64) -> Result<Options, Bad> {
    match val(arg) {
        Val::Str(kind) => Options::for_type(&kind).ok_or(Bad::Type),
        Val::Obj(obj) => Options::from_object(obj),
        _ => Err(Bad::Type),
    }
}

/// `dgram.createSocket(type)` / `dgram.createSocket(options)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_CREATE_SOCKET(arg: u64) -> u64 {
    match parse_first(arg) {
        Ok(opts) => build(opts),
        Err(bad) => {
            bad.throw();
            0
        }
    }
}

/// `dgram.createSocket(type, listener)` / `createSocket(options, listener)` —
/// `listener` is attached as a `'message'` listener.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_CREATE_SOCKET_CB(arg: u64, listener: u64) -> u64 {
    let handle = __RTS_FN_NODE_DGRAM_CREATE_SOCKET(arg);
    if handle != 0 && let Val::Func(cb) = val(listener) {
        emitter::add(handle, "message", cb, false, false);
    }
    handle
}
