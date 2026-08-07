//! Key exchange for the TLS provider: X25519 (preferred — the group a TLS 1.3
//! peer offers first) and P-256 (ECDHE, fallback for a peer that only speaks
//! it), both over crates `crates.md` §6 names.
//!
//! # X25519
//!
//! `x25519-dalek`'s `static_secrets` feature (now enabled on this crate's
//! dependency — see `Cargo.toml`'s comment beside it) exposes
//! `StaticSecret::from([u8; 32])` and `PublicKey::from(&StaticSecret)`, so a
//! secret can be built from this provider's own `getrandom`-filled bytes with
//! no `rand_core::RngCore` in the picture at all.
//!
//! # P-256
//!
//! ECDHE over `p256`, used already by [`super::verify`] and [`super::sign`].
//! `elliptic_curve::ecdh::diffie_hellman` (which `p256` re-exports) needs no
//! RNG either: it is a pure function over a scalar and a point, and the
//! scalar comes from this provider's own `getrandom`-filled bytes.

use p256::elliptic_curve::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use rustls::NamedGroup;
use rustls::crypto::{ActiveKeyExchange, SharedSecret, SupportedKxGroup};

#[derive(Debug)]
pub(crate) struct X25519;

impl SupportedKxGroup for X25519 {
    fn start(&self) -> Result<Box<dyn ActiveKeyExchange>, rustls::Error> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| rustls::Error::FailedToGetRandomBytes)?;
        let secret = x25519_dalek::StaticSecret::from(bytes);
        let public = x25519_dalek::PublicKey::from(&secret);
        Ok(Box::new(ActiveX25519 { secret, public }))
    }

    fn name(&self) -> NamedGroup {
        NamedGroup::X25519
    }
}

struct ActiveX25519 {
    secret: x25519_dalek::StaticSecret,
    public: x25519_dalek::PublicKey,
}

impl ActiveKeyExchange for ActiveX25519 {
    fn complete(self: Box<Self>, peer_pub_key: &[u8]) -> Result<SharedSecret, rustls::Error> {
        let peer_bytes: [u8; 32] = peer_pub_key
            .try_into()
            .map_err(|_| rustls::Error::General("invalid X25519 peer public key".into()))?;
        let peer = x25519_dalek::PublicKey::from(peer_bytes);
        let shared = self.secret.diffie_hellman(&peer);
        Ok(SharedSecret::from(shared.as_bytes().as_slice()))
    }

    fn pub_key(&self) -> &[u8] {
        self.public.as_bytes()
    }

    fn group(&self) -> NamedGroup {
        NamedGroup::X25519
    }
}

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
