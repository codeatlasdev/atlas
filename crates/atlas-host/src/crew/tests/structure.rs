//! A grep guard for one class of hole (security reviews 4 and 5, N1): hostd runs as the crew user, which reaches
//! every crewmate's tree, and an isolated crewmate controls its own tree completely (links, `.git/config`,
//! `atlas.yaml`). Code of hostd that runs something with a cwd, config or script from that tree, or opens a path in
//! it with `std::fs` (which follows links), hands the crew user's reach to the crewmate. Execution goes through the
//! spawn helper as the crewmate (`gitrun::Git`, `Request::Spawn`); files are opened with `spawn::layout`'s
//! `*_beneath` / `*_nofollow` helpers.
//!
//! This test cannot know where a path comes from, so it makes every use visible instead:
//!
//! 1. In the files that handle a crewmate's tree ([`TREE_FILES`]) each line that touches the file system or starts
//!    a program carries a `tree-access:` comment (on the line or up to two above) saying why it is safe: a path of
//!    the crew store, an own tree, a spawn through the helper. A new `std::fs` line there fails until someone
//!    reads it and writes that sentence.
//! 2. `git` is started only by `gitrun` (which decides who runs it), the mirror (hostd's own bare repos) and the
//!    helper itself.
//! 3. Project config, the affected plugin and the changed-files walk are used only by the UI gate's `Tree`, which
//!    reads an isolated tree beneath its root and refuses the plugin command.

use std::path::{Path, PathBuf};

/// The files whose code reads, writes or runs something for a crewmate's tree (relative to `src/crew`).
const TREE_FILES: &[&str] = &[
    "attachments.rs",
    "capture/detectors.rs",
    "capture/mod.rs",
    "engine/launch.rs",
    "locate/mod.rs",
    "locate/rank.rs",
    "locate/route.rs",
    "project.rs",
    "runner/gate.rs",
    "runner/guard.rs",
    "runner/isolation.rs",
    "runner/prompt.rs",
    "runner/reporting.rs",
    "uigate/check.rs",
    "uigate/mod.rs",
    "uigate/plan.rs",
    "uigate/regressions.rs",
    "uigate/results.rs",
    "uigate/scenario.rs",
    "uigate/tree.rs",
];

/// What touches a path or starts a program.
const ACCESS: &[&str] = &[
    "std::fs::",
    "tokio::fs::",
    ".exists()",
    ".is_file()",
    ".is_dir()",
    ".canonicalize()",
    "read_to_string(",
    "Command::new",
    "process::Command",
];

/// Used only by the UI gate's `Tree` (see the module docs, 3).
const GATE_ONLY: &[&str] = &[
    "load_or_detect",
    "affected::changed_files(",
    "affected::resolve",
    "from_command(",
];

fn crew_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/crew")
}

/// The code of a file without its test module.
fn production(src: &str) -> &str {
    let mut at = src.len();
    let mut off = 0;
    let lines: Vec<&str> = src.split_inclusive('\n').collect();
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with("#[cfg(test)]") || l.starts_with("#[cfg(all(test") {
            // A test module follows (attributes, then `mod`): an attribute on a single test item is not the cut.
            let next = lines[i + 1..].iter().find(|n| !n.starts_with("#["));
            if next.is_some_and(|n| n.starts_with("mod ") || n.starts_with("pub mod ")) {
                at = off;
                break;
            }
        }
        off += l.len();
    }
    &src[..at]
}

/// The lines of `src` (production code) that touch the file system or start a program without a `tree-access:`
/// comment on the line or the two above, as `line: text`.
fn unexplained(src: &str) -> Vec<String> {
    let lines: Vec<&str> = production(src).lines().collect();
    let mut bad = vec![];
    for (i, l) in lines.iter().enumerate() {
        let code = l.trim_start();
        if code.starts_with("//") || !ACCESS.iter().any(|a| l.contains(a)) {
            continue;
        }
        if !lines[i.saturating_sub(2)..=i]
            .iter()
            .any(|c| c.contains("tree-access:"))
        {
            bad.push(format!("{}: {}", i + 1, code));
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
fn every_use_of_the_file_system_or_a_program_in_tree_code_says_why_it_is_safe() {
    let mut bad = vec![];
    for f in TREE_FILES {
        let src = std::fs::read_to_string(crew_dir().join(f))
            .unwrap_or_else(|e| panic!("{f} is listed in TREE_FILES but cannot be read: {e}"));
        bad.extend(unexplained(&src).into_iter().map(|l| format!("{f}:{l}")));
    }
    assert!(
        bad.is_empty(),
        "these lines touch a path or start a program in code that handles a crewmate's tree, with no `tree-access:` \
         comment. An isolated crewmate controls its tree (links, .git/config, atlas.yaml): run things through the spawn \
         helper (`gitrun::Git`) and open its files with `spawn::layout::{{read_bytes_beneath, write_beneath, \
         read_dir_beneath, ..}}`; if the path is hostd's own (the store, scratch), say so in a `// tree-access:` line.\n{}",
        bad.join("\n")
    );
}

#[test]
fn git_is_started_only_by_the_code_that_decides_who_runs_it() {
    let mut files = vec![];
    rust_files(&crew_dir(), &mut files);
    let mut bad = vec![];
    for f in files {
        let rel = f
            .strip_prefix(crew_dir())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        // `github/mod.rs`: `git config --global` of the crew user's own sign-in (no cwd, no tree).
        if rel.starts_with("spawn/")
            || rel.starts_with("tests/")
            || rel.ends_with("tests.rs")
            || rel == "gitrun.rs"
            || rel == "github/mod.rs"
            || rel.starts_with("mirror/")
        {
            continue;
        }
        let src = std::fs::read_to_string(&f).unwrap();
        for (i, l) in production(&src).lines().enumerate() {
            if l.contains("Command::new(\"git\")") || l.contains("Command::new(git_program())") {
                bad.push(format!("{rel}:{}: {}", i + 1, l.trim()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "start git through `gitrun::Git` (as the crewmate for an isolated tree):\n{}",
        bad.join("\n")
    );
}

#[test]
fn project_config_the_affected_plugin_and_the_changed_files_walk_are_used_only_by_the_gates_tree() {
    let mut files = vec![];
    rust_files(&crew_dir(), &mut files);
    let mut bad = vec![];
    for f in files {
        let rel = f
            .strip_prefix(crew_dir())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with("tests/")
            || rel == "uigate/plan.rs"
            || rel == "uigate/tree.rs"
            || rel == "uigate/tests.rs"
        {
            continue;
        }
        let src = std::fs::read_to_string(&f).unwrap();
        for (i, l) in production(&src).lines().enumerate() {
            if !l.trim_start().starts_with("//") && GATE_ONLY.iter().any(|g| l.contains(g)) {
                bad.push(format!("{rel}:{}: {}", i + 1, l.trim()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "these run project config or its plugin on a tree; go through `uigate::Tree`:\n{}",
        bad.join("\n")
    );
}

// The guard has to see what it claims to: an unexplained line is reported, an explained one and a comment are not,
// and the test module is not scanned.
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
