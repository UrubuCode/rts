//! The level-B TypedArray VIEW: a typed array laid over a SHARED `ArrayBuffer`.
//!
//! A view is a keyed object whose slots are [`VIEW_KEYS`] (slot 0 is the shape
//! header). Reads and writes go through `TA_GET_ELEM`/`TA_SET_ELEM` on the
//! shared `Entry::Buffer`, so sibling views of one buffer observe each other's
//! writes, and the `(byteOffset, length)` WINDOW is applied on every access —
//! which is what makes `new Uint8Array(buf, 4, 3)` a real view rather than a
//! whole-buffer alias.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::super::PolyValue;
use super::super::genops;
use super::Kind;

/// The engine-owned slot names of a level-B view, in POSITIONAL order (slot 0 is
/// the shape header, so `__ta_buf` is slot 1). Spelled ONCE — the ctor, the shape
/// id and the decoder all read this array, so a layout change cannot desync them.
const VIEW_KEYS: [&str; 6] = [
    "__ta_buf",
    "__ta_bytes",
    "__ta_signed",
    "__ta_float",
    "__ta_off",
    "__ta_len",
];

/// A decoded level-B view: the shared buffer plus the WINDOW over it.
///
/// `off`/`count` are what make `new Uint8Array(buf, byteOffset, length)` a real
/// view instead of a whole-buffer one. A view built from the 1-arg ctor is just
/// the degenerate window `off = 0`, `count = byteLength / bytes`, so ONE code
/// path serves both — there is no separate "whole buffer" representation.
#[derive(Clone, Copy)]
pub(crate) struct View {
    /// The shared `Entry::Buffer` handle.
    pub bh: u64,
    /// Element width in bytes (1/2/4/8) — never 0 (the decoder rejects that).
    pub bytes: i64,
    pub signed: i64,
    pub float: i64,
    /// `byteOffset` — the window's start, in BYTES from the buffer base.
    pub off: i64,
    /// The window's element count (`length`).
    pub count: i64,
}

impl View {
    /// The window start as an ELEMENT index into the buffer.
    ///
    /// `byteOffset` is exactly divisible by the element width for every legal
    /// view (JS throws a RangeError on a misaligned offset, and [`ta_view_new`]
    /// clamps one to an empty view), so this division is never lossy.
    fn base_elem(&self) -> i64 {
        self.off / self.bytes
    }

    /// `byteLength` — the window's size in bytes.
    pub(crate) fn byte_len(&self) -> i64 {
        self.count * self.bytes
    }

    /// Read `self[i]` as a JS number word. Out of the WINDOW ⇒ `undefined`.
    pub(crate) fn get(&self, i: i64) -> u64 {
        use rts_runtime::namespaces::buffer as rt_buf;
        if i < 0 || i >= self.count {
            return PolyValue::undefined().raw();
        }
        let raw = rt_buf::__RTS_FN_GL_TA_GET_ELEM(
            self.bh,
            self.base_elem() + i,
            self.bytes,
            self.signed,
            self.float,
        );
        if self.float != 0 {
            PolyValue::from_f64(f64::from_bits(raw as u64)).raw()
        } else {
            PolyValue::from_f64(raw as f64).raw()
        }
    }

    /// Write `self[i] = v` through the shared buffer. Out of the WINDOW ⇒ a
    /// no-op (JS drops an out-of-range typed-array index write silently).
    pub(crate) fn set(&self, i: i64, val_word: u64) {
        use rts_runtime::namespaces::buffer as rt_buf;
        if i < 0 || i >= self.count {
            return;
        }
        let n = genops::to_number(PolyValue::from_raw(val_word));
        let raw = if self.float != 0 {
            n.to_bits() as i64
        } else {
            n.trunc() as i64
        };
        rt_buf::__RTS_FN_GL_TA_SET_ELEM(
            self.bh,
            self.base_elem() + i,
            self.bytes,
            self.float,
            raw,
        );
    }
}

/// LEVEL-B VIEW: a typed array constructed OVER an ArrayBuffer is a keyed
/// object laid out as [`VIEW_KEYS`] — reads/writes go through
/// `TA_GET_ELEM`/`TA_SET_ELEM` on the SHARED `Entry::Buffer`, so two views over
/// one buffer see each other's writes (JS semantics). The engine-owned `__ta_*`
/// slot names are the shape detector (like `#items`).
///
/// `off_word`/`len_word` are the ctor's optional 2nd/3rd arguments
/// (`byteOffset`, `length`), `undefined` when omitted:
///
/// - omitted `byteOffset` ⇒ 0; omitted `length` ⇒ the rest of the buffer
///   (`(byteLength - byteOffset) / elem_bytes`), which is the 1-arg ctor.
/// - an out-of-range or MISALIGNED (`byteOffset % elem_bytes != 0`) window is
///   clamped to EMPTY. JS throws a RangeError for both; the ctor ABI here has no
///   throw channel, and an empty view is the honest answer (every read is
///   `undefined`, every write a no-op) — never a read of the wrong bytes.
pub(super) fn ta_view_new(buf_word: u64, k: Kind, off_word: u64, len_word: u64) -> u64 {
    let bytes = k.elem_bytes as i64;
    let buf_bytes = buffer_byte_len(buf_word);
    let opt = |w: u64| -> Option<i64> {
        let v = PolyValue::from_raw(w);
        if v.is_undefined() {
            return None;
        }
        let n = genops::to_number(v);
        Some(if n.is_finite() { n.trunc() as i64 } else { 0 })
    };
    let off = opt(off_word).unwrap_or(0);
    // Clamp to EMPTY on a window the buffer cannot back (see the doc above).
    let valid = off >= 0 && off <= buf_bytes && off % bytes == 0;
    let (off, count) = if !valid {
        (0, 0)
    } else {
        let rest = (buf_bytes - off) / bytes;
        match opt(len_word) {
            None => (off, rest),
            Some(n) if n >= 0 && n <= rest => (off, n),
            // An explicit length past the end of the buffer: RangeError in JS.
            Some(_) => (0, 0),
        }
    };

    let keys: Vec<String> = VIEW_KEYS.iter().map(|s| s.to_string()).collect();
    let shape = rts_engine::heap::shapes::intern_global_shape(&keys);
    let h = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let mut push = |w: u64| rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, w as i64);
    push(PolyValue::from_i32(shape as i32).raw());
    push(buf_word);
    push(PolyValue::from_i32(k.elem_bytes as i32).raw());
    push(PolyValue::from_i32(k.signed as i32).raw());
    push(PolyValue::from_i32(k.float as i32).raw());
    push(PolyValue::from_f64(off as f64).raw());
    push(PolyValue::from_f64(count as f64).raw());
    // The ctor ABI returns the RAW Vec handle (like `finish`) — the engine's
    // array rebox boxes it; a keyed shape header makes it an OBJECT word there.
    h
}

/// The `byteLength` of an ArrayBuffer WORD (0 for anything else).
fn buffer_byte_len(buf_word: u64) -> i64 {
    let v = PolyValue::from_raw(buf_word);
    if !v.is_object() {
        return 0;
    }
    let h = rt_handles::__rtsn_poly_to_handle(v.as_handle());
    rts_engine::heap::handles::with_entry(h, |e| match e {
        Some(rts_engine::heap::handles::Entry::Buffer(b)) => b.len() as i64,
        _ => 0,
    })
}

/// The interned shape id of a level-B view, computed once.
///
/// `intern_global_shape` is idempotent for an identical key sequence, so this is
/// a memoized constant, not a snapshot that can go stale.
fn view_shape_id() -> u32 {
    static ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ID.get_or_init(|| {
        let keys: Vec<String> = VIEW_KEYS.iter().map(|s| s.to_string()).collect();
        rts_engine::heap::shapes::intern_global_shape(&keys)
    })
}

/// Decompose a level-B VIEW word into a [`View`]. `None` for anything that is
/// not a `__ta_buf`-shaped keyed object.
///
/// ## Reads SLOTS, not properties — the reason this function existed to be fixed
///
/// [`ta_view_new`] lays a view out positionally: the shape id in slot 0, then
/// [`VIEW_KEYS`] in order. The parts sit at fixed indices, identified by the
/// shape id in slot 0. That is the hidden-class contract the whole engine is
/// built on — compare the shape id, then load a fixed offset.
///
/// The previous implementation asked for the four `__ta_*` fields BY NAME through
/// full `__rtsadp_obj_get`. Since `view_parts` runs on EVERY `view[i]` read and
/// write, that meant, per element access: four `intern_poly` calls (each a
/// NON-deduped `STRING_NEW` heap allocation — abi_adapter.rs / string_pool.rs),
/// and four `obj_get` walks that each relocked the shard and re-cloned the
/// shape's key vector. Roughly fifty shard-mutex locks and a dozen string/Vec
/// allocations to answer a question whose answer is four integers already sitting
/// in fixed slots. Worse, the string flood pushed the handle table past its GC
/// floor, so the collector then ran every 256 allocations — the cost was paid
/// twice. A read-only loop could allocate its way to millions of live handles.
///
/// This reads the slots under ONE `with_entry`, validates slot 0 against the
/// interned view shape, and allocates nothing.
pub(crate) fn view_parts(word: u64) -> Option<View> {
    use rts_engine::heap::handles::{Entry, with_entry};

    const N: usize = VIEW_KEYS.len() + 1;
    let v = PolyValue::from_raw(word);
    if !v.is_object() {
        return None;
    }
    let h = rt_handles::__rtsn_poly_to_handle(v.as_handle());
    let slots: Option<[u64; N]> = with_entry(h, |e| match e {
        Some(Entry::Vec(vec)) if vec.len() >= N => {
            let mut out = [0u64; N];
            for (o, s) in out.iter_mut().zip(vec.iter()) {
                *o = *s as u64;
            }
            Some(out)
        }
        _ => None,
    });
    let slots = slots?;

    // Slot 0 must be THIS shape — any other keyed object (or an array whose first
    // element happens to be an int) is not a view.
    let shape = PolyValue::from_raw(slots[0]);
    if !shape.is_int32() || shape.as_i32() as u32 != view_shape_id() {
        return None;
    }

    let bv = PolyValue::from_raw(slots[1]);
    if !bv.is_object() {
        return None;
    }
    let num = |w: u64| genops::to_number(PolyValue::from_raw(w)) as i64;
    let bytes = num(slots[2]);
    if bytes <= 0 {
        return None;
    }
    let bh = rt_handles::__rtsn_poly_to_handle(bv.as_handle());
    Some(View {
        bh,
        bytes,
        signed: num(slots[3]),
        float: num(slots[4]),
        off: num(slots[5]),
        count: num(slots[6]),
    })
}

/// Resolve a level-B view to its buffer's RAW BASE POINTER and ELEMENT COUNT,
/// for the native inline element path (`CRANELIFT_IMPLEMENTATION.md` step 5b).
///
/// Writes `(base_ptr, elem_count)` through the out-params and returns nothing.
/// On any failure (not a view, buffer gone) it writes `(0, 0)`, so the caller's
/// bounds check (`idx < count`, unsigned) rejects every access and no load ever
/// dereferences the null base.
///
/// ## Why a raw pointer is sound here
///
/// `new ArrayBuffer(n)` allocates ONE `Entry::Buffer(Vec<u8>)` that is never
/// resized (no resizable ArrayBuffer; `slice`/`DataView` build FRESH buffers) and
/// never moved (the GC is non-moving mark+sweep). So the data pointer is stable
/// for the buffer's lifetime. The pointer is a DERIVED interior pointer the
/// conservative scanner does not trace — but the VIEW WORD (a `TAG_OBJECT`
/// PolyValue) stays live on the caller's stack across the access, which keeps the
/// buffer reachable; the emitter must not drop the view word once only the base
/// is used.
///
/// The caller passes the ALREADY-DECODED [`View`], so this takes one lock (the
/// buffer) and does no re-decode. The base is the WINDOW's first byte
/// (`buffer_base + byteOffset`) and the count the WINDOW's element count, so the
/// inline path indexes the view, not the whole buffer.
pub(crate) fn view_base_len(v: &View) -> (i64, i64) {
    if v.bytes <= 0 || v.count <= 0 {
        return (0, 0);
    }
    rts_engine::heap::handles::with_entry_mut(v.bh, |e| match e {
        Some(rts_engine::heap::handles::Entry::Buffer(b)) => {
            // A window the buffer no longer backs (it can only shrink by being
            // freed) yields the null base, and the caller's bounds check rejects
            // every access.
            if v.off + v.byte_len() > b.len() as i64 {
                return (0, 0);
            }
            ((b.as_mut_ptr() as i64) + v.off, v.count)
        }
        _ => (0, 0),
    })
}

/// `ta.set(src, offset?)` — copy `src`'s elements into the array starting at
/// NATIVE element-path entry point (`CRANELIFT_IMPLEMENTATION.md` step 5b):
/// resolve a level-B view WORD to its buffer base pointer + element count.
///
/// Writes `*out_base` and `*out_count`, and RETURNS the element byte-width (1/2/
/// 4/8) OR `0` when `view_word` is not a level-B view. The lowering hoists this
/// ONE call out of an element loop, then emits inline `base + (i << log2)` loads.
/// On a non-view / dead buffer it writes `(0, 0)` and returns `0`, so the caller
/// takes its fallback path and never dereferences the null base.
///
/// SAFETY of the raw base pointer: see [`view_base_len`]. The `out_*` pointers
/// are stack slots the emitted code owns.
// NOT `#[rtse::abi]`: the two `*mut i64` out-params have no single-slot ABI
// spelling — they are stack slots the emitted code owns and writes through, not
// values crossing by copy. Keeps its hand-written `abi_sig` row.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_ta_view_base_len(
    view_word: u64,
    out_base: *mut i64,
    out_count: *mut i64,
) -> i64 {
    let view = match view_parts(view_word) {
        Some(p) => p,
        None => {
            unsafe {
                *out_base = 0;
                *out_count = 0;
            }
            return 0;
        }
    };
    let (base, count) = view_base_len(&view);
    unsafe {
        *out_base = base;
        *out_count = count;
    }
    if base == 0 { 0 } else { view.bytes }
}
