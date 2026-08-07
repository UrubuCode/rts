//! `node:http2` — against `docs/reference/node/http2.md`.
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md`'s table is entirely about value
//! encodings, shapes and ABI inside `rts-cranelift`/`rts-core-rwk` — none of
//! which a binary wire-protocol parser or a compression table touches, so
//! the machine layer has nothing to call instead of writing
//! [`frame`]/[`hpack`] by hand, which `http2.md` §5.1 specifies anyway (no
//! HPACK dependency is in `Cargo.toml`, and this module adds none). Inside
//! this crate, `http/parser.rs` is the worked example this module's
//! [`frame`] follows for the shape a hand-written wire parser should have
//! here: plain Rust in, plain Rust out, no `Context` touched until a caller
//! decides to act on the result — see that file's own module doc for the
//! two mistakes a naive parser makes that this module avoids the same way
//! (case-insensitive header lookup reused as-is is not applicable to HTTP/2,
//! which forbids mixed-case names outright; the chunked-body "ends on a
//! sentinel, not on EOF" lesson has its HPACK-side analogue in
//! `hpack/huffman.rs`'s padding check, which distinguishes truncated real
//! content from the required all-1s padding rather than accepting either).
//! `tls/mod.rs`'s own doc already names `https`/`http2` as "not built on this
//! yet" — this module does not change that.
//!
//! # What is here
//!
//! [`frame`] — the 9-byte frame header, the connection preface, and
//! `SETTINGS`/`HEADERS`/`DATA`/`WINDOW_UPDATE`/`RST_STREAM`/`GOAWAY`/`PING`/
//! `PRIORITY` framing, all real and unit-tested against RFC 9113's own wire
//! shapes.
//!
//! [`hpack`] — RFC 7541 in full: the static table (Appendix A), the integer
//! and string primitives (§5.1/§5.2) with continuation-bit and Huffman
//! handling, the dynamic table with eviction (§4), and a [`hpack::Decoder`]/
//! [`hpack::Encoder`] pair covering all four header-field representations
//! plus the dynamic-table-size-update pseudo-field (§6). Huffman **decoding**
//! works and is required — see `hpack/huffman.rs`'s module doc for why a
//! peer's Huffman-coded strings must decode even though this module never
//! Huffman-*encodes* its own output.
//!
//! `getDefaultSettings`/`getPackedSettings`/`getUnpackedSettings` are pure
//! functions over [`frame::parse_settings`]/[`frame::write_settings`] and a
//! fixed default table — no session, no socket, so they are wired below.
//! `http2.constants` is wired in full (§2.3 of the spec doc).
//!
//! # Not implemented, by name
//!
//! **The entire session/stream lifecycle**: `connect`, `createServer`,
//! `createSecureServer`, `performServerHandshake`, `Http2Session` (and
//! `ServerHttp2Session`/`ClientHttp2Session`), `Http2Stream` (and
//! `ClientHttp2Stream`/`ServerHttp2Stream`), `Http2Server`/
//! `Http2SecureServer`, and the Compatibility API
//! (`Http2ServerRequest`/`Http2ServerResponse`). None of these classes are
//! constructed by this module and no JS program can obtain one.
//!
//! This is a deliberate stop, not an oversight: a session needs a live
//! connection preface handshake, `SETTINGS` exchange, per-session/per-stream
//! flow-control windows, a multiplexed frame dispatch loop over
//! `node:net`/`node:tls` (owned by other agents' modules under this crate's
//! shared file lock, per this task's own instructions), the rapid-reset
//! (CVE-2023-44487) mitigation `http2.md` §4 calls mandatory, and the
//! `EventEmitter`/`Duplex` chaining `set_prototype_in` provides only once
//! there is a real object to chain — building a session object that emits
//! `'stream'` for frames it never actually multiplexed, or that claims
//! `close()`/`goaway()` without an owned socket loop behind them, is exactly
//! the "claims a session and drops frames it never learned" failure this
//! task's own instructions rule out. What is real here — framing and HPACK —
//! is what a session implementation would be built on next.
//!
//! `http2.sensitiveHeaders` is also not implemented: it is a well-known
//! *computed symbol* key on a headers object, and
//! `rts-core-rwk/src/entry/modules.rs` (the only value API this module may
//! use) exposes no symbol constructor — only `rts-core-rwk`'s own
//! `#[rtse::class]` natives can mint one (see that crate's own symbol
//! module), which is out of reach for a host crate without changing a file
//! this task does not own.
//!
//! **Padding, `ALTSVC`, `ORIGIN`, `PUSH_PROMISE`, `CONTINUATION`, extended
//! CONNECT (RFC 8441)**: [`frame`] strips the padding/priority prefix HPACK
//! callers don't need but never generates padding on write, and does not
//! parse or emit `ALTSVC`/`ORIGIN`/`PUSH_PROMISE`/`CONTINUATION` — see
//! `frame.rs`'s own doc for exactly why `CONTINUATION`'s absence matters
//! (a `HEADERS` frame without `END_HEADERS` is never reassembled here).

pub mod frame;
pub mod hpack;

use rts_core_rwk::entry::{self, Provided};

/// The namespace `node:http2` is — `constants` and the three pure
/// settings-table functions only; see the module doc for everything else.
pub fn namespace(context: &mut entry::Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("getDefaultSettings", get_default_settings),
        ("getPackedSettings", get_packed_settings),
        ("getUnpackedSettings", get_unpacked_settings),
    ];
    let namespace = entry::make_namespace(context, members);
    let constants = constants_object(context);
    entry::put_member(context, namespace, "constants", constants);
    namespace
}

/// The default `SettingsObject` — `http2.md` §3's documented defaults.
fn default_settings() -> [(u16, u32); 6] {
    [
        (0x1, 4096),      // headerTableSize
        (0x2, 1),         // enablePush (true)
        (0x3, 4_294_967_295), // maxConcurrentStreams (Infinity, saturated to u32::MAX)
        (0x4, 65535),     // initialWindowSize
        (0x5, 16384),     // maxFrameSize
        (0x6, 65535),     // maxHeaderListSize
    ]
}

extern "C" fn get_default_settings(e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let _ = e;
    entry::with_runtime(|context| settings_object(context, &default_settings()))
}

fn settings_object(context: &mut entry::Context, pairs: &[(u16, u32)]) -> u64 {
    let object = entry::make_object(context);
    for &(id, value) in pairs {
        if let Some(name) = setting_name(id) {
            let number = entry::make_number(f64::from(value));
            entry::put_member(context, object, name, number);
        }
    }
    object
}

fn setting_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x1 => "headerTableSize",
        0x2 => "enablePush",
        0x3 => "maxConcurrentStreams",
        0x4 => "initialWindowSize",
        0x5 => "maxFrameSize",
        0x6 => "maxHeaderListSize",
        0x8 => "enableConnectProtocol",
        _ => return None,
    })
}

fn setting_id(name: &str) -> Option<u16> {
    Some(match name {
        "headerTableSize" => 0x1,
        "enablePush" => 0x2,
        "maxConcurrentStreams" => 0x3,
        "initialWindowSize" => 0x4,
        "maxFrameSize" => 0x5,
        "maxHeaderListSize" => 0x6,
        "enableConnectProtocol" => 0x8,
        _ => return None,
    })
}

/// `http2.getPackedSettings(settings?)` — 6 bytes per setting (2-byte id,
/// 4-byte value), no session or socket required, matching `http2.md` §2.2.
extern "C" fn get_packed_settings(_e: u64, _this: u64, settings: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let pairs = read_settings_object(context, settings).unwrap_or_else(|| default_settings().to_vec());
        let bytes = frame::write_settings(&pairs);
        entry::make_bytes(context, &bytes)
    })
}

fn read_settings_object(context: &mut entry::Context, value: u64) -> Option<Vec<(u16, u32)>> {
    if !entry::is_object(context, value) {
        return None;
    }
    let mut pairs = Vec::new();
    for &name in &[
        "headerTableSize",
        "enablePush",
        "maxConcurrentStreams",
        "initialWindowSize",
        "maxFrameSize",
        "maxHeaderListSize",
        "enableConnectProtocol",
    ] {
        let member = entry::get_member(context, value, name);
        if let Some(number) = entry::number_of(member)
            && let Some(id) = setting_id(name)
        {
            pairs.push((id, number as u32));
        }
    }
    Some(pairs)
}

/// `http2.getUnpackedSettings(buf)` — the read half of
/// [`get_packed_settings`].
extern "C" fn get_unpacked_settings(_e: u64, _this: u64, buf: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(bytes) = entry::bytes_of(context, buf) else {
            return entry::undefined_in(context);
        };
        let pairs = frame::parse_settings(&bytes);
        settings_object(context, &pairs)
    })
}

/// `http2.constants` — session type, RST_STREAM/GOAWAY error codes, padding
/// strategy, pseudo/well-known header names, methods and statuses, all from
/// `http2.md` §2.3. Values are literal per the spec doc; nothing here is
/// derived from a live session because none exists.
fn constants_object(context: &mut entry::Context) -> u64 {
    let object = entry::make_object(context);
    let put = |ctx: &mut entry::Context, name: &str, value: f64| {
        let number = entry::make_number(value);
        entry::put_member(ctx, object, name, number);
    };

    put(context, "NGHTTP2_SESSION_SERVER", 0.0);
    put(context, "NGHTTP2_SESSION_CLIENT", 1.0);

    let error_codes: &[(&str, u32)] = &[
        ("NGHTTP2_NO_ERROR", 0x00),
        ("NGHTTP2_PROTOCOL_ERROR", 0x01),
        ("NGHTTP2_INTERNAL_ERROR", 0x02),
        ("NGHTTP2_FLOW_CONTROL_ERROR", 0x03),
        ("NGHTTP2_SETTINGS_TIMEOUT", 0x04),
        ("NGHTTP2_STREAM_CLOSED", 0x05),
        ("NGHTTP2_FRAME_SIZE_ERROR", 0x06),
        ("NGHTTP2_REFUSED_STREAM", 0x07),
        ("NGHTTP2_CANCEL", 0x08),
        ("NGHTTP2_COMPRESSION_ERROR", 0x09),
        ("NGHTTP2_CONNECT_ERROR", 0x0a),
        ("NGHTTP2_ENHANCE_YOUR_CALM", 0x0b),
        ("NGHTTP2_INADEQUATE_SECURITY", 0x0c),
        ("NGHTTP2_HTTP_1_1_REQUIRED", 0x0d),
    ];
    for &(name, value) in error_codes {
        put(context, name, f64::from(value));
    }

    put(context, "PADDING_STRATEGY_NONE", 0.0);
    put(context, "PADDING_STRATEGY_ALIGNED", 1.0);
    put(context, "PADDING_STRATEGY_MAX", 2.0);
    put(context, "PADDING_STRATEGY_CALLBACK", 1.0); // alias, matches Node's ALIGNED/CALLBACK overlap

    for &(name, wire) in header_name_constants() {
        let text = entry::make_string(context, wire);
        entry::put_member(context, object, name, text);
    }
    for &(name, wire) in method_constants() {
        let text = entry::make_string(context, wire);
        entry::put_member(context, object, name, text);
    }
    for &(name, code) in status_constants() {
        put(context, name, f64::from(code));
    }
    object
}

fn header_name_constants() -> &'static [(&'static str, &'static str)] {
    &[
        ("HTTP2_HEADER_STATUS", ":status"),
        ("HTTP2_HEADER_METHOD", ":method"),
        ("HTTP2_HEADER_AUTHORITY", ":authority"),
        ("HTTP2_HEADER_SCHEME", ":scheme"),
        ("HTTP2_HEADER_PATH", ":path"),
        ("HTTP2_HEADER_PROTOCOL", ":protocol"),
        ("HTTP2_HEADER_ACCEPT_ENCODING", "accept-encoding"),
        ("HTTP2_HEADER_ACCEPT_LANGUAGE", "accept-language"),
        ("HTTP2_HEADER_ACCEPT_RANGES", "accept-ranges"),
        ("HTTP2_HEADER_ACCEPT", "accept"),
        ("HTTP2_HEADER_ACCESS_CONTROL_ALLOW_CREDENTIALS", "access-control-allow-credentials"),
        ("HTTP2_HEADER_ACCESS_CONTROL_ALLOW_HEADERS", "access-control-allow-headers"),
        ("HTTP2_HEADER_ACCESS_CONTROL_ALLOW_METHODS", "access-control-allow-methods"),
        ("HTTP2_HEADER_ACCESS_CONTROL_ALLOW_ORIGIN", "access-control-allow-origin"),
        ("HTTP2_HEADER_ACCESS_CONTROL_EXPOSE_HEADERS", "access-control-expose-headers"),
        ("HTTP2_HEADER_ACCESS_CONTROL_MAX_AGE", "access-control-max-age"),
        ("HTTP2_HEADER_ACCESS_CONTROL_REQUEST_HEADERS", "access-control-request-headers"),
        ("HTTP2_HEADER_ACCESS_CONTROL_REQUEST_METHOD", "access-control-request-method"),
        ("HTTP2_HEADER_AGE", "age"),
        ("HTTP2_HEADER_AUTHORIZATION", "authorization"),
        ("HTTP2_HEADER_CACHE_CONTROL", "cache-control"),
        ("HTTP2_HEADER_CONNECTION", "connection"),
        ("HTTP2_HEADER_CONTENT_DISPOSITION", "content-disposition"),
        ("HTTP2_HEADER_CONTENT_ENCODING", "content-encoding"),
        ("HTTP2_HEADER_CONTENT_LENGTH", "content-length"),
        ("HTTP2_HEADER_CONTENT_TYPE", "content-type"),
        ("HTTP2_HEADER_COOKIE", "cookie"),
        ("HTTP2_HEADER_DATE", "date"),
        ("HTTP2_HEADER_ETAG", "etag"),
        ("HTTP2_HEADER_FORWARDED", "forwarded"),
        ("HTTP2_HEADER_HOST", "host"),
        ("HTTP2_HEADER_IF_MODIFIED_SINCE", "if-modified-since"),
        ("HTTP2_HEADER_IF_NONE_MATCH", "if-none-match"),
        ("HTTP2_HEADER_IF_RANGE", "if-range"),
        ("HTTP2_HEADER_LAST_MODIFIED", "last-modified"),
        ("HTTP2_HEADER_LINK", "link"),
        ("HTTP2_HEADER_LOCATION", "location"),
        ("HTTP2_HEADER_RANGE", "range"),
        ("HTTP2_HEADER_REFERER", "referer"),
        ("HTTP2_HEADER_SERVER", "server"),
        ("HTTP2_HEADER_SET_COOKIE", "set-cookie"),
        ("HTTP2_HEADER_STRICT_TRANSPORT_SECURITY", "strict-transport-security"),
        ("HTTP2_HEADER_TRANSFER_ENCODING", "transfer-encoding"),
        ("HTTP2_HEADER_TE", "te"),
        ("HTTP2_HEADER_UPGRADE", "upgrade"),
        ("HTTP2_HEADER_USER_AGENT", "user-agent"),
        ("HTTP2_HEADER_VARY", "vary"),
        ("HTTP2_HEADER_X_CONTENT_TYPE_OPTIONS", "x-content-type-options"),
        ("HTTP2_HEADER_X_FRAME_OPTIONS", "x-frame-options"),
        ("HTTP2_HEADER_KEEP_ALIVE", "keep-alive"),
        ("HTTP2_HEADER_PROXY_AUTHENTICATE", "proxy-authenticate"),
        ("HTTP2_HEADER_PROXY_AUTHORIZATION", "proxy-authorization"),
        ("HTTP2_HEADER_X_XSS_PROTECTION", "x-xss-protection"),
        ("HTTP2_HEADER_ALT_SVC", "alt-svc"),
    ]
}

fn method_constants() -> &'static [(&'static str, &'static str)] {
    &[
        ("HTTP2_METHOD_ACL", "ACL"),
        ("HTTP2_METHOD_BASELINE_CONTROL", "BASELINE-CONTROL"),
        ("HTTP2_METHOD_BIND", "BIND"),
        ("HTTP2_METHOD_CHECKIN", "CHECKIN"),
        ("HTTP2_METHOD_CHECKOUT", "CHECKOUT"),
        ("HTTP2_METHOD_CONNECT", "CONNECT"),
        ("HTTP2_METHOD_COPY", "COPY"),
        ("HTTP2_METHOD_DELETE", "DELETE"),
        ("HTTP2_METHOD_GET", "GET"),
        ("HTTP2_METHOD_HEAD", "HEAD"),
        ("HTTP2_METHOD_LABEL", "LABEL"),
        ("HTTP2_METHOD_LOCK", "LOCK"),
        ("HTTP2_METHOD_MERGE", "MERGE"),
        ("HTTP2_METHOD_MKACTIVITY", "MKACTIVITY"),
        ("HTTP2_METHOD_MKCALENDAR", "MKCALENDAR"),
        ("HTTP2_METHOD_MKCOL", "MKCOL"),
        ("HTTP2_METHOD_MKREDIRECTREF", "MKREDIRECTREF"),
        ("HTTP2_METHOD_MKWORKSPACE", "MKWORKSPACE"),
        ("HTTP2_METHOD_MOVE", "MOVE"),
        ("HTTP2_METHOD_OPTIONS", "OPTIONS"),
        ("HTTP2_METHOD_ORDERPATCH", "ORDERPATCH"),
        ("HTTP2_METHOD_PATCH", "PATCH"),
        ("HTTP2_METHOD_POST", "POST"),
        ("HTTP2_METHOD_PRI", "PRI"),
        ("HTTP2_METHOD_PROPFIND", "PROPFIND"),
        ("HTTP2_METHOD_PROPPATCH", "PROPPATCH"),
        ("HTTP2_METHOD_PUT", "PUT"),
        ("HTTP2_METHOD_REPORT", "REPORT"),
        ("HTTP2_METHOD_SEARCH", "SEARCH"),
        ("HTTP2_METHOD_TRACE", "TRACE"),
        ("HTTP2_METHOD_UNCHECKOUT", "UNCHECKOUT"),
        ("HTTP2_METHOD_UNLINK", "UNLINK"),
        ("HTTP2_METHOD_UNLOCK", "UNLOCK"),
        ("HTTP2_METHOD_UPDATE", "UPDATE"),
        ("HTTP2_METHOD_VERSION_CONTROL", "VERSION-CONTROL"),
    ]
}

fn status_constants() -> &'static [(&'static str, u32)] {
    &[
        ("HTTP_STATUS_CONTINUE", 100),
        ("HTTP_STATUS_SWITCHING_PROTOCOLS", 101),
        ("HTTP_STATUS_OK", 200),
        ("HTTP_STATUS_CREATED", 201),
        ("HTTP_STATUS_ACCEPTED", 202),
        ("HTTP_STATUS_NON_AUTHORITATIVE_INFORMATION", 203),
        ("HTTP_STATUS_NO_CONTENT", 204),
        ("HTTP_STATUS_RESET_CONTENT", 205),
        ("HTTP_STATUS_PARTIAL_CONTENT", 206),
        ("HTTP_STATUS_MULTIPLE_CHOICES", 300),
        ("HTTP_STATUS_MOVED_PERMANENTLY", 301),
        ("HTTP_STATUS_FOUND", 302),
        ("HTTP_STATUS_SEE_OTHER", 303),
        ("HTTP_STATUS_NOT_MODIFIED", 304),
        ("HTTP_STATUS_USE_PROXY", 305),
        ("HTTP_STATUS_TEMPORARY_REDIRECT", 307),
        ("HTTP_STATUS_PERMANENT_REDIRECT", 308),
        ("HTTP_STATUS_BAD_REQUEST", 400),
        ("HTTP_STATUS_UNAUTHORIZED", 401),
        ("HTTP_STATUS_PAYMENT_REQUIRED", 402),
        ("HTTP_STATUS_FORBIDDEN", 403),
        ("HTTP_STATUS_NOT_FOUND", 404),
        ("HTTP_STATUS_METHOD_NOT_ALLOWED", 405),
        ("HTTP_STATUS_NOT_ACCEPTABLE", 406),
        ("HTTP_STATUS_PROXY_AUTHENTICATION_REQUIRED", 407),
        ("HTTP_STATUS_REQUEST_TIMEOUT", 408),
        ("HTTP_STATUS_CONFLICT", 409),
        ("HTTP_STATUS_GONE", 410),
        ("HTTP_STATUS_LENGTH_REQUIRED", 411),
        ("HTTP_STATUS_PRECONDITION_FAILED", 412),
        ("HTTP_STATUS_PAYLOAD_TOO_LARGE", 413),
        ("HTTP_STATUS_URI_TOO_LONG", 414),
        ("HTTP_STATUS_UNSUPPORTED_MEDIA_TYPE", 415),
        ("HTTP_STATUS_RANGE_NOT_SATISFIABLE", 416),
        ("HTTP_STATUS_EXPECTATION_FAILED", 417),
        ("HTTP_STATUS_TEAPOT", 418),
        ("HTTP_STATUS_MISDIRECTED_REQUEST", 421),
        ("HTTP_STATUS_UNPROCESSABLE_ENTITY", 422),
        ("HTTP_STATUS_LOCKED", 423),
        ("HTTP_STATUS_FAILED_DEPENDENCY", 424),
        ("HTTP_STATUS_TOO_EARLY", 425),
        ("HTTP_STATUS_UPGRADE_REQUIRED", 426),
        ("HTTP_STATUS_PRECONDITION_REQUIRED", 428),
        ("HTTP_STATUS_TOO_MANY_REQUESTS", 429),
        ("HTTP_STATUS_REQUEST_HEADER_FIELDS_TOO_LARGE", 431),
        ("HTTP_STATUS_UNAVAILABLE_FOR_LEGAL_REASONS", 451),
        ("HTTP_STATUS_INTERNAL_SERVER_ERROR", 500),
        ("HTTP_STATUS_NOT_IMPLEMENTED", 501),
        ("HTTP_STATUS_BAD_GATEWAY", 502),
        ("HTTP_STATUS_SERVICE_UNAVAILABLE", 503),
        ("HTTP_STATUS_GATEWAY_TIMEOUT", 504),
        ("HTTP_STATUS_HTTP_VERSION_NOT_SUPPORTED", 505),
        ("HTTP_STATUS_VARIANT_ALSO_NEGOTIATES", 506),
        ("HTTP_STATUS_INSUFFICIENT_STORAGE", 507),
        ("HTTP_STATUS_LOOP_DETECTED", 508),
        ("HTTP_STATUS_NOT_EXTENDED", 510),
        ("HTTP_STATUS_NETWORK_AUTHENTICATION_REQUIRED", 511),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_match_the_documented_defaults() {
        let pairs = default_settings();
        assert!(pairs.contains(&(0x4, 65535))); // initialWindowSize
        assert!(pairs.contains(&(0x5, 16384))); // maxFrameSize
    }

    #[test]
    fn packed_settings_round_trip_through_frame_layer() {
        let pairs = default_settings();
        let packed = frame::write_settings(&pairs);
        assert_eq!(frame::parse_settings(&packed), pairs);
    }
}
