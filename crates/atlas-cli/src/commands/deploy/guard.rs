//! Refuse to ship a commit that would silently roll production back.
//!
//! Before packaging we read the sha the host is serving (`current` →
//! `releases/<id>/.atlas-release`, the same record the driver's noop check and
//! rollback use) and relate it to local `HEAD`. The decision is a pure function
//! ([`decide`]); [`Ancestry`] gathers its inputs from git.

use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use super::ctx::Ctx;
use super::remote::shq;

/// What the host is serving: the release id and the commit it was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployed {
    pub id: String,
    pub sha: String,
}

/// How local `HEAD` relates to the deployed commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// Same commit: the existing noop / `--force` behaviour applies.
    Same,
    /// `HEAD` descends from the deployed commit.
    Ahead,
    /// The deployed commit descends from `HEAD`: shipping `HEAD` rolls back.
    Downgrade,
    /// Neither contains the other.
    Diverged,
    /// The deployed commit is not in the local repository.
    Unknown,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Overrides {
    pub allow_downgrade: bool,
    pub allow_diverged: bool,
    pub allow_unknown_base: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Proceed,
    Refuse(Refusal),
}

/// Which override would let the deploy through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Downgrade,
    Diverged,
    UnknownBase,
}

impl Refusal {
    pub fn flag(self) -> &'static str {
        match self {
            Self::Downgrade => "--allow-downgrade",
            Self::Diverged => "--allow-diverged",
            Self::UnknownBase => "--allow-unknown-base",
        }
    }
}

/// The decision table from `docs/deploy.md`.
pub fn decide(rel: Relation, o: Overrides) -> Verdict {
    let (allowed, why) = match rel {
        Relation::Same | Relation::Ahead => return Verdict::Proceed,
        Relation::Downgrade => (o.allow_downgrade, Refusal::Downgrade),
        Relation::Diverged => (o.allow_diverged, Refusal::Diverged),
        Relation::Unknown => (o.allow_unknown_base, Refusal::UnknownBase),
    };
    if allowed {
        Verdict::Proceed
    } else {
        Verdict::Refuse(why)
    }
}

/// Parse the probe's output: `<release-id> <sha>` on one line, nothing when
/// there is no `current` or its record has no sha (a first deploy).
pub fn parse_probe(stdout: &str) -> Option<Deployed> {
    let line = stdout.lines().find(|l| !l.trim().is_empty())?;
    let (id, sha) = line.trim().split_once(' ')?;
    let sha = sha.trim();
    let hex = !sha.is_empty() && sha.bytes().all(|b| b.is_ascii_hexdigit());
    // Never interpolate host output into a git command line unchecked.
    (hex && sha.len() >= 7 && !id.is_empty()).then(|| Deployed {
        id: id.into(),
        sha: sha.to_ascii_lowercase(),
    })
}

/// The bash probe: id and sha of the release `current` points at.
fn probe_script(dir: &str) -> String {
    format!(
        r#"cd {d} 2>/dev/null && [ -L current ] || exit 0
id=$(basename "$(readlink current)")
sha=$(sed -n 's/.*"sha":"\([^"]*\)".*/\1/p' "releases/$id/.atlas-release" 2>/dev/null)
[ -n "$sha" ] && echo "$id $sha"; true"#,
        d = shq(dir)
    )
}

/// Local view of the two commits, from git.
#[derive(Debug, PartialEq, Eq)]
pub struct Ancestry {
    pub relation: Relation,
    /// Commits in `deployed..HEAD` (what this deploy adds).
    pub ahead: usize,
    /// Commits in `HEAD..deployed` (what this deploy drops).
    pub behind: usize,
    pub merge_base: Option<String>,
}

fn git_ok(root: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn git_out(root: &Path, args: &[&str]) -> Option<String> {
    let o = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn has_commit(root: &Path, sha: &str) -> bool {
    git_ok(root, &["cat-file", "-e", &format!("{sha}^{{commit}}")])
}

fn count(root: &Path, range: &str) -> usize {
    git_out(root, &["rev-list", "--count", range])
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Relate `head` to `deployed`. A deployed commit missing locally is fetched
/// from `origin` once (`git fetch origin <sha>`) before giving up.
pub fn ancestry(root: &Path, head: &str, deployed: &str) -> Ancestry {
    let unknown = Ancestry {
        relation: Relation::Unknown,
        ahead: 0,
        behind: 0,
        merge_base: None,
    };
    if !has_commit(root, deployed) {
        eprintln!("▸ deployed commit {deployed} not in this repo: git fetch origin");
        // Best effort: no remote, no network or a server refusing by-sha fetches
        // all land in the same "still unknown" answer below.
        let _ = Command::new("git")
            .args(["fetch", "--quiet", "origin", deployed])
            .current_dir(root)
            .output();
        if !has_commit(root, deployed) {
            return unknown;
        }
    }
    let range_ahead = format!("{deployed}..{head}");
    let range_behind = format!("{head}..{deployed}");
    let (ahead, behind) = (count(root, &range_ahead), count(root, &range_behind));
    let relation = if head == deployed {
        Relation::Same
    } else if git_ok(root, &["merge-base", "--is-ancestor", deployed, head]) {
        Relation::Ahead
    } else if git_ok(root, &["merge-base", "--is-ancestor", head, deployed]) {
        Relation::Downgrade
    } else {
        Relation::Diverged
    };
    Ancestry {
        relation,
        ahead,
        behind,
        merge_base: git_out(root, &["merge-base", head, deployed]).filter(|s| !s.is_empty()),
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

/// Refusal text: both shas, what would be dropped, and the override.
pub fn refusal_message(
    why: Refusal,
    head: &str,
    dep: &Deployed,
    a: &Ancestry,
    dropped: &str,
) -> String {
    let mut m = match why {
        Refusal::Downgrade => format!(
            "HEAD {} is OLDER than what is deployed ({} @ {}): this would roll production back by {} commit(s).",
            short(head),
            dep.id,
            short(&dep.sha),
            a.behind
        ),
        Refusal::Diverged => format!(
            "HEAD {} and the deployed {} @ {} have diverged (merge-base {}): +{} / -{} commits.",
            short(head),
            dep.id,
            short(&dep.sha),
            a.merge_base.as_deref().map_or("none", short),
            a.ahead,
            a.behind
        ),
        Refusal::UnknownBase => format!(
            "deployed commit {} ({}) is not in this repository and could not be fetched: cannot tell whether HEAD {} is newer.",
            short(&dep.sha),
            dep.id,
            short(head)
        ),
    };
    if !dropped.is_empty() {
        m.push_str("\nCommits that would be dropped (git log --oneline HEAD..deployed):\n");
        m.push_str(dropped);
    }
    m.push_str(&format!(
        "\nHEAD     {head}\ndeployed {}\nPass {} to ship anyway (to go back deliberately, `atlas deploy rollback`).",
        dep.sha,
        why.flag()
    ));
    m
}

/// `deploying <id> (<sha>) over <deployed-id> (<deployed-sha>): +N commits`.
pub fn announce(id: &str, head: &str, dep: &Deployed, a: &Ancestry) -> String {
    let delta = match a.relation {
        Relation::Same => "+0 commits (same commit)".to_string(),
        Relation::Ahead => format!("+{} commits", a.ahead),
        Relation::Downgrade => format!("-{} commits", a.behind),
        Relation::Diverged => format!("+{} / -{} commits (diverged)", a.ahead, a.behind),
        Relation::Unknown => "base unknown locally".to_string(),
    };
    format!(
        "deploying {id} ({}) over {} ({}): {delta}",
        short(head),
        dep.id,
        short(&dep.sha)
    )
}

/// Read what the host serves; `None` when nothing is deployed yet.
async fn read_deployed(ctx: &Ctx) -> Result<Option<Deployed>> {
    let out = ctx.remote.exec(&probe_script(&ctx.dir)).await?;
    if out.code != 0 {
        bail!(
            "cannot read the deployed release on {}: {}",
            ctx.target.host,
            out.stderr.trim()
        );
    }
    Ok(parse_probe(&out.stdout))
}

/// Run the guard for `atlas deploy`: the deployed release and its relation to
/// `head`, `None` on a first deploy, or an error when the deploy must not proceed.
pub async fn check(
    ctx: &Ctx,
    head: &str,
    overrides: Overrides,
) -> Result<Option<(Deployed, Ancestry)>> {
    let Some(dep) = read_deployed(ctx).await? else {
        return Ok(None); // first deploy
    };
    let a = ancestry(&ctx.root, head, &dep.sha);
    if let Verdict::Refuse(why) = decide(a.relation, overrides) {
        let dropped = if a.relation == Relation::Unknown {
            String::new()
        } else {
            git_out(
                &ctx.root,
                &["log", "--oneline", &format!("{head}..{}", dep.sha)],
            )
            .unwrap_or_default()
            .lines()
            .take(10)
            .collect::<Vec<_>>()
            .join("\n")
        };
        bail!("{}", refusal_message(why, head, &dep, &a, &dropped));
    }
    Ok(Some((dep, a)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Overrides = Overrides {
        allow_downgrade: false,
        allow_diverged: false,
        allow_unknown_base: false,
    };

    // Regression: `atlas deploy` from a stale checkout shipped an older commit
    // over production without a word (issue #170).
    #[test]
    fn decision_table_refuses_everything_but_same_and_ahead_by_default() {
        assert_eq!(decide(Relation::Same, NONE), Verdict::Proceed);
        assert_eq!(decide(Relation::Ahead, NONE), Verdict::Proceed);
        assert_eq!(
            decide(Relation::Downgrade, NONE),
            Verdict::Refuse(Refusal::Downgrade)
        );
        assert_eq!(
            decide(Relation::Diverged, NONE),
            Verdict::Refuse(Refusal::Diverged)
        );
        assert_eq!(
            decide(Relation::Unknown, NONE),
            Verdict::Refuse(Refusal::UnknownBase)
        );
    }

    #[test]
    fn each_override_unlocks_only_its_own_case() {
        let cases = [
            (
                Relation::Downgrade,
                Overrides {
                    allow_downgrade: true,
                    ..NONE
                },
            ),
            (
                Relation::Diverged,
                Overrides {
                    allow_diverged: true,
                    ..NONE
                },
            ),
            (
                Relation::Unknown,
                Overrides {
                    allow_unknown_base: true,
                    ..NONE
                },
            ),
        ];
        for (rel, o) in cases {
            assert_eq!(decide(rel, o), Verdict::Proceed, "{rel:?}");
            for (other, _) in cases.iter().filter(|(r, _)| *r != rel) {
                assert!(
                    matches!(decide(*other, o), Verdict::Refuse(_)),
                    "{rel:?} override must not unlock {other:?}"
                );
            }
        }
    }

    #[test]
    fn probe_output_parses_and_rejects_garbage() {
        let sha = "f1dc8a0f1dc8a0f1dc8a0f1dc8a0f1dc8a0abcd";
        assert_eq!(
            parse_probe(&format!("20251113120000-f1dc8a0 {sha}\n")),
            Some(Deployed {
                id: "20251113120000-f1dc8a0".into(),
                sha: sha.into()
            })
        );
        // No `current` / no sha recorded: nothing deployed.
        assert_eq!(parse_probe(""), None);
        assert_eq!(parse_probe("\n"), None);
        assert_eq!(parse_probe("20251113120000-f1dc8a0"), None);
        // Host output ends up in git argv: only hex shas get through.
        assert_eq!(parse_probe("id --upload-pack=evil"), None);
        assert_eq!(parse_probe("id abc"), None);
    }

    #[test]
    fn probe_script_quotes_the_dir() {
        assert!(probe_script("/opt/it's").contains(r"'/opt/it'\''s'"));
    }

    #[test]
    fn announcement_names_both_releases_and_the_commit_delta() {
        let dep = Deployed {
            id: "20251112000000-815dce2".into(),
            sha: "815dce2aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        };
        let a = Ancestry {
            relation: Relation::Ahead,
            ahead: 3,
            behind: 0,
            merge_base: None,
        };
        assert_eq!(
            announce(
                "20251113120000-f1dc8a0",
                "f1dc8a0bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                &dep,
                &a
            ),
            "deploying 20251113120000-f1dc8a0 (f1dc8a0) over 20251112000000-815dce2 (815dce2): +3 commits"
        );
    }

    #[test]
    fn refusal_messages_carry_both_shas_the_override_and_rollback_hint() {
        let dep = Deployed {
            id: "20251113120000-f1dc8a0".into(),
            sha: "f1dc8a0bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        };
        let head = "815dce2aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let down = Ancestry {
            relation: Relation::Downgrade,
            ahead: 0,
            behind: 4,
            merge_base: Some(head.into()),
        };
        let m = refusal_message(Refusal::Downgrade, head, &dep, &down, "f1dc8a0 feat: x");
        for needle in [
            head,
            dep.sha.as_str(),
            "--allow-downgrade",
            "atlas deploy rollback",
            "f1dc8a0 feat: x",
            "4 commit",
        ] {
            assert!(m.contains(needle), "{needle} missing in:\n{m}");
        }
        let div = Ancestry {
            relation: Relation::Diverged,
            ahead: 2,
            behind: 5,
            merge_base: Some("abc1234ffff".into()),
        };
        let m = refusal_message(Refusal::Diverged, head, &dep, &div, "");
        for needle in ["abc1234", "+2 / -5", "--allow-diverged"] {
            assert!(m.contains(needle), "{needle} missing in:\n{m}");
        }
        let unk = Ancestry {
            relation: Relation::Unknown,
            ahead: 0,
            behind: 0,
            merge_base: None,
        };
        let m = refusal_message(Refusal::UnknownBase, head, &dep, &unk, "");
        assert!(m.contains("--allow-unknown-base") && m.contains("could not be fetched"));
    }

    /// Real git, temp repo: the ancestry queries behind the decision.
    mod git {
        use super::*;

        fn g(dir: &Path, args: &[&str]) -> String {
            let o = Command::new("git")
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }

        fn commit(dir: &Path, name: &str) -> String {
            std::fs::write(dir.join(name), name).unwrap();
            g(dir, &["add", "-A"]);
            g(dir, &["commit", "-qm", name]);
            g(dir, &["rev-parse", "HEAD"])
        }

        // main: a - b - c ; side: a - b - x
        fn fixture() -> (tempfile::TempDir, [String; 4]) {
            let t = tempfile::tempdir().unwrap();
            let d = t.path();
            g(d, &["init", "-q", "-b", "main"]);
            let a = commit(d, "a");
            let b = commit(d, "b");
            let c = commit(d, "c");
            g(d, &["checkout", "-q", "-b", "side", &b]);
            let x = commit(d, "x");
            (t, [a, b, c, x])
        }

        #[test]
        fn relations_and_counts_match_git() {
            let (t, [a, b, c, x]) = fixture();
            let d = t.path();

            let same = ancestry(d, &b, &b);
            assert_eq!(same.relation, Relation::Same);

            // Deploying c over b: fast-forward by one commit.
            let ahead = ancestry(d, &c, &b);
            assert_eq!(
                (ahead.relation, ahead.ahead, ahead.behind),
                (Relation::Ahead, 1, 0)
            );

            // Deploying a over c (the R2K mistake): downgrade by two commits.
            let down = ancestry(d, &a, &c);
            assert_eq!(
                (down.relation, down.ahead, down.behind),
                (Relation::Downgrade, 0, 2)
            );
            assert_eq!(
                decide(down.relation, NONE),
                Verdict::Refuse(Refusal::Downgrade)
            );

            // c and x both descend from b only.
            let div = ancestry(d, &x, &c);
            assert_eq!(
                (div.relation, div.ahead, div.behind),
                (Relation::Diverged, 1, 1)
            );
            assert_eq!(div.merge_base.as_deref(), Some(b.as_str()));
        }

        #[test]
        fn unknown_sha_is_unknown_not_an_error_even_when_fetch_is_impossible() {
            let (t, [_, b, ..]) = fixture();
            // No `origin` remote: the fetch attempt must fail quietly.
            let ghost = "deadbeef".repeat(5);
            let a = ancestry(t.path(), &b, &ghost);
            assert_eq!(a.relation, Relation::Unknown);
            assert_eq!(
                decide(a.relation, NONE),
                Verdict::Refuse(Refusal::UnknownBase)
            );
        }

        #[test]
        fn unknown_sha_is_fetched_from_origin_when_it_exists_there() {
            let (t, [_, b, c, _]) = fixture();
            // A second clone that only has `b`; `c` lives in the origin.
            let clone = tempfile::tempdir().unwrap();
            // file:// so git really negotiates a fetch (a bare path hardlink-clones).
            let origin = format!("file://{}", t.path().display());
            g(
                clone.path(),
                &["clone", "-q", "--single-branch", "-b", "side", &origin, "."],
            );
            assert!(!has_commit(clone.path(), &c), "fixture: c must be missing");
            let head = g(clone.path(), &["rev-parse", "HEAD"]);
            let a = ancestry(clone.path(), &head, &c);
            assert_eq!(a.relation, Relation::Diverged, "fetched, then compared");
            assert_eq!(a.merge_base.as_deref(), Some(b.as_str()));
        }
    }
}
