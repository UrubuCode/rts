//! `Blob` — an immutable byte/text blob (`size`/`type` getters, `text()`/
//! `arrayBuffer()`/`slice()`). Parity source: `crates/rts-shared/src/stdlib/
//! webapi.ts`'s `class Blob` (the live surface) + the deleted hand-written
//! Rust (`git show HEAD~1:crates/rts-std/src/globals/blob/mod.rs`).
//!
//! `#[rtse::class]`-authored: ctor (`optional = 2` covers `new Blob()`/`new
//! Blob(parts)`/`new Blob(parts, opts)` in one Rust fn — F4), `size`/`type`
//! getters, `text()`. Hand-written residual (same pattern as `Headers`/
//! `UrlSearchParams`): `arrayBuffer()` (returns a raw `number[]`, not a
//! `Vec<String>`/`Vec<Handle>` — outside macro F8's element types) and
//! `slice()` (constructs a FRESH `Blob` instance, which only `#[rtse::ctor]`
//! can `alloc_rtse`-wrap — a plain method can't return `Self`).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{Entry, alloc_rtse, with_entry, with_rtse};
use rts_engine::{AbiType, DefaultArg, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use super::{concat_parts, opt_str, slice_text, text_to_byte_array};

/// The blob's content, normalized to decoded UTF-8 TEXT at construction time
/// (matches `webapi.ts`'s model: every part — string or byte-source — is
/// folded into one `String`; `size`/`text`/`arrayBuffer`/`slice` all derive
/// from it). `kind` is the lowercased MIME type (`""` if unset).
#[rtse::class("Blob")]
#[derive(Clone, Default)]
pub struct Blob {
    text: String,
    kind: String,
}

impl Blob {
    /// Shared ctor body (also used by `File::build`, which embeds a `Blob`).
    pub(crate) fn build(parts: Handle, opts: Handle) -> Blob {
        let text = concat_parts(parts);
        let kind = opt_str(opts, "type")
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        Blob { text, kind }
    }

    pub(crate) fn text_ref(&self) -> &str {
        &self.text
    }

    pub(crate) fn kind_ref(&self) -> &str {
        &self.kind
    }

    /// Build directly from already-decoded text + a MIME type (used by
    /// `slice()`, which doesn't go through `BlobPart[]` parsing).
    pub(crate) fn from_parts(text: String, kind: String) -> Blob {
        Blob { text, kind }
    }
}

#[rtse::class("Blob")]
impl Blob {
    /// `new Blob()` / `new Blob(parts)` / `new Blob(parts, opts?)` — F4
    /// (`optional = 2`) covers all three arities with one Rust fn.
    #[rtse::ctor(optional = 2)]
    fn new(parts: Handle, opts: Handle) -> Self {
        Blob::build(parts, opts)
    }

    /// `blob.size` — UTF-8 BYTE length (`String::len()` is already the byte
    /// count, not the char count).
    #[rtse::getter]
    fn size(self: &Blob) -> i64 {
        self.text.len() as i64
    }

    /// `blob.type` — lowercased MIME type, `""` if unset.
    #[rtse::getter(name = "type")]
    fn kind(self: &Blob) -> String {
        self.kind.clone()
    }

    /// `blob.text()` — the decoded text, directly (the engine's interim async
    /// model passes a non-Promise `await` through unchanged, matching
    /// `webapi.ts`'s synchronous `text(): string`).
    #[rtse::method]
    fn text(self: &Blob) -> String {
        self.text.clone()
    }
}

// ---------------------------------------------------------------------------
// Hand-written residual: `arrayBuffer()` (raw `number[]`, outside F8's element
// types) and `slice()` (constructs a fresh `Blob`, which only `#[rtse::ctor]`
// can `alloc_rtse`-wrap). Both read the SAME `Blob` struct via `with_rtse`.
// ---------------------------------------------------------------------------

/// `blob.arrayBuffer()` — UTF-8 bytes as a plain `number[]` (matches
/// `webapi.ts`'s `__utf8_encode`; not yet a real `ArrayBuffer` object).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_ARRAY_BUFFER(h: Handle) -> Handle {
    let text = with_rtse::<Blob, _>(h, |s| s.map(|b| b.text.clone())).unwrap_or_default();
    text_to_byte_array(&text)
}

/// `blob.slice(start?, end?, contentType?)` — byte-range extraction into a
/// FRESH `Blob`. `contentType`, when a string handle, becomes the new blob's
/// `type` (lowercased); otherwise the slice keeps the source's `type`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_SLICE(h: Handle, start: i64, end: i64, content_type: Handle) -> Handle {
    let (text, kind) =
        with_rtse::<Blob, _>(h, |s| s.map(|b| (b.text.clone(), b.kind.clone()))).unwrap_or_default();
    let piece = slice_text(&text, start, end);
    let new_kind = if content_type == 0 {
        kind
    } else {
        with_entry(content_type, |e| match e {
            Some(Entry::String(b)) => super::utf8_decode(b).to_ascii_lowercase(),
            _ => kind,
        })
    };
    alloc_rtse("Blob", Blob::from_parts(piece, new_kind))
}

#[allow(clippy::too_many_arguments)]
fn member(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        ret_class: None,
        emit: None,
    }
}

/// Reopens the macro-generated `Blob` class (ctor/size/type/text) and appends
/// the hand-written residual (`arrayBuffer`/`slice`) — same pattern as
/// `headers::register_headers_class_spec`.
pub fn register_blob_class_spec(e: &mut Engine) {
    register(e);
    let macro_members: Vec<Member> = e
        .registry()
        .class("Blob")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    let mut cb = e.class("Blob").doc("Blob — immutable byte/text blob.");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(member(
        "arrayBuffer",
        MemberKind::InstanceMethod,
        Sig::new(vec![AbiType::Handle], AbiType::Handle),
        "__RTS_FN_GL_BLOB_ARRAY_BUFFER",
        "arrayBuffer(): number[]",
        "blob.arrayBuffer() — UTF-8 bytes as a plain number[].",
        __RTS_FN_GL_BLOB_ARRAY_BUFFER as *const u8,
        true,
    ))
    .member(member(
        "slice",
        MemberKind::InstanceMethod,
        Sig::with_defaults(
            vec![AbiType::Handle, AbiType::I64, AbiType::I64, AbiType::Handle],
            AbiType::Handle,
            vec![DefaultArg::Int(0), DefaultArg::Int(-1), DefaultArg::Undefined],
        ),
        "__RTS_FN_GL_BLOB_SLICE",
        "slice(start?: number, end?: number, contentType?: any): Blob",
        "blob.slice(start?, end?, contentType?) — byte-range into a fresh Blob.",
        __RTS_FN_GL_BLOB_SLICE as *const u8,
        true,
    ))
    .done();
}
