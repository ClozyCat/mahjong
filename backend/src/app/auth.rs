use anyhow::Result;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use password_hash::SaltString;
use rand::Rng;
use sha2::{Digest, Sha256};

pub(crate) struct AuthenticatedUser {
    pub(crate) user_id: i64,
    pub(crate) username: String,
    pub(crate) display_name: String,
}

pub(crate) fn hash_password(password: &str) -> Result<String> {
    let salt = generate_salt();
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string();
    Ok(password_hash)
}

pub(crate) fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub(crate) fn generate_session_token() -> String {
    generate_hex_token(32)
}

pub(crate) fn hash_session_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn generate_invite_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rng();
    (0..12)
        .map(|_| {
            let index = rng.random_range(0..ALPHABET.len());
            ALPHABET[index] as char
        })
        .collect()
}

fn generate_salt() -> SaltString {
    let mut salt = [0_u8; 16];
    rand::rng().fill(&mut salt);
    SaltString::encode_b64(&salt).expect("generated salt should encode")
}

fn generate_hex_token(bytes: usize) -> String {
    let mut data = vec![0_u8; bytes];
    rand::rng().fill(data.as_mut_slice());
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_round_trip_rejects_wrong_password() {
        let password_hash = hash_password("correct horse battery staple").expect("hash");

        assert!(verify_password("correct horse battery staple", &password_hash));
        assert!(!verify_password("wrong password", &password_hash));
    }

    #[test]
    fn generated_tokens_are_distinct_and_hash_stably() {
        let first = generate_session_token();
        let second = generate_session_token();

        assert_ne!(first, second);
        assert_eq!(hash_session_token(&first), hash_session_token(&first));
        assert_ne!(hash_session_token(&first), hash_session_token(&second));
    }

    #[test]
    fn invite_codes_are_human_readable_and_distinct() {
        let first = generate_invite_code();
        let second = generate_invite_code();

        assert_ne!(first, second);
        assert_eq!(first.len(), 12);
        assert!(first.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit()));
    }
}
