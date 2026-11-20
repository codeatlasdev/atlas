//! A grep guard for one class of hole (security reviews 4, 5 and 6): hostd runs as the crew user, which reaches
//! every crewmate's tree, and an isolated crewmate controls its own tree completely (links, `.git/config`,
//! `atlas.yaml`) and chooses the text of its own hook calls (the pane key, the arguments). Code of hostd that runs
//! something with a cwd, config or script from that tree, or opens a path in it with `std::fs` (which follows
//! links), hands the crew user's reach to the crewmate. Execution goes through the spawn helper as the crewmate
//! (`gitrun::Git`, `Request::Spawn`); files are opened with `spawn::layout`'s `*_beneath` / `*_nofollow` helpers.
//!
//! This test cannot know where a path comes from, so it makes every use visible instead:
//!
//! 1. EVERY file of `src/crew`, `src/agents` and `src/mobile` is scanned (a new file is covered the day it is made;
//!    review 6 found the hole in a file the old fixed list never named). Each line of production code that touches
//!    the file system or starts a program carries a `tree-access:` comment (on the line or up to two above) saying
//!    why it is safe: a path of the crew store, an own tree, a spawn through the helper. A new `fs::` line fails
//!    until someone reads it and writes that sentence. Only the files of [`STORE_ONLY`] (each says why) and the
//!    spawn helper, which implements the safe access, are exempt; test code is cut (an inline `mod tests { .. }`
//!    only, never the code after it).
//! 2. `git` is started only by `gitrun` (which decides who runs it), the mirror (hostd's own bare repos) and the
//!    helper itself; any other program is started with a literal name, or from a file of [`PROGRAMS`].
//! 3. Project config, the affected plugin, the changed-files walk and the task cache's tree walk are used only by
//!    the UI gate's `Tree`, which reads an isolated tree beneath its root and refuses the plugin command.

use std::path::{Path, PathBuf};

/// Files that only touch hostd's own paths (the crew store, its scratch, the hook endpoint, the user's own
/// settings), never a path a crewmate names or a file in a crewmate's tree (relative to `src/`).
const STORE_ONLY: &[(&str, &str)] = &[
    (
        "crew/audit/mod.rs",
        "hostd's own audit log under the hostd dir",
    ),
    (
        "crew/audit/redact.rs",
        "hostd's own hook endpoint file, read to redact the token",
    ),
    (
        "crew/github/cli.rs",
        "the crew user's own ~/.local/bin (the gh install), no cwd and no tree",
    ),
    (
        "crew/github/mod.rs",
        "the crew user's own ~/.local/bin and `git config --global`, no cwd and no tree",
    ),
    (
        "crew/mcp.rs",
        "hostd's own hook endpoint file (the test client reads it)",
    ),
    ("crew/memory_cap.rs", "the kernel's /proc/meminfo"),
    (
        "crew/mirror/mod.rs",
        "hostd's own bare mirrors under the hostd dir; never a crewmate's tree",
    ),
    ("crew/push.rs", "the push token file under the hostd dir"),
    (
        "crew/relay/mod.rs",
        "the relay registry and tunnel sockets under the hostd dir; ssh to a host the operator registered",
    ),
    (
        "crew/store/archive.rs",
        "the crew store's export and import, ids checked by valid_id",
    ),
    (
        "crew/store/hooktoken.rs",
        "the crewmate's hook token file in the crew store",
    ),
    (
        "crew/store/jsonl.rs",
        "the crew store's append-only logs, paths the store built",
    ),
    (
        "crew/store/mod.rs",
        "the crew store: <hostd dir>/crew/<id>/…, ids and names hostd made",
    ),
    (
        "crew/store/tasklog.rs",
        "the crew store's task log and its index, paths the store built",
    ),
    (
        "crew/store/usage.rs",
        "the crew store's usage files, paths the store built",
    ),
    (
        "crew/triggers/github.rs",
        "the trigger's seen-file and log under the hostd dir",
    ),
    (
        "mobile/mod.rs",
        "the phone's devices, invites, key and audit log under the hostd dir",
    ),
];

/// Files that start a program whose name is not a literal: the engines, `gh`, the capture runner. They run it
/// through the helper for an isolated crewmate, or hostd's own tool with hostd's own arguments (relative to `src/`).
const PROGRAMS: &[(&str, &str)] = &[
    (
        "crew/voice/engine.rs",
        "whisper-cli and ffmpeg from hostd's own voice dir (installed by `atlas crew voice install`) or the server's ffmpeg, on files hostd wrote in its own scratch dir, with hostd's own arguments",
    ),
    (
        "crew/github/app/gh_shim.rs",
        "the crewmate's own `gh`, started by the process that is the crewmate (never by hostd), with a token for that call",
    ),
    (
        "agents/integrations.rs",
        "hostd's own binary, to install the user's agent hooks",
    ),
    (
        "crew/capture/mod.rs",
        "hostd's own capture runner (node, or the test override), arguments hostd built",
    ),
    (
        "crew/engine/launch.rs",
        "the engine of a crewmate that is not isolated (an isolated one goes through Request::Spawn)",
    ),
    (
        "crew/engine_login.rs",
        "hostd's own engine binary for the sign-in, in the crew user's home, never a tree",
    ),
    (
        "crew/proc.rs",
        "`gh` (or the configured gh path) with hostd's own arguments, no tree cwd",
    ),
    (
        "crew/runner/github.rs",
        "`gh` with hostd's own arguments, no tree cwd",
    ),
    (
        "crew/runner/summary.rs",
        "the reply summary: the engine binary runs through the helper (ctx.exec) as the crewmate",
    ),
];

/// Directories not scanned (relative to `src/`): test code, and the spawn helper, which is the layer that
/// implements the safe access (`openat` + `O_NOFOLLOW`, `setpriv`) and is reviewed as such.
const UNSCANNED: &[&str] = &["crew/tests/", "crew/spawn/"];

/// What touches a path or starts a program. `fs::` and `File::` are matched as whole words, so `use std::fs;` then
/// `fs::read(p)`, `atlas_core::fs::write_atomic` and `File::open` are all seen.
const ACCESS: &[&str] = &[
    "std::fs",
    "fs::",
    "File::",
    "OpenOptions",
    ".join(task_id",
    ".exists()",
    ".is_file()",
    ".is_dir()",
    ".metadata()",
    "symlink_metadata(",
    "read_link(",
    ".canonicalize()",
    "canonicalize(",
    "read_dir(",
    "read_to_string(",
    "write_atomic",
    "Command::new(",
];

/// Used only by the UI gate's `Tree` (see the module docs, 3).
const GATE_ONLY: &[&str] = &[
    "load_or_detect",
    "changed_files(",
    "affected::resolve",
    "from_command(",
    "key_with(",
    ".key()",
];

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every scanned file under `src/crew`, `src/agents` and `src/mobile` as `(path relative to src/, source)`.
fn scanned() -> Vec<(String, String)> {
    let mut files = vec![];
    for d in ["crew", "agents", "mobile"] {
        rust_files(&src_dir().join(d), &mut files);
    }
    let mut out = vec![];
    for f in files {
        let rel = f
            .strip_prefix(src_dir())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let name = rel.rsplit('/').next().unwrap();
        if UNSCANNED.iter().any(|u| rel.starts_with(u))
            || name == "tests.rs"
            || name.ends_with("_tests.rs")
        {
            continue;
        }
        out.push((rel, std::fs::read_to_string(&f).unwrap()));
    }
    out
}

/// The code of a file without its inline test modules: from a `mod x {` line under `#[cfg(test)]` (or
/// `#[cfg(all(test, ..))]`) to the closing `}` at column 0. What follows the module is still code.
fn production(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut skipping = false;
    let mut cfg_test = false;
    for l in src.split_inclusive('\n') {
        if skipping {
            // rustfmt puts the closing brace of a top-level module at column 0 on a line of its own.
            if l.trim_end() == "}" {
                skipping = false;
            }
            out.push('\n');
            continue;
        }
        let opens_mod =
            l.starts_with("mod ") || l.starts_with("pub mod ") || l.starts_with("pub(crate) mod ");
        if cfg_test && opens_mod && l.trim_end().ends_with('{') {
            skipping = true;
            out.push('\n');
            continue;
        }
        if l.starts_with("#[cfg(test)]") || l.starts_with("#[cfg(all(test") {
            cfg_test = true;
        } else if !l.starts_with("#[") {
            cfg_test = false;
        }
        out.push_str(l);
    }
    assert!(
        !skipping,
        "an inline test module is not closed by a `}}` at column 0"
    );
    out
}

/// Whether `line` holds `pat`; a pattern that begins with a letter must not be the tail of a longer name
/// (`fs::` is not `ffs::`, `File::` is not `TempFile::`).
fn has(line: &str, pat: &str) -> bool {
    let word = pat.starts_with(|c: char| c.is_ascii_alphabetic());
    line.match_indices(pat).any(|(i, _)| {
        !word
            || !line[..i]
                .chars()
                .next_back()
                .is_some_and(|p| p.is_alphanumeric() || p == '_')
    })
}

/// The lines of `src` (production code) that touch the file system or start a program without a `tree-access:`
/// comment on the line or the two above, as `line: text`.
fn unexplained(src: &str) -> Vec<String> {
    let code = production(src);
    let lines: Vec<&str> = code.lines().collect();
    let mut bad = vec![];
    for (i, l) in lines.iter().enumerate() {
        let text = l.trim_start();
        if text.starts_with("//") || !ACCESS.iter().any(|a| has(l, a)) {
            continue;
        }
        if !lines[i.saturating_sub(2)..=i]
            .iter()
            .any(|c| c.contains("tree-access:"))
        {
            bad.push(format!("{}: {}", i + 1, text));
        }
    }
    bad
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn every_use_of_the_file_system_or_a_program_says_why_it_is_safe() {
    let mut bad = vec![];
    for (rel, src) in scanned() {
        if STORE_ONLY.iter().any(|(f, _)| *f == rel) {
            continue;
        }
        bad.extend(unexplained(&src).into_iter().map(|l| format!("{rel}:{l}")));
    }
    assert!(
        bad.is_empty(),
        "these lines touch a path or start a program with no `tree-access:` comment. An isolated crewmate controls \
         its tree (links, .git/config, atlas.yaml) and the text of its hook calls (the pane key): run things through \
         the spawn helper (`gitrun::Git`) and open its files with `spawn::layout::{{read_bytes_beneath, \
         write_beneath, read_dir_beneath, ..}}`; if the path is hostd's own (the store, scratch), say so in a \
         `// tree-access:` line, or list a file that only does so in STORE_ONLY.\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_exemptions_name_files_that_exist_and_say_why() {
    for (list, name) in [(STORE_ONLY, "STORE_ONLY"), (PROGRAMS, "PROGRAMS")] {
        for (f, why) in list {
            assert!(
                src_dir().join(f).is_file(),
                "{name} lists {f}, which is gone"
            );
            assert!(why.len() > 15, "{name}: say why {f} is exempt");
        }
    }
}

/// The program a `Command::new(` line starts, when it is a literal: `"git"` in `Command::new("git")`; the next
/// lines count when rustfmt wrapped the call.
fn program_of(lines: &[&str], i: usize) -> String {
    let at = lines[i].find("Command::new(").unwrap() + "Command::new(".len();
    let mut arg = lines[i][at..].to_string();
    for next in lines.iter().skip(i + 1).take(2) {
        if arg.contains(')') {
            break;
        }
        arg.push_str(next.trim());
    }
    arg
}

#[test]
fn git_is_started_only_by_the_code_that_decides_who_runs_it() {
    let mut bad = vec![];
    for (rel, src) in scanned() {
        // `github/mod.rs`: `git config --global` of the crew user's own sign-in (no cwd, no tree).
        // `github/app/gh_shim.rs`: runs as the crewmate itself (`gh` is a link to atlas on its PATH), asking its own git
        // which repo its checkout is.
        if rel == "crew/gitrun.rs"
            || rel == "crew/github/mod.rs"
            || rel == "crew/github/app/gh_shim.rs"
            || rel.starts_with("crew/mirror/")
        {
            continue;
        }
        let code = production(&src);
        let lines: Vec<&str> = code.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if l.trim_start().starts_with("//") || !l.contains("Command::new(") {
                continue;
            }
            let arg = program_of(&lines, i);
            let literal = arg.trim_start().starts_with('"');
            let named = PROGRAMS.iter().any(|(f, _)| *f == rel);
            if arg.to_lowercase().contains("git") || (!literal && !named) {
                bad.push(format!("{rel}:{}: {}", i + 1, l.trim()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "start git through `gitrun::Git` (as the crewmate for an isolated tree); a program whose name is not a \
         literal needs its file in PROGRAMS:\n{}",
        bad.join("\n")
    );
}

#[test]
fn project_config_the_affected_plugin_and_the_trees_walks_are_used_only_by_the_gates_tree() {
    let mut bad = vec![];
    for (rel, src) in scanned() {
        if rel == "crew/uigate/plan.rs" || rel == "crew/uigate/tree.rs" {
            continue;
        }
        for (i, l) in production(&src).lines().enumerate() {
            let through_tree = l.contains("tree.changed_files(") || l.contains("tree.cache_key(");
            if !l.trim_start().starts_with("//")
                && !through_tree
                && GATE_ONLY.iter().any(|g| l.contains(g))
            {
                bad.push(format!("{rel}:{}: {}", i + 1, l.trim()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "these run project config or its plugin, or walk a tree; go through `uigate::Tree`:\n{}",
        bad.join("\n")
    );
}

// The guard has to see what it claims to: an unexplained line is reported, an explained one and a comment are not,
// the test module is not scanned but the code after it is, and every way to touch a path or start a program shows.
#[test]
fn the_guard_reports_an_unexplained_line_and_only_that() {
    let src = "fn a(p: &Path) {\n    let t = std::fs::read_to_string(p);\n    // tree-access: the store\n    let u = std::fs::read(p);\n    // std::fs::write(p, x)\n    let w = p.exists(); // tree-access: scratch\n    let _x = 1;\n    let _y = 2;\n    let c = tokio::process::Command::new(\"sh\");\n}\n#[cfg(test)]\nmod tests {\n    fn t() { std::fs::write(1, 2); }\n}\n";
    assert_eq!(
        unexplained(src),
        [
            "2: let t = std::fs::read_to_string(p);",
            "9: let c = tokio::process::Command::new(\"sh\");"
        ]
    );
}

#[test]
fn the_guard_sees_the_forms_the_first_version_missed() {
    for line in [
        "use std::fs;",
        "let b = fs::read(p);",
        "let f = File::open(p)?;",
        "let f = std::fs::File::create(p)?;",
        "let f = OpenOptions::new().append(true).open(p)?;",
        "let m = p.metadata()?;",
        "let m = std::fs::symlink_metadata(p)?;",
        "for e in read_dir(p)? {}",
        "atlas_core::fs::write_atomic(&p, b, Mode::Default)?;",
        "write_atomic(&p, b, Mode::Default)?;",
        "let c = Command::new(&git);",
        "let c = Command::new(GIT);",
        "let c = tokio::process::Command::new(bin);",
    ] {
        assert_eq!(unexplained(line).len(), 1, "{line}");
    }
    for line in ["let x = ffs::read(1);", "let t = TempFile::new();"] {
        assert!(unexplained(line).is_empty(), "{line}");
    }
    // Only an inline test module is cut: code after it, and an out-of-line `mod tests;`, are scanned.
    let src = "#[cfg(test)]\nmod tests;\nfn late() {\n    let b = fs::read(p);\n}\n";
    assert_eq!(unexplained(src), ["4: let b = fs::read(p);"]);
    let src = "#[cfg(test)]\nmod tests {\n    fn t() {\n        fs::read(p);\n    }\n}\nfn gate_dir() {\n    fs::create_dir_all(p);\n}\n";
    assert_eq!(unexplained(src), ["8: fs::create_dir_all(p);"]);
}
