//! Paths as git prints them. With `core.quotepath` on (the default) git wraps a path that has a non-ASCII letter in
//! double quotes and writes each byte of it as an octal escape (`"a/a\303\247\303\243o.txt"`); a `"`, a `\`, a tab, a
//! newline or any other control character is quoted whatever the setting. `-z` output (`status`, `ls-files`,
//! `--name-only`, `--numstat`) is never quoted, but a patch has no `-z` form: the `diff --git` header and the
//! `rename from` / `copy from` lines of every diff Atlas parses go through this module.

/// A path as git printed it, with its C-style quoting removed: `"a/\303\247.txt"` is `a/ç.txt`. A string that is
/// not wrapped in double quotes is a plain path and comes back as it is. An unknown escape keeps its backslash.
pub fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) else {
        return s.to_string();
    };
    let b = inner.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' || i + 1 == b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let c = b[i + 1];
        let simple = match c {
            b'a' => Some(0x07),
            b'b' => Some(0x08),
            b'f' => Some(0x0c),
            b'n' => Some(b'\n'),
            b'r' => Some(b'\r'),
            b't' => Some(b'\t'),
            b'v' => Some(0x0b),
            b'\\' | b'"' => Some(c),
            _ => None,
        };
        if let Some(byte) = simple {
            out.push(byte);
            i += 2;
        } else if matches!(c, b'0'..=b'3')
            && b.get(i + 2).is_some_and(|d| matches!(d, b'0'..=b'7'))
            && b.get(i + 3).is_some_and(|d| matches!(d, b'0'..=b'7'))
        {
            out.push((c - b'0') << 6 | (b[i + 2] - b'0') << 3 | (b[i + 3] - b'0'));
            i += 4;
        } else {
            out.push(b'\\');
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Where the quoted token at the start of `s` ends (just after its closing quote).
fn quoted_end(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// `(old, new)` of the text after `diff --git `: `a/<old> b/<new>`, each half quoted or not (git quotes them one by
/// one), with the `a/` and `b/` prefixes taken off. The prefixes are the ones the callers force with
/// `--src-prefix=a/ --dst-prefix=b/`. Both halves are the same unless the file was renamed; with spaces in a plain
/// name the halves are found by their ` b/` marker, taking the one that makes the two sides symmetric first.
pub fn diff_header(rest: &str) -> (String, String) {
    let strip = |p: String, prefix: &str| p.strip_prefix(prefix).map_or(p.clone(), str::to_string);
    if rest.starts_with('"') {
        if let Some(end) = quoted_end(rest) {
            return (
                strip(unquote(&rest[..end]), "a/"),
                strip(unquote(rest[end..].trim_start()), "b/"),
            );
        }
    } else if let Some(i) = rest.find(" \"") {
        // A plain name never has a quote in it, so the first one starts the quoted half.
        return (
            strip(rest[..i].to_string(), "a/"),
            strip(unquote(&rest[i + 1..]), "b/"),
        );
    }
    let body = rest.strip_prefix("a/").unwrap_or(rest);
    let mut at = 0;
    while let Some(i) = body[at..].find(" b/") {
        let split = at + i;
        let (old, new) = (&body[..split], &body[split + 3..]);
        if old == new {
            return (old.to_string(), new.to_string());
        }
        at = split + 1;
    }
    // A rename: the new path is after the last ` b/`.
    match body.rsplit_once(" b/") {
        Some((old, new)) => (old.to_string(), new.to_string()),
        None => (body.to_string(), body.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_review::{
        REVIEW_DIFF, STAGED_CHANGES, new_file_args, parse, parse_status, split_files,
    };
    use std::path::Path;
    use std::process::Command;

    // ── the pure functions, on what git printed (copied from a real repo) ──────────────────────────────────────

    #[test]
    fn unquote_decodes_octal_utf8_and_the_named_escapes() {
        assert_eq!(unquote(r#""a\303\247\303\243o.txt""#), "ação.txt");
        assert_eq!(unquote(r#""\346\227\245\346\234\254.txt""#), "日本.txt");
        assert_eq!(unquote(r#""tab\there.txt""#), "tab\there.txt");
        assert_eq!(unquote(r#""new\nline.txt""#), "new\nline.txt");
        assert_eq!(
            unquote(r#""name with \"quote\".txt""#),
            "name with \"quote\".txt"
        );
        assert_eq!(unquote(r#""back\\slash""#), "back\\slash");
        assert_eq!(unquote(r#""\a\b\f\r\v""#), "\x07\x08\x0c\r\x0b");
    }

    #[test]
    fn unquote_leaves_a_plain_path_and_a_half_quoted_one_alone() {
        assert_eq!(unquote("src/a.rs"), "src/a.rs");
        assert_eq!(unquote("ação.txt"), "ação.txt");
        // Quotes in the middle of a plain path are part of its name, not quoting.
        assert_eq!(unquote(r#"a"b"#), r#"a"b"#);
        assert_eq!(unquote(r#""unterminated"#), r#""unterminated"#);
        assert_eq!(unquote(""), "");
        assert_eq!(unquote("\""), "\"");
    }

    #[test]
    fn unquote_keeps_what_it_does_not_understand() {
        assert_eq!(unquote(r#""a\qb""#), r"a\qb");
        assert_eq!(unquote(r#""a\9b""#), r"a\9b");
        assert_eq!(unquote(r#""a\30""#), r"a\30");
        // A backslash that is the last character before the closing quote.
        assert_eq!(unquote(r#""a\""#), r"a\");
    }

    #[test]
    fn diff_header_reads_every_shape_git_prints() {
        let h = |s: &str| diff_header(s);
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(h("a/src/a.rs b/src/a.rs"), pair("src/a.rs", "src/a.rs"));
        assert_eq!(
            h("a/my file.txt b/my file.txt"),
            pair("my file.txt", "my file.txt")
        );
        assert_eq!(h("a/old.txt b/new.txt"), pair("old.txt", "new.txt"));
        assert_eq!(
            h(r#""a/a\303\247\303\243o.txt" "b/a\303\247\303\243o.txt""#),
            pair("ação.txt", "ação.txt")
        );
        // Renamed from a plain name to a quoted one and back: git quotes each half on its own.
        assert_eq!(
            h(r#"a/old name.txt "b/nova a\303\247\303\243o.txt""#),
            pair("old name.txt", "nova ação.txt")
        );
        assert_eq!(
            h(r#""a/nova a\303\247\303\243o.txt" b/old name.txt"#),
            pair("nova ação.txt", "old name.txt")
        );
        // A quote inside a quoted half does not end it.
        assert_eq!(
            h(r#""a/x \"b\" y.txt" "b/x \"b\" y.txt""#),
            pair("x \"b\" y.txt", "x \"b\" y.txt")
        );
    }

    // ── real git, in a temp repo, through every parser of its output ──────────────────────────────────────────

    /// The names the bug report listed, plus a folder with a space and a letter outside ASCII. The ones the
    /// file system of Windows refuses (`"`, tab, newline, `\`) only run on Unix.
    fn names() -> Vec<&'static str> {
        let mut v = vec![
            "ação.txt",
            "configuração.md",
            "日本.txt",
            "pasta ação/sub dir/é.rs",
        ];
        if cfg!(unix) {
            v.extend([
                "name with \"quote\".txt",
                "tab\there.txt",
                "new\nline.txt",
                "back\\slash.txt",
            ]);
        }
        v
    }

    /// `git -C dir -c core.quotepath=<q> <args>`: the setting is pinned so the machine's own config decides nothing.
    fn git(dir: &Path, quotepath: bool, args: &[&str]) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                &format!("core.quotepath={quotepath}"),
                "-c",
                "user.email=a@b",
                "-c",
                "user.name=n",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            o.status.success() || o.status.code() == Some(1),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    const BODY: &str = "line one\nline two\nline three\nline four\nline five\n";

    /// A repo whose first commit has `files`, each with [`BODY`].
    fn repo_with(files: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), true, &["init", "-q", "."]);
        write(d.path(), "README.md", "hello\n");
        for f in files {
            write(d.path(), f, BODY);
        }
        git(d.path(), true, &["add", "-A"]);
        git(d.path(), true, &["commit", "-qm", "init"]);
        d
    }

    fn sorted(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v
    }

    // Incident (batch S09): the Changes view headed a file `"a/a\303\247\303\243o.txt" "b/a\303\247\303\243o.txt"`
    // because the parser kept git's quoting. Every file is named exactly as it is on disk, with git's quoting on
    // (the default) and off (`quotepath=false` still quotes `"`, tab, newline and `\`).
    #[test]
    fn the_changes_view_names_every_file_as_it_is_on_disk() {
        for quotepath in [true, false] {
            let d = repo_with(&names());
            for n in names() {
                write(d.path(), n, &format!("{BODY}changed\n"));
            }
            let args = &REVIEW_DIFF;
            let files = parse(&git(d.path(), quotepath, args), false);
            let got = sorted(files.iter().map(|f| f.path.clone()).collect());
            assert_eq!(
                got,
                sorted(names().iter().map(|n| n.to_string()).collect()),
                "quotepath={quotepath}"
            );
            for f in &files {
                assert_eq!(f.status, "modified", "{}", f.path);
                assert_eq!(f.hunks.len(), 1, "{}", f.path);
                assert!(f.old_path.is_none(), "{}", f.path);
            }
        }
    }

    #[test]
    fn an_added_and_a_deleted_file_are_named_as_they_are_on_disk() {
        for quotepath in [true, false] {
            let d = repo_with(&names());
            for n in names() {
                std::fs::remove_file(d.path().join(n)).unwrap();
            }
            git(d.path(), quotepath, &["add", "-A"]);
            let staged = [&REVIEW_DIFF[..], &["--cached"]].concat();
            let files = parse(&git(d.path(), quotepath, &staged), true);
            let want = sorted(names().iter().map(|n| n.to_string()).collect());
            assert_eq!(
                sorted(files.iter().map(|f| f.path.clone()).collect()),
                want,
                "quotepath={quotepath}"
            );
            assert!(
                files.iter().all(|f| f.status == "deleted"),
                "quotepath={quotepath}"
            );

            git(d.path(), quotepath, &["commit", "-qm", "gone"]);
            for n in names() {
                write(d.path(), n, BODY);
            }
            git(d.path(), quotepath, &["add", "-A"]);
            let files = parse(&git(d.path(), quotepath, &staged), true);
            assert_eq!(
                sorted(files.iter().map(|f| f.path.clone()).collect()),
                want,
                "quotepath={quotepath}"
            );
            assert!(
                files.iter().all(|f| f.status == "added"),
                "quotepath={quotepath}"
            );
        }
    }

    /// A repo whose first commit has each of `files` with a body of its own (git pairs a rename by content, so two
    /// files with one body would be paired either way round).
    fn repo_with_distinct(files: &[String]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), true, &["init", "-q", "."]);
        write(d.path(), "README.md", "hello\n");
        for (i, f) in files.iter().enumerate() {
            write(d.path(), f, &format!("{BODY}unique {i}\n"));
        }
        git(d.path(), true, &["add", "-A"]);
        git(d.path(), true, &["commit", "-qm", "init"]);
        d
    }

    /// Move `from` to `to` on disk (making the folders) and stage it all.
    fn move_all(d: &Path, moves: &[(String, String)]) {
        for (from, to) in moves {
            std::fs::create_dir_all(d.join(to).parent().unwrap()).unwrap();
            std::fs::rename(d.join(from), d.join(to)).unwrap();
        }
        git(d, true, &["add", "-A"]);
    }

    // `rename from` / `rename to` quote like the header, and each side on its own: a plain name renamed to a quoted
    // one, and the other way round. Every name of one scenario is renamed in one repo and read by one diff per
    // quotepath setting: a repo and a `git mv` per name was 144 processes, 17 s on a Windows runner.
    #[test]
    fn a_rename_to_or_from_such_a_name_keeps_both_names() {
        let staged = [&REVIEW_DIFF[..], &["--cached"]].concat();
        let ns = names();
        let plain_old = |i: usize| format!("plain old {i}.txt");
        let plain_new = |i: usize| format!("plain new {i}.txt");
        let other = |i: usize| format!("nova ç {i}.txt");

        // plain -> name
        let d = repo_with_distinct(&(0..ns.len()).map(plain_old).collect::<Vec<_>>());
        let moves: Vec<_> = ns
            .iter()
            .enumerate()
            .map(|(i, n)| (plain_old(i), n.to_string()))
            .collect();
        move_all(d.path(), &moves);
        for quotepath in [true, false] {
            let f = parse(&git(d.path(), quotepath, &staged), true);
            assert_eq!(f.len(), ns.len(), "quotepath={quotepath}");
            for (i, n) in ns.iter().enumerate() {
                let one = f
                    .iter()
                    .find(|x| x.path == *n)
                    .unwrap_or_else(|| panic!("{n:?} quotepath={quotepath}: {f:?}"));
                assert_eq!(one.old_path.as_deref(), Some(plain_old(i).as_str()), "{n:?}");
                assert_eq!(one.status, "renamed", "{n:?}");
            }
        }

        // name -> plain, and name -> another name
        for (to, what) in [(&plain_new as &dyn Fn(usize) -> String, "plain"), (&other, "another name")] {
            let d = repo_with_distinct(&ns.iter().map(|n| n.to_string()).collect::<Vec<_>>());
            let moves: Vec<_> = ns
                .iter()
                .enumerate()
                .map(|(i, n)| (n.to_string(), to(i)))
                .collect();
            move_all(d.path(), &moves);
            for quotepath in [true, false] {
                let f = parse(&git(d.path(), quotepath, &staged), true);
                assert_eq!(f.len(), ns.len(), "{what} quotepath={quotepath}");
                for (i, n) in ns.iter().enumerate() {
                    let one = f
                        .iter()
                        .find(|x| x.path == to(i))
                        .unwrap_or_else(|| panic!("{what} {n:?} quotepath={quotepath}: {f:?}"));
                    assert_eq!(one.old_path.as_deref(), Some(*n), "{what} quotepath={quotepath}");
                    assert_eq!(one.status, "renamed", "{what} {n:?}");
                }
            }
        }
    }

    // The commit message is written from `diff --cached`: the sections are filed under the real names, so the
    // secret filter and the per-file budget see the name the user sees.
    #[test]
    fn the_commit_message_digest_splits_sections_under_the_real_names() {
        for quotepath in [true, false] {
            let d = repo_with(&["plain old.txt"]);
            for n in names() {
                write(d.path(), n, "text\n");
            }
            git(
                d.path(),
                quotepath,
                &["mv", "plain old.txt", "movido ação.txt"],
            );
            git(d.path(), quotepath, &["add", "-A"]);
            let diff = git(d.path(), quotepath, &STAGED_CHANGES);
            let got = sorted(split_files(&diff).into_iter().map(|(p, _)| p).collect());
            let mut want: Vec<String> = names().iter().map(|n| n.to_string()).collect();
            want.push("movido ação.txt".into());
            assert_eq!(got, sorted(want), "quotepath={quotepath}\n{diff}");
        }
    }

    // `git diff --no-index /dev/null <path>` (an untracked file in the "nothing staged" fallback) quotes its header too.
    #[test]
    fn an_untracked_file_diffed_against_dev_null_is_named_as_it_is_on_disk() {
        for quotepath in [true, false] {
            let d = repo_with(&[]);
            for n in names() {
                write(d.path(), n, "text\n");
                let diff = git(d.path(), quotepath, &new_file_args(n));
                let got = split_files(&diff);
                assert_eq!(got.len(), 1, "{n:?} quotepath={quotepath}\n{diff}");
                assert_eq!(got[0].0, n, "quotepath={quotepath}");
            }
        }
    }

    // The status parser reads `-z` output, which git never quotes: nothing to undo, and this keeps it so.
    #[test]
    fn the_status_never_quotes_a_name_because_it_is_nul_separated() {
        let d = repo_with(&["plain old.txt"]);
        for n in names() {
            write(d.path(), n, "text\n");
        }
        git(d.path(), true, &["mv", "plain old.txt", "movido ação.txt"]);
        git(d.path(), true, &["add", "movido ação.txt"]);
        let out = git(d.path(), true, &crate::git_review::STATUS_ARGS);
        let (_, _, _, entries) = parse_status(&out);
        let got = sorted(
            entries
                .iter()
                .map(|e| e["path"].as_str().unwrap().to_string())
                .collect(),
        );
        let mut want: Vec<String> = names().iter().map(|n| n.to_string()).collect();
        want.push("movido ação.txt".into());
        assert_eq!(got, sorted(want));
        let renamed = entries
            .iter()
            .find(|e| e["path"] == "movido ação.txt")
            .unwrap();
        assert_eq!(renamed["orig"], "plain old.txt");
    }

    // `ls-files`, `diff --name-only` and `--numstat` have a `-z` form that is never quoted: the ones Atlas reads
    // (`atlas_core::affected`, the desktop file list, the untracked list of the Changes view) use it.
    #[test]
    fn the_nul_separated_listings_atlas_reads_hold_the_real_names() {
        let d = repo_with(&names());
        let ls = git(d.path(), true, &["ls-files", "-z"]);
        let got = sorted(
            ls.split('\0')
                .filter(|s| !s.is_empty() && *s != "README.md")
                .map(str::to_string)
                .collect(),
        );
        assert_eq!(got, sorted(names().iter().map(|n| n.to_string()).collect()));
        let ls = git(
            d.path(),
            true,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        );
        assert_eq!(ls, "");
        for n in names() {
            write(d.path(), n, "changed\n");
        }
        let changed = git(d.path(), true, &["diff", "--name-only", "-z"]);
        let got = sorted(
            changed
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        );
        assert_eq!(got, sorted(names().iter().map(|n| n.to_string()).collect()));
    }
}
