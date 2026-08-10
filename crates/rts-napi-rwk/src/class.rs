//! Classes an addon defines, and the descriptors it defines them with.
//!
//! `napi_define_class` is how a C++ addon exposes a type — a constructor, a
//! prototype with methods on it, and static members on the constructor itself.
//!
//! # Built the way a `class` is, not the way a builtin is
//!
//! `rts-core` has `make_prototype`, which is what `Math` and `Error` are made
//! with, and it is the wrong tool here for two stated reasons of its own: it
//! takes a `&'static str` (an addon's class name is read from the addon at run
//! time), and it PANICS when two callers define the same name from different
//! files — deliberately, because inside the engine that can only be a
//! programming error. An addon's name comes from a program, so two addons
//! defining `Wrapper` would be a crash a script could cause. That is exactly
//! what its doc says cannot happen, so it must not be made possible.
//!
//! So a class is assembled here the way the language assembles one: a callable
//! marked as a constructor, a plain object as its prototype, methods put on the
//! prototype, and the two linked by `prototype` and `constructor`. Everything
//! is `rts-core`'s; nothing about what a class MEANS is decided here.
//!
//! # The attributes that are honoured, and the ones that are not
//!
//! `napi_static` decides which half a member lands on, and it is honoured
//! because it changes where the member IS. `writable`, `enumerable` and
//! `configurable` are recorded in the ABI's struct and are NOT honoured: this
//! engine's `put_member` makes an ordinary property, and pretending otherwise
//! would be the hollow surface `CLAUDE.md` names — a flag accepted and ignored
//! reads as supported. `rts-core` has an `integrity` module that knows about
//! attributes; wiring descriptors through it is a separate change with its own
//! tests, and until then this comment is the whole truth.

use core::ffi::c_void;

use crate::abi::{napi_callback, napi_env, napi_status, napi_value};
use crate::functions::callable_word;
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_object_expected, napi_ok};

/// What a property may be, as a bit set. **The values are the ABI.**
#[allow(missing_docs)]
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum napi_property_attributes {
    napi_default = 0,
    napi_writable = 1,
    napi_enumerable = 2,
    napi_configurable = 4,
    napi_static = 1024,
}

/// One member of a class or object, as the addon describes it.
///
/// **The field order and the layout are the ABI's.** An addon builds an array
/// of these as a C aggregate and passes a pointer; a reordered field is read as
/// a different one, with no diagnostic anywhere.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct napi_property_descriptor {
    /// The name, as a C string. One of this and `name` is set.
    pub utf8name: *const core::ffi::c_char,
    /// The name, as a value — for a symbol key, which a C string cannot spell.
    pub name: napi_value,
    /// A method.
    pub method: napi_callback,
    /// A getter.
    pub getter: napi_callback,
    /// A setter.
    pub setter: napi_callback,
    /// A plain value, when it is none of the above.
    pub value: napi_value,
    /// Where it goes, and what the language would say about it.
    pub attributes: napi_property_attributes,
    /// Handed back to whichever callback above runs.
    pub data: *mut c_void,
}

/// Whether a descriptor asked for the constructor rather than the prototype.
///
/// A bit test rather than equality: the ABI's attributes are a set, and
/// `static | enumerable` is an ordinary thing for an addon to write.
fn is_static(descriptor: &napi_property_descriptor) -> bool {
    (descriptor.attributes as u32) & (napi_property_attributes::napi_static as u32) != 0
}

/// The name a descriptor carries, from whichever of its two spellings is set.
///
/// # Safety
///
/// `utf8name` must be null or NUL-terminated; `name` must be null or a handle.
unsafe fn name_of(descriptor: &napi_property_descriptor) -> Option<String> {
    if !descriptor.utf8name.is_null() {
        // SAFETY: the caller's contract.
        return unsafe { core::ffi::CStr::from_ptr(descriptor.utf8name) }
            .to_str()
            .ok()
            .map(str::to_owned);
    }
    // SAFETY: the caller's contract.
    unsafe { value_of(descriptor.name) }.and_then(rts_core::entry::text_of)
}

/// Puts one descriptor on `target`.
///
/// # Safety
///
/// The descriptor's pointers must be as the ABI describes.
unsafe fn apply(env: napi_env, target: u64, descriptor: &napi_property_descriptor) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(name) = (unsafe { name_of(descriptor) }) else {
        return napi_status::napi_name_expected;
    };

    if descriptor.getter.is_some() || descriptor.setter.is_some() {
        // An accessor. The key is a NUMBER here rather than a string, because
        // that is what `define_getter` takes — the same numbering the compiler
        // mints from, reached through `key_number`.
        let key = rts_core::entry::with_runtime(|context| {
            rts_core::entry::make_string(context, &name)
        });
        let key = rts_core::entry::key_number(key);
        if let Some(getter) = descriptor.getter {
            let code = callable_word(env, Some(getter), descriptor.data);
            rts_core::entry::define_getter(target, key, code);
        }
        if let Some(setter) = descriptor.setter {
            let code = callable_word(env, Some(setter), descriptor.data);
            rts_core::entry::define_setter(target, key, code);
        }
        return napi_ok;
    }

    let word = match descriptor.method {
        Some(method) => callable_word(env, Some(method), descriptor.data),
        // SAFETY: the caller's contract — a handle or null.
        None => match unsafe { value_of(descriptor.value) } {
            Some(word) => word,
            None => rts_core::entry::undefined_value(),
        },
    };
    rts_core::entry::with_runtime(|context| {
        rts_core::entry::put_member(context, target, &name, word)
    });
    napi_ok
}

/// `napi_define_class`.
///
/// # Safety
///
/// `utf8name` must be NUL-terminated, and `properties` must point at
/// `property_count` descriptors.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn napi_define_class(
    env: napi_env,
    utf8name: *const core::ffi::c_char,
    _length: usize,
    constructor: napi_callback,
    data: *mut c_void,
    property_count: usize,
    properties: *const napi_property_descriptor,
    result: *mut napi_value,
) -> napi_status {
    if constructor.is_none() || result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    let name = match utf8name.is_null() {
        true => String::new(),
        false => match unsafe { core::ffi::CStr::from_ptr(utf8name) }.to_str() {
            Ok(name) => name.to_owned(),
            Err(_) => return napi_invalid_arg,
        },
    };

    let callable = callable_word(env, constructor, data);
    // What makes `C()` without `new` a TypeError, which is what a class is.
    rts_core::entry::mark_class_constructor(callable);

    let prototype = rts_core::entry::with_runtime(rts_core::entry::make_object);
    rts_core::entry::with_runtime(|context| {
        rts_core::entry::put_member(context, callable, "prototype", prototype);
        rts_core::entry::put_member(context, prototype, "constructor", callable);
        if !name.is_empty() {
            let text = rts_core::entry::make_string(context, &name);
            rts_core::entry::put_member(context, callable, "name", text);
        }
    });

    for at in 0..property_count {
        if properties.is_null() {
            return napi_invalid_arg;
        }
        // SAFETY: the caller's contract — `property_count` descriptors.
        let descriptor = unsafe { &*properties.add(at) };
        let target = match is_static(descriptor) {
            true => callable,
            false => prototype,
        };
        // SAFETY: the descriptor is the addon's, as the ABI describes it.
        let status = unsafe { apply(env, target, descriptor) };
        if status != napi_ok {
            return status;
        }
    }

    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let handle = env.current().handle(callable);
    // SAFETY: the caller's contract.
    match unsafe { write_out(result, handle) } {
        true => napi_ok,
        false => napi_invalid_arg,
    }
}

/// `napi_define_properties` — the same descriptors, on an object that exists.
///
/// # Safety
///
/// As [`napi_define_class`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_define_properties(
    env: napi_env,
    object: napi_value,
    property_count: usize,
    properties: *const napi_property_descriptor,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(target) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    if !rts_core::entry::with_runtime(|context| rts_core::entry::is_object(context, target)) {
        return napi_object_expected;
    }
    for at in 0..property_count {
        if properties.is_null() {
            return napi_invalid_arg;
        }
        // SAFETY: the caller's contract.
        let descriptor = unsafe { &*properties.add(at) };
        // `napi_static` means nothing here: there is no constructor half to put
        // it on, and the ABI says so too. Ignored rather than refused, which is
        // what Node does.
        // SAFETY: as above.
        let status = unsafe { apply(env, target, descriptor) };
        if status != napi_ok {
            return status;
        }
    }
    napi_ok
}

/// `napi_new_instance` — `new C(...)`.
///
/// # Safety
///
/// `argv` must point at `argc` handles from an open scope.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_new_instance(
    env: napi_env,
    constructor: napi_value,
    argc: usize,
    argv: *const napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(callee) = (unsafe { value_of(constructor) }) else {
        return napi_invalid_arg;
    };
    if !rts_core::entry::with_runtime(|context| rts_core::entry::is_callable_in(context, callee)) {
        return napi_status::napi_function_expected;
    }

    // Four slots, which is the convention every compiled call takes — the same
    // limit `napi_get_cb_info` reports from the other side. An addon passing
    // more is refused rather than silently truncated, because a constructor
    // reading its fifth argument as `undefined` builds a wrong object quietly.
    if argc > crate::env::ARGUMENTS {
        return napi_invalid_arg;
    }
    let absent = rts_core::entry::undefined_value();
    let mut words = [absent; crate::env::ARGUMENTS];
    for (at, slot) in words.iter_mut().enumerate().take(argc) {
        if argv.is_null() {
            return napi_invalid_arg;
        }
        // SAFETY: the caller's contract — `argc` readable handles.
        let handle = unsafe { *argv.add(at) };
        // SAFETY: a handle from an open scope.
        match unsafe { value_of(handle) } {
            Some(word) => *slot = word,
            None => return napi_invalid_arg,
        }
    }
    let made = rts_core::entry::construct(callee, words[0], words[1], words[2], words[3]);

    // Rule 8 from the outside, as `napi_call_function` does it: a constructor
    // that threw produced no object, and handing one back would be handing back
    // whatever `construct` answered for a call that did not finish.
    if rts_core::entry::pending().is_some() {
        return napi_status::napi_pending_exception;
    }

    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let handle = env.current().handle(made);
    // SAFETY: the caller's contract.
    match unsafe { write_out(result, handle) } {
        true => napi_ok,
        false => napi_invalid_arg,
    }
}

/// `napi_instanceof`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_instanceof(
    _env: napi_env,
    object: napi_value,
    constructor: napi_value,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let (Some(object), Some(constructor)) =
        (unsafe { value_of(object) }, unsafe { value_of(constructor) })
    else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = rts_core::entry::instance_of(object, constructor) };
    napi_ok
}
