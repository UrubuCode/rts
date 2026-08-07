//! The provider's CSPRNG — `getrandom`, the same source
//! `node:crypto::random`'s `randomBytes` already uses (`crypto/random.rs`).

#[derive(Debug)]
pub(crate) struct Random;

impl rustls::crypto::SecureRandom for Random {
    fn fill(&self, buf: &mut [u8]) -> Result<(), rustls::crypto::GetRandomFailed> {
        getrandom::fill(buf).map_err(|_| rustls::crypto::GetRandomFailed)
    }
}
