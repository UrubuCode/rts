//! `File extends Blob` — adds `name`/`lastModified`. Parity source:
//! `webapi.ts`'s `class File extends Blob` + the deleted hand-written Rust.
//!
//! Composition + forwarding (DRAIN_MOTOR §11 "Herança"): `File` embeds a
//! `Blob` field and forwards `size`/`type`/`text`/`arrayBuffer`; `slice()`
//! forwards too and — matching the real DOM spec (`Blob.prototype.slice`
//! always returns a plain `Blob`, never the subclass) — returns a `Blob`, not
//! a `File`. `extends = "Blob"` links the Registry parent so `file instanceof
//! Blob` resolves via the class hierarchy (no chain-walk dispatch needed since
//! every method here is forwarded explicitly).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{alloc_rtse, with_rtse};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use super::blob::Blob;
use super::{opt_num, slice_text, text_to_byte_array};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[rtse::class("File")]
#[derive(Clone, Default)]
pub struct File {
    base: Blob,
    name: String,
    #[rtse::variable(readonly)]
    last_modified: i64,
}

impl File {
    pub(crate) fn text_ref(&self) -> &str {
        self.base.text_ref()
    }
}

#[rtse::class("File", extends = "Blob")]
impl File {
    /// `new File(parts, name, opts?)` — `opts.lastModified` if present,
    /// else `Date.now()` (matches `webapi.ts`'s fallback).
    #[rtse::ctor(optional = 1)]
    fn new(parts: Handle, name: &str, opts: Handle) -> Self {
        let base = Blob::build(parts, opts);
        let last_modified = opt_num(opts, "lastModified")
            .map(|n| n as i64)
            .unwrap_or_else(now_ms);
        File {
            base,
            name: name.to_string(),
            last_modified,
        }
    }

    /// `file.name` — a computed `String` getter (can't be `#[rtse::variable]`,
    /// scalar-only).
    #[rtse::getter]
    fn name(self: &File) -> String {
        self.name.clone()
    }

    /// `file.size` — forwarded to the embedded `Blob`.
    #[rtse::getter]
    fn size(self: &File) -> i64 {
        self.base.text_ref().len() as i64
    }

    /// `file.type` — forwarded to the embedded `Blob`.
    #[rtse::getter(name = "type")]
    fn kind(self: &File) -> String {
        self.base.kind_ref().to_string()
    }

    /// `file.text()` — forwarded to the embedded `Blob`.
    #[rtse::method]
    fn text(self: &File) -> String {
        self.base.text_ref().to_string()
    }
}

// ---------------------------------------------------------------------------
// Hand-written residual (same macro gaps as `Blob`): `arrayBuffer()` (raw
// `number[]`) and `slice()` (fresh instance — per spec, a plain `Blob`).
// ---------------------------------------------------------------------------

/// `file.arrayBuffer()` — forwarded to the embedded `Blob`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FILE_ARRAY_BUFFER(h: Handle) -> Handle {
    let text = with_rtse::<File, _>(h, |s| s.map(|f| f.base.text_ref().to_string())).unwrap_or_default();
    text_to_byte_array(&text)
}

/// `file.slice(start?, end?, contentType?)` — per spec, `Blob.prototype.slice`
/// always returns a plain `Blob`, never the subclass.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FILE_SLICE(h: Handle, start: i64, end: i64, content_type: Handle) -> Handle {
    let (text, kind) = with_rtse::<File, _>(h, |s| {
        s.map(|f| (f.base.text_ref().to_string(), f.base.kind_ref().to_string()))
    })
    .unwrap_or_default();
    let piece = slice_text(&text, start, end);
    let new_kind = if content_type == 0 {
        kind
    } else {
        rts_engine::heap::handles::with_entry(content_type, |e| match e {
            Some(rts_engine::heap::handles::Entry::String(b)) => {
                super::utf8_decode(b).to_ascii_lowercase()
            }
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

/// Reopens the macro-generated `File` class (ctor/name/size/type/text) and
/// appends the hand-written residual (`arrayBuffer`/`slice`).
pub fn register_file_class_spec(e: &mut Engine) {
    register(e);
    let macro_members: Vec<Member> = e
        .registry()
        .class("File")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    let mut cb = e
        .class("File")
        .extends("Blob")
        .doc("File — a named Blob with a lastModified timestamp (extends Blob).");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(member(
        "arrayBuffer",
        MemberKind::InstanceMethod,
        Sig::new(vec![AbiType::Handle], AbiType::Handle),
        "__RTS_FN_GL_FILE_ARRAY_BUFFER",
        "arrayBuffer(): number[]",
        "file.arrayBuffer() — forwarded to the embedded Blob.",
        __RTS_FN_GL_FILE_ARRAY_BUFFER as *const u8,
        true,
    ))
    .member(member(
        "slice",
        MemberKind::InstanceMethod,
        Sig::with_defaults(
            vec![AbiType::Handle, AbiType::I64, AbiType::I64, AbiType::Handle],
            AbiType::Handle,
            vec![
                rts_engine::DefaultArg::Int(0),
                rts_engine::DefaultArg::Int(-1),
                rts_engine::DefaultArg::Undefined,
            ],
        ),
        "__RTS_FN_GL_FILE_SLICE",
        "slice(start?: number, end?: number, contentType?: any): Blob",
        "file.slice(start?, end?, contentType?) — returns a plain Blob (spec).",
        __RTS_FN_GL_FILE_SLICE as *const u8,
        true,
    ))
    .done();
}
