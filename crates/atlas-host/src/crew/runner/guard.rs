//! Guards against the agent itself (#214). After a run succeeded and before the task is `Done`, the
//! crewmate's work tree and its own words are checked for the ways an agent "finishes" without
//! having done the work:
//!
//! 1. **Weakened tests.** The diff against the task's base removes or weakens assertions (`assert!`,
//!    `expect(`, `should`, `assert_eq`…), deletes a test file, adds a skip (`#[ignore]`, `.skip`,
//!    `it.only`, `xit`, `skip:`) or raises a waiver, threshold or budget in a known config key.
//!    Rust, TypeScript/JavaScript and Dart.
//! 2. **Stray files.** A new untracked file outside the places a change belongs. The rule: an
//!    untracked path is expected when it sits under a directory the repo already tracks files in
//!    (a new source file next to its siblings) and is not a scratch name. It is refused at the repo
//!    root unless the root already tracks that kind of file, and under any `tmp`/`scratch`/`probe`
//!    directory or with a scratch name (`probe*`, `scratch*`, `tmp*`, `debug*`, `*.orig`, `*.rej`,
//!    `*.bak`, `*.log`). An absolute `/tmp/…` path the agent wrote with a tool is outside the work
//!    tree, so git cannot see it: the audit's tool calls are scanned for it.
//! 3. **Invented references.** Every `#N` in the commit messages of the task, the PR body and the
//!    reply must exist (`GithubApi::issue`; a PR is an issue there).
//! 4. **Unproven claims.** A sentence of the result that says something was verified, tested or
//!    passes needs a command of this task that could prove it (a test or check command) with
//!    output that shows no failure. Otherwise the sentence is removed from the result and noted.
//!
//! A refusal parks the task (`WaitingAction` + `TaskAction::Refused`); the owner answers `override`
//! (done as it was) or `retry` (the reasons are appended to the prompt and the task runs again).
//! Everything here is a pure function of its input except the git and GitHub calls, so each check
//! has a test on a real temp repository.

use std::path::Path;

use tokio::process::Command;

use crate::crew::github::GithubApi;

/// The answer that accepts the work as it is.
pub(crate) const OVERRIDE: &str = "override";
/// The answer that runs the task again with the reasons added to its prompt.
pub(crate) const RETRY: &str = "retry";

/// What the guard looks at.
pub(crate) struct Input<'a> {
    pub work_dir: &'a Path,
    /// git's environment in `work_dir` (an isolated crewmate's tree belongs to another user and
    /// opens only with that crewmate's config; empty otherwise).
    pub git_env: &'a [(String, String)],
    /// `owner/repo` for the `#N` check; `None` skips it (no GitHub project).
    pub repo: Option<&'a str>,
    /// The agent's final message.
    pub result: &'a str,
    /// What the agent ran in this task (`tape`).
    pub commands: &'a [Ran],
}

/// A command the agent ran in this task, with what it printed when the hook reported it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Ran {
    pub command: String,
    pub output: Option<String>,
    /// The tool that ran it (`bash`, `Bash`…), to match the output omp reports when the call ends.
    pub tool: Option<String>,
}

/// What the guard decided.
#[derive(Debug, Default)]
pub(crate) struct Verdict {
    /// Why the work is refused; empty when it passes.
    pub reasons: Vec<String>,
    /// The result with the unproven claims removed.
    pub result: String,
    /// The sentences removed, for the note appended to the result.
    pub removed: Vec<String>,
}

impl Verdict {
    pub(crate) fn refused(&self) -> bool {
        !self.reasons.is_empty()
    }
}

/// Run every check. Git or GitHub failing is a reason to refuse only where silence would let the
/// agent through (a diff that cannot be read is not "no weakened test").
pub(crate) async fn review(input: &Input<'_>, api: &dyn GithubApi) -> Verdict {
    let mut v = Verdict::default();
    let tree = Tree {
        dir: input.work_dir,
        env: input.git_env,
    };
    let base = base_of(tree).await;
    // A folder that is not a git work tree (a crewmate without a project) has no diff and no new
    // files to read: those two checks do not apply. Inside a work tree, git failing is a refusal.
    if in_work_tree(tree).await {
        match diff_of(tree, &base).await {
            Ok(diff) => v.reasons.extend(weakened_tests(&diff)),
            Err(e) => v
                .reasons
                .push(format!("could not read the diff to check it: {e}")),
        }
        match untracked_of(tree).await {
            Ok(files) => {
                let tracked = tracked_of(tree).await.unwrap_or_default();
                v.reasons.extend(stray_files(&files, &tracked));
            }
            Err(e) => v
                .reasons
                .push(format!("could not list the new files to check them: {e}")),
        }
    }
    v.reasons.extend(outside_writes(input.commands));
    if let Some(repo) = input.repo {
        let mut text = commit_messages(tree, &base).await;
        text.push('\n');
        text.push_str(input.result);
        for c in input.commands.iter().filter(|c| publishes(&c.command)) {
            text.push('\n');
            text.push_str(&c.command);
        }
        v.reasons.extend(invented_refs(&text, repo, api).await);
    }
    let (result, removed) = strip_unproven(input.result, input.commands);
    v.result = result;
    v.removed = removed;
    v
}

// ─── the task's base and its diff ────────────────────────────────────────────

/// The work tree git runs in, with git's environment for it (an isolated crewmate's tree belongs to
/// another user and opens only with that crewmate's config; empty otherwise).
#[derive(Clone, Copy)]
struct Tree<'a> {
    dir: &'a Path,
    env: &'a [(String, String)],
}

async fn git(t: Tree<'_>, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(t.dir)
        .envs(t.env.iter().map(|(k, v)| (k, v)))
        .output()
        .await
        .map_err(|e| format!("git {}: {e}", args.first().copied().unwrap_or("")))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether `dir` is inside a git work tree.
async fn in_work_tree(t: Tree<'_>) -> bool {
    git(t, &["rev-parse", "--is-inside-work-tree"])
        .await
        .is_ok_and(|o| o.trim() == "true")
}

/// What the task's work is measured against: where the branch left its upstream (the crewmate's
/// branch tracks `origin/<branch>`, so commits not pushed yet count), else the remote's default
/// branch, else the empty tree (a repo with no remote: everything is the agent's).
async fn base_of(t: Tree<'_>) -> String {
    for upstream in ["@{upstream}", "origin/HEAD"] {
        if let Ok(b) = git(t, &["merge-base", "HEAD", upstream]).await {
            let b = b.trim();
            if !b.is_empty() {
                return b.to_string();
            }
        }
    }
    // The well-known hash of git's empty tree.
    "4b825dc642cb6eb9a060e54bf8d69288fbee4904".to_string()
}

/// Committed and uncommitted changes since `base`, with no context lines (only the changed lines
/// matter) and no renames (a moved test is a deleted one and a new one: the new one still counts).
async fn diff_of(t: Tree<'_>, base: &str) -> Result<String, String> {
    git(
        t,
        &[
            "diff",
            "--no-color",
            "--no-renames",
            // The parser below wants `a/` and `b/`: the user's `diff.mnemonicPrefix` and
            // `diff.noprefix` must not change them.
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "-U0",
            base,
            "--",
        ],
    )
    .await
}

async fn untracked_of(t: Tree<'_>) -> Result<Vec<String>, String> {
    let out = git(t, &["ls-files", "--others", "--exclude-standard"]).await?;
    Ok(out.lines().map(str::to_string).collect())
}

async fn tracked_of(t: Tree<'_>) -> Result<Vec<String>, String> {
    let out = git(t, &["ls-files"]).await?;
    Ok(out.lines().map(str::to_string).collect())
}

async fn commit_messages(t: Tree<'_>, base: &str) -> String {
    git(t, &["log", "--format=%B", &format!("{base}..HEAD")])
        .await
        .unwrap_or_default()
}

// ─── 1. weakened tests ───────────────────────────────────────────────────────

/// One file of a diff: its path and the lines it removed and added.
struct FileDiff {
    path: String,
    deleted: bool,
    removed: Vec<String>,
    added: Vec<String>,
}

fn parse_diff(diff: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = vec![];
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            // `a/<path> b/<path>`: with no renames both are the same path.
            let path = rest
                .rsplit_once(" b/")
                .map(|(_, p)| p)
                .unwrap_or(rest)
                .to_string();
            files.push(FileDiff {
                path,
                deleted: false,
                removed: vec![],
                added: vec![],
            });
        } else if let Some(f) = files.last_mut() {
            if line.starts_with("deleted file mode") {
                f.deleted = true;
            } else if line.starts_with("+++") || line.starts_with("---") {
                // File header lines, not content.
            } else if let Some(l) = line.strip_prefix('-') {
                f.removed.push(l.to_string());
            } else if let Some(l) = line.strip_prefix('+') {
                f.added.push(l.to_string());
            }
        }
    }
    files
}

/// Whether `path` is a test file of Rust, TypeScript/JavaScript or Dart.
fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let in_dir = lower
        .split('/')
        .any(|d| matches!(d, "tests" | "test" | "__tests__" | "spec" | "e2e"));
    if lower.ends_with(".rs") {
        return in_dir
            || name == "tests.rs"
            || name.ends_with("_tests.rs")
            || name.ends_with("_test.rs");
    }
    if lower.ends_with(".dart") {
        return name.ends_with("_test.dart") || in_dir;
    }
    if [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".svelte"]
        .iter()
        .any(|e| lower.ends_with(e))
    {
        return name.contains(".test.") || name.contains(".spec.") || in_dir;
    }
    false
}

/// An assertion on this line, in any of the three languages.
fn assertions_in(line: &str) -> usize {
    const MARKS: &[&str] = &[
        "assert!(",
        "assert_eq!(",
        "assert_ne!(",
        "debug_assert!(",
        "assert!(matches!",
        "assert(",
        "assert.",
        "assertEquals(",
        "expect(",
        "expect!(",
        "expectLater(",
        ".should",
        "should.",
        "verify(",
        "t.is(",
        "t.true(",
    ];
    MARKS.iter().map(|m| line.matches(m).count()).sum()
}

/// A test that is switched off or narrowed to the others' exclusion on this line.
fn skip_marker(path: &str, line: &str) -> Option<&'static str> {
    let l = line.trim();
    // `.skip(` is a test runner's call in JS/TS; in Rust it is `Iterator::skip`, in Dart it never is.
    let js = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".svelte"]
        .iter()
        .any(|e| path.ends_with(e));
    if l.contains(".skip(") && !js {
        return None;
    }
    const MARKS: &[(&str, &str)] = &[
        ("#[ignore", "#[ignore]"),
        ("it.skip", "it.skip"),
        ("test.skip", "test.skip"),
        ("describe.skip", "describe.skip"),
        ("it.only", "it.only"),
        ("test.only", "test.only"),
        ("describe.only", "describe.only"),
        ("xit(", "xit"),
        ("xdescribe(", "xdescribe"),
        ("xtest(", "xtest"),
        ("skip:", "skip:"),
        ("@Skip(", "@Skip"),
        ("test.todo", "test.todo"),
        // Last: the named forms above are more precise than a bare `.skip(`.
        (".skip(", ".skip"),
    ];
    MARKS.iter().find(|(m, _)| l.contains(m)).map(|(_, n)| *n)
}

/// Config keys whose numbers an agent raises to make a gate pass. Matched on the key name, in
/// JSON, TOML, YAML and plain assignment forms.
const LIMIT_KEYS: &[&str] = &[
    "timeout",
    "threshold",
    "budget",
    "max_bytes",
    "max_size",
    "maxsize",
    "max_warnings",
    "max-warnings",
    "maxwarnings",
    "waiver",
    "waivers",
    "tolerance",
    "coverage_threshold",
    "fail_under",
    "max_lines",
    "max_complexity",
    "bundle_size",
    "maxduration",
    "max_duration",
    "retries",
];

/// `(key, number)` for a line like `"timeout": 30`, `threshold = 80`, `max_warnings: 10,`.
fn limit_of(line: &str) -> Option<(String, f64)> {
    let l = line.trim().trim_end_matches(',');
    let sep = l.find([':', '='])?;
    let key = l[..sep]
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '-' || c == ' ')
        .to_ascii_lowercase();
    if !LIMIT_KEYS
        .iter()
        .any(|k| key == *k || key.ends_with(&format!("_{k}")) || key.ends_with(&format!("-{k}")))
    {
        return None;
    }
    let value: String = l[sep + 1..]
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '_')
        .filter(|c| *c != '_')
        .collect();
    value.parse::<f64>().ok().map(|n| (key, n))
}

/// Whether a bigger number for `key` is the lenient direction. Most keys are ceilings (a higher
/// timeout, budget or waiver count lets more through); `coverage_threshold` and `fail_under` are
/// floors, where lower is the lenient way.
fn higher_is_lenient(key: &str) -> bool {
    !(key.contains("coverage") || key.contains("fail_under"))
}

/// The reasons a diff weakens the checks of the project. One reason per finding, with the file.
pub(crate) fn weakened_tests(diff: &str) -> Vec<String> {
    let mut reasons = vec![];
    for f in parse_diff(diff) {
        if f.deleted && is_test_path(&f.path) {
            reasons.push(format!("deletes the test file {}", f.path));
            continue;
        }
        if is_test_path(&f.path) {
            let removed: usize = f.removed.iter().map(|l| assertions_in(l)).sum();
            let added: usize = f.added.iter().map(|l| assertions_in(l)).sum();
            if removed > added {
                reasons.push(format!(
                    "removes {} assertion(s) from {} ({removed} removed, {added} added)",
                    removed - added,
                    f.path
                ));
            }
            // A skip is new only when it is not on a removed line too (a reflowed attribute).
            for l in &f.added {
                if let Some(m) = skip_marker(&f.path, l)
                    && !f.removed.iter().any(|r| skip_marker(&f.path, r) == Some(m))
                {
                    reasons.push(format!("adds {m} to a test in {}: `{}`", f.path, l.trim()));
                }
            }
            // A test function that disappeared with no other taking its place.
            let tests = |ls: &[String]| {
                ls.iter()
                    .filter(|l| {
                        let t = l.trim();
                        t.starts_with("#[test]")
                            || t.starts_with("#[tokio::test")
                            || t.starts_with("test(")
                            || t.starts_with("it(")
                            || t.starts_with("testWidgets(")
                            || t.starts_with("test_widgets(")
                            || t.starts_with("test.each")
                    })
                    .count()
            };
            let (rt, at) = (tests(&f.removed), tests(&f.added));
            // `it(` turned into `it.skip(` is one finding (the skip), not also a deleted test.
            let skipped = f.added.iter().any(|l| skip_marker(&f.path, l).is_some());
            if rt > at && !skipped {
                reasons.push(format!(
                    "deletes {} test(s) from {} ({rt} removed, {at} added)",
                    rt - at,
                    f.path
                ));
            }
        }
        // Limits raised, in any file: the config that gates a project is not a test file.
        for r in &f.removed {
            let Some((key, before)) = limit_of(r) else {
                continue;
            };
            let raised = f
                .added
                .iter()
                .filter_map(|a| limit_of(a))
                .find(|(k, _)| *k == key);
            if let Some((_, after)) = raised
                && ((higher_is_lenient(&key) && after > before)
                    || (!higher_is_lenient(&key) && after < before))
            {
                reasons.push(format!(
                    "loosens `{key}` in {}: {before} to {after}",
                    f.path
                ));
            }
        }
    }
    reasons
}

// ─── 2. stray files ──────────────────────────────────────────────────────────

/// Scratch names: the files an agent leaves behind while probing.
fn scratch_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "probe",
        "scratch",
        "tmp",
        "temp",
        "debug",
        "repro",
        "playground",
    ]
    .iter()
    .any(|p| n.starts_with(p))
        || [".orig", ".rej", ".bak", ".log", ".swp", ".tmp", "~"]
            .iter()
            .any(|e| n.ends_with(e))
}

/// The refusals for `untracked` files (paths relative to the work tree), given the tracked ones.
pub(crate) fn stray_files(untracked: &[String], tracked: &[String]) -> Vec<String> {
    let tracked_dirs: std::collections::HashSet<&str> = tracked
        .iter()
        .map(|p| p.rsplit_once('/').map_or("", |(d, _)| d))
        .collect();
    let mut out = vec![];
    for p in untracked {
        // Atlas's own files in the work tree (task attachments live under `.atlas/`) are not the agent's.
        if p.starts_with(".atlas/") {
            continue;
        }
        let (dir, name) = p.rsplit_once('/').unwrap_or(("", p.as_str()));
        let scratch_dir = dir.split('/').any(|d| {
            matches!(
                d.to_ascii_lowercase().as_str(),
                "tmp" | "temp" | "scratch" | "probes" | "playground"
            )
        });
        if scratch_name(name) || scratch_dir {
            out.push(format!("a scratch file was left in the work tree: {p}"));
        } else if dir.is_empty() {
            // A new file at the root, where the repo tracks nothing of its kind.
            let ext = name.rsplit_once('.').map(|(_, e)| e);
            let root_has_kind = tracked
                .iter()
                .any(|t| !t.contains('/') && t.rsplit_once('.').map(|(_, e)| e) == ext);
            if !root_has_kind {
                out.push(format!(
                    "a new file at the repository root, where nothing like it lives: {p}"
                ));
            }
        } else if !tracked_dirs.contains(dir)
            && !tracked_dirs
                .iter()
                .any(|t| t.starts_with(&format!("{dir}/")))
        {
            // A new directory with the file in it: a feature may add one, but not under a scratch name
            // (handled above) and not at a depth the repo has no sibling for.
            let parent = dir.rsplit_once('/').map_or("", |(d, _)| d);
            if !parent.is_empty() && !tracked_dirs.contains(parent) {
                out.push(format!(
                    "a new file in a directory the repository has no sibling for: {p}"
                ));
            }
        }
    }
    out
}

/// Files the agent wrote outside the work tree with a tool (`/tmp/x.sh`): git cannot see them, the
/// tool calls can. Reading is fine; only a redirect or a write command names a file as its target.
pub(crate) fn outside_writes(commands: &[Ran]) -> Vec<String> {
    let mut out = vec![];
    for c in commands {
        for tok in c.command.split_whitespace() {
            let t = tok.trim_matches(|ch| ch == '"' || ch == '\'' || ch == ';');
            let target = t.strip_prefix('>').unwrap_or(t);
            let writes = c.command.contains(&format!(">{target}"))
                || c.command.contains(&format!("> {target}"))
                || c.command.contains(&format!("tee {target}"));
            if (target.starts_with("/tmp/") || target.starts_with("/var/tmp/")) && writes {
                out.push(format!(
                    "a command wrote a file outside the work tree: {target}"
                ));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Whether a command publishes text the agent wrote (a PR, an issue or a comment, a commit message):
/// its own arguments are that text.
fn publishes(command: &str) -> bool {
    let c = command.trim_start();
    [
        "gh pr ",
        "gh issue ",
        "gh api ",
        "git commit",
        "gh release ",
    ]
    .iter()
    .any(|p| c.contains(p))
}

// ─── 3. invented references ──────────────────────────────────────────────────

/// Every `#N` in `text`, once each, in order. `#` followed by digits and not preceded by a word
/// character (a colour like `#123456` is 6 digits and is skipped: GitHub numbers here stay below
/// 100 000).
pub(crate) fn refs_in(text: &str) -> Vec<u64> {
    let mut found = vec![];
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '#'
            && (i == 0
                || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '&' || chars[i - 1] == '/'))
        {
            let digits: String = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty()
                && digits.len() <= 5
                && !chars
                    .get(i + 1 + digits.len())
                    .is_some_and(|c| c.is_alphanumeric())
                && let Ok(n) = digits.parse::<u64>()
                && n > 0
                && !found.contains(&n)
            {
                found.push(n);
            }
            i += 1 + digits.len();
        } else {
            i += 1;
        }
    }
    found
}

/// A refusal per `#N` that GitHub says does not exist. A call that fails for another reason than
/// "not found" is a refusal too: an unchecked reference is not a checked one.
pub(crate) async fn invented_refs(text: &str, repo: &str, api: &dyn GithubApi) -> Vec<String> {
    let mut out = vec![];
    for n in refs_in(text) {
        if let Err(e) = api.issue(repo, n).await {
            if e.contains("404") || e.to_ascii_lowercase().contains("not found") {
                out.push(format!("cites #{n}, which does not exist in {repo}"));
            } else {
                out.push(format!(
                    "cites #{n}, which could not be checked in {repo}: {e}"
                ));
            }
        }
    }
    out
}

// ─── 4. unproven claims ──────────────────────────────────────────────────────

const CLAIM_WORDS: &[&str] = &[
    "verified",
    "verifiquei",
    "tested",
    "testei",
    "tests pass",
    "tests are passing",
    "test passes",
    "all tests",
    "passes",
    "passed",
    "passing",
    "passam",
    "passou",
    "check is green",
    "ci is green",
    "build succeeds",
    "builds cleanly",
    "compiles",
    "works as expected",
    "confirmed",
];

/// Commands that run tests or checks, by what they invoke.
const PROVING: &[&str] = &[
    "cargo test",
    "cargo nextest",
    "cargo check",
    "cargo clippy",
    "bun test",
    "bun run test",
    "bun run check",
    "bun run lint",
    "npm test",
    "npm run test",
    "npm run lint",
    "pnpm test",
    "yarn test",
    "npx vitest",
    "vitest",
    "jest",
    "pytest",
    "go test",
    "go vet",
    "flutter test",
    "flutter analyze",
    "dart test",
    "dart analyze",
    "make test",
    "make check",
    "atlas run",
    "svelte-check",
    "tsc",
];

/// Failure text a passing run does not print.
const FAILURE_MARKS: &[&str] = &[
    "test result: failed",
    "failed",
    "error[",
    "error:",
    "panicked at",
    "assertion failed",
    "fail ",
    "failures:",
    "exit code 1",
    "exit status 1",
    "not ok",
];

fn is_proving(command: &str) -> bool {
    let c = command.to_ascii_lowercase();
    PROVING.iter().any(|p| c.contains(p))
}

/// Whether a command ran a check and its recorded output shows no failure. A command without
/// recorded output proves nothing: Atlas only knows the output when the agent's hook reports it.
fn proves(c: &Ran) -> bool {
    if !is_proving(&c.command) {
        return false;
    }
    let Some(out) = c.output.as_deref().map(str::trim).filter(|o| !o.is_empty()) else {
        return false;
    };
    let lower = out.to_ascii_lowercase();
    // "0 failed" and "failed: 0" are the passing forms.
    let cleaned = lower
        .replace("0 failed", "")
        .replace("failed: 0", "")
        .replace("0 failures", "");
    !FAILURE_MARKS.iter().any(|m| cleaned.contains(m))
}

/// Sentences of `text`: split at `. `, `! `, `? ` and newlines, keeping their punctuation.
fn sentences(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        cur.push(*ch);
        let end = *ch == '\n'
            || (matches!(ch, '.' | '!' | '?')
                && chars.get(i + 1).is_none_or(|n| n.is_whitespace()));
        if end {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn claims(sentence: &str) -> bool {
    let s = sentence.to_ascii_lowercase();
    CLAIM_WORDS.iter().any(|w| s.contains(w))
}

/// `text` without the sentences that claim a verification no command of the task backs, and those
/// sentences. A task with a proving command (a test or check that printed no failure) keeps them.
pub(crate) fn strip_unproven(text: &str, commands: &[Ran]) -> (String, Vec<String>) {
    if commands.iter().any(proves) {
        return (text.to_string(), vec![]);
    }
    let mut kept = String::new();
    let mut removed = vec![];
    for s in sentences(text) {
        if claims(&s) {
            removed.push(s.trim().to_string());
        } else {
            kept.push_str(&s);
        }
    }
    if removed.is_empty() {
        return (text.to_string(), removed);
    }
    (kept.trim().to_string(), removed)
}

/// The note the result ends with when sentences were removed.
pub(crate) fn removal_note(removed: &[String]) -> String {
    format!(
        "\n\n[Atlas removed {} sentence(s) claiming verification: no test or check command of this task backs them: {}]",
        removed.len(),
        removed
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join("; ")
    )
}

/// The prompt of the task run again after a `retry`: the original, then what was refused.
pub(crate) fn retry_prompt(prompt: &str, reasons: &[String]) -> String {
    format!(
        "{prompt}\n\nYour previous attempt was refused by Atlas's guards for these reasons:\n{}\n\
         Fix the work so none of them applies (restore what you weakened, remove stray files, cite only \
         references that exist, run the tests and report what they print), then finish again.",
        reasons
            .iter()
            .map(|r| format!("- {r}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

#[cfg(all(test, unix))]
mod tests;
