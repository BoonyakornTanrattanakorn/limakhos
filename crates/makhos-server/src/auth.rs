//! Local Argon2id password hashing, username validation, and session token generation.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::RngCore;

/// Validate a local account username (3..=24 ASCII alphanumeric, `_`, `-`).
pub fn validate_username(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.len() < 3 || trimmed.len() > 24 {
        return Err("Username must be between 3 and 24 characters".to_string());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Username may only contain letters, digits, '_' and '-'".to_string());
    }
    Ok(trimmed.to_string())
}

/// Validate a local account password (at least 6 characters, max 128).
pub fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < 6 {
        return Err("Password must be at least 6 characters".to_string());
    }
    if password.len() > 128 {
        return Err("Password must be at most 128 characters".to_string());
    }
    Ok(())
}

/// Hash a plaintext password with Argon2id and a fresh cryptographic random salt.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("Failed to hash password: {e}"))
}

/// Verify a plaintext password against a stored PHC Argon2id hash string.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Generate a cryptographically random 256-bit hex session token.
pub fn generate_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_hash_and_verify_round_trip() {
        let hash = hash_password("makhos-secret-123").expect("hash should succeed");
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password("makhos-secret-123", &hash));
        assert!(!verify_password("wrong-password", &hash));
        assert!(!verify_password("makhos-secret-123", "not-a-valid-phc-hash"));
    }

    #[test]
    fn session_tokens_are_unique_hex_strings() {
        let t1 = generate_session_token();
        let t2 = generate_session_token();
        assert_eq!(t1.len(), 64);
        assert_eq!(t2.len(), 64);
        assert_ne!(t1, t2);
        assert!(t1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn username_and_password_validation() {
        assert_eq!(validate_username("  Somchai_8 ").unwrap(), "Somchai_8");
        assert!(validate_username("ab").is_err());
        assert!(validate_username("bad user!").is_err());
        assert!(validate_password("12345").is_err());
        assert!(validate_password("123456").is_ok());
    }
}
