//! `net.SocketAddress` — an immutable value object. `address`/`family`/
//! `flowlabel`/`port` are plain data properties set once at construction and
//! never written again by any method here, which is what "immutable" means
//! for a hand-written class in this crate (no accessor/setter machinery
//! exists outside `#[rtse::class]` — see `stream/common.rs`'s own doc for
//! the same convention).

use rts_core::entry::{self, Provided};

const METHODS: &[(&str, Provided)] = &[];

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    entry::make_prototype(context, "SocketAddress", METHODS)
}

/// `new net.SocketAddress(options?)`.
pub(super) extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::common::self_or_new(context, this, prototype);
        let family = super::common::option_text(context, options, "family").unwrap_or_else(|| "ipv4".to_owned());
        let default_address = if family == "ipv6" { "::" } else { "127.0.0.1" };
        let address = super::common::option_text(context, options, "address").unwrap_or_else(|| default_address.to_owned());
        let port = super::common::option_num(context, options, "port").unwrap_or(0.0);
        let flowlabel = super::common::option_num(context, options, "flowlabel").unwrap_or(0.0);
        fill(context, instance, &address, &family, port, flowlabel);
        instance
    })
}

fn fill(context: &mut entry::Context, instance: u64, address: &str, family: &str, port: f64, flowlabel: f64) {
    let address_v = entry::make_string(context, address);
    let family_v = entry::make_string(context, family);
    entry::put_member(context, instance, "address", address_v);
    entry::put_member(context, instance, "family", family_v);
    super::common::set_num(context, instance, "port", port);
    super::common::set_num(context, instance, "flowlabel", flowlabel);
}

/// `net.SocketAddress.parse(input)` — `"ip:port"` / `"[ipv6]:port"`.
/// `undefined` on parse failure, never throws, matching the reference doc.
pub(super) extern "C" fn parse(_e: u64, _this: u64, input: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(text) = entry::text_of(input) else { return absent };
    let Some((address, port, family)) = split_host_port(&text) else { return absent };
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = entry::make_instance(context, prototype);
        fill(context, instance, &address, family, port as f64, 0.0);
        instance
    })
}

fn split_host_port(text: &str) -> Option<(String, u16, &'static str)> {
    if let Some(rest) = text.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = after.strip_prefix(':')?.parse::<u16>().ok()?;
        host.parse::<std::net::Ipv6Addr>().ok()?;
        return Some((host.to_owned(), port, "ipv6"));
    }
    let (host, port) = text.rsplit_once(':')?;
    let port = port.parse::<u16>().ok()?;
    host.parse::<std::net::Ipv4Addr>().ok()?;
    Some((host.to_owned(), port, "ipv4"))
}
