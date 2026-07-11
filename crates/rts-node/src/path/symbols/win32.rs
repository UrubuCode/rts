//! node:path — Win32 `extern "C"` entry points (non-variadic functions;
//! `join`/`resolve` overloads are generated in the parent module).

use super::super::flavor::{chars, Flavor};
use super::super::{classify, glob, parse as pmod, win32, words};
use super::{drive_cwd, process_cwd, read_str};

const F: Flavor = Flavor::Win32;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_BASENAME(p: *const u8, l: i64) -> u64 {
    words::intern(&classify::basename(&chars(read_str(p, l)), None, F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_BASENAME2(
    p: *const u8,
    l: i64,
    sp: *const u8,
    sl: i64,
) -> u64 {
    let suffix = chars(read_str(sp, sl));
    words::intern(&classify::basename(&chars(read_str(p, l)), Some(&suffix), F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_DIRNAME(p: *const u8, l: i64) -> u64 {
    words::intern(&classify::dirname(&chars(read_str(p, l)), F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_EXTNAME(p: *const u8, l: i64) -> u64 {
    words::intern(&classify::extname(&chars(read_str(p, l)), F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_ISABSOLUTE(p: *const u8, l: i64) -> i64 {
    classify::is_absolute(&chars(read_str(p, l)), F) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_NORMALIZE(p: *const u8, l: i64) -> u64 {
    words::intern(&win32::normalize(read_str(p, l)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_RELATIVE(
    fp: *const u8,
    fl: i64,
    tp: *const u8,
    tl: i64,
) -> u64 {
    words::intern(&win32::relative(
        read_str(fp, fl),
        read_str(tp, tl),
        &process_cwd(),
        drive_cwd,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_PARSE(p: *const u8, l: i64) -> u64 {
    words::parsed_object(&pmod::win32_parse(read_str(p, l)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_FORMAT(obj: u64) -> u64 {
    let (root, dir, base, name, ext) = words::format_fields(obj);
    words::intern(&pmod::format(root, dir, base, name, ext, F))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_TONAMESPACED(p: *const u8, l: i64) -> u64 {
    let path = read_str(p, l);
    let resolved = win32::resolve(&[path.to_string()], &process_cwd(), drive_cwd);
    words::intern(&win32::to_namespaced(path, &resolved))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PATH_WIN32_MATCHESGLOB(
    p: *const u8,
    l: i64,
    pat: *const u8,
    patl: i64,
) -> i64 {
    glob::matches_glob(read_str(p, l), read_str(pat, patl), F) as i64
}
