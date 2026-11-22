//! The GitHub App's JWT: RS256 over `{iat, exp, iss}`, signed with `ring`.
//!
//! GitHub's rules (generating a JSON Web Token for a GitHub App): `iss` is the app's **client id**, `iat` is
//! sixty seconds in the past (clock skew), `exp` is at most ten minutes after the time GitHub receives it.
//! The token is made per request and never stored: it is only ever the proof hostd sends to `/app/...`.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};

/// How far back `iat` is set, for a clock that runs ahead of GitHub's.
pub(crate) const IAT_SKEW_SECS: i64 = 60;
/// How long the token lives after `now`. With the skew it spans nine minutes plus one: under GitHub's ten.
pub(crate) const LIFETIME_SECS: i64 = 9 * 60;

/// The key pair in `pem`, a PKCS#1 (`RSA PRIVATE KEY`, what GitHub's conversion returns) or PKCS#8
/// (`PRIVATE KEY`) PEM. Lines starting with `#` are comments (the test keys carry one).
fn key_pair(pem: &str) -> Result<RsaKeyPair, String> {
    let mut pkcs8 = false;
    let mut body = String::new();
    for line in pem.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(label) = line.strip_prefix("-----BEGIN ") {
            pkcs8 = label.starts_with("PRIVATE KEY");
        } else if !line.starts_with("-----") {
            body.push_str(line);
        }
    }
    let der = STANDARD
        .decode(body)
        .map_err(|_| "the app key is not valid PEM".to_string())?;
    let pair = if pkcs8 {
        RsaKeyPair::from_pkcs8(&der)
    } else {
        RsaKeyPair::from_der(&der)
    };
    pair.map_err(|_| "the app key is not a usable RSA private key".to_string())
}

/// The JWT for `client_id` at unix time `now`, signed with the key in `pem`.
pub(crate) fn sign(pem: &str, client_id: &str, now: i64) -> Result<String, String> {
    let pair = key_pair(pem)?;
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
    // Built by hand, not through a map: the byte order of the payload is part of what is signed.
    let payload = format!(
        r#"{{"iat":{},"exp":{},"iss":{}}}"#,
        now - IAT_SKEW_SECS,
        now + LIFETIME_SECS,
        serde_json::Value::from(client_id)
    );
    let signing_input = format!("{header}.{}", URL_SAFE_NO_PAD.encode(payload));
    let mut sig = vec![0u8; pair.public().modulus_len()];
    pair.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        signing_input.as_bytes(),
        &mut sig,
    )
    .map_err(|_| "signing the app JWT failed".to_string())?;
    Ok(format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(sig)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PKCS1: &str = include_str!("testdata/test_key_pkcs1.pem");
    const PKCS8: &str = include_str!("testdata/test_key_pkcs8.pem");
    /// Made once with `openssl dgst -sha256 -sign` over `{"iat":1699999940,"exp":1700000540,"iss":"Iv23liTESTCLIENT"}`:
    /// the signing is not ours, so a mistake in how we sign cannot agree with it.
    const KNOWN: &str = include_str!("testdata/jwt_known.txt");

    // iat = now - 60 and exp = now + 540 for now = 1700000000.
    const NOW: i64 = 1_700_000_000;

    #[test]
    fn a_pkcs1_key_signs_the_same_jwt_openssl_does() {
        assert_eq!(sign(PKCS1, "Iv23liTESTCLIENT", NOW).unwrap(), KNOWN.trim());
    }

    #[test]
    fn a_pkcs8_key_signs_the_same_jwt() {
        assert_eq!(sign(PKCS8, "Iv23liTESTCLIENT", NOW).unwrap(), KNOWN.trim());
    }

    // GitHub refuses an exp more than ten minutes ahead, and an iat in its future.
    #[test]
    fn the_payload_is_issued_in_the_past_and_expires_within_ten_minutes() {
        let jwt = sign(PKCS1, "Iv23liTESTCLIENT", NOW).unwrap();
        let payload = jwt.split('.').nth(1).unwrap();
        let v: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
        assert_eq!(v["iss"], "Iv23liTESTCLIENT");
        assert_eq!(v["iat"], NOW - 60);
        let (iat, exp) = (v["iat"].as_i64().unwrap(), v["exp"].as_i64().unwrap());
        assert!(exp - iat <= 10 * 60, "{} s", exp - iat);
        assert!(exp > NOW);
    }

    #[test]
    fn the_signature_verifies_against_the_public_key() {
        use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
        let jwt = sign(PKCS1, "Iv23liTESTCLIENT", NOW).unwrap();
        let (input, sig) = jwt.rsplit_once('.').unwrap();
        let public = key_pair(PKCS1).unwrap();
        UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, public.public().as_ref())
            .verify(input.as_bytes(), &URL_SAFE_NO_PAD.decode(sig).unwrap())
            .unwrap();
    }

    #[test]
    fn a_key_that_is_not_pem_or_not_rsa_is_an_error_not_a_panic() {
        assert!(sign("not a key", "c", NOW).is_err());
        assert!(
            sign(
                "-----BEGIN RSA PRIVATE KEY-----\nAAAA\n-----END RSA PRIVATE KEY-----",
                "c",
                NOW
            )
            .is_err()
        );
        assert!(sign("", "c", NOW).is_err());
    }
}
