//! Whose checkout the gate is working in, and so who may open what in it (security review 5, N1).
//!
//! hostd runs as the crew user, which reaches every crewmate's tree and the crew store. A crewmate that runs as its
//! own Linux user controls its tree completely: a link it plants where the gate reads `atlas.yaml` or writes the
//! regression registry aims hostd at another crewmate's files or at `hostd.token`, and a command its `atlas.yaml`
//! names runs as the crew user. For such a tree ([`Tree::Isolated`]) the gate therefore
//!
//! - reads and writes files only with the `*_beneath` helpers (no link followed anywhere on the path);
//! - runs git through the spawn helper, as the crewmate (its `.git/config` is its own code);
//! - lists the files of the cache key with that git, not with a directory walk;
//! - never runs the project's `affected.command`: there is no way to run an arbitrary shell line as the crewmate
//!   (the helper starts engine binaries and git only), and as hostd it is the crew user's code.

use std::io;
use std::path::Path;

use atlas_core::taskcache::{Key, Spec};

use crate::crew::gitrun::Git;
use crate::crew::spawn::layout;

/// The most the gate reads from a file of the project (`atlas.yaml`, the registry).
const MAX_FILE: u64 = 1 << 20;

/// Whose tree it is.
#[derive(Clone)]
pub enum Tree {
    /// Nobody else's: a project path on the server, or a crewmate that shares the crew user's account (nothing
    /// separates it from hostd, so there is nothing to protect).
    Own,
    /// An isolated crewmate's checkout; `Git` runs git as that crewmate.
    Isolated(Git),
}

impl Tree {
    /// The text of `root/rel`, `None` when it does not exist. An isolated tree's file is opened beneath `root`: a
    /// link anywhere on the way, or a file that is not a regular one, is an error (it never names its target).
    pub fn read_text(&self, root: &Path, rel: &str) -> Result<Option<String>, String> {
        let read = match self {
            // tree-access: own tree (`Tree::Own`); an isolated tree is read beneath the root below
            Self::Own => std::fs::read_to_string(root.join(rel)),
            Self::Isolated(_) => layout::read_beneath(root, rel, MAX_FILE),
        };
        match read {
            Ok(t) => Ok(Some(t)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("ui gate: cannot read {rel}: {}", e)),
        }
    }

    /// `text` as `root/rel`, its directories made on the way. An own tree gets an atomic replace; an isolated
    /// one is written in place, without following a link anywhere on the path.
    pub fn write_text(&self, root: &Path, rel: &str, text: &str) -> Result<(), String> {
        let wrote = match self {
            Self::Own => {
                let p = root.join(rel);
                // tree-access: own tree (`Tree::Own`); an isolated tree is written beneath the root below
                std::fs::create_dir_all(p.parent().unwrap_or(root)).and_then(|()| {
                    // Atomic: a crash mid-write must not leave a file `load` refuses.
                    atlas_core::fs::write_atomic(&p, text.as_bytes(), atlas_core::fs::Mode::Default)
                })
            }
            Self::Isolated(_) => layout::write_beneath(root, rel, &mut text.as_bytes()),
        };
        wrote.map_err(|e| format!("ui gate: cannot write {rel}: {}", e))
    }

    /// The files the change touches in `root`, as `git` reports them (as the crewmate for an isolated tree).
    pub fn changed_files(&self, root: &Path) -> Result<Vec<String>, String> {
        match self {
            Self::Own => atlas_core::affected::changed_files(root, None),
            Self::Isolated(git) => atlas_core::affected::changed_files_with(root, None, &|args| {
                git_out(git, root, args).map(|b| String::from_utf8_lossy(&b).into_owned())
            }),
        }
    }

    /// The task cache's key of `spec` (the bytes of the files of `spec.cwd`). For an isolated tree the files are
    /// the ones git lists as the crewmate (tracked and untracked, not ignored: what the directory walk of an own
    /// tree finds) and each is opened beneath the root; a link and a file that is gone are skipped, as the walk
    /// does not take them. A tree that is not a git checkout has no key: the gate captures again.
    pub fn cache_key(&self, spec: &Spec) -> io::Result<Key> {
        match self {
            Self::Own => spec.key(),
            Self::Isolated(git) => spec.key_with(
                &|| {
                    let listed = git_out(
                        git,
                        &spec.cwd,
                        &[
                            "ls-files",
                            "-z",
                            "--cached",
                            "--others",
                            "--exclude-standard",
                        ],
                    )
                    .map_err(io::Error::other)?;
                    let mut rels: Vec<String> = listed
                        .split(|b| *b == 0)
                        .filter(|p| !p.is_empty())
                        .filter_map(|p| std::str::from_utf8(p).ok().map(str::to_string))
                        .collect();
                    rels.sort();
                    rels.dedup();
                    Ok(rels)
                },
                &|rel| open_input(&spec.cwd, rel),
            ),
        }
    }
}

/// An input of the key: the file, or `None` for what the directory walk of an own tree does not take either: a file
/// that is gone, a link (refused by the open, never followed) and anything that is not a regular file.
// tree-access: a type name; the open itself is `layout::open_regular_beneath`
fn open_input(root: &Path, rel: &str) -> io::Result<Option<std::fs::File>> {
    match layout::open_regular_beneath(root, rel) {
        Ok(f) => Ok(Some(f)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) if e.kind() == io::ErrorKind::InvalidInput => Ok(None),
        Err(e) if layout::is_link_refusal(&e) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Stdout of `git args` in `cwd`, or git's own words when it fails.
fn git_out(git: &Git, cwd: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = git.run_sync(cwd, args)?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
