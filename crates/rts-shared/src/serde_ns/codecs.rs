//! `serde` — pickle extension codecs for the non-primordial classes this crate
//! owns: `Date` and `RegExp`. The engine's pickle core dispatches to these by
//! wire tag (`OP_EXT`), so it never names either class (doctrine). Payloads are
//! self-contained little-endian blocks, versioned by the outer RTSP header.

use std::sync::Once;

use rts_engine::heap::handles::{alloc_rtse, with_entry_mut, Entry, RegexEngine};
use rts_engine::heap::pickle::{register_ext_codec, ExtCodec};

use crate::globals::date::instance::Date;

/// Install both codecs (idempotent).
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        register_ext_codec(ExtCodec {
            tag: "Date",
            encode: date_encode,
            decode: date_decode,
        });
        register_ext_codec(ExtCodec {
            tag: "RegExp",
            encode: regex_encode,
            decode: regex_decode,
        });
    });
}

// ── Date — 8 bytes LE: ms since Unix epoch (i64::MIN = Invalid Date) ────────

fn date_encode(e: &Entry) -> Option<Vec<u8>> {
    match e {
        Entry::Rtse { class: "Date", data } => {
            data.downcast_ref::<Date>().map(|d| d.ms.to_le_bytes().to_vec())
        }
        _ => None,
    }
}

fn date_decode(payload: &[u8]) -> Option<u64> {
    let ms = i64::from_le_bytes(payload.get(..8)?.try_into().ok()?);
    Some(alloc_rtse("Date", Date { ms }))
}

// ── RegExp — u32 src_len + src + u32 flags_len + flags + i64 last_index ─────

fn regex_encode(e: &Entry) -> Option<Vec<u8>> {
    let Entry::Regex(rx) = e else { return None };
    let src = match &rx.engine {
        RegexEngine::Fast(r) => r.as_str(),
        RegexEngine::Fancy(f) => f.as_str(),
    };
    let mut out = Vec::with_capacity(src.len() + rx.flags.len() + 16);
    out.extend_from_slice(&(src.len() as u32).to_le_bytes());
    out.extend_from_slice(src.as_bytes());
    out.extend_from_slice(&(rx.flags.len() as u32).to_le_bytes());
    out.extend_from_slice(rx.flags.as_bytes());
    out.extend_from_slice(&(rx.last_index as i64).to_le_bytes());
    Some(out)
}

fn regex_decode(payload: &[u8]) -> Option<u64> {
    let mut i = 0usize;
    let take = |i: &mut usize, n: usize| -> Option<&[u8]> {
        let s = payload.get(*i..*i + n)?;
        *i += n;
        Some(s)
    };
    let src_len = u32::from_le_bytes(take(&mut i, 4)?.try_into().ok()?) as usize;
    let src = std::str::from_utf8(take(&mut i, src_len)?).ok()?;
    let flags_len = u32::from_le_bytes(take(&mut i, 4)?.try_into().ok()?) as usize;
    let flags = std::str::from_utf8(take(&mut i, flags_len)?).ok()?;
    let last_index = i64::from_le_bytes(take(&mut i, 8)?.try_into().ok()?);
    // Recompile through the SAME path `/pat/flags` literals use.
    let h = crate::regex::__RTS_FN_NS_REGEX_COMPILE(
        src.as_ptr(),
        src.len() as i64,
        flags.as_ptr(),
        flags.len() as i64,
    );
    if h == 0 {
        return None;
    }
    with_entry_mut(h, |e| {
        if let Some(Entry::Regex(rx)) = e {
            rx.last_index = last_index.max(0) as usize;
        }
    });
    Some(h)
}
