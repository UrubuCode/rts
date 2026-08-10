//! `node:child_process` — spawning and controlling OS subprocesses, against
//! `docs/reference/node/child_process.md`.
//!
//! # Reuse check (RULE 0b)
//!
//! Searched `rts-cranelift` (nothing there is process-shaped — spawning an
//! OS process is a runtime/host concern, not a machine one),
//! `rts-core::entry` (its `Context`/table inventory has no process
//! table; `entry::modules` is exactly the value API every other
//! `rts-node` module already builds on, so this reuses it rather than
//! adding a second one), and this crate's siblings: [`crate::fs`] is the
//! nearest cousin (a synchronous surface plus a `notify`-backed async one
//! delivered through a queue-and-pump, exactly the shape this module needs)
//! and [`crate::events`] is the real `EventEmitter` prototype `ChildProcess`
//! chains onto, the same way `crate::fs::watch`'s `FSWatcher` already does.
//! Nothing here invents a second value encoding, a second numbering, or a
//! second delivery mechanism — [`spawn_async::pump`]'s queue-then-drain
//! shape is [`crate::fs::watch::pump`]'s, restated for a different event
//! set because the two modules cannot share code across a folder boundary
//! that is also an ownership boundary in this pass.
//!
//! # Split: synchronous is real, asynchronous is honest about its limit
//!
//! [`sync_ops`] — `spawnSync`, `execSync`, `execFileSync` — runs a child to
//! completion on the calling thread and answers a value, which is exactly
//! what a native entry point here can do (see [`crate::fs`]'s module doc for
//! the same argument). [`spawn_async`] — `spawn()`/`ChildProcess` — is
//! event-driven, and this engine has no event loop; its module doc states
//! precisely when a listener fires (never synchronously, only at the start
//! of the next `child_process` call this program makes) rather than
//! shipping a `ChildProcess` whose events silently never run.
//!
//! # Security: `execSync` always shells out
//!
//! `execSync(command, options?)` interpolates `command` into `/bin/sh -c`
//! (POSIX) or `%ComSpec%` (Windows, typically `cmd.exe`) verbatim — exactly
//! like Node's own `exec`/`execSync`. A caller building `command` from
//! untrusted input has a command injection, the same warning Node's own
//! docs carry. `spawnSync`/`execFileSync`/`spawn()` do NOT shell out unless
//! `options.shell` asks for it, and are the safe default when `file`/`args`
//! can stay separate values.
//!
//! # Not implemented, by name
//!
//! - **`exec`, `execFile`, `fork`, `ChildProcess.prototype.send`/
//!   `.disconnect`/`.channel`, `[Symbol.dispose]`, piped stdio and
//!   `'data'` events, `uid`/`gid`/`detached`/`timeout`/`AbortSignal`
//!   options.** See [`spawn_async`]'s module doc for the mechanism refused
//!   for each.
//! - **Real POSIX/Windows signal delivery.** `kill()` always forcibly
//!   terminates (`std::process::Child::kill`) regardless of the signal name
//!   passed in — see [`capture`]'s module doc: sending an arbitrary named
//!   signal needs `libc`/`windows-sys`, dependencies this module may not add
//!   (it owns this folder only, not `Cargo.toml`).
//! - **`windowsVerbatimArguments`, `windowsHide`, `.bat`/`.cmd`
//!   auto-shell-routing.** Node's own auto-detection of Windows batch files
//!   is a documented CVE-driven special case this module does not
//!   replicate; a `.bat`/`.cmd` file must be spawned with `shell: true`
//!   here, same as `execFile()` without `shell: true` already requires on
//!   real Windows Node.
//! - **`'advanced'` IPC serialization, `sendHandle` fd-passing.** No IPC
//!   channel exists at all (no `fork()`), so these have nothing to attach
//!   to.
//! - **`maxBuffer` does not stop a `spawnSync`/`execSync`/`execFileSync`
//!   child early.** [`capture::run`] captures to completion, then
//!   truncates; a child producing gigabytes of output is not killed
//!   mid-stream the way Node's own streaming buffer check does it — see
//!   [`capture`]'s doc.

mod capture;
mod command;
mod shared;
mod spawn_async;
mod sync_ops;

use rts_core::entry::{Context, Provided};

/// This module as a loop source; see the function it re-exports.
pub use spawn_async::source;

/// The namespace `node:child_process` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] =
        &[("spawnSync", sync_ops::spawn_sync), ("execSync", sync_ops::exec_sync), ("execFileSync", sync_ops::exec_file_sync), ("spawn", spawn_async::spawn)];
    let namespace = rts_core::entry::make_namespace(context, members);
    // `ChildProcess` — exported for `instanceof` parity, per the reference
    // doc's §2.1: real Node does not document a meaningful direct
    // `new ChildProcess()`, and neither does this. Registered with its real
    // methods HERE, once — [`rts_core::entry::make_prototype`] records a
    // name against whatever it is first called with and every later call
    // (here: [`spawn_async::spawn`], each time it runs) is a no-op read of
    // the SAME object, so a second call with a different member list would
    // silently never install it. Only one caller may be the first.
    let child_process_class = rts_core::entry::make_prototype(context, "ChildProcess", spawn_async::METHODS);
    rts_core::entry::put_member(context, namespace, "ChildProcess", child_process_class);
    namespace
}
