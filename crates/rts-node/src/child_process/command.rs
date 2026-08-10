//! Builds a `std::process::Command` off `(file, args, options)` — the one
//! piece every top-level function in this module shares, per Node's own
//! `CommonSpawnOptions`.
//!
//! # `shell` — the one option this module cannot fully honor
//!
//! `options.shell === true` runs `/bin/sh -c` on POSIX and
//! `%ComSpec% /d /s /c` (falling back to `cmd.exe`) on Windows, per the
//! reference doc's §5.1. A string value is used as the shell path/binary
//! verbatim instead. `command`/`args` are joined with a single space and NO
//! quoting — a caller passing an argument containing a space or a shell
//! metacharacter gets exactly what a real shell would do with that
//! character, which is the injection risk `execSync`'s own doc section
//! states rather than launders.

use std::process::Command;

use super::shared::{option_text, option_value, own_key_strings};

/// `command`/`file` plus everything `CommonSpawnOptions` can carry, read out
/// of the JS arguments once so every caller in this module builds a
/// `Command` the same way.
pub(super) struct Spec {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) cwd: Option<String>,
}

/// Reads `(file, args, options)` into a [`Spec`] — `shell` folded into
/// `program`/`args` right here, so nothing downstream needs to know it was
/// ever set.
pub(super) fn spec_of(file: &str, args: Vec<String>, options: u64) -> Spec {
    let cwd = option_text(options, "cwd");
    let shell_flag = option_value(options, "shell");
    match shell_flag {
        None => Spec { program: file.to_owned(), args, cwd },
        Some(value) => {
            let absent = rts_core::entry::undefined_value();
            if value == absent || value == super::shared::bool_value(false) {
                return Spec { program: file.to_owned(), args, cwd };
            }
            let shell_path = option_text(options, "shell").unwrap_or_else(default_shell);
            let joined = std::iter::once(file.to_owned()).chain(args).collect::<Vec<_>>().join(" ");
            let shell_args = shell_command_args(&joined);
            Spec { program: shell_path, args: shell_args, cwd }
        }
    }
}

/// `execSync`'s spec — ALWAYS shells out, regardless of an `options.shell`
/// key (`ExecOptions.shell` is a shell PATH override, not an on/off switch —
/// `exec`/`execSync` have no off switch at all, per the reference doc's §3).
pub(super) fn shell_spec(line: &str, options: u64) -> Spec {
    let shell_path = option_text(options, "shell").unwrap_or_else(default_shell);
    let shell_args = shell_command_args(line);
    Spec { program: shell_path, args: shell_args, cwd: option_text(options, "cwd") }
}

/// The platform default shell — `$SHELL`-independent, matching Node's own
/// fixed default rather than whatever the user's interactive shell is.
fn default_shell() -> String {
    if cfg!(windows) {
        std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_owned())
    } else {
        "/bin/sh".to_owned()
    }
}

/// The flags a shell takes before the command string — `-c` on POSIX,
/// `/d /s /c` on `cmd.exe` (matching the reference doc's §5.8(d) plan).
fn shell_command_args(command: &str) -> Vec<String> {
    if cfg!(windows) {
        vec!["/d".to_owned(), "/s".to_owned(), "/c".to_owned(), command.to_owned()]
    } else {
        vec!["-c".to_owned(), command.to_owned()]
    }
}

/// A `Command`, with `cwd`/`env`/`args` from `spec`/`options` applied.
/// stdin/stdout/stderr are left for the caller — [`super::sync_ops`] pipes
/// all three to capture them, [`super::spawn_async`] inherits them (see its
/// module doc for why piping is not implemented there yet).
pub(super) fn build(spec: &Spec, options: u64) -> Command {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    apply_env(&mut command, options);
    command
}

/// `options.env` REPLACES the child's environment when given at all — Node's
/// own documented behaviour, not a merge — falling back to inheriting the
/// parent's when the option is absent.
fn apply_env(command: &mut Command, options: u64) {
    let Some(env_object) = option_value(options, "env") else {
        return;
    };
    command.env_clear();
    for key in own_key_strings(env_object) {
        let value = rts_core::entry::with_runtime(|context| {
            rts_core::entry::get_member(context, env_object, &key)
        });
        if let Some(text) = super::shared::text(value) {
            command.env(&key, text);
        }
    }
}

/// The three `stdio` shapes this module distinguishes — `Stdio` itself is
/// opaque (no way to ask "which variant is this" back out of one), so a
/// caller that needs to branch on the mode reads this instead of a built
/// `Stdio`.
#[derive(PartialEq, Eq)]
pub(super) enum StdioMode {
    Pipe,
    Inherit,
    Ignore,
}

/// `stdio: 'inherit'`/`'ignore'` vs the default `'pipe'`.
pub(super) fn stdio_mode(options: u64) -> StdioMode {
    match option_text(options, "stdio").as_deref() {
        Some("inherit") => StdioMode::Inherit,
        Some("ignore") => StdioMode::Ignore,
        _ => StdioMode::Pipe,
    }
}
