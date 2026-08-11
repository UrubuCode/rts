//! Opening a `.node`.
//!
//! P8c, and the last piece: the file is a shared library, it is mapped, and the
//! entry point is looked up by name.
//!
//! # Why this comes after the export table and not before
//!
//! A `.node` has UNDEFINED references to `napi_create_double` and its siblings,
//! resolved against the process that maps it. Before P8b this process exported
//! none of them, so every open would have failed at the first one — and the
//! error a loader reports in that situation names the addon, which is the wrong
//! place to look. The order was chosen for the diagnostic, not for the code.
//!
//! # Two entry points, one of them silent
//!
//! A modern addon exports `napi_register_module_v1` and this finds it by name.
//! An older one calls `napi_module_register` from a static constructor, which
//! runs while the library is being MAPPED — before this code gets to look at
//! anything. So both are handled and the order is: map, see whether anything
//! registered itself during the mapping, and only then look for the symbol.
//!
//! # Nothing is ever unloaded
//!
//! No `FreeLibrary`, no `dlclose`, deliberately. The addon's code stays
//! reachable from every value it produced — a callable's code address, a
//! finalizer, a threadsafe function's callback — and unmapping it turns each of
//! those into a jump into nothing. Node does not unload addons either, and for
//! the same reason. The library is leaked, once per addon, for the life of the
//! process.

use core::ffi::c_void;
use std::path::Path;

use crate::abi::napi_env;
use crate::module::Register;

/// A mapped addon, and how to ask it for its exports.
pub struct Addon {
    /// The platform's handle. Held so the mapping is visibly owned, and never
    /// released — see the module doc.
    #[allow(dead_code)]
    library: *mut c_void,
    /// The entry point, when the addon exports one.
    register: Register,
    /// The name it registered under during mapping, when it took the older
    /// path instead.
    registered: Option<String>,
}

impl Addon {
    /// Runs the addon's registrar and answers what it exports.
    ///
    /// # Safety
    ///
    /// `env` must be live for as long as the addon is — which is forever, since
    /// nothing unloads one.
    pub unsafe fn exports(&self, env: napi_env) -> Option<u64> {
        if self.register.is_some() {
            // SAFETY: the caller's contract, and the symbol came from this
            // library's own export table.
            return unsafe { crate::module::run(env, self.register) };
        }
        let name = self.registered.as_deref()?;
        // SAFETY: as above.
        unsafe { crate::module::exports_of(env, name) }
    }
}

/// Maps `path` and finds how to ask it for its exports.
///
/// # Safety
///
/// The file is arbitrary native code, mapped into this process and run: its
/// static constructors execute during the call. Nothing here can check that it
/// is an addon rather than a virus, which is the same trust `require` of a
/// `.node` has always asked for.
pub unsafe fn open(path: &Path) -> Result<Addon, String> {
    let before = crate::module::registered();
    // SAFETY: the caller's contract.
    let library = unsafe { map(path) }?;

    // Looked up FIRST as a symbol, because a modern addon is the common case
    // and its entry point is explicit.
    // SAFETY: a handle `map` just produced.
    let register: Register = unsafe { symbol(library, c"napi_register_module_v1") }
        .map(|address| unsafe { core::mem::transmute::<*mut c_void, _>(address) });

    // An older addon registered while it was being mapped, which is why this
    // is a difference of counts rather than a question asked of the library.
    let registered = match crate::module::registered() > before {
        true => crate::module::last_registered(),
        false => None,
    };

    if register.is_none() && registered.is_none() {
        return Err(format!(
            "{} exports no `napi_register_module_v1` and registered no module \
             while loading — it is a shared library, but not an addon",
            path.display()
        ));
    }
    Ok(Addon {
        library,
        register,
        registered,
    })
}

/// Maps a shared library, or says why it could not.
///
/// # Safety
///
/// See [`open`] — this is where the addon's constructors run.
#[cfg(windows)]
unsafe fn map(path: &Path) -> Result<*mut c_void, String> {
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetLastError() -> u32;
    }
    // UTF-16 and NUL-terminated, which is what the wide API takes. The lossy
    // path — the ANSI entry point — mangles a name with a character outside the
    // active code page, and a user's home directory is exactly where such a
    // name lives.
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(core::iter::once(0))
        .collect();
    // SAFETY: a NUL-terminated wide string.
    let library = unsafe { LoadLibraryW(wide.as_ptr()) };
    match library.is_null() {
        // SAFETY: an ordinary Win32 call.
        true => Err(format!(
            "cannot load {}: Windows error {}",
            path.display(),
            unsafe { GetLastError() }
        )),
        false => Ok(library),
    }
}

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

/// The same, everywhere else.
///
/// # Safety
///
/// See [`open`].
#[cfg(unix)]
unsafe fn map(path: &Path) -> Result<*mut c_void, String> {
    unsafe extern "C" {
        fn dlopen(name: *const core::ffi::c_char, flags: i32) -> *mut c_void;
        fn dlerror() -> *const core::ffi::c_char;
    }
    // `RTLD_NOW | RTLD_LOCAL`: every undefined symbol resolved before anything
    // runs, rather than at the first call. An addon whose symbols are missing
    // must fail HERE, where the path is still in hand to name — lazily, the
    // same failure arrives inside somebody's callback with no context at all.
    const RTLD_NOW: i32 = 2;
    let text = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| format!("{} has a NUL in it", path.display()))?;
    // SAFETY: a NUL-terminated path.
    let library = unsafe { dlopen(text.as_ptr(), RTLD_NOW) };
    if !library.is_null() {
        return Ok(library);
    }
    // SAFETY: `dlerror` answers a static string or null, immediately after the
    // failure it describes.
    let reason = unsafe { dlerror() };
    let reason = match reason.is_null() {
        true => "no reason given".to_owned(),
        // SAFETY: a NUL-terminated string from the loader.
        false => unsafe { core::ffi::CStr::from_ptr(reason) }
            .to_string_lossy()
            .into_owned(),
    };
    Err(format!("cannot load {}: {reason}", path.display()))
}

/// One exported symbol of a mapped library.
///
/// # Safety
///
/// `library` must be a handle [`map`] produced.
#[cfg(windows)]
unsafe fn symbol(library: *mut c_void, name: &core::ffi::CStr) -> Option<*mut c_void> {
    unsafe extern "system" {
        fn GetProcAddress(library: *mut c_void, name: *const core::ffi::c_char) -> *mut c_void;
    }
    // SAFETY: the caller's contract, and a NUL-terminated name.
    let address = unsafe { GetProcAddress(library, name.as_ptr()) };
    (!address.is_null()).then_some(address)
}

/// The same, everywhere else.
///
/// # Safety
///
/// As above.
#[cfg(unix)]
unsafe fn symbol(library: *mut c_void, name: &core::ffi::CStr) -> Option<*mut c_void> {
    unsafe extern "C" {
        fn dlsym(library: *mut c_void, name: *const core::ffi::c_char) -> *mut c_void;
    }
    // SAFETY: the caller's contract.
    let address = unsafe { dlsym(library, name.as_ptr()) };
    (!address.is_null()).then_some(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_is_not_there_is_an_error_and_not_a_panic() {
        // A `require` of a missing `.node` is an ordinary program error, and
        // this runs across an FFI boundary where a panic would abort.
        // SAFETY: nothing is mapped when the path does not exist.
        let opened = unsafe { open(Path::new("no_such_addon_here.node")) };
        let message = opened.err().expect("no file, no addon");
        assert!(
            message.contains("no_such_addon_here.node"),
            "the message must name the file: {message}"
        );
    }

    /// A library every one of these platforms has, and a symbol it exports.
    ///
    /// The point is not the library — it is that `map` and `symbol` do what
    /// they say against something real, which no `.node` in this repository can
    /// demonstrate because there is none to build.
    #[cfg(windows)]
    const KNOWN: (&str, &core::ffi::CStr) = ("kernel32.dll", c"GetProcAddress");
    #[cfg(target_os = "linux")]
    const KNOWN: (&str, &core::ffi::CStr) = ("libc.so.6", c"getpid");
    #[cfg(target_os = "macos")]
    const KNOWN: (&str, &core::ffi::CStr) = ("/usr/lib/libSystem.B.dylib", c"getpid");

    #[test]
    fn mapping_and_looking_up_work_against_a_real_library() {
        // SAFETY: a system library, mapped and never unmapped.
        let library = unsafe { map(Path::new(KNOWN.0)) }.expect("a library every host has");
        // SAFETY: a handle just produced.
        assert!(
            unsafe { symbol(library, KNOWN.1) }.is_some(),
            "a symbol that library certainly exports"
        );
        // SAFETY: same.
        assert!(
            unsafe { symbol(library, c"no_symbol_by_this_name") }.is_none(),
            "and one it certainly does not"
        );
    }

    #[test]
    fn a_library_that_is_not_an_addon_is_refused_by_name() {
        // The failure a user meets when they point `require` at the wrong file:
        // it maps, and then has nothing to say for itself.
        // SAFETY: a system library.
        let opened = unsafe { open(Path::new(KNOWN.0)) };
        let message = opened.err().expect("not an addon");
        assert!(
            message.contains("not an addon"),
            "and the message says which of the two things went wrong: {message}"
        );
    }
}
