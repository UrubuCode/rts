//! node:net — the `SocketAddress` class: an immutable `{address, family, port,
//! flowlabel}` value.
//!
//! It has no state beyond those four fields and no mutation, so the
//! `#[rtse::class]` instance just HOLDS them; the getters read the struct back.
//! Read-only follows for free: the class registers getters and no setters.

use std::net::IpAddr;

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::rtse_class_of;

use super::blocklist::rules::{family_of_type, parse_addr, AddressValue};
use crate::values::{opt_num, opt_str};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

fn throw(code: &str, class: &str, message: &str) {
    let msg = format!("{code}: {message}");
    unsafe {
        __rtsadp_throw_js_error(class.as_ptr(), class.len() as i64, msg.as_ptr(), msg.len() as i64);
    }
}

/// A port option must be an integer in `[0, 65535]`.
fn port_of(n: f64) -> Option<u16> {
    if !n.is_finite() || n.fract() != 0.0 || !(0.0..=65535.0).contains(&n) {
        throw(
            "ERR_SOCKET_BAD_PORT",
            "RangeError",
            &format!("Port should be >= 0 and < 65536. Received {n}."),
        );
        return None;
    }
    Some(n as u16)
}

/// A `SocketAddress` instance: the four immutable fields.
#[rtse::class("SocketAddress")]
#[derive(Clone)]
pub struct SocketAddress {
    address: IpAddr,
    port: u16,
    flowlabel: u32,
}

#[rtse::class("SocketAddress")]
impl SocketAddress {
    /// `new SocketAddress()` — Node's defaults: `127.0.0.1`, family `ipv4`, port 0.
    #[rtse::ctor]
    fn new() -> Self {
        SocketAddress {
            address: IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port: 0,
            flowlabel: 0,
        }
    }

    /// `new SocketAddress(options)` — `{address, family, port, flowlabel}`. The
    /// address default follows the family: `127.0.0.1` (ipv4) / `::` (ipv6).
    #[rtse::ctor(throws)]
    fn with_options(options: Handle) -> Option<Self> {
        let family = opt_str(options, "family").unwrap_or_else(|| "ipv4".to_string());
        let Some(v6) = family_of_type(&family) else {
            throw(
                "ERR_INVALID_ARG_VALUE",
                "TypeError",
                &format!("The argument 'family' is invalid. Received '{family}'"),
            );
            return None;
        };
        let text = opt_str(options, "address")
            .unwrap_or_else(|| if v6 { "::".to_string() } else { "127.0.0.1".to_string() });
        let Some(address) = parse_addr(&text, v6) else {
            throw(
                "ERR_INVALID_ADDRESS",
                "TypeError",
                &format!("Invalid address: '{text}'"),
            );
            return None;
        };
        let Some(port) = port_of(opt_num(options, "port").unwrap_or(0.0)) else {
            return None;
        };
        let flowlabel = opt_num(options, "flowlabel").unwrap_or(0.0).max(0.0) as u32;
        Some(SocketAddress { address, port, flowlabel })
    }

    /// `SocketAddress.parse(input)` (static) — `'1.2.3.4:80'` / `'[::1]:80'`.
    /// Returns `undefined` on any parse failure; never throws.
    #[rtse::statical]
    fn parse(input: &str) -> Option<Self> {
        AddressValue::parse(input).map(|v| SocketAddress {
            address: v.address,
            port: v.port,
            flowlabel: v.flowlabel,
        })
    }

    /// `socketaddress.address`.
    #[rtse::getter]
    fn address(&self) -> String {
        self.address.to_string()
    }

    /// `socketaddress.family`.
    #[rtse::getter]
    fn family(&self) -> String {
        if self.address.is_ipv6() { "ipv6" } else { "ipv4" }.to_string()
    }

    /// `socketaddress.port`.
    #[rtse::getter]
    fn port(&self) -> f64 {
        f64::from(self.port)
    }

    /// `socketaddress.flowlabel`.
    #[rtse::getter]
    fn flowlabel(&self) -> f64 {
        f64::from(self.flowlabel)
    }
}

/// The `address` of a `SocketAddress` instance — how another class reads one
/// passed where Node accepts `string | SocketAddress`.
pub fn address_of(handle: u64) -> Option<String> {
    if rtse_class_of(handle) != Some(SocketAddress::RTSE_CLASS) {
        return None;
    }
    rts_engine::heap::handles::with_rtse::<SocketAddress, _>(handle, |s| s.map(|s| s.address.to_string()))
}
