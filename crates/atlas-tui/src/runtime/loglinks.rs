//! Clickable things inside a log line: URLs and `path:line[:col]` references
//! (compiler errors, stack traces, test failures). Pure text scanning over the
//! visible text of one line; results are byte ranges into that text.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Url(String),
    /// `line`/`col` are 1-based.
    Path {
        path: String,
        line: u32,
        col: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub range: Range<usize>,
    pub target: Target,
}

static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\b(?:https?|file)://[^\s<>"'`\x00-\x1f]+"#).expect("url"));

/// `src/a.rs:12`, `./x/y.ts:3:9`, `/abs/p.go:7`, `C:\x.rs:1` is not supported
/// (POSIX paths only), `file.py:10` (a name with an extension). `(file:line)`
/// and `at file:line:col` wrappers are handled by the trailing trim.
static PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        (?:^|[\s(\[{<'`\x22=])
        (?P<p>
            (?:\.{0,2}/)?                      # optional ./ ../ /
            (?:[A-Za-z0-9_@~.+-]+/)*           # directories
            [A-Za-z0-9_@+-][A-Za-z0-9_@.+-]*   # file name
            \.[A-Za-z0-9]{1,8}                 # extension (letter required, checked below)
        )
        :(?P<l>[0-9]{1,7})
        (?::(?P<c>[0-9]{1,5}))?
        ",
    )
    .expect("path")
});

/// URLs and path references in `text`, left to right, non-overlapping.
pub fn find(text: &str) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    for m in URL_RE.find_iter(text) {
        let end = m.start() + trim_url(m.as_str()).len();
        // Nothing left after the scheme: not a link.
        if text[m.start()..end]
            .split_once("://")
            .is_none_or(|(_, rest)| rest.is_empty())
        {
            continue;
        }
        out.push(Link {
            range: m.start()..end,
            target: Target::Url(text[m.start()..end].to_string()),
        });
    }
    for c in PATH_RE.captures_iter(text) {
        let (Some(p), Some(l)) = (c.name("p"), c.name("l")) else {
            continue;
        };
        let range = p.start()..c.get(0).map_or(p.end(), |m| m.end());
        // A path inside a URL is part of the URL.
        if out
            .iter()
            .any(|u| u.range.start < range.end && range.start < u.range.end)
        {
            continue;
        }
        // `1.2:3` / `10.0.0.1:80` are numbers, not files: the extension needs a letter.
        let ext = p.as_str().rsplit('.').next().unwrap_or("");
        if !ext.bytes().any(|b| b.is_ascii_alphabetic()) {
            continue;
        }
        let Ok(line) = l.as_str().parse::<u32>() else {
            continue;
        };
        if line == 0 {
            continue;
        }
        out.push(Link {
            range,
            target: Target::Path {
                path: p.as_str().to_string(),
                line,
                col: c.name("c").and_then(|m| m.as_str().parse().ok()),
            },
        });
    }
    out.sort_by_key(|l| l.range.start);
    out
}

/// `url` without trailing sentence punctuation and closing brackets that have
/// no opener inside the URL (`Rust_(language).` keeps its `)`, drops the `.`).
fn trim_url(url: &str) -> &str {
    let mut end = url.len();
    while let Some(c) = url[..end].chars().next_back() {
        let open = match c {
            ')' => '(',
            ']' => '[',
            '}' => '{',
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' => '\0',
            _ => break,
        };
        if open != '\0' {
            let body = &url[..end];
            if body.matches(open).count() >= body.matches(c).count() {
                break;
            }
        }
        end -= c.len_utf8();
    }
    &url[..end]
}

/// Resolve a detected path against the project root; `None` when it does not
/// exist (so `12:30:45` or `v1.2:3` never opens anything).
pub fn resolve(root: &Path, path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    };
    full.is_file().then_some(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(text: &str) -> Vec<(String, u32, Option<u32>, &str)> {
        find(text)
            .into_iter()
            .filter_map(|l| match l.target {
                Target::Path { path, line, col } => Some((path, line, col, &text[l.range])),
                Target::Url(_) => None,
            })
            .collect()
    }

    fn urls(text: &str) -> Vec<String> {
        find(text)
            .into_iter()
            .filter_map(|l| match l.target {
                Target::Url(u) => Some(u),
                Target::Path { .. } => None,
            })
            .collect()
    }

    #[test]
    fn rustc_node_python_go_references() {
        assert_eq!(
            paths("error[E0308]: mismatched types\n --> src/main.rs:12:5"),
            [("src/main.rs".to_string(), 12, Some(5), "src/main.rs:12:5")]
        );
        assert_eq!(
            paths("    at foo (/app/src/a.ts:30:9)"),
            [(
                "/app/src/a.ts".to_string(),
                30,
                Some(9),
                "/app/src/a.ts:30:9"
            )]
        );
        assert_eq!(
            paths("File \"x.py\", line 3 in main  see util.py:10"),
            [("util.py".to_string(), 10, None, "util.py:10")]
        );
        assert_eq!(
            paths("./lib/a-b.test.js:7: failed"),
            [(
                "./lib/a-b.test.js".to_string(),
                7,
                None,
                "./lib/a-b.test.js:7"
            )]
        );
    }

    #[test]
    fn not_a_path_reference() {
        assert!(paths("12:30:45 started").is_empty());
        assert!(paths("listening on localhost:3000").is_empty());
        assert!(paths("version 1.2:3 ok").is_empty());
        assert!(paths("file.rs:0").is_empty(), "line 0 is invalid");
        assert!(paths("noext:12").is_empty());
    }

    #[test]
    fn urls_trim_trailing_punctuation_and_unbalanced_parens() {
        assert_eq!(urls("see https://a.test/x, then"), ["https://a.test/x"]);
        assert_eq!(urls("(https://a.test/x)"), ["https://a.test/x"]);
        assert_eq!(
            urls("https://en.wikipedia.org/wiki/Rust_(language)."),
            ["https://en.wikipedia.org/wiki/Rust_(language)"]
        );
        assert_eq!(
            urls("http://localhost:3000/api?a=1&b=2"),
            ["http://localhost:3000/api?a=1&b=2"]
        );
        assert_eq!(urls("\"http://x.test/p\""), ["http://x.test/p"]);
    }

    #[test]
    fn path_inside_url_is_part_of_the_url() {
        let l = find("GET http://h.test/a/b.js:12 200");
        assert_eq!(l.len(), 1);
        assert!(matches!(l[0].target, Target::Url(_)));
    }

    #[test]
    fn resolve_only_existing_files() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/a.rs"), "x").unwrap();
        assert_eq!(
            resolve(d.path(), "src/a.rs"),
            Some(d.path().join("src/a.rs"))
        );
        assert!(resolve(d.path(), "src/missing.rs").is_none());
        assert!(
            resolve(d.path(), "src").is_none(),
            "directories don't count"
        );
        let abs = d.path().join("src/a.rs");
        assert_eq!(
            resolve(Path::new("/nowhere"), abs.to_str().unwrap()),
            Some(abs)
        );
    }
}
