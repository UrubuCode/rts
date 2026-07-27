//! `Date` MULTI-COMPONENT setters — hand-written residuals of the `#[rtse::class]`
//! migration.
//!
//! `setFullYear(y, mo?, d?)` / `setMonth(mo, d?)` / `setHours(h, mi?, s?, ms?)` /
//! `setMinutes(mi, s?, ms?)` / `setSeconds(s, ms?)` use an `i64::MIN` "keep the
//! current field" sentinel for an OMITTED optional arg. That is the exact
//! OPPOSITE of the macro's `optional=N`, which injects `undefined` (→ NaN =
//! "invalidate the whole Date"). The macro therefore cannot author these — they
//! stay ordinary `#[no_mangle] extern "C"` fns, registered with
//! `Sig::with_defaults([… Int(i64::MIN) …])` so the engine pads an omitted arg
//! with the keep-sentinel. Both the plain and the `setUTC*` alias members point
//! at the same symbol (RTS stores ms in UTC, so local == UTC). `toGMTString` (a
//! deprecated alias of `toUTCString`) is registered here too, pointing at the
//! macro-generated `toUTCString` extern.
//!
//! All read/write the SAME `super::instance::Date` struct via `with_rtse` /
//! `with_rtse_mut` — one storage representation, two authoring styles.

use rts_engine::heap::handles::{with_rtse, with_rtse_mut};
use rts_engine::{AbiType, ClassBuilder, DefaultArg, FnPtr, Member, MemberFlags, MemberKind, Sig};

use super::instance::{Date, INVALID_MS, keep_f, ms_to_parts};

// ── handle-based storage helpers ────────────────────────────────────────────

fn get_ms(handle: u64) -> i64 {
    with_rtse::<Date, _>(handle, |d| d.map(|d| d.ms).unwrap_or(0))
}

fn set_ms(handle: u64, new_ms: i64) {
    with_rtse_mut::<Date, _>(handle, |d| {
        if let Some(d) = d {
            d.ms = new_ms;
        }
    });
}

/// Base parts of a setter: the current instant's, or — for an Invalid Date —
/// those of `t = +0` (spec 21.4.4.*: a complete setter REVIVES the date).
fn setter_base_parts(handle: u64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let cur = get_ms(handle);
    if cur == INVALID_MS {
        (1970, 0, 1, 0, 0, 0, 0)
    } else {
        ms_to_parts(cur)
    }
}

/// Close a setter: `None` on any component → INVALID + NaN; else store + return.
fn finish_setter(handle: u64, parts: Option<(i64, i64, i64, i64, i64, i64, i64)>) -> f64 {
    match parts {
        Some((y, mo, d, h, mi, s, ms)) => {
            let new_ms = crate::date::__RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, h, mi, s, ms);
            set_ms(handle, new_ms);
            new_ms as f64
        }
        None => {
            set_ms(handle, INVALID_MS);
            f64::NAN
        }
    }
}

// ── the multi-component setter externs ──────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_FULL_YEAR(
    handle: u64,
    year: f64,
    month: f64,
    day: f64,
) -> f64 {
    let (_, mo, d, h, mi, s, ms) = setter_base_parts(handle);
    let parts = (|| {
        Some((
            keep_f(year, 0)?,
            keep_f(month, mo)?,
            keep_f(day, d)?,
            h,
            mi,
            s,
            ms,
        ))
    })();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_MONTH(handle: u64, month: f64, day: f64) -> f64 {
    let (y, _, d, h, mi, s, ms) = setter_base_parts(handle);
    let parts = (|| Some((y, keep_f(month, 0)?, keep_f(day, d)?, h, mi, s, ms)))();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_HOURS(
    handle: u64,
    hour: f64,
    min: f64,
    sec: f64,
    msec: f64,
) -> f64 {
    let (y, mo, d, _, mi, s, ms) = setter_base_parts(handle);
    let parts = (|| {
        Some((
            y,
            mo,
            d,
            keep_f(hour, 0)?,
            keep_f(min, mi)?,
            keep_f(sec, s)?,
            keep_f(msec, ms)?,
        ))
    })();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_MINUTES(handle: u64, min: f64, sec: f64, msec: f64) -> f64 {
    let (y, mo, d, h, _, s, ms) = setter_base_parts(handle);
    let parts = (|| {
        Some((
            y,
            mo,
            d,
            h,
            keep_f(min, 0)?,
            keep_f(sec, s)?,
            keep_f(msec, ms)?,
        ))
    })();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_SECONDS(handle: u64, sec: f64, msec: f64) -> f64 {
    let (y, mo, d, h, mi, _, ms) = setter_base_parts(handle);
    let parts = (|| Some((y, mo, d, h, mi, keep_f(sec, 0)?, keep_f(msec, ms)?)))();
    finish_setter(handle, parts)
}

// ── member registration ─────────────────────────────────────────────────────

/// A `Sig` for a multi-component setter: `(recv, nvals × f64) -> f64`, with the
/// first `required` value params `Required` and the rest carrying the `i64::MIN`
/// keep-current sentinel default.
fn setter_sig(nvals: usize, required: usize) -> Sig {
    let mut args = vec![AbiType::Handle];
    let mut defs = vec![DefaultArg::Required]; // receiver
    for i in 0..nvals {
        args.push(AbiType::F64);
        defs.push(if i < required {
            DefaultArg::Required
        } else {
            DefaultArg::Int(i64::MIN)
        });
    }
    Sig::with_defaults(args, AbiType::F64, defs)
}

fn m(name: &str, symbol: &str, fp: *const u8, sig: Sig, ts: &str, doc: &str) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::InstanceMethod,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Append the hand-written multi-component setters (plain + `setUTC*` aliases)
/// and the `toGMTString` alias to the reopened `Date` class builder.
pub(super) fn append_members(mut cb: ClassBuilder) -> ClassBuilder {
    // (symbol, fp, nvals, first-value-param ts). One row per distinct extern; the
    // plain name and its `setUTC*` alias both reference it (RTS stores UTC).
    let rows: [(&str, &str, *const u8, usize, &str); 5] = [
        ("setFullYear", "__RTS_FN_GL_DATE_SET_FULL_YEAR", __RTS_FN_GL_DATE_SET_FULL_YEAR as *const u8, 3, "year: number"),
        ("setMonth", "__RTS_FN_GL_DATE_SET_MONTH", __RTS_FN_GL_DATE_SET_MONTH as *const u8, 2, "month: number"),
        ("setHours", "__RTS_FN_GL_DATE_SET_HOURS", __RTS_FN_GL_DATE_SET_HOURS as *const u8, 4, "hour: number"),
        ("setMinutes", "__RTS_FN_GL_DATE_SET_MINUTES", __RTS_FN_GL_DATE_SET_MINUTES as *const u8, 3, "min: number"),
        ("setSeconds", "__RTS_FN_GL_DATE_SET_SECONDS", __RTS_FN_GL_DATE_SET_SECONDS as *const u8, 2, "sec: number"),
    ];
    const DOC: &str =
        "Replaces the given component(s); omitted args keep the current field. Returns the new ms.";
    for (name, symbol, fp, nvals, param_ts) in rows {
        let utc_name = format!("setUTC{}", &name["set".len()..]);
        for js_name in [name.to_string(), utc_name] {
            let ts = format!("{js_name}({param_ts}): number");
            cb = cb.member(m(&js_name, symbol, fp, setter_sig(nvals, 1), &ts, DOC));
        }
    }

    // `toGMTString` — deprecated alias of the macro-authored `toUTCString`.
    cb = cb.member(m(
        "toGMTString",
        "__rtsm_global_date_to_utc_string",
        super::instance::__rtsm_global_date_to_utc_string as *const u8,
        Sig::new(vec![AbiType::Handle], AbiType::Handle),
        "toGMTString(): string",
        "Deprecated alias of toUTCString.",
    ));
    cb
}
