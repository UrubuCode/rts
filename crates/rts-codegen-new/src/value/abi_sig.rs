//! Real-symbol signature descriptors — the fix for the StrPtr=2-slots bug.
//!
//! The fake mini-runtime treated EVERY slot as `i64` (correct only for the
//! PolyValue-in/out `__rtsn_*` symbols). The REAL symbols use the runtime's
//! `AbiType` (`Void`/`Bool`/`I32`/`I64`/`U64`/`F64`/`StrPtr`/`Handle`), and
//! `StrPtr` lowers to **two** Cranelift slots (`ptr` + `len`). Mis-marshaling a
//! single-slot string where the ABI expects two → SIGILL.
//!
//! A symbol's params + return, lowered to a Cranelift `Signature` (expanding
//! `StrPtr` per the runtime's own
//! [`rts_runtime::abi::signature::lower_params`] rule).
//!
//! # Three sources, in order of authority
//!
//! [`sig_of`] asks, in order:
//!
//! 1. **The BAKED table** — `rts-symbol-baker` derives a signature for every
//!    `#[rtse::abi]` declaration from its Rust signature, through the same
//!    `rts_abi::tymap` the macro uses. The entry is the declaration speaking,
//!    not a transcription of it.
//! 2. **The REGISTRY** ([`registry_sigs`]) — a runtime symbol registered as a
//!    namespace/class member already PUBLISHED its `Sig`. Reading it beats
//!    copying it, for the same reason (1) does.
//! 3. **[`sig_of_hand`]** — what neither covers yet.
//!
//! # The drain, measured
//!
//! **159 → 35 rows (2026-07-28).** The whole `__rtsadp_*` surface became source
//! (1): `crates/rts-runtime/src/adapters/value/` was converted to `#[rtse::abi]`
//! en masse, plus the three `__rtsadp_str_*_auto` dispatchers in
//! `rts-primitives`. The Registry-backed `__RTS_FN_*` members (math, io,
//! collections, the `GL_*` class methods) became source (2).
//!
//! # What is left, and why each one is still here
//!
//! - **`__RTS_FN_NS_GC_*` (47)** — the engine-internal primitives: string pool,
//!   gcell, generator + async state machine. They are deliberately NOT Registry
//!   members (no member holds their address), so source (2) cannot reach them,
//!   and source (1) needs `#[rtse::abi("__RTS_FN_NS_GC_…")]` — which needs
//!   `rts-engine` to depend on `rts-macro` AND `extern crate self as rts_engine`,
//!   because the generated code names `::rts_engine::*`. A separate change.
//! - **`__rtsadp_ta_view_base_len`** — two `*mut i64` OUT-parameters, which have
//!   no single-slot ABI spelling. Its declaration says so.
//! - **A few `__rtsn_*` / `GL_ENCODE_*` / `GL_TEXTENC_*`** — not yet declared
//!   either way.
//!
//! Do NOT add a row. See docs/engine/architecture.md.

use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_module::Module;

use rts_runtime::abi::AbiType;

/// The ABI shape of one callable symbol: its param `AbiType`s (pre-expansion,
/// `StrPtr` still one entry) and its return `AbiType`.
#[derive(Clone, Copy)]
pub struct SymSig {
    pub params: &'static [AbiType],
    pub ret: AbiType,
}

/// A `SymSig` copied off a macro-derived `rts_engine::abi::SymbolDesc` — the
/// field shapes already match, this just narrows the type.
fn from_desc(d: rts_engine::abi::SymbolDesc) -> SymSig {
    SymSig { params: d.params, ret: d.ret }
}

/// The Cranelift IR type a scalar `AbiType` lowers to. `StrPtr` is NEVER passed
/// here (it is expanded into two `I64` entries before this is reached); `Void` is
/// only legal as a return and contributes no slot.
fn scalar_to_cl(ty: AbiType) -> types::Type {
    match ty {
        AbiType::F64 => types::F64,
        AbiType::I32 => types::I32,
        // Bool/I64/U64/Handle/PolyValue all ride an i64 register at the boundary.
        AbiType::Bool | AbiType::I64 | AbiType::U64 | AbiType::Handle | AbiType::PolyValue => {
            types::I64
        }
        AbiType::StrPtr => unreachable!("StrPtr must be expanded before scalar_to_cl"),
        AbiType::Void => unreachable!("Void has no Cranelift slot"),
    }
}

impl SymSig {
    /// Build the Cranelift `Signature` for this symbol under the module's default
    /// call convention, expanding each `StrPtr` param into `(ptr: i64, len: i64)`.
    pub fn to_cranelift(&self, module: &dyn Module) -> Signature {
        let mut sig = Signature::new(module.isa().default_call_conv());
        for &p in self.params {
            match p {
                AbiType::StrPtr => {
                    // ptr + len, two slots (matches the runtime's lower_params).
                    sig.params.push(AbiParam::new(types::I64));
                    sig.params.push(AbiParam::new(types::I64));
                }
                AbiType::Void => panic!("Void is not a valid parameter type"),
                other => sig.params.push(AbiParam::new(scalar_to_cl(other))),
            }
        }
        if !matches!(self.ret, AbiType::Void) {
            sig.returns.push(AbiParam::new(scalar_to_cl(self.ret)));
        }
        sig
    }

    /// Number of Cranelift param slots (StrPtr counts as 2). The lowering uses
    /// this to assert it passes the right number of marshaled values.
    pub fn param_slot_count(&self) -> usize {
        self.params
            .iter()
            .map(|p| if matches!(p, AbiType::StrPtr) { 2 } else { 1 })
            .sum()
    }

    /// Whether this symbol returns a value.
    pub fn returns(&self) -> bool {
        !matches!(self.ret, AbiType::Void)
    }
}

/// Build a Cranelift `Signature` directly from a parameter `AbiType` slice + a
/// return `AbiType`, under the module's default call convention, expanding each
/// `StrPtr` into `(ptr: i64, len: i64)`. This is the Registry-driven path (Pilar
/// 6): the signature comes from the real `Member.sig` (`AbiType`s), NOT from the
/// hand-written [`sig_of`] table — so a runtime/Registry class method is called
/// with the EXACT ABI the runtime published, with no codegen-side metadata.
pub fn cranelift_sig_from_abis(module: &dyn Module, params: &[AbiType], ret: AbiType) -> Signature {
    let mut sig = Signature::new(module.isa().default_call_conv());
    for &p in params {
        match p {
            AbiType::StrPtr => {
                sig.params.push(AbiParam::new(types::I64));
                sig.params.push(AbiParam::new(types::I64));
            }
            AbiType::Void => panic!("Void is not a valid parameter type"),
            other => sig.params.push(AbiParam::new(scalar_to_cl(other))),
        }
    }
    if !matches!(ret, AbiType::Void) {
        sig.returns.push(AbiParam::new(scalar_to_cl(ret)));
    }
    sig
}

/// The BAKED signature table, looked up by binary search.
///
/// `rts-symbol-baker` derives a signature for every `#[rtse::abi]` declaration
/// from its Rust signature (through `rts_abi::tymap`, the same mapping the macro
/// uses), so an entry here is the DECLARATION speaking, not a transcription of
/// it. Consulted before the hand table below, which is why converting a symbol
/// to `#[rtse::abi]` deletes its hand row: the baked entry takes over and the
/// row becomes unreachable.
///
/// `rts_runtime::symbol_table::SIGNATURES` is a `static` array sorted ascending
/// by name (the same order `SYMBOLS` uses), so this is a real `O(log n)`
/// binary search over memory already resident in `.rodata` — no `HashMap`
/// build, no `OnceLock`, nothing to warm on first call.
fn baked_sigs(name: &str) -> Option<SymSig> {
    let table = rts_runtime::symbol_table::signatures();
    let i = table.binary_search_by(|row| row.0.cmp(name)).ok()?;
    let (_, params, ret) = table[i];
    Some(SymSig { params, ret })
}

/// The REGISTRY's signatures, indexed by SYMBOL.
///
/// A `__RTS_FN_*` runtime symbol registered as a namespace/class member already
/// carries its `Sig` (`args`/`returns`) in the Registry — the runtime PUBLISHED
/// it. Transcribing that into the hand table below is the same duplication the
/// baked table removed for `__rtsadp_*`, just with a different owner, so this
/// reads it instead.
///
/// The Registry is keyed by `(module, member, argc)`; this is the reverse index
/// by symbol, which is what a call site emitting a known symbol name needs.
/// Where one symbol backs several arities the FIRST registration wins — they
/// share a symbol precisely because they share an ABI (the differing arity is
/// handled by `Sig::default_args` at the call site, not by a different
/// signature).
fn registry_sigs() -> &'static std::collections::HashMap<&'static str, SymSig> {
    static SIGS: std::sync::OnceLock<std::collections::HashMap<&'static str, SymSig>> =
        std::sync::OnceLock::new();
    SIGS.get_or_init(|| {
        let reg = crate::front::run::registry::registry();
        let mut out: std::collections::HashMap<&'static str, SymSig> =
            std::collections::HashMap::new();
        let members = reg
            .modules()
            .flat_map(|m| m.members.iter())
            .chain(reg.classes().flat_map(|c| c.members.iter()));
        for m in members {
            if m.symbol.is_empty() {
                continue;
            }
            // Leaked with the Registry itself (built once, never freed), so the
            // borrows are genuinely `'static`.
            let sym: &'static str = m.symbol.as_str();
            let params: &'static [AbiType] = m.sig.args.as_slice();
            out.entry(sym).or_insert(SymSig {
                params,
                ret: m.sig.returns,
            });
        }
        out
    })
}

/// Resolve a symbol name to its [`SymSig`], from the most authoritative source
/// that has it:
///
/// 1. the **BAKED** table — derived from the `#[rtse::abi]` declaration;
/// 2. the **REGISTRY** — the signature the runtime published for the member;
/// 3. the hand-written table below — what neither of the above covers yet.
///
/// `None` for an unknown symbol (the lowering turns that into an explicit
/// `Unsupported` bail, never a guess).
pub fn sig_of(name: &str) -> Option<SymSig> {
    if let Some(s) = baked_sigs(name) {
        return Some(s);
    }
    if let Some(s) = registry_sigs().get(name) {
        return Some(*s);
    }
    sig_of_hand(name)
}

/// The HAND-WRITTEN half — every symbol not yet declared with `#[rtse::abi]`.
/// A DRAINING TARGET: each conversion moves a row from here into the baked table.
fn sig_of_hand(name: &str) -> Option<SymSig> {
    use AbiType::*;
    Some(match name {
        // ---- REAL string pool (rts-std collector::string_pool) ----
        // STRING_NEW(ptr,len) -> handle  — StrPtr = two slots.
        // module-level mutable global cells (epic #195): GET(id)->word, SET(id,word).
        "__RTS_FN_NS_GC_STRING_NEW" | "__RTS_FN_NS_GC_STRING_FROM_STATIC" => SymSig {
            params: &[StrPtr],
            ret: Handle,
        },
        "__RTS_FN_NS_GC_STRING_PTR" => SymSig {
            params: &[Handle],
            ret: U64,
        },
        // (`gc.string_len/free/concat/eq/cmp/from_i64` were CONVERTED to
        // `#[rtse::abi("<legacy symbol>")]` on 2026-07-30 — the baked table now
        // derives their signatures from the Rust ones, identical to the rows that
        // used to sit here modulo the `Handle`→`U64` spelling, which lowers to the
        // same single i64 slot. `string_new`/`string_ptr` did NOT convert: `new`
        // takes raw `(*const u8, i64)` bytes and must NOT gain the UTF-8 validation
        // a `&str` param would impose, and `ptr` returns `*const u8`, which has no
        // written-token spelling in `rts_abi::tymap`.)
        // (`__rtsn_string_from_f64` was CONVERTED to `#[rtse::abi]` in
        // `rts-engine` — the baked table derives `[F64] -> Handle` from the Rust
        // signature, identical to the row that used to sit here.)

        // ---- REAL io (rts-std io) ----
        // IO_PRINT(ptr,len) -> void  — StrPtr = two slots, appends a newline.

        // ---- PRIVATE engine namespace (rts-std engine) — arch/time/trace ----
        // arch / trace_capture return a GC string handle; the timestamp + trace
        // fns are I64/Void. trace_push takes (file: StrPtr, fn: StrPtr, line, col).
        // engine.num_* / engine.num_from_str no longer exist — Number moved to a
        // pure-Rust `#[rtse::class("Number", value)]` value-class
        // (`rts-primitives/src/number/mod.rs`); the formatting bodies compute
        // directly in Rust instead of bridging through a `.ts` prelude.
        // engine.fs_read_bytes / fs_write_bytes / fs_append_bytes — the PRIVATE
        // file-IO bridge for the `.ts` ReadStream/WriteStream prelude. All-words in
        // (path word, data word), a word out (Uint8Array for read, byte-count
        // number for write/append). Impl in rts-node `fs/streambridge.rs`.
        // `promise.create(fn_addr, args_vec)` — the async-fn CALL spawn
        // (front/run/asyncspawn.rs): runs the body on the shared runtime,
        // returns the pending PromiseAsync handle.
        // The typed async spawn: registers the callee's packed param/ret kinds
        // (an f64 param travels as bits and reads back into xmm), then
        // delegates to `promise.create`.
        // Register a user fn's packed ABI for raw-pointer consumers
        // (`getPointer` emission — thread.spawn f64 workers, #247).
        // Counted-args fn-value invoke through INVOKE_AUTO (variadic packing;
        // the singleton-override dispatch).
        // `new Function(p…, body)` — args vec of PolyValue words → TAG_FUNCTION
        // word (`undefined` on a failed compile).
        // `f.bind(thisArg, …partial)` — clone the fn value; a THIS-FIRST fn
        // binds thisArg into bound_args[0], partial args append after.

        // ---- REAL collections Vec (rts-shared collections::vec) ----
        // Fused payload-addressed slot access: takes the 48-bit PolyValue payload
        // straight, instead of POLY_TO_HANDLE first. Same shapes as the VEC_*
        // entries above except the leading arg is a payload, not a handle — and
        // SET returns i64 (1/0) rather than Void, so a caller can tell a rejected
        // slot from a stored one.

        // ---- REAL PolyValue <-> handle bridge (rts-engine heap::handles) ----
        // FROM_HANDLE: full real handle -> bare 48-bit slot+shard payload.
        // TO_HANDLE: 48-bit payload -> full real handle (gen reconstructed from
        // the live slot). These REPLACE the old `__rtsadp_store/_load` table.
        // (The `__rtsadp_*` generic operators that shared this arm — typeof /
        // to_string / to_boolean / await / word_to_abi_i64 / box_handle_auto /
        // class_proto, and the whole two-word arithmetic + comparison + bitwise
        // family — are DERIVED now; only the engine-buffer externs below still
        // need a hand row.)
        // SCALAR float `%` fast path (step 9): raw f64 in/out, no PolyValue boxing.
        // ---- generic unary (P4.8 + P5.6): one PolyValue word ----
        // console.* line sink: (ptr, len) StrPtr (two slots) + a to_stderr flag
        // (I64: 0 = stdout/IO_PRINT, !=0 = stderr/IO_EPRINT), void.
        // inspect(value_word, top_level) -> string PolyValue word. value is a raw
        // PolyValue word (U64); top_level is an I64 flag (1 = direct arg / bare
        // string, 0 = nested / quoted string); the result is a string PolyValue.
        // inspect_object(value_word, top_level) -> string PolyValue word. Same ABI
        // as `__rtsadp_inspect` but renders the `TAG_OBJECT` word as an OBJECT
        // (`{ k: v }`), recovering keys from the slot-0 global shape-id. The
        // lowering calls this for a statically-OBJECT console.log arg (P3.6).

        // ---- codegen-owned FUNCTION-value trampolines (__rtsadp_fn_*, P4.6) ----
        // reify(addr, nparams, has_rest, env_word) -> 48-bit slot+shard payload
        // (U64). env_word is the captured-env PolyValue word (0 = non-capturing).
        // invoke(fn_word, a0, a1, a2, a3, rest) -> result PolyValue word (U64).
        // All slots are raw PolyValue words (U64); the fixed uniform call ABI.
        // ---- new <value>() through a class VALUE (slice 2) ----
        // register_ctor_thunk(addr) -> void: mark a class NEW-THUNK address valid.
        // new_invoke(fn_word, a0, a1, a2, a3, rest) -> instance PolyValue word (U64);
        // throws (pending error) when the value is not a registered constructor.
        // ---- function-as-constructor side-table (new F() / x instanceof F) ----
        // fn_ptr(fn_word) -> ctor identity (U64, 0 = not a fn).
        // ctor_mark(obj_word, fn_ptr) -> void (record instance→ctor identity).
        // instanceof_fn(obj_word, fn_ptr) -> bool PolyValue word (U64).
        // ---- function-VALUE data properties (`F.foo = v` / `F.foo`) (Phase 4) ----
        // get_prop(fn_word, key_word) -> stored value PolyValue word, or undefined.
        // set_prop(fn_word, key_word, value_word) -> value_word. key_word is an
        // interned string PolyValue (the static property name), like __rtsadp_obj_get.
        // step 5b: (view_word, *out_base, *out_count) -> elem_bytes. The two
        // pointers are stack-slot addresses (I64); the return is the byte-width
        // (0 = not a view).
        "__rtsadp_ta_view_base_len" => SymSig {
            params: &[U64, I64, I64],
            ret: I64,
        },

        // ---- codegen-owned Array trampolines (__rtsadp_arr_*, P4.5) ----
        // All slots are u64/i64 (no StrPtr): slot 0 is the array's REAL Vec handle
        // (`POLY_TO_HANDLE` of the array word); needle args are raw PolyValue words
        // (U64); index/range args are I64; results are PolyValue words (U64) / a
        // string Handle (join) / an i64 (index_of/push) / a bool (includes).
        // arity-variant + ES2023 Array trampolines (recv U64 + the explicit args)

        // ---- codegen-owned GLOBAL / static trampolines (P5.2) ----
        // Coercions + predicates: one PolyValue word in, one PolyValue word out.
        // parseInt(value_word, radix): radix is an I64 (0 = auto).
        // str.split(recvHandle, sepHandle, limit) -> a TAG_OBJECT array word.
        // math_reduce(arr_word, op) -> f64 (op: 0=min, 1=max, 2=hypot).
        // canon_double(word) -> a guaranteed inline-double PolyValue word.
        // arr_spread_append(dst_word, src_word) -> void (push all src elements).

        // string→string GLOBAL fns (encodeURIComponent/btoa/…): one StrPtr arg
        // (ptr+len) → a string Handle.

        // (The codegen-owned Map/Set instance trampolines that used to sit here —
        // `__rtsadp_map_*` / `__rtsadp_set_*`, 11 names — were DELETED 2026-07-28:
        // none of those symbols exists anywhere in the tree. Map/Set moved to the
        // `.ts` stdlib (`rts-shared/src/stdlib/map_set.ts`) and their trampolines
        // went with them; these rows outlived them, naming symbols that could only
        // ever have produced an unresolved call. Found by `baked_existence`, the
        // guard added with the baked table — nothing had checked before.)

        // (The pending-error slot — `__rtsadp_throw_set`/`err_pending`/`err_take`/
        // `err_clear` — was CONVERTED to `#[rtse::abi]` on 2026-07-28 and its rows
        // DELETED: the baker now derives their signatures from the Rust ones, and
        // `sig_of` finds them in the baked table before reaching this one. That is
        // the drain pattern for every remaining row here.)

        // ---- codegen-owned DYNAMIC method dispatch (P5.9) ----
        // Uniform PolyValue-word ABI: the receiver word + 0..2 PolyValue-word args,
        // one PolyValue-word result. The trampoline branches on the receiver's tag
        // at runtime and delegates to the per-class op; an unexpected tag returns
        // the `undefined` word (sound — never a wrong value).
        // 1-word (receiver only): toString/length/pop/toUpper/toLower/trim.
        // 2-word (receiver + 1 arg): indexOf/includes/at/concat/join/push/charAt/
        // charCodeAt/split/startsWith/endsWith/repeat.
        // 3-word (receiver + 2 args): slice(start, end).

        // ---- codegen-owned RegExp + string-regex-method trampolines (P5.12) ----
        // All slots are raw PolyValue words (U64): compile takes (pattern, flags)
        // string words → a RegExp instance word; test takes (re, subject) → a bool
        // word; source/flags/global take the re word → a string/bool word; the
        // string-regex methods take (subject, re[, repl]) words → string/array/
        // number/null words. The trampolines do the string-handle marshaling.
        // Full JS-spec split: (subj, re, limit_word) — `undefined` limit = none.
        // replace/replaceAll — the ONE polymorphic replacement trampoline
        // (string → JS GetSubstitution; function → invoked per match; else
        // ToString): (subj_word, re_word, repl_word, all_flag) -> string word.

        // ---- codegen-owned DYNAMIC property access (P5.5) ----
        // obj_get(obj_word, key_str_handle) -> PolyValue word; obj_set adds a
        // value-word param and returns the value word; obj_has returns a Bool
        // (i64 0/1). The key is a string PolyValue word (U64) — a literal interned
        // at lowering or a computed key's ToString.
        // The `globalThis` singleton object word (no args; returns a TAG_OBJECT
        // PolyValue). `globalThis.prop` get/set then reuse __rtsadp_obj_get/_set.
        // Prototype chain (Object.create): create(proto)->obj, proto_of(obj)->proto,
        // is_prototype_of(proto, obj)->bool word. All raw PolyValue words.
        // Property descriptors + extensibility (real state). prop_flags → packed
        // flags / -1 (I64); define_prop → 1/0 (I64); prevent_ext/is_extensible → 1/0.
        // obj_keys/values/entries(obj_word) -> fresh array PolyValue word (dynamic
        // Object.keys/values/entries; Object is primordial → engine-direct).

        // (The Map/Set instanceof tags `__rtsadp_is_map`/`__rtsadp_is_set` were
        // DELETED 2026-07-28 for the same reason as the trampolines above: the
        // symbols do not exist. The wrapper ctors + Error family had already gone
        // the same way — `__rtsadp_w_*`/`__rtsadp_err_*`/`__rtsadp_is_error` — when
        // those became `.ts` prelude classes.)

        // ---- codegen-owned Array CALLBACK trampolines (__rtsadp_arr_*, P4.7) ----
        // Slot 0 = the array's REAL Vec handle; slot 1 = the callback as a
        // TAG_FUNCTION PolyValue word (U64). map/filter return a fresh TAG_OBJECT
        // array word (U64); forEach returns undefined (U64); find returns the
        // element word (U64); findIndex returns an index (I64); some/every return
        // a bool. reduce takes an extra init word (U64) + a has_init flag (I64).

        // ---- REAL global-class instance methods (P4 data-driven dispatch) ----
        // String: the non-regex method surface migrated to the primordial `String`
        // value-class (`rts-primitives/src/string/value_class.rs`), computed via
        // `strops` — no GL_STRING extern. Only the regex-argument family remains
        // (its runtime string-vs-regex dispatch the macro cannot yet express):
        // `search` → I64; `match`/`matchAll` → a Vec-of-strings Handle (0 ⇒ null).
        // (Their rows are gone: the three `_auto` dispatchers carry `#[rtse::abi]`
        // in `rts-primitives/src/string/search.rs`, spelling `Handle` in the Rust
        // signature, so the baked table supplies them.)
        // Number's toFixed/toPrecision/toExponential/toString(radix) are now
        // value-class METHODS computed directly in Rust (`rts-primitives/src/
        // number/mod.rs` + `format.rs`) — no `__RTS_FN_GL_NUMBER_TO_*` symbol
        // called by name anymore.

        // ---- REAL Math namespace symbols (P5.4, rts-shared math) ----
        // 1-arg f64 → f64.
        // 2-arg f64 → f64.
        // no-arg f64 (the seeded PRNG).

        // ---- REAL Number static predicates (P5.4) ----
        // `rts-macro-single-source.md` pilot: each row copies params/ret off the
        // `#[rtse::class]`-derived `SymbolDesc` (`NumberWrapper::IS_*_SYM`)
        // instead of hand-typing them — the drift this file's own doc warns
        // about is now a copy, not a second derivation.
        "__rtsm_global_Number_is_integer" => from_desc(rts_runtime::NumberWrapper::IS_INTEGER_SYM),
        "__rtsm_global_Number_is_finite" => from_desc(rts_runtime::NumberWrapper::IS_FINITE_SYM),
        "__rtsm_global_Number_is_nan" => from_desc(rts_runtime::NumberWrapper::IS_NAN_SYM),
        "__rtsm_global_Number_is_safe_integer" => {
            from_desc(rts_runtime::NumberWrapper::IS_SAFE_INTEGER_SYM)
        }

        _ => return None,
    })
}

#[cfg(test)]
mod desc_agreement {
    //! Every symbol carrying `#[rtse::abi]` must agree with this file's
    //! hand-written table — until the table is deleted and the descriptors ARE
    //! the source (F0 of `docs/engine/architecture.md`).
    //!
    //! This is the guard that did not exist. `abi_sig`'s own doc says it exists
    //! because "mis-marshaling a single-slot string where the ABI expects two →
    //! SIGILL", yet nothing checked it against the real `extern "C"` signature;
    //! the two could drift silently and the failure mode is a runtime SIGILL,
    //! not a link error. `#[rtse::abi]` derives the descriptor FROM the Rust
    //! signature, so this test compares derived-truth against hand-written-truth
    //! and fails the moment they disagree.
    use super::sig_of;
    use rts_engine::abi::SymbolDesc;

    /// Assert the hand table agrees with the macro-derived descriptor.
    fn agree(d: SymbolDesc) {
        let hand = sig_of(d.name)
            .unwrap_or_else(|| panic!("`{}` has a SymbolDesc but no abi_sig entry", d.name));
        assert_eq!(
            hand.params, d.params,
            "params disagree for `{}`: abi_sig table vs #[rtse::abi]-derived",
            d.name
        );
        assert_eq!(
            hand.ret, d.ret,
            "return disagrees for `{}`: abi_sig table vs #[rtse::abi]-derived",
            d.name
        );
    }

    #[test]
    fn descriptors_match_the_hand_table() {
        // Grows as `#[rtse::abi]` is applied to more symbols (F7 drains the rest).
        agree(rts_runtime::adapters::value::objops::OBJ_GET_ABI);
        // Number predicates pilot (`#[rtse::class]`-derived, not `#[rtse::abi]`) —
        // same guard, same `SymbolDesc` shape either macro emits.
        agree(rts_runtime::NumberWrapper::IS_INTEGER_SYM);
        agree(rts_runtime::NumberWrapper::IS_FINITE_SYM);
        agree(rts_runtime::NumberWrapper::IS_NAN_SYM);
        agree(rts_runtime::NumberWrapper::IS_SAFE_INTEGER_SYM);
    }
}

#[cfg(test)]
pub(super) mod baked_existence {
    //! Every symbol NAMED in this file's table must EXIST in the baked symbol
    //! table — the guard that turns a renamed/mistyped symbol into a test
    //! failure instead of a runtime SIGILL.
    //!
    //! `sig_of` is a `match`, so there is no runtime list of its arms to iterate.
    //! Rather than hand-write one — which would be a THIRD mirror of the same
    //! names, exactly what this campaign deletes — the test reads its own source
    //! and extracts the match arms. Self-maintaining: a row added or deleted is
    //! picked up with no second place to edit.
    //!
    //! Membership in the baked table is proof of existence because
    //! `rts-symbol-baker` derives that table from the DECLARATIONS in the source
    //! (see docs/engine/architecture.md). This is the cheap half of
    //! F12; the expensive half is deleting these rows in favour of the
    //! macro-derived `SymbolDesc`, which also gives the SIGNATURE, not just the
    //! name.

    /// The symbol names spelled in `sig_of`'s match arms, read from this file.
    ///
    /// An arm may list SEVERAL names sharing one signature, on one line
    /// (`"__a" | "__b" => SymSig {`) or continued across lines (a line ending in
    /// `|`), so every quoted `__…` token on a match-arm line counts — taking only
    /// the first would silently under-check the alternates.
    /// This module.s own source, read at compile time — see [`table_names`].
    const SRC: &str = include_str!("abi_sig.rs");

    pub(super) fn table_names() -> Vec<&'static str> {
        SRC.lines()
            .filter(|l| {
                let l = l.trim();
                // A continued arm's later lines start with the `|` separator
                // (`| "__rtsadp_map_has"`), not with the quote.
                (l.starts_with('"') || l.starts_with("| \""))
                    && (l.contains("=> SymSig") || l.ends_with('|') || l.ends_with('"'))
            })
            .flat_map(|l| {
                // Odd-indexed pieces of a split on `"` are the quoted tokens.
                l.split('"').skip(1).step_by(2)
            })
            .filter(|n| n.starts_with("__"))
            .collect()
    }

    #[test]
    fn every_named_symbol_is_in_the_baked_table() {
        let names = table_names();
        // Sanity-check the EXTRACTION, not the table's size: every match arm must
        // have yielded at least one name. A fixed floor would have to be lowered
        // on every drain — and would eventually be lowered past the point where it
        // still detects a broken scan.
        let arms = SRC.lines().filter(|l| l.contains("=> SymSig")).count();
        assert!(
            names.len() >= arms && arms > 0,
            "the source scan found {} names for {arms} match arms — the extraction \
             broke, not the table",
            names.len()
        );
        let baked: std::collections::HashSet<&str> = rts_runtime::symbol_table::symbols()
            .iter()
            .map(|e| e.name)
            .collect();
        let mut missing: Vec<&str> = names
            .into_iter()
            .filter(|n| !baked.contains(n))
            .collect();
        missing.sort_unstable();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "{} abi_sig row(s) name a symbol that is NOT in the baked table — the \
             symbol was renamed/deleted (this row now marshals a call that cannot \
             resolve) or the baker's scan does not reach its declaration:\n{}",
            missing.len(),
            missing.join("\n")
        );
    }
}

#[cfg(test)]
mod hand_rows_are_drained {
    //! A hand row whose symbol the BAKED signature table already carries is dead
    //! code: `sig_of` consults the baked entry first, so the row is unreachable.
    //! This lists them so each conversion's rows get deleted in the same commit
    //! instead of lingering as an unreachable second opinion.
    use super::baked_existence::table_names;

    #[test]
    fn no_hand_row_is_shadowed_by_a_derived_signature() {
        let baked: std::collections::HashSet<&str> = rts_runtime::symbol_table::signatures()
            .iter()
            .map(|&(n, _, _)| n)
            .collect();
        // A row is also unreachable when the REGISTRY carries the signature: the
        // lookup order is baked -> registry -> hand.
        let from_registry = super::registry_sigs();
        let mut shadowed: Vec<&str> = table_names()
            .into_iter()
            .filter(|n| baked.contains(n) || from_registry.contains_key(n))
            .collect();
        shadowed.sort_unstable();
        shadowed.dedup();
        assert!(
            shadowed.is_empty(),
            "{} hand row(s) are shadowed by a DERIVED signature and can never be \
             reached — delete them:\n{}",
            shadowed.len(),
            shadowed.join("\n")
        );
    }
}

#[cfg(test)]
mod remaining_rows_report {
    //! What is LEFT in the hand table, and which mechanism could supply it.
    //! Measurement for the drain, printed with `--nocapture`; not an assertion.
    #[test]
    fn report() {
        let names = super::baked_existence::table_names();
        let baked: std::collections::HashSet<&str> = rts_runtime::symbol_table::signatures()
            .iter()
            .map(|&(n, _, _)| n)
            .collect();
        let mut adp = 0;
        let mut rt = 0;
        let mut other = 0;
        for n in &names {
            if baked.contains(n) {
                continue;
            }
            if n.starts_with("__rtsadp_") {
                adp += 1;
            } else if n.starts_with("__RTS_FN_") {
                rt += 1;
            } else {
                other += 1;
            }
        }
        eprintln!(
            "hand rows left: {} (__rtsadp_ {} | __RTS_FN_ {} | other {}); derived: {}",
            adp + rt + other,
            adp,
            rt,
            other,
            baked.len()
        );
    }
}
