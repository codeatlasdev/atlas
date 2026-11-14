//! Behavior of "what does this change affect" on real files and real git repos in temp dirs.
//! Guards issue #128: an agent decides what to run without starting a process.

use std::fs;
use std::path::Path;
use std::process::Command;

use super::*;

fn write(p: &Path, s: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C"])
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

fn rule(globs: &[&str]) -> Rule {
    Rule {
        matches: strs(globs),
        ..Rule::default()
    }
}

// ---- the declared graph ----

#[test]
fn declared_rules_map_changed_files_to_tasks_tests_routes_and_builds() {
    let rules = vec![
        Rule {
            tasks: strs(&["Run checks"]),
            tests: strs(&["cargo test -p atlas-core"]),
            routes: strs(&["/docs"]),
            builds: strs(&["site"]),
            ..rule(&["crates/atlas-core/**"])
        },
        Rule {
            tests: strs(&["bun test web"]),
            ..rule(&["apps/web/**/*.ts", "apps/web/package.json"])
        },
    ];
    let files = strs(&["crates/atlas-core/src/lib.rs", "README.md"]);
    let got = from_rules(&rules, &files).unwrap();
    assert_eq!(got.tasks, ["Run checks"]);
    assert_eq!(got.tests, ["cargo test -p atlas-core"]);
    assert_eq!(got.routes, ["/docs"]);
    assert_eq!(got.builds, ["site"]);
    // `**` crosses folders; the second rule did not match anything.
    assert!(!got.tests.contains(&"bun test web".to_string()));
}

#[test]
fn declared_rules_union_without_duplicates_and_keep_first_seen_order() {
    let rules = vec![
        Rule {
            tests: strs(&["b", "a"]),
            ..rule(&["x/**"])
        },
        Rule {
            tests: strs(&["a", "c"]),
            ..rule(&["y/**"])
        },
    ];
    let got = from_rules(&rules, &strs(&["x/1", "y/2", "x/3"])).unwrap();
    assert_eq!(got.tests, ["b", "a", "c"]);
}

#[test]
fn a_bad_glob_in_a_rule_is_an_error_naming_it() {
    let e = from_rules(&[rule(&["a/[**"])], &strs(&["a/b"])).unwrap_err();
    assert!(e.contains("a/[**"), "{e}");
}

#[test]
fn no_changed_files_affect_nothing() {
    let got = from_rules(&[rule(&["**"])], &[]).unwrap();
    assert_eq!(got, Impact::default());
}

// ---- the fallback: actions whose cache inputs match ----

#[test]
fn fallback_picks_actions_whose_cache_inputs_match_a_changed_file() {
    let actions = vec![
        ActionInputs {
            label: "Run checks".into(),
            inputs: strs(&["crates/**", "Cargo.toml"]),
        },
        ActionInputs {
            label: "Site build".into(),
            inputs: strs(&["site/**"]),
        },
    ];
    let got = from_action_inputs(&actions, &strs(&["crates/a/src/lib.rs"])).unwrap();
    assert_eq!(got.tasks, ["Run checks"]);
    assert!(got.tests.is_empty() && got.routes.is_empty() && got.builds.is_empty());
}

#[test]
fn fallback_treats_an_action_with_no_input_globs_as_affected_by_any_change() {
    // `cache:` without `inputs` hashes every file git would not ignore: any change moves its key.
    let actions = vec![ActionInputs {
        label: "Everything".into(),
        inputs: vec![],
    }];
    assert_eq!(
        from_action_inputs(&actions, &strs(&["a.txt"]))
            .unwrap()
            .tasks,
        ["Everything"]
    );
    assert!(from_action_inputs(&actions, &[]).unwrap().tasks.is_empty());
}

// ---- the plugin command ----

#[cfg(unix)]
#[test]
fn plugin_receives_changed_files_on_stdin_and_its_json_is_the_answer() {
    let t = tempfile::tempdir().unwrap();
    // Echoes how many files it was given as a "test" so the stdin contract is observable.
    let script = r#"n=$(wc -l | tr -d ' '); printf '{"tasks":["t"],"tests":["saw %s"],"routes":["/r"],"builds":["b"]}' "$n""#;
    let got = from_command(script, t.path(), &strs(&["a.rs", "b.rs"])).unwrap();
    assert_eq!(got.tests, ["saw 2"]);
    assert_eq!(got.tasks, ["t"]);
    assert_eq!(got.routes, ["/r"]);
    assert_eq!(got.builds, ["b"]);
}

#[cfg(unix)]
#[test]
fn plugin_may_omit_keys_and_runs_in_the_project_root() {
    let t = tempfile::tempdir().unwrap();
    write(&t.path().join("marker.txt"), "x");
    let got = from_command(
        r#"printf '{"tests":["%s"]}' "$(ls marker.txt)""#,
        t.path(),
        &strs(&["f"]),
    )
    .unwrap();
    assert_eq!(got.tests, ["marker.txt"]);
    assert!(got.tasks.is_empty() && got.routes.is_empty() && got.builds.is_empty());
}

#[cfg(unix)]
#[test]
fn plugin_failure_and_garbage_output_are_errors_with_the_reason() {
    let t = tempfile::tempdir().unwrap();
    let e = from_command("echo boom >&2; exit 3", t.path(), &[]).unwrap_err();
    assert!(
        e.contains("exit") && e.contains('3') && e.contains("boom"),
        "{e}"
    );
    let e = from_command("echo not json", t.path(), &[]).unwrap_err();
    assert!(e.contains("JSON"), "{e}");
}

// ---- changed files from git ----

/// A repo whose `main` has one commit, plus a feature branch checked out.
fn repo(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    write(&root.join("keep.txt"), "1");
    write(&root.join("src/a.rs"), "1");
    write(&root.join("src/b.rs"), "1");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "init"]);
    git(root, &["checkout", "-q", "-b", "feature"]);
}

#[test]
fn changed_files_cover_committed_staged_unstaged_and_untracked_but_not_ignored() {
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    repo(r);
    write(&r.join(".gitignore"), "target\n");
    git(r, &["add", ".gitignore"]);
    git(r, &["commit", "-q", "-m", "ignore"]);
    // committed on the branch
    write(&r.join("src/a.rs"), "2");
    git(r, &["commit", "-q", "-am", "edit a"]);
    // staged
    write(&r.join("src/new.rs"), "n");
    git(r, &["add", "src/new.rs"]);
    // unstaged (tracked file edited after the base)
    write(&r.join("src/b.rs"), "2");
    // untracked, plus an ignored file that must not show
    write(&r.join("docs/x.md"), "x");
    write(&r.join("target/out.bin"), "ignored");

    let got = changed_files(r, Some("main")).unwrap();
    assert_eq!(
        got,
        [
            ".gitignore",
            "docs/x.md",
            "src/a.rs",
            "src/b.rs",
            "src/new.rs"
        ]
    );
}

#[test]
fn changed_files_use_the_merge_base_not_the_tip_of_the_base() {
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    repo(r);
    write(&r.join("src/a.rs"), "feature");
    git(r, &["commit", "-q", "-am", "feature edit"]);
    // main moves on after the branch point: its edit is NOT this change's.
    git(r, &["checkout", "-q", "main"]);
    write(&r.join("keep.txt"), "main moved");
    git(r, &["commit", "-q", "-am", "main edit"]);
    git(r, &["checkout", "-q", "feature"]);

    assert_eq!(changed_files(r, Some("main")).unwrap(), ["src/a.rs"]);
}

#[test]
fn changed_files_without_a_base_default_to_the_repos_main_branch() {
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    repo(r);
    write(&r.join("src/a.rs"), "2");
    assert_eq!(changed_files(r, None).unwrap(), ["src/a.rs"]);
}

#[test]
fn changed_files_with_an_unknown_base_is_an_error_naming_it() {
    let t = tempfile::tempdir().unwrap();
    repo(t.path());
    let e = changed_files(t.path(), Some("no-such-ref")).unwrap_err();
    assert!(e.contains("no-such-ref"), "{e}");
}

#[test]
fn changed_files_outside_a_git_repo_is_an_error_saying_to_pass_files() {
    let t = tempfile::tempdir().unwrap();
    let e = changed_files(t.path(), None).unwrap_err();
    assert!(e.contains("files"), "{e}");
}

#[test]
fn changed_files_paths_are_relative_to_the_project_root_in_a_subfolder() {
    // The atlas.yaml folder may sit below the repo root: paths are relative to it, and files
    // outside it are not its business.
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    repo(r);
    write(&r.join("src/a.rs"), "2");
    write(&r.join("keep.txt"), "2");
    assert_eq!(
        changed_files(&r.join("src"), Some("main")).unwrap(),
        ["a.rs"]
    );
}

// ---- the whole resolution order ----

#[test]
fn command_beats_rules_and_rules_beat_the_fallback() {
    let t = tempfile::tempdir().unwrap();
    let files = strs(&["a.rs"]);
    let actions = vec![ActionInputs {
        label: "act".into(),
        inputs: vec![],
    }];
    let rules = vec![Rule {
        tests: strs(&["from rules"]),
        ..rule(&["**"])
    }];

    // The command is a POSIX `printf`: only checked where `sh` runs it.
    #[cfg(unix)]
    {
        let declared = Declared {
            command: Some(r#"printf '{"tests":["from command"]}'"#.into()),
            rules: rules.clone(),
        };
        let (src, got) = resolve(Some(&declared), &actions, t.path(), &files).unwrap();
        assert_eq!((src, got.tests), (Source::Command, strs(&["from command"])));
    }
    let only_rules = Declared {
        command: None,
        rules,
    };
    let (src, got) = resolve(Some(&only_rules), &actions, t.path(), &files).unwrap();
    assert_eq!((src, got.tests), (Source::Rules, strs(&["from rules"])));

    let (src, got) = resolve(None, &actions, t.path(), &files).unwrap();
    assert_eq!((src, got.tasks), (Source::CacheInputs, strs(&["act"])));
}

#[test]
fn an_empty_declaration_falls_back_to_cache_inputs() {
    let t = tempfile::tempdir().unwrap();
    let actions = vec![ActionInputs {
        label: "act".into(),
        inputs: strs(&["**"]),
    }];
    let (src, _) = resolve(
        Some(&Declared::default()),
        &actions,
        t.path(),
        &strs(&["a"]),
    )
    .unwrap();
    assert_eq!(src, Source::CacheInputs);
}

#[cfg(unix)]
#[test]
fn a_failing_declared_command_is_an_error_not_a_silent_fallback() {
    let t = tempfile::tempdir().unwrap();
    let d = Declared {
        command: Some("exit 1".into()),
        rules: vec![],
    };
    assert!(resolve(Some(&d), &[], t.path(), &strs(&["a"])).is_err());
}
