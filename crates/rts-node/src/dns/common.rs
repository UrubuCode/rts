//! Shared helpers between [`super::lookup`] and [`super::resolve`]: reading
//! an options object's fields, building the plain-object error shape both
//! resolution paths answer, and boxing a plain string. See the module doc
//! ("Errors are plain objects, not `Error` instances") for why the error
//! shape is a hand-built object rather than a real `Error`.

use rts_core::entry;

/// The plain object a failed lookup/resolve answers — see the module doc
/// for why it is not a real `Error`.
pub(super) fn error_object(code: &str, syscall: &str, hostname: &str) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let code_v = entry::make_string(context, code);
        entry::put_member(context, object, "code", code_v);
        let syscall_v = entry::make_string(context, syscall);
        entry::put_member(context, object, "syscall", syscall_v);
        let host_v = entry::make_string(context, hostname);
        entry::put_member(context, object, "hostname", host_v);
        let message = format!("{syscall} {code} {hostname}");
        let message_v = entry::make_string(context, &message);
        entry::put_member(context, object, "message", message_v);
        object
    })
}

/// A JS string built from a Rust one.
pub(super) fn string_value(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// A boolean member of an options object, `false` when absent or `options`
/// is not an object at all.
pub(super) fn option_bool(options: u64, name: &str) -> bool {
    let absent = entry::undefined_value();
    if options == absent {
        return false;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    value == entry::boolean_value(true)
}

/// A numeric member of an options object.
pub(super) fn option_number(options: u64, name: &str) -> Option<f64> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    entry::number_of(value)
}

/// A text member of an options object.
pub(super) fn option_text(options: u64, name: &str) -> Option<String> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    entry::text_of(value)
}
