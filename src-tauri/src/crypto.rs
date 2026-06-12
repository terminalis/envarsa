//! Opt-in passphrase encryption of the store file, using the standard
//! age format (scrypt passphrase recipient).
//!
//! Deliberate choice: an encrypted Envarsa store is a plain age file,
//! so the user can always decrypt it without Envarsa:
//!
//! ```text
//! age -d envarsa.store > store.json
//! ```
//!
//! Ownership of the bytes survives the tool.

const AGE_MAGIC: &[u8] = b"age-encryption.org/";

/// True when the given store file bytes are age-encrypted (as opposed
/// to plaintext JSON).
pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.starts_with(AGE_MAGIC)
}

pub fn encrypt(plaintext: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    let recipient = age::scrypt::Recipient::new(passphrase.to_owned().into());
    age::encrypt(&recipient, plaintext).map_err(|e| format!("encryption failed: {e}"))
}

pub fn decrypt(ciphertext: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    let identity = age::scrypt::Identity::new(passphrase.to_owned().into());
    age::decrypt(&identity, ciphertext).map_err(|e| match e {
        age::DecryptError::DecryptionFailed => "wrong passphrase".to_string(),
        other => format!("decryption failed: {other}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let data = b"{\"hello\":\"world\"}";
        let ct = encrypt(data, "correct horse battery staple").unwrap();
        assert!(is_encrypted(&ct));
        assert!(!is_encrypted(data));
        let pt = decrypt(&ct, "correct horse battery staple").unwrap();
        assert_eq!(pt, data);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let ct = encrypt(b"secret", "right").unwrap();
        let err = decrypt(&ct, "wrong").unwrap_err();
        assert!(err.contains("wrong passphrase"), "got: {err}");
    }
}
