//! Secrets at rest (IMAP passwords, OAuth tokens) and session tokens.

use anyhow::{Result, anyhow, bail};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::RngCore;
use sha2::{Digest, Sha256};

const NONCE_LEN: usize = 12;

pub fn derive_key(secret: &str) -> [u8; 32] {
    Sha256::digest(format!("jev-labeller:v1:{secret}").as_bytes()).into()
}

/// Encrypts `plaintext`; output is `nonce || ciphertext`.
pub fn seal(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext).map_err(|_| anyhow!("encryption failed"))?;
    let mut out = nonce.to_vec();
    out.extend(ciphertext);
    Ok(out)
}

pub fn open(key: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < NONCE_LEN {
        bail!("sealed secret is too short");
    }
    let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
    ChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow!("could not decrypt secret; was APP_SECRET changed?"))
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_round_trip() {
        let key = derive_key("test-secret-123456");
        let sealed = seal(&key, b"hunter2").unwrap();
        assert_eq!(open(&key, &sealed).unwrap(), b"hunter2");
        assert!(open(&derive_key("other-secret-12345"), &sealed).is_err());
    }
}
