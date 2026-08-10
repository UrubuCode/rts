//! Reading the arguments Node documents for the three KDFs, once.
//!
//! Shared by [`super::sync`] and [`super::deferred`] because the callback form
//! of each function takes the SAME arguments as its `*Sync` twin plus a trailing
//! callback — `crypto.md` §3's table says so — and two readers of one signature
//! is how `hkdfSync` came to read `keylen` out of `info`.
//!
//! # The borrow discipline
//!
//! Every reader here that touches an ambient entry point ([`util::binary_like`],
//! [`util::extra_argument`], both of which take the runtime borrow themselves)
//! runs OUTSIDE [`entry::with_runtime`]. A nested borrow aborts the process
//! rather than failing, which is why the reads are ordered rather than inlined
//! where they are used. `assert/mod.rs` states the same discipline for options
//! objects, and `timers/promises.rs` for the promise entry points.

use rts_core::entry::{self, Context};

use super::super::util;
use super::derive;

/// Everything `scryptSync`'s fourth argument can carry.
pub(super) struct ScryptOptions {
    pub(super) n: u64,
    pub(super) r: u32,
    pub(super) p: u32,
    pub(super) maxmem: u64,
}

impl ScryptOptions {
    /// Node's defaults, which apply whenever the options object is absent or
    /// carries no such property. `crypto.md`'s `ScryptOptions`.
    pub(super) fn defaults() -> Self {
        Self {
            n: derive::SCRYPT_DEFAULT_N,
            r: derive::SCRYPT_DEFAULT_R,
            p: derive::SCRYPT_DEFAULT_P,
            maxmem: derive::SCRYPT_DEFAULT_MAXMEM,
        }
    }

    /// The options an object carries, over the defaults.
    ///
    /// Both spellings of each cost parameter are read, because Node accepts
    /// both: `N`/`cost`, `r`/`blockSize`, `p`/`parallelization`. The short name
    /// wins when both are present — Node itself refuses that combination with
    /// `ERR_INCOMPATIBLE_OPTION_PAIR`, and refusing it here would mean raising
    /// from inside a reader whose only other failure mode is "absent".
    /// Preferring one is the smaller divergence, and it is stated rather than
    /// discovered.
    pub(super) fn read(context: &mut Context, options: u64) -> Self {
        let mut held = Self::defaults();
        // A free `fn` and not a closure capturing `options`: a closure annotated
        // `|context: &mut Context|` is called with an already-borrowed `&mut`,
        // and the reborrow that makes that legal is easier to lose than to
        // read. Nothing is captured, so nothing is gained by one.
        fn number(context: &mut Context, options: u64, first: &str, second: &str) -> Option<f64> {
            let value = entry::get_member(context, options, first);
            entry::number_of(value).or_else(|| {
                let value = entry::get_member(context, options, second);
                entry::number_of(value)
            })
        }
        // A non-finite or negative value is dropped rather than truncated: `as
        // u64` on a NaN is zero, which `scrypt_bytes` would then refuse with
        // "Invalid scrypt params" — the right outcome by luck, from a cast that
        // is wrong in general. Filtering says it on purpose.
        if let Some(value) = number(context, options, "N", "cost").filter(|v| v.is_finite() && *v >= 0.0) {
            held.n = value as u64;
        }
        if let Some(value) = number(context, options, "r", "blockSize").filter(|v| v.is_finite() && *v >= 0.0) {
            held.r = value as u32;
        }
        if let Some(value) = number(context, options, "p", "parallelization").filter(|v| v.is_finite() && *v >= 0.0) {
            held.p = value as u32;
        }
        let maxmem = entry::get_member(context, options, "maxmem");
        if let Some(value) = entry::number_of(maxmem).filter(|v| v.is_finite() && *v >= 0.0) {
            held.maxmem = value as u64;
        }
        held
    }
}

/// `(password, salt, iterations, keylen, digest)` — `pbkdf2Sync`'s five, in
/// Node's order.
///
/// `digest` is the FIFTH, past the four slots the calling convention carries, so
/// it comes through [`util::extra_argument`]. An ABSENT one is `None` and the
/// caller refuses: Node made `digest` mandatory (`crypto.md` §4, "omitting it
/// … is now a hard `TypeError`") because the SHA-1 default it replaced
/// downgraded callers who never typed the word.
pub(super) struct Pbkdf2Arguments {
    pub(super) password: Vec<u8>,
    pub(super) salt: Vec<u8>,
    pub(super) rounds: u32,
    pub(super) keylen: usize,
    pub(super) digest: Option<String>,
}

impl Pbkdf2Arguments {
    pub(super) fn read(password: u64, salt: u64, iterations: u64, keylen: u64) -> Self {
        let named = util::extra_argument(4, password, salt, iterations, keylen);
        let digest = entry::with_runtime(|context| util::text(context, named));
        let rounds = entry::number_of(iterations).filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0) as u32;
        let len = entry::number_of(keylen).filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0) as usize;
        Self {
            password: util::binary_like(password),
            salt: util::binary_like(salt),
            rounds,
            keylen: len,
            digest,
        }
    }
}

/// `(digest, ikm, salt, info, keylen)` — `hkdfSync`'s five, in Node's order.
/// `keylen` is the fifth and comes through [`util::extra_argument`].
pub(super) struct HkdfArguments {
    pub(super) digest: Option<String>,
    pub(super) ikm: Vec<u8>,
    pub(super) salt: Vec<u8>,
    pub(super) info: Vec<u8>,
    pub(super) keylen: usize,
}

impl HkdfArguments {
    pub(super) fn read(digest: u64, ikm: u64, salt: u64, info: u64) -> Self {
        let named = util::extra_argument(4, digest, ikm, salt, info);
        let len = entry::number_of(named).filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0) as usize;
        let name = entry::with_runtime(|context| util::text(context, digest));
        Self {
            digest: name,
            ikm: util::binary_like(ikm),
            salt: util::binary_like(salt),
            info: util::binary_like(info),
            keylen: len,
        }
    }
}

/// `(password, salt, keylen, options?)` — `scryptSync`'s four, all inside the
/// convention's slots.
pub(super) struct ScryptArguments {
    pub(super) password: Vec<u8>,
    pub(super) salt: Vec<u8>,
    pub(super) keylen: usize,
    pub(super) options: ScryptOptions,
}

impl ScryptArguments {
    pub(super) fn read(password: u64, salt: u64, keylen: u64, options: u64) -> Self {
        let len = entry::number_of(keylen).filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0) as usize;
        let password = util::binary_like(password);
        let salt = util::binary_like(salt);
        let options = entry::with_runtime(|context| ScryptOptions::read(context, options));
        Self { password, salt, keylen: len, options }
    }
}
