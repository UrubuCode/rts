//! `Cipheriv`/`Decipheriv` — `createCipheriv`/`createDecipheriv` over
//! AES-128/256 in GCM and CBC.
//!
//! # Where the cipher state lives, and why that is not a new decision
//!
//! Exactly where `hash.rs` puts a digest mid-update: an instance carries one
//! hidden `__cipherId` number, and [`TABLE`] holds the bytes. A key, an IV and
//! an accumulated message are native state no value in this engine's
//! shape-and-property system can hold, which is the same sentence `fs/fd.rs`
//! writes about an open file. Nothing here mints a number from anything but its
//! own counter, and nothing here holds a JS value across a collection — the
//! table is bytes only, so it is not a root that could be missing from a list
//! (`docs/engine/lost-roots.md` is the class this stays out of by construction).
//!
//! # `update()` accumulates and answers nothing; `final()` does the work
//!
//! This is the divergence from Node worth reading before writing against it.
//! Real Node streams: `update()` answers the ciphertext of what it could
//! process, `final()` answers the remainder. Here `update()` appends to a buffer
//! and answers an EMPTY `Buffer`, and `final()` answers the whole result.
//!
//! `Buffer.concat([c.update(x), c.final()])` — which is how the call is written
//! in practice, and how Baileys and every libsignal port write it — is correct
//! under both models, which is what makes the divergence affordable. What is NOT
//! correct here is streaming a message larger than memory, and what is not
//! offered at all is per-call output.
//!
//! The reason is GCM and not convenience: an authenticated cipher cannot answer
//! plaintext for a prefix of a message whose tag it has not checked yet. Node
//! answers it anyway — its `Decipheriv.update()` hands back unauthenticated
//! bytes, and the caller is expected to discard them if `final()` throws. That
//! is a real footgun in Node, and reproducing it faithfully would mean this
//! module handing out plaintext it has not authenticated. CBC could stream
//! honestly, but then `update()` would mean one thing for CBC and another for
//! GCM, and a caller reading the doc would have to hold two models. One model,
//! stated, was judged the better of the two honest options.
//!
//! # Not implemented, by name
//!
//! - **`setAutoPadding(false)`** — CBC here always uses PKCS#7. A no-padding
//!   mode that silently padded anyway is the hollow surface CLAUDE.md's `sync`
//!   note refuses; an absent method fails at the call.
//! - **Every algorithm outside [`algo::CipherAlgo::NAMES`]** — CTR, ECB,
//!   ChaCha20-Poly1305, the `-wrap` family. Adding one is a line in
//!   `CipherAlgo` and a crate in the manifest, not a change of shape here.
//! - **`authTagLength`** and the options object generally — GCM's tag is 16
//!   bytes, which is the only length `aes-gcm` as configured produces.
//! - **The stream interface** (`Cipheriv` as a `Transform`) — `node:stream` is
//!   a separate surface and piping into a cipher is a `node:stream` question.

pub(super) mod algo;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core::entry::{self, Context, Provided};

use self::algo::CipherAlgo;
use super::util;

/// One cipher in progress. Bytes only — see this module's doc for why that
/// matters beyond tidiness.
struct CipherState {
    algo: CipherAlgo,
    key: Vec<u8>,
    iv: Vec<u8>,
    /// `setAAD` data, authenticated but not encrypted. Empty when never set,
    /// which is the same input GCM sees for a call that never mentions it.
    aad: Vec<u8>,
    /// What `update()` has accumulated so far.
    input: Vec<u8>,
    /// Decrypt only: the tag `setAuthTag` was given, consumed by `final()`.
    expected_tag: Vec<u8>,
    /// Encrypt only: the tag `final()` produced, for `getAuthTag()`.
    produced_tag: Option<Vec<u8>>,
    decrypting: bool,
}

static TABLE: Mutex<Option<HashMap<u64, CipherState>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, CipherState>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[
    ("update", update),
    ("final", finalize),
    ("setAAD", set_aad),
    ("setAuthTag", set_auth_tag),
    ("getAuthTag", get_auth_tag),
];

/// `crypto.createCipheriv(algorithm, key, iv)`.
pub(crate) extern "C" fn create_cipheriv(
    _e: u64,
    _this: u64,
    algorithm: u64,
    key: u64,
    iv: u64,
    _a3: u64,
) -> u64 {
    create(algorithm, key, iv, false)
}

/// `crypto.createDecipheriv(algorithm, key, iv)`.
pub(crate) extern "C" fn create_decipheriv(
    _e: u64,
    _this: u64,
    algorithm: u64,
    key: u64,
    iv: u64,
    _a3: u64,
) -> u64 {
    create(algorithm, key, iv, true)
}

/// `crypto.getCiphers()` — [`algo::CipherAlgo::NAMES`], which is what
/// [`algo::CipherAlgo::parse`] accepts and nothing more. This used to answer
/// `[]` because nothing backed a name; the list and the parser are one table now
/// so that they cannot disagree.
pub(crate) extern "C" fn get_ciphers(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let values = CipherAlgo::NAMES
            .iter()
            .map(|name| entry::make_string(context, name))
            .collect();
        entry::make_array_in(context, values)
    })
}

/// The shared half of both constructors. Every argument is validated HERE — an
/// unknown algorithm, a key of the wrong length, an IV of the wrong length —
/// rather than at `final()`, because a program that mistyped `aes-256-gcm`
/// should learn it at the line that names it, not several calls later inside a
/// function that never mentions the algorithm.
fn create(algorithm: u64, key: u64, iv: u64, decrypting: bool) -> u64 {
    enum Refusal {
        Algorithm,
        KeyLength(usize, usize),
        IvLength(usize, usize),
    }
    let outcome = entry::with_runtime(|context| {
        let name = util::text(context, algorithm).unwrap_or_default();
        let Some(algo) = CipherAlgo::parse(&name) else {
            return Err(Refusal::Algorithm);
        };
        let key = util::binary_bytes(context, key);
        let iv = util::binary_bytes(context, iv);
        if key.len() != algo.key_len() {
            return Err(Refusal::KeyLength(algo.key_len(), key.len()));
        }
        if iv.len() != algo.iv_len() {
            return Err(Refusal::IvLength(algo.iv_len(), iv.len()));
        }
        Ok(build(
            context,
            CipherState {
                algo,
                key,
                iv,
                aad: Vec::new(),
                input: Vec::new(),
                expected_tag: Vec::new(),
                produced_tag: None,
                decrypting,
            },
        ))
    });
    match outcome {
        Ok(instance) => instance,
        Err(refusal) => {
            let message = match refusal {
                Refusal::Algorithm => "Unknown cipher".to_owned(),
                Refusal::KeyLength(want, got) => {
                    format!("Invalid key length: expected {want} bytes, got {got}")
                }
                Refusal::IvLength(want, got) => {
                    format!("Invalid initialization vector: expected {want} bytes, got {got}")
                }
            };
            entry::throw_type_error(&message);
            entry::undefined_value()
        }
    }
}

fn build(context: &mut Context, state: CipherState) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let name = if state.decrypting { "Decipheriv" } else { "Cipheriv" };
    with_table(|table| {
        table.insert(id, state);
    });
    let prototype = entry::make_prototype(context, name, METHODS);
    let instance = entry::make_instance(context, prototype);
    util::put_hidden_number(context, instance, "__cipherId", id);
    instance
}

/// `cipher.update(data, inputEncoding?)` — accumulates, answers an empty
/// `Buffer`. See this module's doc for why the answer is empty rather than the
/// ciphertext of `data`.
///
/// An empty `Buffer` and not `this`: `Buffer.concat([c.update(x), c.final()])`
/// is the call this has to stay correct under, and `concat` of a non-buffer
/// throws. Chaining (`c.update(a).update(b)`) is what that costs, and Node does
/// not chain here either — its `update` answers bytes too.
extern "C" fn update(_e: u64, this: u64, data: u64, input_encoding: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(id) = util::hidden_number(context, this, "__cipherId") else {
            return entry::undefined_in(context);
        };
        let encoding = util::text(context, input_encoding);
        let bytes = util::binary_bytes_encoded(context, data, encoding.as_deref());
        with_table(|table| {
            if let Some(state) = table.get_mut(&id) {
                state.input.extend_from_slice(&bytes);
            }
        });
        entry::make_buffer(context, &[])
    })
}

/// `cipher.setAAD(data)` — additional authenticated data, GCM only. Chainable
/// (answers `this`), which is what Node does and what makes
/// `createCipheriv(...).setAAD(h)` read.
///
/// Called on a CBC instance it throws rather than being ignored: AAD that is
/// accepted and never authenticated is a caller believing a message is bound to
/// a header when it is not.
extern "C" fn set_aad(_e: u64, this: u64, data: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let ok = entry::with_runtime(|context| {
        let Some(id) = util::hidden_number(context, this, "__cipherId") else {
            return true;
        };
        let bytes = util::binary_bytes(context, data);
        with_table(|table| match table.get_mut(&id) {
            Some(state) if state.algo.is_gcm() => {
                state.aad = bytes;
                true
            }
            Some(_) => false,
            None => true,
        })
    });
    if !ok {
        entry::throw_type_error("setAAD is only supported for authenticated ciphers");
        return entry::undefined_value();
    }
    this
}

/// `decipher.setAuthTag(tag)` — held until `final()`, which is the only place
/// it can be checked. Chainable.
extern "C" fn set_auth_tag(_e: u64, this: u64, tag: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let ok = entry::with_runtime(|context| {
        let Some(id) = util::hidden_number(context, this, "__cipherId") else {
            return true;
        };
        let bytes = util::binary_bytes(context, tag);
        with_table(|table| match table.get_mut(&id) {
            Some(state) if state.decrypting && state.algo.is_gcm() => {
                state.expected_tag = bytes;
                true
            }
            Some(_) => false,
            None => true,
        })
    });
    if !ok {
        entry::throw_type_error("setAuthTag is only supported for decryption with an authenticated cipher");
        return entry::undefined_value();
    }
    this
}

/// `cipher.getAuthTag()` — the 16-byte GCM tag, valid only after `final()`.
/// Before `final()` there is no tag to answer, and this throws rather than
/// answering 16 zero bytes: a zero tag is a value a caller would happily
/// transmit.
extern "C" fn get_auth_tag(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let tag = entry::with_runtime(|context| {
        let id = util::hidden_number(context, this, "__cipherId")?;
        with_table(|table| table.get(&id).and_then(|state| state.produced_tag.clone()))
    });
    match tag {
        Some(bytes) => entry::with_runtime(|context| entry::make_buffer(context, &bytes)),
        None => {
            entry::throw_type_error("Attempt to get auth tag in unsupported state");
            entry::undefined_value()
        }
    }
}

/// `cipher.final(outputEncoding?)` — the whole ciphertext or plaintext.
///
/// The state is CONSUMED: the entry leaves the table, and a second `final()`
/// throws rather than answering the same bytes again. `hash.rs` answers
/// `undefined` on its second `digest()` and this does not follow it — there the
/// wrong answer is a missing digest, here it would be a caller encrypting a
/// message twice under one nonce and not being told, which is the failure that
/// destroys GCM outright.
extern "C" fn finalize(_e: u64, this: u64, output_encoding: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let taken = entry::with_runtime(|context| {
        let id = util::hidden_number(context, this, "__cipherId")?;
        with_table(|table| table.remove(&id)).map(|state| (id, state))
    });
    let Some((id, state)) = taken else {
        entry::throw_type_error("Cipher job is already finalized");
        return entry::undefined_value();
    };
    let outcome = match (state.decrypting, state.algo.is_gcm()) {
        (false, true) => algo::gcm_encrypt(state.algo, &state.key, &state.iv, &state.aad, &state.input)
            .map(|(ciphertext, tag)| (ciphertext, Some(tag))),
        (true, true) => algo::gcm_decrypt(
            state.algo,
            &state.key,
            &state.iv,
            &state.aad,
            &state.input,
            &state.expected_tag,
        )
        .map(|plaintext| (plaintext, None)),
        (false, false) => algo::cbc_encrypt(state.algo, &state.key, &state.iv, &state.input)
            .map(|ciphertext| (ciphertext, None)),
        (true, false) => algo::cbc_decrypt(state.algo, &state.key, &state.iv, &state.input)
            .map(|plaintext| (plaintext, None)),
    };
    match outcome {
        Ok((bytes, tag)) => entry::with_runtime(|context| {
            // The tag outlives the state the rest of the entry held, and only
            // the tag: a re-inserted entry carrying key and input would make a
            // second `final()` possible, which the doc above says it is not.
            if let Some(tag) = tag {
                with_table(|table| {
                    table.insert(
                        id,
                        CipherState {
                            produced_tag: Some(tag),
                            key: Vec::new(),
                            iv: Vec::new(),
                            aad: Vec::new(),
                            input: Vec::new(),
                            expected_tag: Vec::new(),
                            ..state
                        },
                    )
                });
            }
            let encoding = util::text(context, output_encoding);
            util::digest_output(context, &bytes, encoding.as_deref())
        }),
        Err(message) => {
            entry::throw_type_error(&message);
            entry::undefined_value()
        }
    }
}
