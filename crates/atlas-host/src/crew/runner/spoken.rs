//! Spoken replies (#648): what a client reads aloud of a finished task, as `Task::spoken`.
//!
//! The agent of a crewmate with `spoken_replies` on is asked, in one line of its instructions, to end the reply
//! with `spoken: <one or two short sentences>`. hostd takes the LAST such line (case-insensitive, at the start of
//! a line, at most [`MAX`] characters) as the summary and removes it from every visible copy of the reply
//! ([`super::action::visible_reply`], the stored `reply`). A reply with no such line is spoken from its first
//! sentences ([`first_sentences`]): code blocks, inline code, URLs, markdown marks and file paths are dropped,
//! and a reply that is only code gives "". The voice itself is the device's.

use crate::crew::store::Crewmate;

/// The longest summary (characters).
pub(crate) const MAX: usize = 300;

const PREFIX: &str = "spoken:";

/// The one line added to the turn's instructions while `spoken_replies` is on; empty otherwise.
pub(crate) fn instruction(c: &Crewmate) -> String {
    if c.spoken_replies {
        "[Atlas] End your reply with one line `spoken: <one or two short sentences for listening, no code, no \
         paths, no markdown>`.\n\n"
            .into()
    } else {
        String::new()
    }
}

/// The text of a `spoken:` line (any case, at the start of the line), `None` for any other line.
fn line_text(line: &str) -> Option<&str> {
    let l = line.trim_start();
    let head = l.get(..PREFIX.len())?;
    head.eq_ignore_ascii_case(PREFIX)
        .then(|| l[PREFIX.len()..].trim())
}

/// Whether `line` is a `spoken:` line (the visible reply drops it).
pub(crate) fn is_line(line: &str) -> bool {
    line_text(line).is_some()
}

/// The agent's summary: its last non-empty `spoken:` line, at most [`MAX`] characters.
pub(crate) fn agent_line(reply: &str) -> Option<String> {
    reply
        .lines()
        .rev()
        .filter_map(line_text)
        .find(|t| !t.is_empty())
        .map(|t| clip(t, MAX))
}

/// `reply` without its `spoken:` lines.
pub(crate) fn without_line(reply: &str) -> String {
    reply
        .lines()
        .filter(|l| !is_line(l))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// `s` cut to at most `max` characters, at a word when it has to be cut.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    match cut.rsplit_once(char::is_whitespace) {
        Some((head, _)) if !head.trim().is_empty() => head.trim_end().to_string(),
        _ => cut,
    }
}

/// Code blocks (fenced), and everything a person would not want read aloud, removed; one line of plain text per
/// line of the reply.
fn plain_lines(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut fenced = false;
    for raw in text.lines() {
        let t = raw.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let line = plain(t);
        if !line.is_empty() {
            out.push(line);
        }
    }
    out
}

/// One line without its markdown marks, inline code, links' targets, URLs and file paths.
fn plain(line: &str) -> String {
    // Leading marks: headings, quotes, bullets, numbers.
    let mut s = line.trim_start_matches(['#', '>', ' ', '\t']).to_string();
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = s.strip_prefix(bullet) {
            s = rest.to_string();
        }
    }
    // Inline code: the span goes, ticks and all.
    let mut out = String::new();
    let mut in_code = false;
    for c in s.chars() {
        if c == '`' {
            in_code = !in_code;
        } else if !in_code {
            out.push(c);
        }
    }
    // `[text](target)`: the text stays.
    let mut link_free = String::new();
    let mut rest = out.as_str();
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        match after
            .find("](")
            .and_then(|mid| after[mid + 2..].find(')').map(|end| (mid, mid + 2 + end)))
        {
            Some((mid, end)) => {
                link_free.push_str(&rest[..open]);
                link_free.push_str(&after[..mid]);
                rest = &after[end + 1..];
            }
            None => {
                link_free.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    link_free.push_str(rest);
    let words: Vec<String> = link_free
        .split_whitespace()
        .filter(|w| !spoken_unfit(w))
        .map(|w| w.trim_matches(['*', '_', '~']).to_string())
        .filter(|w| !w.is_empty())
        .collect();
    words.join(" ")
}

/// A word that is a URL or a file path: it is not read aloud.
fn spoken_unfit(word: &str) -> bool {
    const EXT: [&str; 24] = [
        "rs", "ts", "tsx", "js", "jsx", "mjs", "json", "md", "toml", "yaml", "yml", "py", "go",
        "java", "c", "h", "cpp", "svelte", "css", "html", "sh", "lock", "txt", "dart",
    ];
    let w =
        word.trim_matches(|c: char| !c.is_alphanumeric() && !matches!(c, '/' | '~' | '.' | '\\'));
    if w.contains("://") || w.starts_with("www.") {
        return true;
    }
    if w.contains('/') || w.contains('\\') || w.starts_with("~") {
        // "and/or" and "24/7" are words; a path has a name part with a dot or a folder start.
        let slashes = w.matches(['/', '\\']).count();
        return slashes > 1
            || w.starts_with(['/', '.', '~'])
            || w.rsplit(['/', '\\'])
                .next()
                .is_some_and(|f| f.contains('.'));
    }
    w.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && EXT.contains(&ext.to_ascii_lowercase().as_str())
    })
}

/// The first sentences of `visible`, cleaned for listening, at most [`MAX`] characters.
pub(crate) fn first_sentences(visible: &str) -> String {
    let mut out = String::new();
    'lines: for line in plain_lines(visible) {
        for sentence in sentences(&line) {
            let next = if out.is_empty() {
                sentence.to_string()
            } else {
                format!("{out} {sentence}")
            };
            if next.chars().count() > MAX {
                if out.is_empty() {
                    out = clip(&next, MAX);
                }
                break 'lines;
            }
            out = next;
        }
    }
    out
}

/// A line split after `.`, `!` or `?` that is followed by a space (so `3.5` and `v1.2` stay whole).
fn sentences(line: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    let bytes: Vec<(usize, char)> = line.char_indices().collect();
    for (i, (pos, c)) in bytes.iter().enumerate() {
        let end = pos + c.len_utf8();
        let ends =
            matches!(c, '.' | '!' | '?') && bytes.get(i + 1).is_none_or(|(_, n)| n.is_whitespace());
        if ends {
            let s = line[start..end].trim();
            if !s.is_empty() {
                out.push(s);
            }
            start = end;
        }
    }
    let tail = line[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_spoken_line_wins_in_any_case_and_leaves_the_visible_reply() {
        let reply = "Fixed the bug in `src/main.rs`.\n\nSPOKEN: old one\nMore detail.\n  Spoken:   I fixed the bug. Tests pass.  \n";
        assert_eq!(
            agent_line(reply).as_deref(),
            Some("I fixed the bug. Tests pass.")
        );
        assert_eq!(
            without_line(reply),
            "Fixed the bug in `src/main.rs`.\n\nMore detail."
        );
        // Only at the start of a line: "not spoken:" mid-line is part of the reply.
        let inline = "He said spoken: hello";
        assert_eq!(agent_line(inline), None);
        assert_eq!(without_line(inline), inline);
        // An empty summary is no summary; the line is still not for people.
        assert_eq!(agent_line("Done.\nspoken:"), None);
        assert_eq!(without_line("Done.\nspoken:"), "Done.");
    }

    #[test]
    fn the_agents_line_is_cut_at_300_characters_on_a_word() {
        let long = "word ".repeat(100);
        let said = agent_line(&format!("x\nspoken: {long}")).unwrap();
        assert!(said.chars().count() <= MAX, "{}", said.chars().count());
        assert!(said.ends_with("word"), "{said}");
        let one_word = "a".repeat(400);
        assert_eq!(
            agent_line(&format!("spoken: {one_word}"))
                .unwrap()
                .chars()
                .count(),
            MAX
        );
    }

    #[test]
    fn without_a_line_the_first_sentences_are_spoken_without_code_urls_paths_or_marks() {
        let reply = "## Result\n\nI renamed **foo_bar** to `baz` in src/lib/a.rs and https://example.com/x. All 12 tests pass.\n\n```rust\nfn main() {}\n```\n\n- later detail one\n- later detail two";
        assert_eq!(
            first_sentences(reply),
            "Result I renamed foo_bar to in and All 12 tests pass. later detail one later detail two"
        );
    }

    #[test]
    fn a_reply_that_is_only_code_gives_nothing() {
        assert_eq!(first_sentences("```\nlet x = 1;\n```"), "");
        assert_eq!(first_sentences("`cargo test`"), "");
        assert_eq!(first_sentences("src/main.rs"), "");
        assert_eq!(first_sentences(""), "");
    }

    #[test]
    fn first_sentences_stop_before_300_characters_and_a_long_first_one_is_cut() {
        let sentence = "This is a sentence of about forty chars.";
        let many = vec![sentence; 20].join(" ");
        let said = first_sentences(&many);
        assert!(said.chars().count() <= MAX, "{}", said.chars().count());
        assert!(said.ends_with('.'), "whole sentences only: {said}");
        let one = format!("{} end.", "long ".repeat(100));
        let said = first_sentences(&one);
        assert!(said.chars().count() <= MAX);
        assert!(!said.is_empty());
    }

    #[test]
    fn numbers_versions_links_and_alternatives_survive() {
        assert_eq!(
            first_sentences("Version 1.5 is out, see [the notes](https://x.dev/n) and/or wait."),
            "Version 1.5 is out, see the notes and/or wait."
        );
        assert_eq!(
            first_sentences("Pronto! Tudo certo?"),
            "Pronto! Tudo certo?"
        );
    }

    #[test]
    fn the_instruction_exists_only_while_the_flag_is_on() {
        let mut c: Crewmate = serde_json::from_value(serde_json::json!({
            "id": "c", "name": "n", "role": "r", "agent": "omp", "mode": "ask", "created_at": 0
        }))
        .unwrap();
        assert_eq!(instruction(&c), "");
        c.spoken_replies = true;
        let i = instruction(&c);
        assert!(i.contains("spoken:") && i.ends_with("\n\n"), "{i}");
        assert_eq!(i.matches('\n').count(), 2, "one line: {i}");
    }
}
