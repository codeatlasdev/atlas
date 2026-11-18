//! The pseudo-locale `en-XA` of a DEBUG build (`atlas_core::lang::pseudo_enabled`): the English catalog accented, padded
//! by 35% and wrapped in `⟦ ⟧`, built in memory from the English messages so it can never go stale and no file is shipped.
//! A release build has no such catalog (`locales()` does not list it). It is the same transform as
//! `scripts/i18n/pseudo.ts` (the desktop and the site read that one); `i18n/pseudo-cases.json` is what both are held to.
//!
//! A message is transformed per pattern: the prose is accented (variables, references, code spans and URLs stay byte for
//! byte) and the pattern carries a [`Part::Letters`] with the count of its prose letters. Padding and brackets are added
//! once per rendered message ([`wrap`]), after references were inlined, as the script does on the inlined text.

use crate::message::{Message, Part, Pattern, parse_pattern};

const OPEN: char = '⟦';
const CLOSE: char = '⟧';
const PAD: char = '~';

fn accent(c: char) -> char {
    match c {
        'a' => 'á',
        'e' => 'é',
        'i' => 'í',
        'o' => 'ó',
        'u' => 'ú',
        'c' => 'ç',
        'n' => 'ñ',
        'y' => 'ý',
        'A' => 'Å',
        'E' => 'É',
        'I' => 'Í',
        'O' => 'Ó',
        'U' => 'Ú',
        'C' => 'Ç',
        'N' => 'Ñ',
        'Y' => 'Ý',
        c => c,
    }
}

/// Where a kept span starts at `s[i..]`, and its length in bytes: an escaped brace, a `{placeable}`, a `` `code span` ``
/// on one line, or a URL. The same four alternatives, in the same order, as `KEEP` in `pseudo.ts`.
fn kept(s: &str, i: usize) -> Option<usize> {
    let rest = &s[i..];
    let mut chars = rest.chars();
    match chars.next()? {
        '\\' if matches!(chars.next(), Some('{' | '}')) => Some(2),
        '{' => {
            let end = rest[1..].find(['{', '}'])?;
            (rest.as_bytes()[1 + end] == b'}').then_some(end + 2)
        }
        '`' => {
            let end = rest[1..].find(['`', '\n'])?;
            (rest.as_bytes()[1 + end] == b'`').then_some(end + 2)
        }
        'h' => {
            let after = rest
                .strip_prefix("https://")
                .or_else(|| rest.strip_prefix("http://"))?;
            let n: usize = after
                .chars()
                .take_while(|c| !c.is_whitespace() && !matches!(c, ')' | '>' | ']' | '|' | '`'))
                .map(char::len_utf8)
                .sum();
            (n > 0).then_some(rest.len() - after.len() + n)
        }
        _ => None,
    }
}

/// `s` with its prose accented and the number of prose letters (kept spans excluded).
fn accent_prose(s: &str) -> (String, usize) {
    let mut out = String::with_capacity(s.len());
    let mut letters = 0;
    let mut i = 0;
    while i < s.len() {
        if let Some(n) = kept(s, i) {
            out.push_str(&s[i..i + n]);
            i += n;
            continue;
        }
        let c = s[i..].chars().next().expect("i < len");
        letters += usize::from(c.is_alphabetic());
        out.push(accent(c));
        i += c.len_utf8();
    }
    (out, letters)
}

/// The source form of a pattern (`{name}`, `{@key}`, braces escaped), which is what the script transforms.
fn source(p: &Pattern) -> String {
    let mut out = String::new();
    for part in p {
        match part {
            Part::Text(t) => out.push_str(&t.replace('{', "\\{").replace('}', "\\}")),
            Part::Var(n) => out.push_str(&format!("{{{n}}}")),
            Part::Ref(k) => out.push_str(&format!("{{@{k}}}")),
            Part::Letters(_) => {}
        }
    }
    out
}

fn pattern(p: &Pattern) -> Pattern {
    let (accented, letters) = accent_prose(&source(p));
    // The accented source parses back to the same structure (only letters of text changed), so this cannot fail; if it
    // ever did the English pattern is the safe answer.
    let mut out = parse_pattern(&accented).unwrap_or_else(|_| p.clone());
    out.insert(0, Part::Letters(letters));
    out
}

/// The English `msg` with every pattern (every plural and `os` variant) pseudo-localized; selectors and keys untouched.
pub fn message(msg: &Message) -> Message {
    match msg {
        Message::Text(p) => Message::Text(pattern(p)),
        Message::Select {
            selectors,
            variants,
        } => Message::Select {
            selectors: selectors.clone(),
            variants: variants
                .iter()
                .map(|(keys, p)| (keys.clone(), pattern(p)))
                .collect(),
        },
    }
}

/// A rendered pseudo message: padded by 35% of its prose letters, wrapped in `⟦ ⟧`.
pub fn wrap(rendered: &str, letters: usize) -> String {
    let pad = (letters as f64 * 0.35).ceil() as usize;
    let mut out = String::with_capacity(rendered.len() + pad + 7);
    out.push(OPEN);
    out.push_str(rendered);
    if pad > 0 {
        out.push(' ');
        out.extend(std::iter::repeat_n(PAD, pad));
    }
    out.push(CLOSE);
    out
}
