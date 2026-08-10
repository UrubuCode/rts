//! `https.Agent`/`https.globalAgent` — real Node gives `https` its own
//! `Agent` subclass (defaulting `port: 443`), but this crate's `http.Agent`
//! already pools nothing (`http/agent.rs`'s own doc: `ClientRequest` opens
//! and connects its own socket per request and never consults an `Agent` at
//! all), so a second class with the same empty behaviour would be a second
//! copy of a no-op. This module hands back `http`'s own `Agent` class
//! directly — same constructor, same prototype, same instance shape — which
//! is honest rather than merely convenient: neither pools, so there is
//! nothing for two classes to disagree about.

use rts_core::entry;

use super::common::http_member;

pub(super) fn agent_ctor(context: &mut entry::Context) -> u64 {
    http_member(context, "Agent")
}
