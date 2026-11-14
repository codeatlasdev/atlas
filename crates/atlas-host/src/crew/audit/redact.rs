//! What never reaches the audit file: the values of a crewmate's secrets and the hook token
//! (`[secret:NAME]`), and strings longer than [`super::MAX_STRING`] (cut, with a marker).

use serde_json::Value;

use super::MAX_STRING;
use crate::crew::store::CrewStore;

/// A value shorter than this is not treated as a secret: replacing `1` or an empty string would
/// blank every digit of the log, and nobody guards a token that short.
const MIN_SECRET_LEN: usize = 4;

/// `(name, value)` of the crewmate's secrets, longest value first so one that contains another is
/// replaced whole.
pub(super) fn secrets_of(store: &CrewStore, crewmate_id: &str) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = store.secret_env(crewmate_id).into_iter().collect();
    v.sort_by_key(|(_, val)| std::cmp::Reverse(val.len()));
    v
}

/// The hook token of this hostd run, from the endpoint file the hook server wrote (`""` when absent).
pub(super) fn hook_token(base: &std::path::Path) -> String {
    std::fs::read_to_string(crate::paths::hook_endpoint(base))
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("ATLAS_HOOK_TOKEN="))
        .unwrap_or_default()
        .to_string()
}

/// Replace every secret and the hook token in `v`, and cap every string.
pub(super) fn scrub(v: &mut Value, secrets: &[(String, String)], hook_token: &str) {
    match v {
        Value::String(s) => *s = scrub_str(s, secrets, hook_token),
        Value::Array(a) => a.iter_mut().for_each(|x| scrub(x, secrets, hook_token)),
        Value::Object(m) => {
            // Keys can carry a value too (a header map keyed by the token); rebuild with clean keys.
            let old = std::mem::take(m);
            for (k, mut x) in old {
                scrub(&mut x, secrets, hook_token);
                m.insert(scrub_str(&k, secrets, hook_token), x);
            }
        }
        _ => {}
    }
}

fn scrub_str(s: &str, secrets: &[(String, String)], hook_token: &str) -> String {
    let mut out = s.to_string();
    for (name, value) in secrets {
        if value.len() >= MIN_SECRET_LEN && out.contains(value.as_str()) {
            out = out.replace(value.as_str(), &format!("[secret:{name}]"));
        }
    }
    if hook_token.len() >= MIN_SECRET_LEN && out.contains(hook_token) {
        out = out.replace(hook_token, "[secret:hook_token]");
    }
    cap(out)
}

/// Cut `s` to [`MAX_STRING`] bytes on a character boundary and say how much there was.
fn cap(s: String) -> String {
    if s.len() <= MAX_STRING {
        return s;
    }
    let mut end = MAX_STRING;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [truncated, {} bytes in all]", &s[..end], s.len())
}
