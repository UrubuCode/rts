//! node:path — POSIX `extern "C"` entry points (the non-variadic functions;
//! `join`/`resolve` overloads are generated in the parent module).

use super::super::flavor::{chars, Flavor};
use super::super::{classify, glob, parse as pmod, posix, words};
use super::{process_cwd, read_str};

const F: Flavor = Flavor::Posix;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_BASENAME(p: *const u8, l: i64) -> u64 {
    words::intern(&classify::basename(&chars(read_str(p, l)), None, F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_BASENAME2(
    p: *const u8,
    l: i64,
    sp: *const u8,
    sl: i64,
) -> u64 {
    let suffix = chars(read_str(sp, sl));
    words::intern(&classify::basename(&chars(read_str(p, l)), Some(&suffix), F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_DIRNAME(p: *const u8, l: i64) -> u64 {
    words::intern(&classify::dirname(&chars(read_str(p, l)), F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_EXTNAME(p: *const u8, l: i64) -> u64 {
    words::intern(&classify::extname(&chars(read_str(p, l)), F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_ISABSOLUTE(p: *const u8, l: i64) -> i64 {
    classify::is_absolute(&chars(read_str(p, l)), F) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_NORMALIZE(p: *const u8, l: i64) -> u64 {
    words::intern(&posix::normalize(read_str(p, l)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_RELATIVE(
    fp: *const u8,
    fl: i64,
    tp: *const u8,
    tl: i64,
) -> u64 {
    words::intern(&posix::relative(read_str(fp, fl), read_str(tp, tl), &process_cwd()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_PARSE(p: *const u8, l: i64) -> u64 {
    words::parsed_object(&posix::parse(read_str(p, l)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_FORMAT(obj: u64) -> u64 {
    let (root, dir, base, name, ext) = words::format_fields(obj);
    words::intern(&pmod::format(root, dir, base, name, ext, F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_TONAMESPACED(p: *const u8, l: i64) -> u64 {
    words::intern(&posix::to_namespaced(read_str(p, l)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_POSIX_MATCHESGLOB(
    p: *const u8,
    l: i64,
    pat: *const u8,
    patl: i64,
) -> i64 {
    glob::matches_glob(read_str(p, l), read_str(pat, patl), F) as i64
}
