//! A fingerprint for an error report: the same bug reported twice, by two people, on two versions, has the same
//! 16 hex characters, so a crash file, an issue body and the "Report issue" screen can all say it and an issue
//! search by that text finds the earlier report (#320).
//!
//! `fingerprint(message, stack)` hashes two things:
//! - the **normalized message**: its first line, lower-cased (ASCII), every path reduced to its file name, every
//!   UUID, long hex run, `0x` number and decimal number (ports, counts, versions, line numbers) replaced by `#`,
//!   whitespace collapsed, at most 200 characters;
//! - the **frame key** of the first frame inside the app (`function|file`, with no line, column, query string,
//!   directory or bundler hash), so the same function at another line, or on another machine, is the same error.
//!
//! The page computes the same value for what the webview throws (`apps/desktop/src/lib/errors/fingerprint.ts`).
//! Both are written as plain scans over ASCII classes, not regexes, so Rust's Unicode-aware `\d`/`\b` and
//! JavaScript's cannot disagree; `apps/desktop/src/lib/errors/__fixtures__/fingerprints.json` is the contract both
//! test against, and this crate is the source of truth for it.

/// Longest normalized message (in characters) that goes into the hash.
const MAX_MESSAGE_CHARS: usize = 200;

/// The fingerprint of an error: 16 lower-case hex characters (FNV-1a, 64 bits, of `message\nframe-key`).
/// `stack` is a JavaScript stack (V8 or WebKit shape), a Rust panic location, or empty.
pub fn fingerprint(message: &str, stack: &str) -> String {
    with_key(message, &frame_key(stack))
}

/// [`fingerprint`] of a message and a frame key already made (`function|file`), for the reports that have no JavaScript
/// stack to read it from: a native crash (`crash_fingerprint`).
pub fn with_key(message: &str, key: &str) -> String {
    let text = format!("{}\n{key}", normalize_message(message));
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// What two reports of the same error have in common in their message. See the module docs.
pub fn normalize_message(message: &str) -> String {
    let first = message
        .split('\n')
        .map(trim_ws)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let lower = first.to_ascii_lowercase();
    let words: Vec<String> = lower
        .split([' ', '\t', '\r', '\n'])
        .filter(|t| !t.is_empty())
        .map(path_token)
        .collect();
    let joined = words.join(" ");
    let masked = mask_numbers(&mask_hex(&joined));
    // Masking can leave a space (a path token that was only punctuation); collapse again.
    let collapsed: Vec<&str> = masked.split(' ').filter(|t| !t.is_empty()).collect();
    collapsed
        .join(" ")
        .chars()
        .take(MAX_MESSAGE_CHARS)
        .collect()
}

/// Trim space, tab, CR and LF only: `str::trim` and JavaScript's `trim()` disagree about U+0085, U+FEFF and a few
/// other characters, and this must come out the same in both.
fn trim_ws(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\r', '\n'])
}
/// A whitespace-free token; one that is a path (it has a `/` or `\`) becomes its file name.
fn path_token(token: &str) -> String {
    const OPEN: &[char] = &['(', '"', '\'', '`', '<', '['];
    const CLOSE: &[char] = &[')', '"', '\'', '`', '>', ']', ',', ';', ':'];
    let core = token.trim_start_matches(OPEN).trim_end_matches(CLOSE);
    if !core.contains(['/', '\\']) {
        return token.to_string();
    }
    let name = core.rsplit(['/', '\\']).next().unwrap_or("");
    strip_position(name).to_string()
}

/// `file.rs:12:5` -> `file.rs`; `file.rs:12` -> `file.rs`; anything else as it is.
fn strip_position(s: &str) -> &str {
    let mut out = s;
    for _ in 0..2 {
        match out.rsplit_once(':') {
            Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => {
                out = head;
            }
            _ => break,
        }
    }
    out
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
}

/// UUIDs, `0x` numbers and runs of 8 or more hex digits (ids, hashes, addresses) become `#`. Input is lower case.
fn mask_hex(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_alphanumeric() {
            let ch = s[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        if is_uuid_at(b, i) {
            out.push('#');
            i += 36;
            continue;
        }
        let end = i + b[i..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric())
            .count();
        let run = &b[i..end];
        let hex_number =
            run.len() > 2 && run.starts_with(b"0x") && run[2..].iter().all(|c| is_hex(*c));
        if hex_number || (run.len() >= 8 && run.iter().all(|c| is_hex(*c))) {
            out.push('#');
        } else {
            out.push_str(&s[i..end]);
        }
        i = end;
    }
    out
}

fn is_uuid_at(b: &[u8], i: usize) -> bool {
    if i + 36 > b.len() {
        return false;
    }
    b[i..i + 36].iter().enumerate().all(|(n, c)| {
        if matches!(n, 8 | 13 | 18 | 23) {
            *c == b'-'
        } else {
            is_hex(*c)
        }
    }) && !b.get(i + 36).is_some_and(|c| c.is_ascii_alphanumeric())
}

/// Decimal numbers, with their dotted tails (`1.5.0`, `3.14`) and a release label after them (`-alpha.27`, `-beta.2`,
/// `-rc.1`), become `#`: the same bug on alpha.27 and on 1.6.0 is one bug.
fn mask_numbers(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            while i < b.len() {
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
                    i += 1;
                } else {
                    break;
                }
            }
            for label in ["-alpha.", "-beta.", "-rc."] {
                let tail = &b[i..];
                if tail.starts_with(label.as_bytes())
                    && tail.get(label.len()).is_some_and(u8::is_ascii_digit)
                {
                    i += label.len();
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                    break;
                }
            }
            out.push('#');
        } else {
            let ch = s[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// The `function|file` of the first frame of `stack` that is inside the app (a `.svelte`, `.ts`, `.js`, `.mjs` or
/// `.rs` file that is not a dependency), else of the first non-empty line; empty for no stack.
pub fn frame_key(stack: &str) -> String {
    let frames: Vec<Frame> = stack
        .lines()
        .map(trim_ws)
        .filter(|l| !l.is_empty())
        .map(parse_frame)
        .collect();
    frames
        .iter()
        .find(|f| f.in_app())
        .or(frames.first())
        .map(|f| format!("{}|{}", f.function, f.file))
        .unwrap_or_default()
}

struct Frame {
    function: String,
    file: String,
    in_app: bool,
}

impl Frame {
    fn in_app(&self) -> bool {
        self.in_app
    }
}

fn parse_frame(line: &str) -> Frame {
    let (function, loc) = if let Some(rest) = line.strip_prefix("at ") {
        match rest.strip_suffix(')').and_then(|r| r.rsplit_once(" (")) {
            Some((f, l)) => (f.to_string(), l.to_string()),
            None => (String::new(), rest.to_string()),
        }
    } else if let Some((f, l)) = line.split_once('@') {
        (f.to_string(), l.to_string())
    } else {
        (String::new(), line.to_string())
    };
    let function = {
        let f = trim_ws(&function);
        let f = f.strip_prefix("async ").unwrap_or(f);
        let f = f.strip_prefix("new ").unwrap_or(f);
        if f == "<anonymous>" || f == "anonymous" {
            String::new()
        } else {
            f.to_string()
        }
    };
    let loc = strip_position(trim_ws(&loc)).replace('\\', "/");
    let loc = loc.split('?').next().unwrap_or("");
    let dependency = loc.contains("node_modules")
        || loc.contains(".cargo/registry")
        || loc.contains(".cargo\\registry")
        || loc.contains("/rustc/");
    let file = match loc.find("crates/") {
        Some(i) => loc[i..].to_string(),
        None => loc.rsplit('/').next().unwrap_or("").to_string(),
    };
    let file = strip_bundler_hash(&file);
    let in_app = !dependency
        && [".svelte", ".ts", ".js", ".mjs", ".rs"]
            .iter()
            .any(|e| file.ends_with(e));
    Frame {
        function,
        file,
        in_app,
    }
}

/// `index-AbCd1234.js` -> `index.js` (Vite names a chunk by its content hash, which changes every release).
fn strip_bundler_hash(file: &str) -> String {
    for ext in [".js", ".mjs"] {
        if let Some(stem) = file.strip_suffix(ext)
            && let Some((head, hash)) = stem.rsplit_once('-')
            && hash.len() == 8
            && hash.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return format!("{head}{ext}");
        }
    }
    file.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/lib/errors/__fixtures__/fingerprints.json"
    );

    fn cases() -> Vec<Value> {
        let text = std::fs::read_to_string(FIXTURES).expect("fingerprints.json");
        serde_json::from_str::<Value>(&text).unwrap()["cases"]
            .as_array()
            .unwrap()
            .clone()
    }

    /// The contract with `fingerprint.ts`: this file holds the values this function produces, and the page's test
    /// asserts the same ones. (#320; set `UPDATE_FINGERPRINTS=1` to rewrite it after changing the algorithm on
    /// purpose, which also changes every fingerprint already in an issue.)
    #[test]
    fn the_shared_fixture_holds_what_rust_computes() {
        let mut doc: Value =
            serde_json::from_str(&std::fs::read_to_string(FIXTURES).unwrap()).unwrap();
        let mut changed = false;
        for c in doc["cases"].as_array_mut().unwrap() {
            let (m, s) = (c["message"].as_str().unwrap(), c["stack"].as_str().unwrap());
            let got = fingerprint(m, s);
            if std::env::var_os("UPDATE_FINGERPRINTS").is_some() {
                changed |= c["fingerprint"] != got.as_str();
                c["fingerprint"] = got.into();
            } else {
                assert_eq!(c["fingerprint"], got.as_str(), "{}", c["name"]);
            }
        }
        if changed {
            std::fs::write(FIXTURES, serde_json::to_string_pretty(&doc).unwrap() + "\n").unwrap();
        }
    }

    /// What the fixture's `same` and `different` groups promise, whatever the hashes are.
    #[test]
    fn cases_in_a_group_agree_and_cases_in_different_groups_differ() {
        let mut by_group: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for c in cases() {
            by_group
                .entry(c["group"].as_str().unwrap().to_string())
                .or_default()
                .push(fingerprint(
                    c["message"].as_str().unwrap(),
                    c["stack"].as_str().unwrap(),
                ));
        }
        let mut firsts = std::collections::BTreeSet::new();
        for (group, fps) in &by_group {
            assert!(fps.iter().all(|f| f == &fps[0]), "group {group}: {fps:?}");
            assert!(
                firsts.insert(fps[0].clone()),
                "group {group} collides with another"
            );
        }
        assert!(by_group.len() >= 5);
    }

    #[test]
    fn a_fingerprint_is_sixteen_lower_case_hex_characters() {
        let f = fingerprint("boom", "");
        assert_eq!(f.len(), 16);
        assert!(f.bytes().all(is_hex));
    }

    #[test]
    fn messages_lose_what_changes_between_machines_and_versions() {
        assert_eq!(
            normalize_message(
                "Cannot open /home/ana/.atlas/state-12.json: No such file (os error 2)"
            ),
            "cannot open state-#.json no such file (os error #)"
        );
        assert_eq!(
            normalize_message(
                "atlas 1.5.0-alpha.27 failed on port 24101 id 3fa9c2d17b04 at 0x7ffd12ab"
            ),
            "atlas # failed on port # id # at #"
        );
        assert_eq!(
            normalize_message("request 123e4567-e89b-12d3-a456-426614174000 timed out"),
            "request # timed out"
        );
        assert_eq!(
            normalize_message("C:\\Users\\Ana\\proj\\src\\lib.rs:12:5 panicked"),
            "lib.rs panicked"
        );
        assert_eq!(
            normalize_message("\n\n  first line  \nsecond"),
            "first line"
        );
    }

    #[test]
    fn the_first_app_frame_is_the_key_whatever_the_line_or_the_directory() {
        let v8 = "at Object.run (node_modules/svelte/src/runtime.js:10:2)\nat load (http://tauri.localhost/_app/immutable/chunks/index-AbCd1234.js:1:200)";
        let moved = "at Object.run (node_modules/svelte/src/runtime.js:99:9)\nat load (http://localhost:1420/_app/immutable/chunks/index-ZzYy9876.js:7:3)";
        assert_eq!(frame_key(v8), "load|index.js");
        assert_eq!(frame_key(v8), frame_key(moved));
        let webkit =
            "run@node_modules/x/y.js:1:1\nopenFile@/Users/ana/atlas/Editor.svelte?t=1:45:6";
        assert_eq!(frame_key(webkit), "openFile|Editor.svelte");
        assert_eq!(
            frame_key("/home/runner/work/atlas/atlas/crates/atlas-core/src/fs.rs:88:9"),
            "|crates/atlas-core/src/fs.rs"
        );
        // No app frame: the first line stands in; no stack: nothing.
        assert_eq!(frame_key("a\nz"), "|a");
        assert_eq!(frame_key(""), "");
    }
}
