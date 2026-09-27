//! `configure(options)` — the DOM config object, and its one option today:
//! `inspector: true | { port }`, the in-program switch of the DevTools endpoint
//! (plan `docs/superpowers/plans/2026-09-26-dom-inspector.md`, F1).
//!
//! # Why a hook and not a call
//!
//! The endpoint is `rts-node`'s, and this crate does not depend on that one.
//! The host installs the starter ([`declare_inspector`]); a build whose host
//! installed none has no inspector, and asking for one THROWS rather than
//! answering as though it had started — the silent version of this would be a
//! program waiting for a DevTools connection that can never come.

use std::cell::Cell;

use rts_core::entry::{self, Context, Provided};

/// Starts the inspector on a port and answers its `ws://` URL.
pub type Starter = fn(&mut Context, u16) -> Result<String, String>;

thread_local! {
    static STARTER: Cell<Option<Starter>> = const { Cell::new(None) };
}

/// Node's inspector port, for `inspector: true`.
const DEFAULT_PORT: u16 = 9229;

/// Installs the function `configure({ inspector })` calls. By the host.
pub fn declare_inspector(starter: Starter) {
    STARTER.with(|cell| cell.set(Some(starter)));
}

pub(crate) const MEMBERS: &[(&str, Provided)] = &[("configure", configure)];

/// `configure({ inspector: true | { port } })` — answers the inspector's
/// `ws://` URL, or `undefined` when the options asked for nothing.
extern "C" fn configure(_e: u64, _t: u64, options: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let port = entry::with_runtime(|context| {
        if !entry::is_object(context, options) {
            return None;
        }
        let asked = entry::get_member(context, options, "inspector");
        if asked == entry::boolean_value(true) {
            return Some(DEFAULT_PORT);
        }
        if !entry::is_object(context, asked) {
            return None;
        }
        let port = entry::get_member(context, asked, "port");
        Some(entry::number_of(port).map_or(DEFAULT_PORT, |n| n as u16))
    });
    let Some(port) = port else {
        return entry::undefined_value();
    };
    let Some(starter) = STARTER.with(Cell::get) else {
        entry::throw_type_error("this build has no inspector (the host did not install one)");
        return entry::undefined_value();
    };
    let started = entry::with_runtime(|context| starter(context, port));
    match started {
        Ok(url) => entry::with_runtime(|context| entry::make_string(context, &url)),
        Err(reason) => {
            entry::throw_type_error(&format!("the inspector did not start: {reason}"));
            entry::undefined_value()
        }
    }
}
