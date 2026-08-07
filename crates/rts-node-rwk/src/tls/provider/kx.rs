//! ECDHE key exchange on P-256, over `p256` — the crate `crates.md` §6 names
//! for it, already used by [`super::verify`] and [`super::sign`].
//!
//! # Why P-256 and not X25519
//!
//! X25519 was tried first (`x25519-dalek`, also vetted in §6) and dropped: every
//! constructor that builds a secret from raw bytes without a `rand_core::RngCore`
//! (which this crate cannot name — it is not a direct dependency, and
//! `x25519-dalek` does not re-export it) sits behind a Cargo feature
//! (`static_secrets` for a from-bytes secret, `getrandom` for
//! `EphemeralSecret::random()`) that is not enabled on this crate's
//! `x25519-dalek` dependency. Enabling it is a `Cargo.toml` edit, which this
//! change may not make (five other agents hold that file's lock) — reported
//! per the task brief rather than reached around. `elliptic_curve::ecdh::
//! diffie_hellman` (which `p256` re-exports) needs no RNG at all: it is a
//! pure function over a scalar and a point, and the scalar comes from this
//! provider's own `getrandom`-filled bytes.

use p256::elliptic_curve::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use rustls::NamedGroup;
use rustls::crypto::{ActiveKeyExchange, SharedSecret, SupportedKxGroup};

#[derive(Debug)]
pub(crate) struct Secp256r1;

impl SupportedKxGroup for Secp256r1 {
    fn start(&self) -> Result<Box<dyn ActiveKeyExchange>, rustls::Error> {
        let mut bytes = [0u8; 32];
        // Vanishingly unlikely to hit a rejected (zero/out-of-range) scalar;
        // retried rather than failing the handshake on that unlikely draw.
        let secret = loop {
            getrandom::fill(&mut bytes).map_err(|_| rustls::Error::FailedToGetRandomBytes)?;
            if let Ok(key) = p256::SecretKey::from_slice(&bytes) {
                break key;
            }
        };
        let public = secret.public_key().to_encoded_point(false).as_bytes().to_vec();
        Ok(Box::new(ActiveSecp256r1 { secret, public }))
    }

    fn name(&self) -> NamedGroup {
        NamedGroup::secp256r1
    }
}

struct ActiveSecp256r1 {
    secret: p256::SecretKey,
    public: Vec<u8>,
}

impl ActiveKeyExchange for ActiveSecp256r1 {
    fn complete(self: Box<Self>, peer_pub_key: &[u8]) -> Result<SharedSecret, rustls::Error> {
        let peer = p256::PublicKey::from_sec1_bytes(peer_pub_key)
            .map_err(|_| rustls::Error::General("invalid P-256 peer public key".into()))?;
        let shared = diffie_hellman(self.secret.to_nonzero_scalar(), peer.as_affine());
        Ok(SharedSecret::from(shared.raw_secret_bytes().as_slice()))
    }

    fn pub_key(&self) -> &[u8] {
        &self.public
    }

    fn group(&self) -> NamedGroup {
        NamedGroup::secp256r1
    }
}
