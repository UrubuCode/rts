//! Where the inspector's two halves meet: `rts-node`'s CDP transport and
//! `rts-dom-bridge`'s document domains. Neither crate names the other; this one
//! may name both (plan `docs/superpowers/plans/2026-09-26-dom-inspector.md`, F4).

use rts_core::entry::Context;
use rts_node::inspector::cdp::{self, Reply};
use serde_json::Value;

/// Asks for the inspector on `port` — what `--inspect[=port]` calls before the
/// program runs.
pub use rts_node::inspector::cdp::request;

/// Wires the domains and the config hook, and starts the endpoint when
/// something asked for it. When nothing did, [`cdp::start_if_requested`]
/// returns before binding, spawning or registering anything.
pub(crate) fn install(context: &mut Context) {
    cdp::declare_domains(domains);
    rts_dom_bridge::declare_inspector(cdp::start);
    cdp::start_if_requested(context);
}

fn domains(method: &str, params: &Value) -> Option<Result<Reply, String>> {
    rts_dom_bridge::inspector::handle(method, params)
        .map(|answered| answered.map(|(result, events)| Reply { result, events }))
}
