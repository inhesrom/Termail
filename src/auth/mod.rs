pub mod google;
pub mod microsoft;
pub mod token_store;

/// Build the XOAUTH2 SASL string for IMAP/SMTP authentication.
/// Format: base64("user=<email>\x01auth=Bearer <token>\x01\x01")
/// Shared by both Google and Microsoft OAuth providers.
pub fn build_xoauth2_string(email: &str, access_token: &str) -> String {
    use base64::Engine;
    let auth_string = format!("user={}\x01auth=Bearer {}\x01\x01", email, access_token);
    base64::engine::general_purpose::STANDARD.encode(auth_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_xoauth2_string() {
        use base64::Engine;

        let result = build_xoauth2_string("user@example.com", "token123");
        let expected_raw = "user=user@example.com\x01auth=Bearer token123\x01\x01";
        let expected = base64::engine::general_purpose::STANDARD.encode(expected_raw);
        assert_eq!(result, expected);

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&result)
            .expect("should be valid base64");
        assert_eq!(String::from_utf8(decoded).unwrap(), expected_raw);
    }
}
