//! Coding vocabulary for dictation (#647): what the engine is told to expect, and what is rewritten after it.
//!
//! **Hints** go to the engine as its initial prompt and bias it toward the words of this work: the project's repo
//! name, its branch, the crewmates' names, and the files and symbols of the crewmate's current or last task. They
//! are automatic and short ([`MAX_HINT_CHARS`]) and are built from the crewmate being dictated to, never from the
//! text of another crewmate's tasks: that is somebody else's conversation. A crewmate's *name* is a word the
//! owner chose and says aloud ("ask Ana to..."), so every name of the crew is a hint; nothing else of another
//! crewmate is.
//!
//! **Replacements** are a small table applied after transcription, deterministic and by whole words:
//! "src slash main dot rs" becomes `src/main.rs`. A row is `say` -> `write` in a language. They apply only to
//! words that stand alone (`slashes` is never touched), never to a URL (a token with `://` or starting `www.`),
//! and the longest phrase wins ("barra invertida" before "barra"). Spacing follows `write`: punctuation joins the
//! words around it (`/ . _ -`), a lone `, ; : ! ?` attaches to the word before it, and a replacement with letters
//! is a word like any other.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::super::store::{CrewStore, ProjectRef};

/// The longest hint, in characters: whisper reads at most 224 tokens of prompt, and a longer one slows every run.
pub(crate) const MAX_HINT_CHARS: usize = 400;
/// Files and symbols kept from the task.
const MAX_SYMBOLS: usize = 12;
/// Crewmate names kept.
const MAX_NAMES: usize = 8;
/// What is read of each text of the task (a result can be long).
const SCAN_CHARS: usize = 4000;
/// Rows the crew may add.
pub(crate) const MAX_ROWS: usize = 200;

/// One row of the table. `lang` is `en`, `pt` or `any`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    pub say: String,
    pub write: String,
    #[serde(default = "any")]
    pub lang: String,
}

fn any() -> String {
    "any".into()
}

fn row(say: &str, write: &str, lang: &str) -> Replacement {
    Replacement {
        say: say.into(),
        write: write.into(),
        lang: lang.into(),
    }
}

/// The table every crew has (never stored: it is always in the effective table).
pub(crate) fn defaults() -> Vec<Replacement> {
    vec![
        row("slash", "/", "en"),
        row("forward slash", "/", "en"),
        row("backslash", "\\", "en"),
        row("back slash", "\\", "en"),
        row("dot", ".", "en"),
        row("underscore", "_", "en"),
        row("dash", "-", "en"),
        row("hyphen", "-", "en"),
        row("colon", ":", "en"),
        row("double colon", "::", "en"),
        row("semicolon", ";", "en"),
        row("at sign", "@", "en"),
        row("barra", "/", "pt"),
        row("barra invertida", "\\", "pt"),
        row("ponto", ".", "pt"),
        row("underline", "_", "pt"),
        row("sublinhado", "_", "pt"),
        row("traço", "-", "pt"),
        row("hífen", "-", "pt"),
        row("dois pontos", ":", "pt"),
        row("ponto e vírgula", ";", "pt"),
        row("arroba", "@", "pt"),
    ]
}

/// The languages a row may name.
pub(crate) const LANGS: [&str; 3] = ["en", "pt", "any"];

/// Check the crew's own rows. Err is what the owner is told.
pub(crate) fn validate(rows: &[Replacement]) -> Result<(), String> {
    if rows.len() > MAX_ROWS {
        return Err(format!("at most {MAX_ROWS} replacements"));
    }
    for r in rows {
        let words = r.say.split_whitespace().count();
        if words == 0 || words > 4 || r.say.chars().count() > 40 {
            return Err(format!(
                "\"{}\": what is said is one to four words, at most 40 characters",
                r.say.chars().take(40).collect::<String>()
            ));
        }
        if r.write.is_empty() || r.write.chars().count() > 40 {
            return Err(format!(
                "\"{}\": what is written is 1 to 40 characters",
                r.say
            ));
        }
        if r.say.chars().chain(r.write.chars()).any(char::is_control) {
            return Err(format!("\"{}\": no control characters", r.say));
        }
        if !LANGS.contains(&r.lang.as_str()) {
            return Err(format!("\"{}\": lang is en, pt or any", r.say));
        }
    }
    Ok(())
}

/// `say` as it is compared: lowercase words, single spaces.
fn key(say: &str) -> String {
    say.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The effective table: the defaults, then the crew's rows. A crew row replaces the default (or an earlier crew
/// row) with the same words in the same language.
pub(crate) fn effective(custom: &[Replacement]) -> Vec<Replacement> {
    let mut out = defaults();
    for c in custom {
        let k = (key(&c.say), c.lang.as_str());
        out.retain(|r| (key(&r.say), r.lang.as_str()) != k);
        out.push(c.clone());
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// `/ . _ -`: no space on either side.
    Join,
    /// A lone `, ; : ! ?`: attached to the word before it, a space after.
    Trailing,
    /// Letters in it: a word like any other.
    Plain,
}

fn mode_of(write: &str) -> Mode {
    if write.chars().any(char::is_alphanumeric) {
        Mode::Plain
    } else if write.chars().count() == 1 && ",;:!?".contains(write) {
        Mode::Trailing
    } else {
        Mode::Join
    }
}

/// One whitespace-separated word of the text: punctuation, the word itself, punctuation.
struct Tok {
    lead: String,
    core: String,
    trail: String,
    /// A URL: left exactly as it is.
    opaque: bool,
}

fn is_url(raw: &str) -> bool {
    raw.contains("://") || raw.to_lowercase().starts_with("www.")
}

fn tokenize(text: &str) -> Vec<Tok> {
    text.split_whitespace()
        .map(|raw| {
            if is_url(raw) {
                return Tok {
                    lead: String::new(),
                    core: raw.to_string(),
                    trail: String::new(),
                    opaque: true,
                };
            }
            let start = raw.find(char::is_alphanumeric).unwrap_or(raw.len());
            let end = raw
                .rfind(char::is_alphanumeric)
                .map(|i| i + raw[i..].chars().next().map_or(0, char::len_utf8))
                .unwrap_or(raw.len());
            if start >= end {
                // No letter at all ("-", "..."): punctuation on its own.
                return Tok {
                    lead: raw.to_string(),
                    core: String::new(),
                    trail: String::new(),
                    opaque: false,
                };
            }
            Tok {
                lead: raw[..start].to_string(),
                core: raw[start..end].to_string(),
                trail: raw[end..].to_string(),
                opaque: false,
            }
        })
        .collect()
}

enum Piece {
    Word { text: String, comma: bool },
    Sym { write: String, mode: Mode },
}

/// Rewrite `text` with `table`. `language` is the language the person was asked to speak (`en`, `pt`): only its
/// rows and the `any` ones apply; `None` (auto) applies every row.
pub(crate) fn apply(text: &str, table: &[Replacement], language: Option<&str>) -> String {
    let mut rows: Vec<(Vec<String>, &Replacement)> = table
        .iter()
        .filter(|r| language.is_none_or(|l| r.lang == l || r.lang == "any"))
        .map(|r| (key(&r.say).split(' ').map(str::to_string).collect(), r))
        .collect();
    // The longest phrase first ("barra invertida" before "barra").
    rows.sort_by_key(|(w, r)| std::cmp::Reverse((w.len(), r.say.len())));
    let toks = tokenize(text);
    let mut pieces: Vec<Piece> = vec![];
    let mut i = 0;
    'next: while i < toks.len() {
        if !toks[i].opaque && !toks[i].core.is_empty() {
            for (words, r) in &rows {
                let n = words.len();
                if i + n > toks.len() {
                    continue;
                }
                let span = &toks[i..i + n];
                let whole = span.iter().enumerate().all(|(k, t)| {
                    !t.opaque
                        && t.core.to_lowercase() == words[k]
                        // Inside a phrase the words are adjacent: no punctuation between them.
                        && (k == 0 || t.lead.is_empty())
                        && (k + 1 == n || t.trail.is_empty())
                });
                if !whole {
                    continue;
                }
                // A comma or period the engine put after the spoken word is its pause, not text.
                let after = &span[n - 1].trail;
                let mode = mode_of(&r.write);
                let keep = if mode == Mode::Plain {
                    after.clone()
                } else {
                    after.strip_prefix(',').unwrap_or(after).to_string()
                };
                match mode {
                    Mode::Plain => pieces.push(Piece::Word {
                        text: format!("{}{}{keep}", span[0].lead, r.write),
                        comma: false,
                    }),
                    m => {
                        pieces.push(Piece::Sym {
                            write: r.write.clone(),
                            mode: m,
                        });
                        if !keep.is_empty() {
                            // Sentence punctuation after a symbol stays with it ("rename it to foo dot.").
                            pieces.push(Piece::Word {
                                text: keep,
                                comma: false,
                            });
                        }
                    }
                }
                i += n;
                continue 'next;
            }
        }
        let t = &toks[i];
        let word = format!("{}{}{}", t.lead, t.core, t.trail);
        pieces.push(Piece::Word {
            comma: !t.opaque && t.trail.ends_with(','),
            text: word,
        });
        i += 1;
    }
    assemble(pieces)
}

fn assemble(pieces: Vec<Piece>) -> String {
    let mut out = String::new();
    // The last thing written glues to what comes next (a joining symbol).
    let mut glued = true;
    let mut prev_word_comma = false;
    for (idx, p) in pieces.iter().enumerate() {
        match p {
            Piece::Word { text, comma } => {
                // A word that is only sentence punctuation sticks to what is before it.
                let punct_only = !text.chars().any(char::is_alphanumeric)
                    && text.chars().all(|c| ".,;:!?".contains(c));
                if !out.is_empty() && !glued && !punct_only {
                    out.push(' ');
                }
                // A comma before a joining symbol was a pause in "src, slash, main": it is not text.
                let before_join = matches!(
                    pieces.get(idx + 1),
                    Some(Piece::Sym {
                        mode: Mode::Join,
                        ..
                    })
                );
                if before_join && *comma && text.ends_with(',') {
                    out.push_str(&text[..text.len() - 1]);
                } else {
                    out.push_str(text);
                }
                glued = false;
                prev_word_comma = *comma;
            }
            Piece::Sym { write, mode } => {
                let _ = prev_word_comma;
                match mode {
                    Mode::Join => {
                        out.push_str(write);
                        glued = true;
                    }
                    Mode::Trailing => {
                        out.push_str(write);
                        glued = false;
                    }
                    Mode::Plain => unreachable!("a word, not a symbol"),
                }
            }
        }
    }
    out
}

// ── hints ────────────────────────────────────────────────────────────────────

/// What the engine is told to expect when `crewmate_id` dictates. Empty when there is nothing to say.
pub(crate) fn hints(store: &CrewStore, crewmate_id: &str) -> String {
    let mut parts: Vec<String> = vec![];
    let crewmate = store.load_crewmate(crewmate_id).ok();
    if let Some(c) = &crewmate {
        match &c.project {
            Some(ProjectRef::Git { url, branch }) => {
                if let Some(repo) = repo_name(url) {
                    parts.push(clean(&repo));
                }
                if let Some(b) = branch.as_deref().filter(|b| !b.is_empty()) {
                    parts.push(format!("branch {}", clean(b)));
                }
            }
            Some(ProjectRef::Path { path }) => {
                if let Some(n) = path
                    .trim_end_matches(['/', '\\'])
                    .rsplit(['/', '\\'])
                    .next()
                {
                    parts.push(clean(n));
                }
            }
            None => {}
        }
    }
    let mut names: Vec<String> = store
        .list_crewmates()
        .iter()
        .map(|c| clean(&c.name))
        .filter(|n| !n.is_empty())
        .collect();
    // This crewmate first: it is the one being talked to.
    if let Some(c) = &crewmate
        && let Some(at) = names.iter().position(|n| *n == clean(&c.name))
    {
        let me = names.remove(at);
        names.insert(0, me);
    }
    names.truncate(MAX_NAMES);
    if !names.is_empty() {
        parts.push(format!("crewmates {}", names.join(", ")));
    }
    // The newest task of THIS crewmate: its own prompt, title and result, never another's.
    let symbols = store
        .read_tasks(crewmate_id, 1)
        .first()
        .map(|t| {
            let mut found: Vec<String> = vec![];
            for text in [
                Some(t.prompt.as_str()),
                t.title.as_deref(),
                t.result.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                found.extend(symbols_of(
                    &text.chars().take(SCAN_CHARS).collect::<String>(),
                ));
            }
            dedupe(found)
        })
        .unwrap_or_default();
    if !symbols.is_empty() {
        parts.push(format!("files and symbols {}", symbols.join(", ")));
    }
    fit(parts)
}

/// `owner/repo(.git)` or a URL -> `repo`.
fn repo_name(url: &str) -> Option<String> {
    let last = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()?
        .trim_end_matches(".git");
    (!last.is_empty()).then(|| last.to_string())
}

/// One line of printable text: no control characters.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

fn dedupe(v: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    v.into_iter()
        .filter(|s| seen.insert(s.to_lowercase()))
        .take(MAX_SYMBOLS)
        .collect()
}

/// The files and code symbols that appear in `text`, in order: a path or file name (`src/main.rs`, `Cargo.toml`)
/// and an identifier that is plainly code (`foo_bar`, `fooBar`, `FooBar`, `foo::bar`).
pub(crate) fn symbols_of(text: &str) -> Vec<String> {
    let mut out = vec![];
    for raw in text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';'
            )
    }) {
        let tok = raw.trim_matches(|c: char| matches!(c, '.' | ':' | '!' | '?' | '*' | '#'));
        if tok.len() < 3 || tok.len() > 60 || is_url(tok) {
            continue;
        }
        if !tok
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | '-' | ':' | '+'))
        {
            continue;
        }
        let has_alpha = tok.chars().any(char::is_alphabetic);
        let ext = tok.rsplit_once('.').is_some_and(|(stem, e)| {
            !stem.is_empty()
                && (1..=5).contains(&e.len())
                && e.chars().all(|c| c.is_ascii_alphanumeric())
                && e.chars().any(|c| c.is_ascii_lowercase())
        });
        let path = tok.contains('/') && has_alpha && !tok.starts_with('/') || ext;
        let ident = has_alpha
            && (tok.contains('_')
                || tok.contains("::")
                || camel(tok) && tok.chars().all(|c| c.is_alphanumeric()));
        if path || ident {
            out.push(tok.to_string());
        }
    }
    out
}

/// `fooBar` or `FooBar`: a capital after the first letter, and a lowercase letter somewhere.
fn camel(tok: &str) -> bool {
    tok.chars().skip(1).any(char::is_uppercase) && tok.chars().any(char::is_lowercase)
}

/// Join `parts` as one sentence-ish prompt, dropping whole parts from the end rather than cutting a word.
fn fit(parts: Vec<String>) -> String {
    let mut out = String::new();
    for p in parts {
        let next = if out.is_empty() {
            p.clone()
        } else {
            format!("{out}. {p}")
        };
        if next.chars().count() > MAX_HINT_CHARS {
            // The last part may be partly kept: its items are comma separated.
            if let Some(cut) = next.char_indices().nth(MAX_HINT_CHARS).map(|(i, _)| i) {
                let kept = &next[..cut];
                if let Some(comma) = kept.rfind(", ") {
                    return kept[..comma].to_string();
                }
            }
            return out;
        }
        out = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Replacement> {
        effective(&[])
    }

    fn say(text: &str) -> String {
        apply(text, &all(), None)
    }

    // The sample of the issue: file names and symbols, said the way people say them.
    #[test]
    fn spoken_symbols_become_a_path_and_an_identifier() {
        assert_eq!(
            say("open src slash main dot rs and rename foo underscore bar"),
            "open src/main.rs and rename foo_bar"
        );
        assert_eq!(
            say("edit crates slash atlas dash host slash src slash lib dot rs"),
            "edit crates/atlas-host/src/lib.rs"
        );
        assert_eq!(say("call Foo double colon bar"), "call Foo::bar");
        assert_eq!(
            say("mail dev at sign example dot com"),
            "mail dev@example.com"
        );
    }

    #[test]
    fn portuguese_is_spoken_the_same_way() {
        assert_eq!(
            say("abra src barra main ponto rs e renomeie foo underline bar"),
            "abra src/main.rs e renomeie foo_bar"
        );
        // The longest phrase wins: "barra invertida" is a backslash, not a slash after "invertida".
        assert_eq!(say("pasta barra invertida temp"), "pasta\\temp");
        assert_eq!(say("nome arroba site ponto com"), "nome@site.com");
        assert_eq!(say("Traço traço"), "--");
    }

    // Engines punctuate what they hear: "Src slash main dot RS." still reads as the path.
    #[test]
    fn the_engines_own_punctuation_and_case_do_not_break_a_phrase() {
        assert_eq!(say("Src slash main dot RS."), "Src/main.RS.");
        assert_eq!(say("src, slash, main, dot, rs."), "src/main.rs.");
        assert_eq!(say("Open Src Slash Main"), "Open Src/Main");
    }

    #[test]
    fn a_word_only_counts_when_it_stands_alone() {
        assert_eq!(
            say("the slashes and dotted lines, dashboard"),
            "the slashes and dotted lines, dashboard"
        );
        assert_eq!(say("Underscored slasher"), "Underscored slasher");
    }

    // Incident class: a rewrite inside a URL breaks the link it was meant to help with.
    #[test]
    fn a_url_is_never_rewritten() {
        let url = "https://example.com/dot/slash/underscore.html";
        assert_eq!(
            say(&format!("see {url} then open src slash main")),
            format!("see {url} then open src/main")
        );
        assert_eq!(
            say("go to www.dot.com slash docs"),
            "go to www.dot.com/docs"
        );
        let ssh = "ssh://git@host/dash/dot";
        assert_eq!(say(ssh), ssh);
    }

    #[test]
    fn a_colon_attaches_to_the_word_before_it() {
        assert_eq!(say("note colon rename it"), "note: rename it");
        assert_eq!(
            say("http colon slash slash example dot com"),
            "http://example.com"
        );
    }

    #[test]
    fn text_without_a_spoken_symbol_comes_back_unchanged() {
        for s in [
            "",
            "   ",
            "Rename the function and run the tests.",
            "Olá, tudo bem? Vamos lá!",
            "-",
            "...",
        ] {
            assert_eq!(say(s), s.trim(), "{s:?}");
        }
    }

    #[test]
    fn spacing_is_normalised_but_nothing_else_is_lost() {
        assert_eq!(say("  open   src  slash  main "), "open src/main");
        assert_eq!(say("Run it. Then slash"), "Run it. Then/");
    }

    // The language a person was asked to speak limits the rows: Portuguese "ponto" is a dot only in pt.
    #[test]
    fn a_forced_language_applies_only_its_rows() {
        let t = all();
        assert_eq!(apply("um ponto dot", &t, Some("pt")), "um.dot");
        assert_eq!(apply("um ponto dot", &t, Some("en")), "um ponto.");
        assert_eq!(apply("um ponto dot", &t, None), "um..");
    }

    #[test]
    fn the_crew_extends_the_table_and_may_override_a_default() {
        let custom = vec![
            row("hash", "#", "any"),
            row("slash", " / ", "en"),
            row("open paren", "(", "en"),
            row("arrow", "->", "any"),
        ];
        let t = effective(&custom);
        assert_eq!(apply("use hash foo", &t, None), "use#foo");
        assert_eq!(apply("a arrow b", &t, None), "a->b");
        // The override replaces the default of the same words and language; the default is not also applied.
        assert_eq!(
            t.iter()
                .filter(|r| key(&r.say) == "slash" && r.lang == "en")
                .count(),
            1
        );
        // A replacement with letters is a word, spaced like one.
        let words = effective(&[row("rust analyzer", "rust-analyzer", "any")]);
        assert_eq!(
            apply("install Rust Analyzer now", &words, None),
            "install rust-analyzer now"
        );
    }

    #[test]
    fn rows_are_validated() {
        assert!(validate(&[row("slash", "/", "en")]).is_ok());
        assert!(validate(&[row("", "/", "en")]).is_err());
        assert!(validate(&[row("a b c d e", "/", "en")]).is_err());
        assert!(validate(&[row("x", "", "en")]).is_err());
        assert!(validate(&[row("x", "y", "fr")]).is_err());
        assert!(validate(&[row("x\n", "y", "en")]).is_err());
        let many: Vec<_> = (0..=MAX_ROWS)
            .map(|i| row(&format!("w{i}"), "x", "any"))
            .collect();
        assert!(validate(&many).is_err());
    }

    #[test]
    fn the_defaults_name_every_language_and_never_collide() {
        let d = defaults();
        for lang in ["en", "pt"] {
            assert!(d.iter().any(|r| r.lang == lang));
        }
        let mut seen = HashSet::new();
        for r in &d {
            assert!(
                seen.insert((key(&r.say), r.lang.clone())),
                "{} twice",
                r.say
            );
        }
    }

    // ── symbols ──

    #[test]
    fn files_and_symbols_are_picked_out_of_a_task() {
        let found = symbols_of(
            "Fix `parse_config` in src/config/mod.rs, rename FooBar to fooBar, see Cargo.toml and \
             https://example.com/a_b.rs. Also std::mem::take, the word hello, and 2026.",
        );
        for want in [
            "parse_config",
            "src/config/mod.rs",
            "FooBar",
            "fooBar",
            "Cargo.toml",
            "std::mem::take",
        ] {
            assert!(found.contains(&want.to_string()), "{want} in {found:?}");
        }
        for not in ["hello", "2026", "https://example.com/a_b.rs", "Fix", "the"] {
            assert!(!found.contains(&not.to_string()), "{not} in {found:?}");
        }
    }

    #[test]
    fn a_long_hint_loses_whole_items_not_half_a_word() {
        let parts = vec![
            "atlas".to_string(),
            format!(
                "files and symbols {}",
                (0..80)
                    .map(|i| format!("sym_{i:03}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ];
        let h = fit(parts);
        assert!(h.chars().count() <= MAX_HINT_CHARS, "{}", h.len());
        assert!(h.starts_with("atlas. files and symbols sym_000"));
        assert!(h.ends_with(|c: char| c.is_ascii_digit()), "{h}");
        let last = h.rsplit(", ").next().unwrap();
        assert_eq!(last.len(), "sym_000".len(), "a whole item: {last}");
    }
}
