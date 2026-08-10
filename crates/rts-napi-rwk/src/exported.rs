//! Every symbol a `.node` resolves out of this process, in one place.
//!
//! # Why this list exists when the functions already do
//!
//! Two jobs, and neither is served by the definitions being scattered across
//! nine modules.
//!
//! **Keeping them.** A `#[unsafe(no_mangle)] extern "C"` function in a
//! DEPENDENCY of a binary is not referenced by anything in that binary, and a
//! linker is entitled to drop it. [`KEEP`] references every one, so they are
//! all present in whatever links this crate.
//!
//! **Exporting them.** An addon resolves them by name at load time, out of the
//! host process, which needs each name handed to the linker — `/EXPORT:` on
//! COFF, `--export-dynamic` on ELF. The root `build.rs` does that, and it
//! PARSES this file rather than restating it: one list, two readers, because a
//! second list in a build script drifts and produces an addon that loads on one
//! platform and not another.
//!
//! # The rule this does NOT break
//!
//! `CLAUDE.md` says never hand-write a symbol name, and states the exception
//! this crate is: these names are a foreign C ABI, and an attribute deriving
//! them would derive the wrong thing. What must not happen is the same name
//! written twice — so [`NAMES`] is generated from [`KEEP`] by the macro below,
//! from one spelling per symbol.
//!
//! # What is still missing to load an addon
//!
//! Nothing, in this crate. The symbols are exported (measured: the linker emits
//! an export library naming them, and did not before) and `crate::loader` maps
//! a file and finds its entry point. What is left is a real `.node` to point it
//! at — P8d, which is a measurement rather than code.

/// One address in [`KEEP`].
///
/// Exists only to be `Sync`, which a `static` requires and a raw pointer is
/// not. The promise is trivially true: this is the address of a function in
/// this binary, it is written once at compile time, and nothing ever reads
/// through it — the array exists so a linker sees a reference, and for no
/// other reason.
#[repr(transparent)]
struct Kept(#[allow(dead_code)] *const core::ffi::c_void);

// SAFETY: see the note above — an immutable address, never dereferenced.
unsafe impl Sync for Kept {}

/// Expands one spelling per symbol into the keep-alive table and the names.
macro_rules! exported {
    ($($module:ident :: $name:ident),+ $(,)?) => {
        /// A reference to every exported function, so a linker keeps them.
        ///
        /// `#[used]` because the array itself is otherwise unreferenced, which
        /// is the same problem one level up.
        ///
        /// The addresses are wrapped rather than stored as `usize`: casting a
        /// function to an integer is not something const evaluation will do
        /// (the address is not known then), while casting it to a pointer is.
        /// A raw pointer is not `Sync` and a `static` must be, so [`Kept`]
        /// carries the promise — see its own note for why that promise is
        /// trivially true here.
        #[used]
        static KEEP: &[Kept] = &[
            $(Kept(crate::$module::$name as *const core::ffi::c_void)),+
        ];

        /// Every exported symbol, by the name an addon looks it up under.
        pub const NAMES: &[&str] = &[$(stringify!($name)),+];
    };
}

exported! {
    async_work::napi_create_async_work,
    async_work::napi_delete_async_work,
    async_work::napi_queue_async_work,
    buffers::napi_create_arraybuffer,
    buffers::napi_create_buffer,
    buffers::napi_create_buffer_copy,
    buffers::napi_get_buffer_info,
    buffers::napi_get_typedarray_info,
    buffers::napi_is_buffer,
    buffers::napi_is_typedarray,
    class::napi_define_class,
    class::napi_define_properties,
    class::napi_instanceof,
    class::napi_new_instance,
    errors::napi_create_error,
    errors::napi_create_range_error,
    errors::napi_create_type_error,
    errors::napi_get_and_clear_last_exception,
    errors::napi_is_error,
    errors::napi_is_exception_pending,
    errors::napi_throw,
    errors::napi_throw_error,
    errors::napi_throw_range_error,
    errors::napi_throw_type_error,
    functions::napi_call_function,
    functions::napi_create_function,
    functions::napi_get_cb_info,
    functions::napi_is_callable,
    module::napi_module_register,
    objects::napi_create_array,
    objects::napi_create_array_with_length,
    objects::napi_create_object,
    objects::napi_delete_property,
    objects::napi_get_array_length,
    objects::napi_get_element,
    objects::napi_get_named_property,
    objects::napi_get_property,
    objects::napi_get_property_names,
    objects::napi_get_value_string_utf8,
    objects::napi_has_property,
    objects::napi_is_array,
    objects::napi_set_element,
    objects::napi_set_named_property,
    objects::napi_set_property,
    references::napi_create_reference,
    references::napi_delete_reference,
    references::napi_get_reference_value,
    references::napi_reference_ref,
    references::napi_reference_unref,
    threadsafe::napi_acquire_threadsafe_function,
    threadsafe::napi_call_threadsafe_function,
    threadsafe::napi_create_threadsafe_function,
    threadsafe::napi_get_threadsafe_function_context,
    threadsafe::napi_ref_threadsafe_function,
    threadsafe::napi_release_threadsafe_function,
    threadsafe::napi_unref_threadsafe_function,
    values::napi_create_double,
    values::napi_create_int32,
    values::napi_create_string_utf8,
    values::napi_get_boolean,
    values::napi_get_null,
    values::napi_get_undefined,
    values::napi_get_value_bool,
    values::napi_get_value_double,
    values::napi_typeof,
    wrap::napi_create_external,
    wrap::napi_get_value_external,
    wrap::napi_remove_wrap,
    wrap::napi_unwrap,
    wrap::napi_wrap,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_spelled_once() {
        // Two entries under one name would be two definitions of one C symbol,
        // which does not link — but the failure would name a mangled path in a
        // linker's voice rather than the duplicate here.
        let mut seen: Vec<&str> = NAMES.to_vec();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(count, seen.len(), "a symbol is listed twice");
    }

    #[test]
    fn the_two_views_are_the_same_length() {
        // They are generated from one spelling, so this can only fail if the
        // macro above is edited into two lists — which is what it exists to
        // prevent.
        assert_eq!(KEEP.len(), NAMES.len());
    }

    #[test]
    fn every_name_is_one_an_addon_would_look_up() {
        for name in NAMES {
            assert!(
                name.starts_with("napi_"),
                "{name} is not a name the ABI defines"
            );
        }
    }
}

#[cfg(test)]
mod completeness {
    use super::NAMES;

    /// Every `napi_*` this crate defines must be in [`NAMES`].
    ///
    /// Read off the source rather than trusted, and the directory is walked
    /// rather than listed: a new module with a new entry point would otherwise
    /// be missing from the list AND from any list of files checking the list.
    /// A symbol absent here is one the export table will not carry, so an
    /// addon calling it fails to load with a name and no explanation.
    #[test]
    fn the_list_names_every_entry_point_in_this_crate() {
        let mut defined: Vec<String> = Vec::new();
        let sources = std::fs::read_dir("src").expect("the crate's own source");
        for entry in sources.flatten() {
            let path = entry.path();
            if path.extension().and_then(|end| end.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source file");
            for line in text.lines() {
                let Some(rest) = line.strip_prefix("pub unsafe extern \"C\" fn ") else {
                    continue;
                };
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if name.starts_with("napi_") {
                    defined.push(name);
                }
            }
        }
        assert!(!defined.is_empty(), "the scan found nothing, so it is broken");

        let missing: Vec<&String> = defined
            .iter()
            .filter(|name| !NAMES.contains(&name.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "defined but not exported, so an addon calling one would fail to \
             load: {missing:?}"
        );
    }
}
