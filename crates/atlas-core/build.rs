//! Embeds the commit this binary was built from as `ATLAS_COMMIT` (`atlas_core::buildinfo`): what a crash report and
//! `atlas version` name so a report can be matched to its source and its published symbols.
//!
//! In order: the `ATLAS_COMMIT` environment variable (a build from a source archive, a packager); `git rev-parse` in
//! a release build; `unknown`. Debug builds skip git on purpose: the commit changes on every commit, and a build
//! script input that changes would rebuild atlas-core and everything above it after each `git commit`.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn main() {
    println!("cargo:rerun-if-env-changed=ATLAS_COMMIT");
    println!("cargo:rerun-if-changed=build.rs");
    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    let commit = std::env::var("ATLAS_COMMIT")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            if !release {
                return None;
            }
            // A worktree's HEAD is not at `<root>/.git/HEAD`: ask git where it is.
            if let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) {
                println!("cargo:rerun-if-changed={head}");
            }
            git(&["rev-parse", "--short=12", "HEAD"])
        })
        .unwrap_or_else(|| "unknown".to_string());
    // The short form everywhere (`ATLAS_COMMIT` may be a full sha).
    let commit: String = commit.trim().chars().take(12).collect();
    println!("cargo:rustc-env=ATLAS_COMMIT={commit}");
}
