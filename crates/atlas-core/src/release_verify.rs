//! Release signatures (#279): the maintainers sign each release's `SHA256SUMS` offline with an
//! ed25519 SSH key (`ssh-keygen -Y sign -n atlas-release`); clients verify that signature with
//! the public keys embedded in `data/release-keys.json` and take every asset hash from the
//! signed file, never from a `.sha256` next to the asset.
//!
//! The verifier is the OpenSSH `sshsig` format restricted to what the release uses: key type
//! `ssh-ed25519`, hash `sha512`, namespace `atlas-release`. It sits on `ring` (already in the
//! tree for TLS), so it adds no crate; `ssh-keygen` is the oracle in the tests, and the shell
//! installers verify with `ssh-keygen -Y verify` against the same key list.
//!
//! Policy: a tag at or after `signed_since` must carry a valid signature; older tags keep the
//! per-asset `.sha256` check. While `signed_since` is null nothing changes.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ring::signature::{ED25519, UnparsedPublicKey};
use serde::Deserialize;
use sha2::{Digest, Sha512};
use thiserror::Error;

/// The sshsig namespace of release signatures (`ssh-keygen -n`).
pub const NAMESPACE: &str = "atlas-release";

const KEY_TYPE: &[u8] = b"ssh-ed25519";
const MAGIC: &[u8] = b"SSHSIG";
const BEGIN: &str = "-----BEGIN SSH SIGNATURE-----";
const END: &str = "-----END SSH SIGNATURE-----";

/// A maintainer's release key and the days (UTC, `YYYYMMDD`) it is accepted: from `valid_after`
/// (inclusive) until `valid_before` (exclusive), the semantics of `allowed_signers`.
#[derive(Debug, Clone, PartialEq)]
pub struct TrustedKey {
    pub name: String,
    /// The SSH wire encoding of the public key (what `ssh-keygen` puts in a signature).
    blob: Vec<u8>,
    pub valid_after: Option<u32>,
    pub valid_before: Option<u32>,
}

impl TrustedKey {
    /// `pubkey` is an OpenSSH public key line (`ssh-ed25519 AAAA… comment`).
    pub fn new(
        name: &str,
        pubkey: &str,
        valid_after: Option<u32>,
        valid_before: Option<u32>,
    ) -> Result<Self> {
        let b64 = match pubkey.split_whitespace().collect::<Vec<_>>()[..] {
            ["ssh-ed25519", b64, ..] => b64,
            _ => bail!("key {name}: only ssh-ed25519 public keys are accepted"),
        };
        let blob = STANDARD
            .decode(b64)
            .with_context(|| format!("key {name}: public key is not base64"))?;
        if ed25519_key(&blob).is_none() {
            bail!("key {name}: not a well-formed ssh-ed25519 public key");
        }
        Ok(Self {
            name: name.to_string(),
            blob,
            valid_after,
            valid_before,
        })
    }

    fn valid_on(&self, day: u32) -> bool {
        self.valid_after.is_none_or(|a| day >= a) && self.valid_before.is_none_or(|b| day < b)
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum SigError {
    #[error("malformed signature file ({0})")]
    Malformed(&'static str),
    #[error("signature namespace is not {NAMESPACE}")]
    Namespace,
    #[error("signature hash is not sha512")]
    Hash,
    #[error("signature key is not an ssh-ed25519 key")]
    KeyType,
    #[error("not signed by a trusted key")]
    Untrusted,
    #[error("signature does not match the file")]
    Invalid,
    #[error("signing key {0} is not valid today (outside its valid-after/valid-before)")]
    Expired(String),
}

/// A length-prefixed SSH string off the front of `b`.
fn field<'a>(b: &mut &'a [u8]) -> Option<&'a [u8]> {
    let n = u32::from_be_bytes(b.get(..4)?.try_into().ok()?) as usize;
    let f = b.get(4..)?.get(..n)?;
    *b = &b[4 + n..];
    Some(f)
}

/// The 32 raw bytes of an `ssh-ed25519` key blob, or None for any other shape.
fn ed25519_key(blob: &[u8]) -> Option<&[u8]> {
    let mut b = blob;
    let (t, k) = (field(&mut b)?, field(&mut b)?);
    (t == KEY_TYPE && k.len() == 32 && b.is_empty()).then_some(k)
}

/// Verify `sig_pem` (an armored `ssh-keygen -Y sign` file) over `sums` against `keys` on `today`
/// (`YYYYMMDD`, UTC). Returns the name of the key that signed.
pub fn verify_sshsig(
    keys: &[TrustedKey],
    today: u32,
    sums: &[u8],
    sig_pem: &str,
    namespace: &str,
) -> Result<String, SigError> {
    use SigError::*;
    let lines: Vec<&str> = sig_pem.lines().map(str::trim).collect();
    let [first, body @ .., last] = lines.as_slice() else {
        return Err(Malformed("no armor"));
    };
    if *first != BEGIN || *last != END {
        return Err(Malformed("no armor"));
    }
    let raw = STANDARD
        .decode(body.concat())
        .map_err(|_| Malformed("not base64"))?;
    let mut r = raw.as_slice();
    if r.get(..MAGIC.len()) != Some(MAGIC)
        || r.get(MAGIC.len()..MAGIC.len() + 4) != Some(&[0, 0, 0, 1])
    {
        return Err(Malformed("not an sshsig v1"));
    }
    r = &r[MAGIC.len() + 4..];
    let (Some(pk_blob), Some(ns), Some(reserved), Some(hash), Some(sig_blob)) = (
        field(&mut r),
        field(&mut r),
        field(&mut r),
        field(&mut r),
        field(&mut r),
    ) else {
        return Err(Malformed("truncated"));
    };
    if !r.is_empty() {
        return Err(Malformed("trailing bytes"));
    }
    if ns != namespace.as_bytes() {
        return Err(Namespace);
    }
    if hash != b"sha512" {
        return Err(Hash);
    }
    let pk = ed25519_key(pk_blob).ok_or(KeyType)?;
    let key = keys.iter().find(|k| k.blob == pk_blob).ok_or(Untrusted)?;

    let mut sb = sig_blob;
    let sig = match (field(&mut sb), field(&mut sb)) {
        (Some(KEY_TYPE), Some(sig)) if sb.is_empty() => sig,
        _ => return Err(Malformed("signature is not ssh-ed25519")),
    };
    // What ssh-keygen signs: magic, namespace, reserved, hash name, then the SHA-512 of the file.
    let mut signed = MAGIC.to_vec();
    for f in [ns, reserved, hash] {
        signed.extend((f.len() as u32).to_be_bytes());
        signed.extend(f);
    }
    signed.extend(64u32.to_be_bytes());
    signed.extend(Sha512::digest(sums));
    UnparsedPublicKey::new(&ED25519, pk)
        .verify(&signed, sig)
        .map_err(|_| Invalid)?;
    if !key.valid_on(today) {
        return Err(Expired(key.name.clone()));
    }
    Ok(key.name.clone())
}

/// `(sha256 hex, file name)` of every line of a `shasum -a 256` file; other lines are skipped.
pub fn parse_sums(sums: &str) -> Vec<(String, String)> {
    sums.lines()
        .filter_map(|l| {
            let (hash, name) = l.split_once(' ')?;
            let name = name.strip_prefix(['*', ' '])?;
            (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && !name.is_empty())
                .then(|| (hash.to_ascii_lowercase(), name.to_string()))
        })
        .collect()
}

/// The signed hash of `asset`. An asset the file does not list, or lists twice with different
/// hashes, is an error: a signature vouches only for what it says.
pub fn expected_hash(sums: &str, asset: &str) -> Result<String> {
    let mut hashes: Vec<String> = parse_sums(sums)
        .into_iter()
        .filter(|(_, n)| n == asset)
        .map(|(h, _)| h)
        .collect();
    hashes.dedup();
    match hashes.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("the signed SHA256SUMS does not list {asset}"),
        _ => bail!("the signed SHA256SUMS lists {asset} with different hashes"),
    }
}

/// Which keys to trust and from which release a signature is required.
#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    signed_since: Option<semver::Version>,
    pub keys: Vec<TrustedKey>,
}

#[derive(Deserialize)]
struct RawPolicy {
    signed_since: Option<String>,
    keys: Vec<RawKey>,
}

#[derive(Deserialize)]
struct RawKey {
    name: String,
    pubkey: String,
    valid_after: Option<String>,
    valid_before: Option<String>,
}

fn version(tag: &str) -> Result<semver::Version, semver::Error> {
    semver::Version::parse(tag.trim_start_matches('v'))
}

/// `YYYY-MM-DD` as the number `YYYYMMDD`.
fn day(s: &str) -> Result<u32> {
    let parts: Vec<&str> = s.split('-').collect();
    let n: Option<Vec<u32>> = parts.iter().map(|p| p.parse().ok()).collect();
    match (parts.as_slice(), n.as_deref()) {
        ([y, m, d], Some(&[yy, mm, dd])) if y.len() == 4 && m.len() == 2 && d.len() == 2 => {
            if !(1..=12).contains(&mm) || !(1..=31).contains(&dd) {
                bail!("{s} is not a date");
            }
            Ok(yy * 10_000 + mm * 100 + dd)
        }
        _ => bail!("{s} is not a YYYY-MM-DD date"),
    }
}

impl Policy {
    /// The policy this binary was built with: `data/release-keys.json`. Debug builds (never a
    /// release) read `ATLAS_RELEASE_POLICY=<file>` instead, so tests can stand in keys.
    pub fn current() -> Result<Self> {
        #[cfg(debug_assertions)]
        if let Some(p) = std::env::var_os("ATLAS_RELEASE_POLICY") {
            let json = std::fs::read_to_string(&p)
                .with_context(|| format!("ATLAS_RELEASE_POLICY {}", p.to_string_lossy()))?;
            return Self::parse(&json);
        }
        Self::parse(include_str!("../data/release-keys.json"))
    }

    pub fn parse(json: &str) -> Result<Self> {
        let raw: RawPolicy = serde_json::from_str(json).context("release keys")?;
        let signed_since = raw
            .signed_since
            .as_deref()
            .map(|t| version(t).with_context(|| format!("signed_since {t} is not a version")))
            .transpose()?;
        let keys = raw
            .keys
            .iter()
            .map(|k| {
                let date = |d: &Option<String>| d.as_deref().map(day).transpose();
                TrustedKey::new(
                    &k.name,
                    &k.pubkey,
                    date(&k.valid_after)?,
                    date(&k.valid_before)?,
                )
            })
            .collect::<Result<_>>()?;
        Ok(Self { signed_since, keys })
    }

    /// Whether `tag` (`v1.5.0-alpha.27`) must be signed. A tag that is not a version cannot be
    /// placed before `signed_since`, so it must be.
    pub fn requires_signature(&self, tag: &str) -> bool {
        self.signed_since
            .as_ref()
            .is_some_and(|since| version(tag).map_or(true, |v| v >= *since))
    }

    /// Whether signatures are enforced for any tag at all.
    pub fn is_active(&self) -> bool {
        self.signed_since.is_some()
    }

    /// [`verify_sshsig`] for the release namespace, today.
    pub fn verify(&self, sums: &[u8], sig_pem: &str) -> Result<String, SigError> {
        verify_sshsig(&self.keys, today(), sums, sig_pem, NAMESPACE)
    }
}

/// Today's UTC date as `YYYYMMDD`.
pub fn today() -> u32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    civil(secs / 86_400)
}

/// `YYYYMMDD` of a day count since 1970-01-01 (Hinnant's civil_from_days).
fn civil(days: u64) -> u32 {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y * 10_000 + m * 100 + d) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// A key made by the real `ssh-keygen`, which is also the signing oracle.
    struct Key {
        _dir: tempfile::TempDir,
        path: PathBuf,
        public: String,
    }

    fn ssh_keygen(args: &[&str]) {
        let o = Command::new("ssh-keygen")
            .args(args)
            .env_remove("SSH_AUTH_SOCK")
            .output()
            .expect("ssh-keygen is required by these tests (the signature oracle)");
        assert!(
            o.status.success(),
            "ssh-keygen {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }

    fn key_of(kind: &[&str]) -> Key {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k");
        let mut args = vec!["-q", "-N", "", "-C", "test", "-f", path.to_str().unwrap()];
        args.extend_from_slice(kind);
        ssh_keygen(&args);
        let public = std::fs::read_to_string(dir.path().join("k.pub")).unwrap();
        Key {
            _dir: dir,
            path,
            public: public.trim().to_string(),
        }
    }

    fn key() -> Key {
        key_of(&["-t", "ed25519"])
    }

    fn sign_with(key: &Path, ns: &str, extra: &[&str], data: &[u8]) -> String {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("SHA256SUMS");
        std::fs::write(&file, data).unwrap();
        let mut args = vec!["-Y", "sign", "-n", ns, "-f", key.to_str().unwrap()];
        args.extend_from_slice(extra);
        args.push(file.to_str().unwrap());
        ssh_keygen(&args);
        std::fs::read_to_string(dir.path().join("SHA256SUMS.sig")).unwrap()
    }

    fn sign(k: &Key, data: &[u8]) -> String {
        sign_with(&k.path, NAMESPACE, &[], data)
    }

    fn trusted(name: &str, k: &Key, after: Option<u32>, before: Option<u32>) -> TrustedKey {
        TrustedKey::new(name, &k.public, after, before).unwrap()
    }

    const SUMS: &[u8] = b"aaaa  atlas-1.tar.gz\n";
    const DAY: u32 = 20_270_615;

    fn verify(keys: &[TrustedKey], sums: &[u8], sig: &str) -> Result<String, SigError> {
        verify_sshsig(keys, DAY, sums, sig, NAMESPACE)
    }

    #[test]
    fn a_signature_by_a_trusted_key_names_the_key() {
        let (a, b) = (key(), key());
        let keys = [
            trusted("alice", &a, None, None),
            trusted("bob", &b, None, None),
        ];
        assert_eq!(verify(&keys, SUMS, &sign(&b, SUMS)).as_deref(), Ok("bob"));
        assert_eq!(verify(&keys, SUMS, &sign(&a, SUMS)).as_deref(), Ok("alice"));
    }

    #[test]
    fn tampered_sums_are_rejected() {
        let k = key();
        let keys = [trusted("alice", &k, None, None)];
        let sig = sign(&k, SUMS);
        assert_eq!(
            verify(&keys, b"aaaa  atlas-1.tar.gz\n\n", &sig),
            Err(SigError::Invalid)
        );
        assert_eq!(verify(&keys, b"", &sig), Err(SigError::Invalid));
    }

    #[test]
    fn a_signature_by_another_key_is_rejected() {
        let (trusted_key, attacker) = (key(), key());
        let keys = [trusted("alice", &trusted_key, None, None)];
        assert_eq!(
            verify(&keys, SUMS, &sign(&attacker, SUMS)),
            Err(SigError::Untrusted)
        );
        assert_eq!(
            verify(&[], SUMS, &sign(&attacker, SUMS)),
            Err(SigError::Untrusted)
        );
    }

    #[test]
    fn another_namespace_is_rejected_both_ways() {
        let k = key();
        let keys = [trusted("alice", &k, None, None)];
        // Signed for something else (e.g. a git commit), or asked for something else.
        assert_eq!(
            verify(&keys, SUMS, &sign_with(&k.path, "git", &[], SUMS)),
            Err(SigError::Namespace)
        );
        assert_eq!(
            verify_sshsig(&keys, DAY, SUMS, &sign(&k, SUMS), "file"),
            Err(SigError::Namespace)
        );
    }

    #[test]
    fn a_key_outside_its_validity_window_is_rejected() {
        let k = key();
        let sig = sign(&k, SUMS);
        let expired = [trusted("alice", &k, None, Some(DAY))]; // valid-before is exclusive
        assert_eq!(
            verify(&expired, SUMS, &sig),
            Err(SigError::Expired("alice".into()))
        );
        let early = [trusted("alice", &k, Some(DAY + 1), None)];
        assert_eq!(
            verify(&early, SUMS, &sig),
            Err(SigError::Expired("alice".into()))
        );
        let inside = [trusted("alice", &k, Some(DAY), Some(DAY + 1))]; // valid-after is inclusive
        assert_eq!(verify(&inside, SUMS, &sig).as_deref(), Ok("alice"));
    }

    #[test]
    fn keys_that_are_not_ed25519_are_rejected() {
        let trusted_key = key();
        let keys = [trusted("alice", &trusted_key, None, None)];
        for kind in [&["-t", "ecdsa"][..], &["-t", "rsa", "-b", "2048"][..]] {
            let other = key_of(kind);
            let sig = sign(&other, SUMS);
            assert_eq!(
                verify(&keys, SUMS, &sig),
                Err(SigError::KeyType),
                "{kind:?}"
            );
            // And such a key cannot be put in the trust list.
            assert!(
                TrustedKey::new("x", &other.public, None, None).is_err(),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_hash_other_than_sha512_is_rejected() {
        let k = key();
        let keys = [trusted("alice", &k, None, None)];
        let sig = sign_with(&k.path, NAMESPACE, &["-O", "hashalg=sha256"], SUMS);
        assert_eq!(verify(&keys, SUMS, &sig), Err(SigError::Hash));
    }

    #[test]
    fn garbage_and_truncated_signature_files_are_rejected_and_never_panic() {
        let k = key();
        let keys = [trusted("alice", &k, None, None)];
        let sig = sign(&k, SUMS);
        for bad in [
            "",
            "garbage",
            BEGIN,
            &format!("{BEGIN}\n{END}"),
            &format!("{BEGIN}\n!!!\n{END}"),
        ] {
            assert!(verify(&keys, SUMS, bad).is_err(), "{bad:?}");
        }
        // Cut at every byte: only the whole file verifies (the trailing newline is not part of it).
        for n in 0..sig.trim_end().len() {
            assert!(verify(&keys, SUMS, &sig[..n]).is_err(), "cut at {n}");
        }
        assert!(verify(&keys, SUMS, &sig).is_ok());
    }

    #[test]
    fn flipping_any_bit_of_the_signature_blob_is_rejected() {
        let k = key();
        let keys = [trusted("alice", &k, None, None)];
        let sig = sign(&k, SUMS);
        let b64: String = sig.lines().filter(|l| !l.starts_with("-----")).collect();
        let raw = STANDARD.decode(b64).unwrap();
        let armor = |b: &[u8]| format!("{BEGIN}\n{}\n{END}\n", STANDARD.encode(b));
        assert!(verify(&keys, SUMS, &armor(&raw)).is_ok());
        for i in 0..raw.len() {
            let mut bad = raw.clone();
            bad[i] ^= 1;
            assert!(
                verify(&keys, SUMS, &armor(&bad)).is_err(),
                "flip in byte {i}"
            );
        }
        let mut longer = raw.clone();
        longer.push(0);
        assert_eq!(
            verify(&keys, SUMS, &armor(&longer)),
            Err(SigError::Malformed("trailing bytes"))
        );
    }

    #[test]
    fn sums_are_parsed_and_the_asset_hash_is_the_signed_one() {
        let (h1, h2) = ("a".repeat(64), "B".repeat(64));
        let sums = format!("{h1}  atlas-1.tar.gz\n{h2} *atlas-1.deb\nnot a line\n{h1}  \n");
        assert_eq!(
            parse_sums(&sums),
            vec![
                (h1.clone(), "atlas-1.tar.gz".to_string()),
                ("b".repeat(64), "atlas-1.deb".to_string())
            ]
        );
        assert_eq!(expected_hash(&sums, "atlas-1.tar.gz").unwrap(), h1);
        assert_eq!(expected_hash(&sums, "atlas-1.deb").unwrap(), "b".repeat(64));
        let missing = expected_hash(&sums, "atlas-2.tar.gz")
            .unwrap_err()
            .to_string();
        assert!(missing.contains("does not list"), "{missing}");
        // The same name twice with different hashes is ambiguous, not "first wins".
        let twice = format!("{h1}  x\n{}  x\n", "c".repeat(64));
        assert!(
            expected_hash(&twice, "x")
                .unwrap_err()
                .to_string()
                .contains("different hashes")
        );
        // A name is matched whole: `atlas-1.tar.gz` is not `1.tar.gz`.
        assert!(expected_hash(&sums, "1.tar.gz").is_err());
    }

    #[test]
    fn requires_signature_follows_semver_order() {
        let k = key();
        let json = |since: &str| {
            format!(
                r#"{{"signed_since":{since},"keys":[{{"name":"a","pubkey":"{}"}}]}}"#,
                k.public
            )
        };
        let p = Policy::parse(&json(r#""v1.5.0-alpha.27""#)).unwrap();
        for (tag, want) in [
            ("v1.5.0-alpha.26", false),
            ("v1.4.0", false),
            ("v1.5.0-alpha.27", true),
            ("v1.5.0-alpha.28", true),
            ("v1.5.0-beta.1", true),
            ("v1.5.0", true),
            ("v1.10.0", true),
            ("not-a-version", true), // cannot be placed before: fail closed
        ] {
            assert_eq!(p.requires_signature(tag), want, "{tag}");
        }
        let unset = Policy::parse(&json("null")).unwrap();
        assert!(!unset.is_active());
        assert!(
            !unset.requires_signature("v9.9.9"),
            "unset = legacy path for every tag"
        );
    }

    #[test]
    fn a_policy_with_a_bad_key_date_or_version_is_refused() {
        let k = key();
        let ecdsa = key_of(&["-t", "ecdsa"]);
        let with = |extra: &str, public: &str| {
            format!(r#"{{"signed_since":null,"keys":[{{"name":"a","pubkey":"{public}"{extra}}}]}}"#)
        };
        assert!(Policy::parse(&with("", &k.public)).is_ok());
        assert!(Policy::parse(&with("", &ecdsa.public)).is_err());
        assert!(Policy::parse(&with(r#","valid_before":"2027-13-01""#, &k.public)).is_err());
        assert!(Policy::parse(&with(r#","valid_before":"soon""#, &k.public)).is_err());
        assert!(Policy::parse(r#"{"signed_since":"later","keys":[]}"#).is_err());
        let p = Policy::parse(&with(
            r#","valid_after":"2026-12-01","valid_before":"2028-06-30""#,
            &k.public,
        ))
        .unwrap();
        assert_eq!(
            (p.keys[0].valid_after, p.keys[0].valid_before),
            (Some(20_261_201), Some(20_280_630))
        );
    }

    #[test]
    fn the_embedded_policy_parses() {
        Policy::parse(include_str!("../data/release-keys.json")).unwrap();
    }

    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(civil(0), 19_700_101);
        assert_eq!(civil(19_782), 20_240_229);
        assert_eq!(civil(19_783), 20_240_301);
        assert_eq!(civil(20_088), 20_241_231);
    }
}
