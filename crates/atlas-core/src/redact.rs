//! Scrubbing of text that leaves the machine: log tails, crash reports, diagnostics. One implementation
//! for everything that writes or sends such text (`atlas-desktop`'s diagnostics, [`crate::crash`]).

pub const REDACTED: &str = "[redacted]";

/// Replace what must never leave the machine: tokens and keys by shape and by name, `Authorization`
/// headers, URL credentials, and the home directory (collapsed to `~`).
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    if let Some(home) = dirs::home_dir().and_then(|h| h.to_str().map(str::to_string))
        && home.len() > 1
    {
        out = out.replace(&home, "~");
    }
    out = redact_users(&out);
    out = redact_url_credentials(&out);
    out = redact_named(&out);
    redact_tokens(&out)
}

/// `/Users/<name>/` and `/home/<name>/` of any user, not only the current one.
fn redact_users(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = ["/Users/", "/home/"]
        .iter()
        .filter_map(|p| rest.find(p))
        .min()
    {
        let prefix_len = if rest[i..].starts_with("/Users/") {
            7
        } else {
            6
        };
        out.push_str(&rest[..i]);
        let after = &rest[i + prefix_len..];
        let name_len = after
            .find(|c: char| c == '/' || c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(after.len());
        out.push('~');
        rest = &after[name_len..];
    }
    out.push_str(rest);
    out
}

/// `scheme://user:password@host` → `scheme://[redacted]@host`.
fn redact_url_credentials(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(at) => {
                out.push_str(REDACTED);
                rest = &tail[at..];
            }
            None => rest = tail,
        }
    }
    out.push_str(rest);
    out
}

/// Names whose value is a secret: `token=…`, `"password": "…"`, `Authorization: Bearer …`, `API_KEY=…`.
fn redact_named(s: &str) -> String {
    const NAMES: &[&str] = &[
        "token",
        "secret",
        "password",
        "passwd",
        "api_key",
        "apikey",
        "api-key",
        "access_key",
        "private_key",
        "authorization",
        "auth",
        "cookie",
        "credential",
    ];
    s.lines()
        .map(|line| redact_line(line, NAMES))
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_line(line: &str, names: &[&str]) -> String {
    let lower = line.to_ascii_lowercase();
    let bytes = line.as_bytes();
    let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        // Nearest secret-looking identifier at or after `i` that starts an identifier.
        let hit = names
            .iter()
            .filter_map(|n| {
                let mut from = i;
                while let Some(p) = lower[from..].find(n) {
                    let at = from + p;
                    if at == 0
                        || !ident(bytes[at - 1])
                        || bytes[at - 1] == b'_'
                        || bytes[at - 1] == b'-'
                    {
                        return Some((at, n.len()));
                    }
                    from = at + n.len();
                }
                None
            })
            .min_by_key(|(p, _)| *p);
        let Some((at, nlen)) = hit else { break };
        // The rest of the identifier (`GITHUB_TOKEN`, `token_id`, `x-api-key`).
        let mut j = at + nlen;
        while j < line.len() && ident(bytes[j]) {
            j += 1;
        }
        let mut k = j;
        if line[k..].starts_with(['"', '\'']) {
            k += 1;
        }
        while line[k..].starts_with(' ') {
            k += 1;
        }
        // Only `name=value` / `name: value` is an assignment; prose that mentions a name is not.
        if !line[k..].starts_with(['=', ':']) {
            out.push_str(&line[i..j]);
            i = j;
            continue;
        }
        k += 1;
        while line[k..].starts_with(' ') {
            k += 1;
        }
        let quote = line[k..].chars().next().filter(|c| matches!(c, '"' | '\''));
        if quote.is_some() {
            k += 1;
        }
        // `Authorization: Bearer abc` keeps the scheme word out of the secret.
        if let Some(p) = ["Bearer ", "Basic ", "bearer ", "basic "]
            .iter()
            .find(|p| line[k..].starts_with(**p))
        {
            k += p.len();
        }
        let end = match quote {
            Some(q) => line[k..].find(q).map_or(line.len(), |e| k + e),
            None => line[k..]
                .find(|c: char| c.is_whitespace() || c == ',' || c == '&' || c == '"' || c == '\'')
                .map_or(line.len(), |e| k + e),
        };
        out.push_str(&line[i..k]);
        out.push_str(REDACTED);
        i = end;
    }
    out.push_str(&line[i..]);
    out
}

/// Well-known token shapes and any long hex/base64-looking run, wherever they appear.
fn redact_tokens(s: &str) -> String {
    const PREFIXES: &[&str] = &[
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "sk-",
        "xoxb-",
        "xoxp-",
        "AKIA",
        "glpat-",
        "npm_",
        "pk_live_",
        "sk_live_",
    ];
    let mut out = String::with_capacity(s.len());
    for (n, word) in s
        .split_inclusive(|c: char| c.is_whitespace() || "\"'(),;<>[]{}=:".contains(c))
        .enumerate()
    {
        let _ = n;
        let trimmed =
            word.trim_end_matches(|c: char| c.is_whitespace() || "\"'(),;<>[]{}=:".contains(c));
        let tail = &word[trimmed.len()..];
        let secret = PREFIXES
            .iter()
            .any(|p| trimmed.starts_with(p) && trimmed.len() >= p.len() + 8)
            || is_long_secretish(trimmed);
        if secret {
            out.push_str(REDACTED);
            out.push_str(tail);
        } else {
            out.push_str(word);
        }
    }
    out
}

/// 32+ chars of hex or base64 alphabet with at least one digit and one letter: a hash or a token,
/// never prose or a path (paths contain `/` only between short segments, which fails the length).
fn is_long_secretish(w: &str) -> bool {
    w.len() >= 32
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'))
        && w.chars().any(|c| c.is_ascii_digit())
        && w.chars().any(|c| c.is_ascii_alphabetic())
        && !w.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic `hostd.log` with every kind of secret a daemon or a child process could print.
    const LOG: &str = "\
atlas hostd 1.5.0-alpha.5 listening on /Users/ana/.atlas/hostd.sock (pid 57750)
spawn failed: GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789 cwd=/Users/ana/work/app
curl -H \"Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.sig\" https://api.example.com
clone https://ana:hunter2@github.com/acme/private.git
{\"password\": \"correct horse battery staple\", \"user\": \"ana\"}
API_KEY=sk-live-0123456789abcdef OPENAI_API_KEY=sk-proj-abcdefghijklmnop
digest 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08 done
plain line, nothing to hide: terminal 3 exited with code 0";

    #[test]
    fn redaction_removes_every_secret_shape_and_keeps_the_rest() {
        let out = redact(LOG);
        for secret in [
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
            "hunter2",
            "correct horse battery staple",
            "sk-live-0123456789abcdef",
            "sk-proj-abcdefghijklmnop",
            "9f86d081884c7d659a2feaa0c55ad015",
            "/Users/ana",
        ] {
            assert!(!out.contains(secret), "{secret} leaked:\n{out}");
        }
        // What a maintainer needs to read stays readable.
        for keep in [
            "atlas hostd 1.5.0-alpha.5 listening on ~/.atlas/hostd.sock (pid 57750)",
            "plain line, nothing to hide: terminal 3 exited with code 0",
            "\"user\": \"ana\"",
            "github.com/acme/private.git",
        ] {
            assert!(out.contains(keep), "{keep} was lost:\n{out}");
        }
        assert!(out.contains(REDACTED));
    }

    /// The bias is deliberate: a secret-named key loses its value even when the value is prose
    /// (`secret: terminal`); leaking a token costs more than losing a word.
    #[test]
    fn a_secret_named_key_is_redacted_even_when_the_value_looks_harmless() {
        assert_eq!(redact("secret: terminal"), format!("secret: {REDACTED}"));
        // ...but prose that merely mentions the word is left alone.
        assert_eq!(redact("the token was refreshed"), "the token was refreshed");
        assert_eq!(redact("authenticated as ana"), "authenticated as ana");
    }

    #[test]
    fn redaction_is_idempotent_and_leaves_clean_text_alone() {
        let once = redact(LOG);
        assert_eq!(redact(&once), once);
        let clean = "terminal host 1.5.0-alpha.4 is older than Atlas 1.5.0-alpha.5; restarting";
        assert_eq!(redact(clean), clean);
    }

    #[test]
    fn the_current_users_home_collapses_to_tilde() {
        let home = dirs::home_dir().unwrap();
        let line = format!("log at {}/.atlas/hostd.log", home.display());
        assert_eq!(redact(&line), "log at ~/.atlas/hostd.log");
    }
}
