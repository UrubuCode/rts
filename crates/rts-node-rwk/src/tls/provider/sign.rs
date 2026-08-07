//! Loading a `SecureContext`'s own private key (server cert, client cert) —
//! the [`rustls::crypto::KeyProvider`] half of the provider. Only PKCS#8 DER
//! keys are accepted, in ECDSA-P256 or Ed25519; the PEM layer that produces
//! this DER is `super::super::pem`.
//!
//! RSA private-key loading (signing) is not implemented — see `mod.rs`'s
//! "Not implemented, by name"; RSA is still verified (`verify.rs`), so a
//! client connecting to an RSA-keyed server works, only serving from one
//! does not.

use std::sync::Arc;

use rustls::pki_types::PrivateKeyDer;
use rustls::sign::{Signer, SigningKey};
use rustls::{Error, SignatureAlgorithm, SignatureScheme};

#[derive(Debug)]
pub(crate) struct KeyProvider;

impl rustls::crypto::KeyProvider for KeyProvider {
    fn load_private_key(&self, key_der: PrivateKeyDer<'static>) -> Result<Arc<dyn SigningKey>, Error> {
        let PrivateKeyDer::Pkcs8(pkcs8) = key_der else {
            return Err(Error::General(
                "tls: only PKCS#8 private keys are accepted (see tls/provider/sign.rs)".into(),
            ));
        };
        let der = pkcs8.secret_pkcs8_der();
        // Not `p256`/`ed25519_dalek`'s own PKCS#8 decoders — see
        // `keyparse.rs`'s module doc for why, and what this reader covers.
        let Some(scalar) = super::keyparse::last_32_after_octet_string_tag(der) else {
            return Err(Error::General("tls: could not locate a 32-byte private scalar in this PKCS#8 key".into()));
        };
        // The algorithm OID tells the two apart: Ed25519's is the short,
        // unambiguous `1.3.101.112` (DER bytes `2B 65 70`); anything else
        // accepted here is tried as ECDSA P-256. No third type is attempted
        // (see this module's doc — RSA signing is not implemented).
        if der.windows(3).any(|w| w == [0x2b, 0x65, 0x70]) {
            return Ok(Arc::new(Ed25519Key(ed25519_dalek::SigningKey::from_bytes(&scalar))));
        }
        let key = p256::ecdsa::SigningKey::from_slice(&scalar)
            .map_err(|_| Error::General("tls: private scalar is not a valid P-256 ECDSA key".into()))?;
        Ok(Arc::new(EcdsaP256Key(key)))
    }
}

#[derive(Debug)]
struct EcdsaP256Key(p256::ecdsa::SigningKey);

impl SigningKey for EcdsaP256Key {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        offered
            .contains(&SignatureScheme::ECDSA_NISTP256_SHA256)
            .then(|| Box::new(EcdsaP256Signer(self.0.clone())) as Box<dyn Signer>)
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ECDSA
    }
}

#[derive(Debug)]
struct EcdsaP256Signer(p256::ecdsa::SigningKey);

impl Signer for EcdsaP256Signer {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        use p256::ecdsa::signature::Signer as _;
        let sig: p256::ecdsa::Signature = self.0.sign(message);
        Ok(sig.to_der().to_bytes().to_vec())
    }

    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ECDSA_NISTP256_SHA256
    }
}

#[derive(Debug)]
struct Ed25519Key(ed25519_dalek::SigningKey);

impl SigningKey for Ed25519Key {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        offered
            .contains(&SignatureScheme::ED25519)
            .then(|| Box::new(Ed25519Signer(self.0.clone())) as Box<dyn Signer>)
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ED25519
    }
}

#[derive(Debug)]
struct Ed25519Signer(ed25519_dalek::SigningKey);

impl Signer for Ed25519Signer {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        use ed25519_dalek::Signer as _;
        Ok(self.0.sign(message).to_bytes().to_vec())
    }

    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}

