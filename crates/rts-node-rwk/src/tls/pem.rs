//! PEM → DER, by hand, over [`rts_core_rwk::entry::decode_base64`] — not
//! `rustls-pemfile`: it is not a direct dependency of this crate (only
//! `rustls`/`webpki-roots`/the RustCrypto set are, per `crates.md` §6), and
//! this crate may not add one for this change. PEM's own shape is three
//! lines (`-----BEGIN X-----`, base64 body, `-----END X-----`) with no
//! escaping inside the body, which is little enough to read directly.

/// Every `-----BEGIN <label>-----`…`-----END <label>-----` block's decoded
/// bytes, in file order. A body line that fails to base64-decode is
/// dropped from that block rather than failing the whole read — the same
/// "permissive" stance [`rts_core_rwk::entry::decode_base64`] itself
/// documents for its own callers.
pub(crate) fn blocks(pem: &str) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    let mut body = String::new();
    let mut inside = false;
    for line in pem.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("-----BEGIN ") {
            inside = true;
            body.clear();
            continue;
        }
        if trimmed.starts_with("-----END ") {
            if inside {
                found.push(rts_core_rwk::entry::decode_base64(&body));
            }
            inside = false;
            continue;
        }
        if inside {
            body.push_str(trimmed);
        }
    }
    found
}

/// The first block, if any — the common case (one cert, or one key) per PEM
/// text.
pub(crate) fn first_block(pem: &str) -> Option<Vec<u8>> {
    blocks(pem).into_iter().next()
}
