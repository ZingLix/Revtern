use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use revtern_core::sha256_hex;
use sha2::{Digest, Sha256};

pub fn hash_secret(secret: &str) -> String {
    sha256_hex(secret.as_bytes())
}

pub fn encrypt_json(secret_key: &str, plaintext: &[u8]) -> Result<String> {
    let cipher = cipher(secret_key);
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| anyhow::anyhow!("encrypt credentials"))?;
    let mut packed = Vec::with_capacity(12 + ciphertext.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ciphertext);
    Ok(STANDARD_NO_PAD.encode(packed))
}

pub fn decrypt_json(secret_key: &str, encoded: &str) -> Result<Vec<u8>> {
    let packed = STANDARD_NO_PAD
        .decode(encoded)
        .context("decode encrypted credentials")?;
    anyhow::ensure!(packed.len() > 12, "encrypted credentials are malformed");
    let (nonce_bytes, ciphertext) = packed.split_at(12);
    let cipher = cipher(secret_key);
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| anyhow::anyhow!("decrypt credentials"))
}

fn cipher(secret_key: &str) -> Aes256Gcm {
    let digest = Sha256::digest(secret_key.as_bytes());
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&digest))
}
