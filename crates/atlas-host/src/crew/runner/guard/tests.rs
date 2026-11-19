//! Each guard on a real temp git repository: the agent's work is made with git and the files, the
//! guard reads it the way it does in production.

use std::path::Path;
use std::process::Command as Sh;
use std::sync::Arc;

use super::*;
use crate::crew::tests::github_fake::FakeGithub;

fn git(dir: &Path, args: &[&str]) {
    let out = Sh::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// A repo with a Rust test file, a TS test, a Dart test and a config, committed as the base the
/// task starts from. The commit is also the "upstream" the work is measured against.
fn repo() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    let d = t.path();
    git(d, &["init", "-q", "-b", "main"]);
    write(
        d,
        "src/lib.rs",
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    );
    write(
        d,
        "src/tests.rs",
        "#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n    assert!(add(0, 0) == 0);\n}\n\n#[test]\nfn negative() {\n    assert_eq!(add(-1, -1), -2);\n}\n",
    );
    write(
        d,
        "web/cart.test.ts",
        "import { it, expect } from \"bun:test\";\nit(\"totals\", () => {\n  expect(total([1, 2])).toBe(3);\n  expect(total([])).toBe(0);\n});\n",
    );
    write(
        d,
        "test/cart_test.dart",
        "void main() {\n  test('totals', () {\n    expect(total([1, 2]), 3);\n  });\n}\n",
    );
    write(
        d,
        "ci.json",
        "{\n  \"timeout\": 30,\n  \"coverage_threshold\": 80\n}\n",
    );
    write(d, "README.md", "hi\n");
    git(d, &["add", "-A"]);
    git(d, &["commit", "-q", "-m", "base"]);
    // The base the guard compares to: the remote's default branch.
    git(d, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git(
        d,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    t
}

async fn run(dir: &Path, result: &str, commands: &[Ran]) -> Verdict {
    let api = FakeGithub::new("bot");
    review(
        &Input {
            work_dir: dir,
            git: &crate::crew::gitrun::Git::local(),
            repo: None,
            result,
            commands,
        },
        &api,
    )
    .await
}

fn has(v: &Verdict, needle: &str) -> bool {
    v.reasons.iter().any(|r| r.contains(needle))
}

// ── 1. weakened tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn an_honest_diff_passes() {
    let t = repo();
    let d = t.path();
    write(
        d,
        "src/lib.rs",
        "pub fn add(a: i32, b: i32) -> i32 { a.wrapping_add(b) }\n",
    );
    // A new test next to its siblings, with more assertions than it replaced.
    write(
        d,
        "src/tests.rs",
        "#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n    assert!(add(0, 0) == 0);\n    assert_eq!(add(2, 2), 4);\n}\n\n#[test]\nfn negative() {\n    assert_eq!(add(-1, -1), -2);\n}\n",
    );
    git(d, &["commit", "-qam", "wrapping add, one more case"]);
    let v = run(d, "Done.", &[]).await;
    assert!(!v.refused(), "{:?}", v.reasons);
}

#[tokio::test]
async fn a_removed_assert_is_refused_with_its_file_and_count() {
    let t = repo();
    let d = t.path();
    write(
        d,
        "src/tests.rs",
        "#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n}\n\n#[test]\nfn negative() {\n    assert_eq!(add(-1, -1), -2);\n}\n",
    );
    let v = run(d, "Done.", &[]).await;
    assert!(
        has(&v, "removes 1 assertion(s) from src/tests.rs"),
        "{:?}",
        v.reasons
    );
}

#[tokio::test]
async fn a_removed_expect_in_typescript_and_dart_is_refused() {
    let t = repo();
    let d = t.path();
    write(
        d,
        "web/cart.test.ts",
        "import { it, expect } from \"bun:test\";\nit(\"totals\", () => {\n  expect(total([1, 2])).toBe(3);\n});\n",
    );
    write(
        d,
        "test/cart_test.dart",
        "void main() {\n  test('totals', () {\n    total([1, 2]);\n  });\n}\n",
    );
    let v = run(d, "Done.", &[]).await;
    assert!(has(&v, "web/cart.test.ts"), "{:?}", v.reasons);
    assert!(has(&v, "test/cart_test.dart"), "{:?}", v.reasons);
}

#[tokio::test]
async fn an_added_skip_is_refused_in_every_language() {
    let t = repo();
    let d = t.path();
    write(
        d,
        "src/tests.rs",
        "#[test]\n#[ignore]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n    assert!(add(0, 0) == 0);\n}\n\n#[test]\nfn negative() {\n    assert_eq!(add(-1, -1), -2);\n}\n",
    );
    write(
        d,
        "web/cart.test.ts",
        "import { it, expect } from \"bun:test\";\nit.skip(\"totals\", () => {\n  expect(total([1, 2])).toBe(3);\n  expect(total([])).toBe(0);\n});\n",
    );
    write(
        d,
        "test/cart_test.dart",
        "void main() {\n  test('totals', skip: 'flaky', () {\n    expect(total([1, 2]), 3);\n  });\n}\n",
    );
    let v = run(d, "Done.", &[]).await;
    assert!(has(&v, "adds #[ignore]"), "{:?}", v.reasons);
    assert!(has(&v, "adds it.skip"), "{:?}", v.reasons);
    assert!(
        !has(&v, "deletes"),
        "a skip is not also a deleted test: {:?}",
        v.reasons
    );
    assert!(has(&v, "adds skip:"), "{:?}", v.reasons);
}

#[tokio::test]
async fn a_deleted_test_file_and_a_deleted_test_are_refused() {
    let t = repo();
    let d = t.path();
    std::fs::remove_file(d.join("web/cart.test.ts")).unwrap();
    write(
        d,
        "src/tests.rs",
        "#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n    assert!(add(0, 0) == 0);\n}\n",
    );
    let v = run(d, "Done.", &[]).await;
    assert!(
        has(&v, "deletes the test file web/cart.test.ts"),
        "{:?}",
        v.reasons
    );
    assert!(
        has(&v, "deletes 1 test(s) from src/tests.rs"),
        "{:?}",
        v.reasons
    );
}

#[tokio::test]
async fn a_raised_threshold_or_timeout_is_refused_and_a_lowered_one_is_not() {
    let t = repo();
    let d = t.path();
    // Timeout up (lenient), coverage floor down (lenient).
    write(
        d,
        "ci.json",
        "{\n  \"timeout\": 300,\n  \"coverage_threshold\": 40\n}\n",
    );
    let v = run(d, "Done.", &[]).await;
    assert!(
        has(&v, "loosens `timeout` in ci.json: 30 to 300"),
        "{:?}",
        v.reasons
    );
    assert!(
        has(&v, "loosens `coverage_threshold` in ci.json: 80 to 40"),
        "{:?}",
        v.reasons
    );

    // Stricter on both sides is fine.
    write(
        d,
        "ci.json",
        "{\n  \"timeout\": 10,\n  \"coverage_threshold\": 90\n}\n",
    );
    let v = run(d, "Done.", &[]).await;
    assert!(!v.refused(), "{:?}", v.reasons);
}

#[tokio::test]
async fn committed_work_counts_as_much_as_uncommitted_work() {
    let t = repo();
    let d = t.path();
    write(
        d,
        "ci.json",
        "{\n  \"timeout\": 999,\n  \"coverage_threshold\": 80\n}\n",
    );
    git(d, &["commit", "-qam", "make CI pass"]);
    let v = run(d, "Done.", &[]).await;
    assert!(has(&v, "loosens `timeout`"), "{:?}", v.reasons);
}

// ── 2. stray files ───────────────────────────────────────────────────────────

#[tokio::test]
async fn a_probe_at_the_root_and_a_scratch_file_are_refused_a_sibling_source_file_is_not() {
    let t = repo();
    let d = t.path();
    write(d, "src/feature.rs", "pub fn f() {}\n");
    let v = run(d, "Done.", &[]).await;
    assert!(
        !v.refused(),
        "a new file next to its siblings: {:?}",
        v.reasons
    );

    write(d, "probe.sh", "curl localhost\n");
    write(d, "src/scratch_notes.txt", "x\n");
    write(d, "run.py", "print(1)\n");
    let v = run(d, "Done.", &[]).await;
    assert!(
        has(&v, "a scratch file was left in the work tree: probe.sh"),
        "{:?}",
        v.reasons
    );
    assert!(has(&v, "src/scratch_notes.txt"), "{:?}", v.reasons);
    assert!(
        has(
            &v,
            "a new file at the repository root, where nothing like it lives: run.py"
        ),
        "{:?}",
        v.reasons
    );
}

#[tokio::test]
async fn a_file_written_to_tmp_by_a_command_is_refused_reading_it_is_not() {
    let t = repo();
    let ran = |c: &str| Ran {
        command: c.into(),
        output: None,
        tool: None,
    };
    let v = run(t.path(), "Done.", &[ran("cat /tmp/x.log"), ran("ls /tmp/")]).await;
    assert!(!v.refused(), "{:?}", v.reasons);
    let v = run(
        t.path(),
        "Done.",
        &[
            ran("echo hi > /tmp/probe.sh"),
            ran("curl x | tee /tmp/out.txt"),
        ],
    )
    .await;
    assert!(
        has(&v, "outside the work tree: /tmp/probe.sh"),
        "{:?}",
        v.reasons
    );
    assert!(
        has(&v, "outside the work tree: /tmp/out.txt"),
        "{:?}",
        v.reasons
    );
}

// Incident (batch S09): git writes a non-ASCII path as `"tests/a\303\247\303\243o.rs"` in a diff header and in a plain
// `ls-files`. The guard read that text as the path, so `is_test_path` saw `.rs"` and a weakened or deleted test in a
// file named in Portuguese or Japanese went unseen.
#[tokio::test]
async fn tests_in_files_with_non_ascii_names_are_guarded_like_any_other() {
    let t = repo();
    let d = t.path();
    let two = "#[test]\nfn a() {\n    assert_eq!(1, 1);\n    assert!(true);\n}\n";
    write(d, "tests/ação.rs", two);
    write(d, "tests/日本.rs", two);
    git(d, &["add", "-A"]);
    git(d, &["commit", "-q", "-m", "tests with names"]);
    git(d, &["update-ref", "refs/remotes/origin/main", "HEAD"]);

    write(
        d,
        "tests/ação.rs",
        "#[test]\nfn a() {\n    assert_eq!(1, 1);\n}\n",
    );
    std::fs::remove_file(d.join("tests/日本.rs")).unwrap();
    let v = run(d, "Done.", &[]).await;
    assert!(
        has(&v, "removes 1 assertion(s) from tests/ação.rs"),
        "{:?}",
        v.reasons
    );
    assert!(
        has(&v, "deletes the test file tests/日本.rs"),
        "{:?}",
        v.reasons
    );
}

// Same incident, the stray-file rule: the new files are listed with `-z`, so a name with a letter outside ASCII
// (or a newline) is judged by its real name: a note next to the README is fine, a scratch log is not.
#[tokio::test]
async fn new_files_with_non_ascii_names_are_judged_by_their_real_name() {
    let t = repo();
    let d = t.path();
    write(d, "ação.md", "notes\n");
    write(d, "src/configuração.rs", "pub fn c() {}\n");
    write(d, "rascunho ação.log", "x\n");
    if cfg!(unix) {
        write(d, "src/new\nline.rs", "pub fn n() {}\n");
    }
    let v = run(d, "Done.", &[]).await;
    assert_eq!(
        v.reasons,
        ["a scratch file was left in the work tree: rascunho ação.log"],
        "only the scratch log is a stray"
    );
}

// ── 3. invented references ───────────────────────────────────────────────────

fn api_with(issues: &[u64]) -> Arc<FakeGithub> {
    let f = Arc::new(FakeGithub::new("bot"));
    for n in issues {
        f.issues.lock().insert(*n, serde_json::json!({"number": n}));
    }
    f
}

#[tokio::test]
async fn a_fake_issue_number_is_refused_and_a_real_one_passes() {
    let api = api_with(&[12]);
    let reasons = invented_refs("Fixes #12 and also #999", "o/r", api.as_ref()).await;
    assert_eq!(
        reasons,
        vec!["cites #999, which does not exist in o/r".to_string()]
    );
    assert!(
        invented_refs("Fixes #12", "o/r", api.as_ref())
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn references_in_commits_and_in_a_published_comment_are_checked() {
    let t = repo();
    let d = t.path();
    write(d, "src/feature.rs", "pub fn f() {}\n");
    git(d, &["add", "-A"]);
    git(d, &["commit", "-qm", "feat: closes #777"]);
    let api = api_with(&[12]);
    let cmds = [Ran {
        command: "gh pr create --body \"See #888\"".into(),
        output: None,
        tool: None,
    }];
    let v = review(
        &Input {
            work_dir: d,
            git: &crate::crew::gitrun::Git::local(),
            repo: Some("o/r"),
            result: "Opened, refs #12.",
            commands: &cmds,
        },
        api.as_ref(),
    )
    .await;
    assert!(has(&v, "#777"), "{:?}", v.reasons);
    assert!(has(&v, "#888"), "{:?}", v.reasons);
    assert!(!has(&v, "#12"), "{:?}", v.reasons);
}

#[test]
fn only_issue_style_references_are_found() {
    assert_eq!(
        refs_in("fix #12, see #7. Colour #123456 and a#4 and (#9) and #0"),
        vec![12, 7, 9]
    );
    assert_eq!(refs_in("#3 first"), vec![3]);
    assert!(refs_in("issue url https://x/y/#5 and &#39;").is_empty());
}

// ── 4. unproven claims ───────────────────────────────────────────────────────

fn ran(command: &str, output: Option<&str>) -> Ran {
    Ran {
        command: command.into(),
        output: output.map(str::to_string),
        tool: None,
    }
}

#[test]
fn an_unproven_claim_is_removed_and_a_proven_one_is_kept() {
    let text = "Added the cart total. All tests pass. Updated the docs.";
    let (out, removed) = strip_unproven(text, &[]);
    assert_eq!(out, "Added the cart total. Updated the docs.");
    assert_eq!(removed, vec!["All tests pass."]);

    let proof = ran(
        "cargo test -p cart",
        Some("test result: ok. 12 passed; 0 failed"),
    );
    let (out, removed) = strip_unproven(text, &[proof]);
    assert_eq!(out, text);
    assert!(removed.is_empty());
}

#[test]
fn a_command_without_output_or_with_a_failure_proves_nothing() {
    let text = "Verified: tests pass.";
    for c in [
        ran("cargo test", None),
        ran(
            "cargo test",
            Some("test result: FAILED. 1 passed; 2 failed"),
        ),
        ran("ls", Some("all good")),
    ] {
        let (out, removed) = strip_unproven(text, std::slice::from_ref(&c));
        assert!(out.is_empty() && removed.len() == 1, "{c:?}: {out:?}");
    }
}

#[test]
fn the_note_names_what_was_removed() {
    let note = removal_note(&["All tests pass.".to_string()]);
    assert!(
        note.contains("1 sentence(s)") && note.contains("\"All tests pass.\""),
        "{note}"
    );
}

#[tokio::test]
async fn review_strips_the_claim_and_leaves_the_diff_checks_alone() {
    let t = repo();
    let v = run(t.path(), "Fixed it. Tested and verified.", &[]).await;
    assert!(!v.refused());
    assert_eq!(v.result, "Fixed it.");
    assert_eq!(v.removed.len(), 1);
}

#[test]
fn a_retry_prompt_keeps_the_task_and_lists_every_reason() {
    let p = retry_prompt("Fix the cart", &["a".into(), "b".into()]);
    assert!(
        p.starts_with("Fix the cart") && p.contains("- a\n- b"),
        "{p}"
    );
}

#[tokio::test]
async fn iterator_skip_in_a_rust_test_is_not_a_skipped_test() {
    let t = repo();
    let d = t.path();
    write(
        d,
        "src/tests.rs",
        "#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n    assert!(add(0, 0) == 0);\n    assert_eq!([1, 2].iter().skip(1).count(), 1);\n}\n\n#[test]\nfn negative() {\n    assert_eq!(add(-1, -1), -2);\n}\n",
    );
    let v = run(d, "Done.", &[]).await;
    assert!(!v.refused(), "{:?}", v.reasons);
}

// Regression: every crewmate without a project works in a plain folder; refusing it for "could not
// read the diff" refused all of their work.
#[tokio::test]
async fn a_folder_that_is_not_a_git_work_tree_has_no_diff_to_refuse() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "notes.txt", "x\n");
    let v = run(t.path(), "Done.", &[]).await;
    assert!(!v.refused(), "{:?}", v.reasons);
}

#[tokio::test]
async fn atlas_own_files_under_dot_atlas_are_not_stray() {
    let t = repo();
    write(t.path(), ".atlas/attachments/t1/shot.png", "png");
    let v = run(t.path(), "Done.", &[]).await;
    assert!(!v.refused(), "{:?}", v.reasons);
}

// Regression (#333): with `diff.mnemonicPrefix` (c/ w/ i/) or `diff.noprefix` (no prefix at all) in
// the user's git config the guard misread its own diff, so a removed assertion went unnoticed.
#[tokio::test]
async fn guards_read_the_diff_whatever_prefix_the_users_git_config_asks_for() {
    for (key, value) in [("diff.mnemonicPrefix", "true"), ("diff.noprefix", "true")] {
        let t = repo();
        let d = t.path();
        git(d, &["config", key, value]);
        write(
            d,
            "src/tests.rs",
            "#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n}\n\n#[test]\nfn negative() {\n    assert_eq!(add(-1, -1), -2);\n}\n",
        );
        write(
            d,
            "ci.json",
            "{\n  \"timeout\": 30,\n  \"coverage_threshold\": 10\n}\n",
        );
        let v = run(d, "Done.", &[]).await;
        assert!(
            has(&v, "removes 1 assertion(s) from src/tests.rs"),
            "{key}: {:?}",
            v.reasons
        );
        assert!(has(&v, "ci.json"), "{key}: {:?}", v.reasons);
    }
}

// Regression (#414): with `diff.external` (difftastic and friends) in the user's git config `git diff` prints no
// `diff --git` header, so the guard saw no files and a removed assertion passed silently.
#[cfg(unix)]
#[tokio::test]
async fn guards_read_the_diff_whatever_external_diff_the_users_git_config_asks_for() {
    let t = repo();
    let d = t.path();
    git(d, &["config", "diff.external", "cat \"$5\" #"]);
    write(
        d,
        "src/tests.rs",
        "#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n}\n\n#[test]\nfn negative() {\n    assert_eq!(add(-1, -1), -2);\n}\n",
    );
    let v = run(d, "Done.", &[]).await;
    assert!(
        has(&v, "removes 1 assertion(s) from src/tests.rs"),
        "{:?}",
        v.reasons
    );
}

// Regression (security review of #448, B2): `git diff` in a repo the crewmate controls ran its
// `core.fsmonitor` as the crew user, who owns `hostd.token` and every crewmate's secrets. Through the helper
// it runs as the crewmate. The planted command records `$USER`: only the crewmate's may ever be there, and
// something must be there (else the test proves nothing).
#[cfg(unix)]
#[tokio::test]
async fn the_guards_never_run_a_crewmates_git_config_as_hostd() {
    use crate::crew::spawn::testing::{Rig, USER, rig};
    let rig = rig();
    let marker = rig.booby_trapped_repo();
    let hostd = tempfile::tempdir().unwrap();
    let store = crate::crew::store::CrewStore::new(hostd.path()).with_isolation(Some(rig.setup()));
    let work = rig.work();
    let git = store.git_in(&work);
    assert!(git.is_crewmate());
    let api = FakeGithub::new("bot");
    let v = review(
        &Input {
            work_dir: &work,
            git: &git,
            repo: None,
            result: "done",
            commands: &[],
        },
        &api,
    )
    .await;
    // The guards read the tree (the diff, the new files): git worked.
    assert!(
        !v.reasons.iter().any(|r| r.contains("could not")),
        "{:?}",
        v.reasons
    );
    let users = Rig::marker_users(&marker);
    assert!(
        !users.is_empty(),
        "no guard command triggered core.fsmonitor"
    );
    assert!(users.iter().all(|u| u == USER), "{users:?}");
}
