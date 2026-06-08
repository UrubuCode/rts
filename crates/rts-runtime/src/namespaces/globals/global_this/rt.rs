//! Runtime impls de funcoes globais nao-classe (#208).

use crate::namespaces::gc::handles::{alloc_entry, Entry};

fn str_from_parts(ptr: i64, len: i64) -> &'static str {
    if ptr == 0 || len <= 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

/// Caracteres seguros em encodeURIComponent (sem escape):
/// A-Z a-z 0-9 - _ . ! ~ * ' ( )
fn is_uri_component_safe(b: u8) -> bool {
    matches!(
        b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
        | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
    )
}

/// JS encodeURI: preserva URI reserved chars + URI-component-safe.
/// Reserved: ; / ? : @ & = + $ , #
fn is_uri_safe(b: u8) -> bool {
    is_uri_component_safe(b)
        || matches!(
            b,
            b';' | b'/' | b'?' | b':' | b'@' | b'&' | b'=' | b'+' | b'$' | b',' | b'#'
        )
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ENCODE_URI(ptr: i64, len: i64) -> u64 {
    let s = str_from_parts(ptr, len);
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_uri_safe(b) {
            out.push(b);
        } else {
            out.push(b'%');
            out.extend_from_slice(format!("{:02X}", b).as_bytes());
        }
    }
    alloc_entry(Entry::String(out))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DECODE_URI(ptr: i64, len: i64) -> u64 {
    // decodeURI preserva reserved chars escapados (%2F nao vira /).
    // Reserved set: ; / ? : @ & = + $ , #
    let s = str_from_parts(ptr, len);
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                let b = (h * 16 + l) as u8;
                if matches!(
                    b,
                    b';' | b'/' | b'?' | b':' | b'@' | b'&' | b'=' | b'+' | b'$' | b',' | b'#'
                ) {
                    out.extend_from_slice(&bytes[i..i + 3]);
                } else {
                    out.push(b);
                }
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    alloc_entry(Entry::String(out))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ENCODE_URI_COMPONENT(ptr: i64, len: i64) -> u64 {
    let s = str_from_parts(ptr, len);
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_uri_component_safe(b) {
            out.push(b);
        } else {
            out.push(b'%');
            out.extend_from_slice(format!("{:02X}", b).as_bytes());
        }
    }
    alloc_entry(Entry::String(out))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DECODE_URI_COMPONENT(ptr: i64, len: i64) -> u64 {
    let s = str_from_parts(ptr, len);
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            match (hi, lo) {
                (Some(h), Some(l)) => {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                    continue;
                }
                _ => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    alloc_entry(Entry::String(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::gc::handles::{with_entry, Entry};

    fn read_str(h: u64) -> String {
        with_entry(h, |e| match e {
            Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        })
    }

    #[test]
    fn encode_basic() {
        let s = b"hello world";
        let h = __RTS_FN_GL_ENCODE_URI_COMPONENT(s.as_ptr() as i64, s.len() as i64);
        assert_eq!(read_str(h), "hello%20world");
    }

    #[test]
    fn decode_basic() {
        let s = b"hello%20world";
        let h = __RTS_FN_GL_DECODE_URI_COMPONENT(s.as_ptr() as i64, s.len() as i64);
        assert_eq!(read_str(h), "hello world");
    }

    #[test]
    fn roundtrip() {
        let s = b"a&b=c+d/e?f";
        let h1 = __RTS_FN_GL_ENCODE_URI_COMPONENT(s.as_ptr() as i64, s.len() as i64);
        let encoded = read_str(h1);
        let h2 = __RTS_FN_GL_DECODE_URI_COMPONENT(encoded.as_ptr() as i64, encoded.len() as i64);
        assert_eq!(read_str(h2), "a&b=c+d/e?f");
    }
}
