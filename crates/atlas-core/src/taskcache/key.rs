//! What a run depends on ([`Spec`]) and its content hash ([`Key`]).

use std::fmt;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use sha2::{Digest, Sha256};

/// Bumped when the hashing below changes, so old entries can never match a new key.
const KEY_VERSION: &str = "atlas-taskcache-key-v1";

/// A run's identity: 64 hex digits of sha256.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(pub(super) String);

impl Key {
    /// `None` unless `s` is a well-formed key (it names a file in the store, so it must be safe).
    pub fn parse(s: &str) -> Option<Self> {
        (s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| Self(s.to_lowercase()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Everything that decides whether two runs are the same run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// Where the command runs; `inputs` and `outputs` are relative to it. Not part of the key.
    pub cwd: PathBuf,
    /// The command: one word is a shell command line, several are an argv.
    pub argv: Vec<String>,
    /// Globs of the files the command reads (`.gitignore` respected). Empty: every file under
    /// `cwd` that git would not ignore.
    pub inputs: Vec<String>,
    /// Paths (files or directories) the command produces, restored on a hit.
    pub outputs: Vec<String>,
    /// Names of the environment variables the command reads; their values are part of the key.
    pub env: Vec<String>,
    /// Programs whose `--version` is part of the key.
    pub tools: Vec<String>,
}

impl Spec {
    /// Hash the argv, the declared globs, env values, tool versions and the bytes of every input.
    /// Reads the files as they are now, so uncommitted edits count.
    pub fn key(&self) -> io::Result<Key> {
        self.check_outputs()?;
        let mut h = Sha256::new();
        field(&mut h, KEY_VERSION.as_bytes());
        list(&mut h, &self.argv);
        list(&mut h, &self.inputs);
        list(&mut h, &self.outputs);
        for name in &self.env {
            field(&mut h, name.as_bytes());
            match std::env::var_os(name) {
                // Unset and empty must differ: a program may test for either.
                None => field(&mut h, b"\0unset"),
                Some(v) => {
                    field(&mut h, b"\0set");
                    field(&mut h, v.as_encoded_bytes());
                }
            }
        }
        for tool in &self.tools {
            field(&mut h, tool.as_bytes());
            field(&mut h, tool_version(tool)?.as_bytes());
        }
        for rel in self.input_files()? {
            // A path (forward slashes on every OS) then the bytes: a rename changes the key.
            field(&mut h, rel.as_bytes());
            hash_file(&mut h, &self.cwd.join(&rel))?;
        }
        Ok(Key(hex::encode(h.finalize())))
    }

    /// Outputs are paths inside `cwd`: restoring one deletes what is there first.
    fn check_outputs(&self) -> io::Result<()> {
        for o in &self.outputs {
            let p = Path::new(o);
            let ok = !o.trim().is_empty()
                && p.is_relative()
                && p.components().all(|c| matches!(c, Component::Normal(_)))
                && !o.contains(['\\', '\0']);
            if !ok {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "output {o:?} must be a path inside the project (no `..`, no leading `/`, not `.`)"
                    ),
                ));
            }
        }
        Ok(())
    }

    /// The input files, relative to `cwd`, sorted, `/`-separated: what the globs match among the
    /// files git would not ignore (an override glob alone would beat `.gitignore`, so the globs
    /// only filter the walk, they do not drive it).
    fn input_files(&self) -> io::Result<Vec<String>> {
        let mut walk = WalkBuilder::new(&self.cwd);
        // `.gitignore` applies without a `.git` (a plain folder, a fresh worktree being set up);
        // hidden files are inputs too (`.env.example`, `.github/`), except git's own database.
        walk.hidden(false)
            .require_git(false)
            .parents(false)
            .follow_links(false)
            .filter_entry(|e| e.file_name() != ".git");
        let globs = if self.inputs.is_empty() {
            None
        } else {
            let mut ov = OverrideBuilder::new(&self.cwd);
            for g in &self.inputs {
                ov.add(g).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidInput, format!("bad glob {g:?}: {e}"))
                })?;
            }
            Some(
                ov.build()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?,
            )
        };
        let mut files = Vec::new();
        for entry in walk.build() {
            let entry = entry.map_err(|e| io::Error::other(e.to_string()))?;
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            if globs
                .as_ref()
                .is_some_and(|g| !g.matched(entry.path(), false).is_whitelist())
            {
                continue;
            }
            let rel = entry.path().strip_prefix(&self.cwd).unwrap_or(entry.path());
            files.push(rel.to_string_lossy().replace('\\', "/"));
        }
        files.sort();
        Ok(files)
    }
}

/// Length-prefixed, so `["ab","c"]` and `["a","bc"]` differ.
fn field(h: &mut Sha256, b: &[u8]) {
    h.update((b.len() as u64).to_le_bytes());
    h.update(b);
}

fn list(h: &mut Sha256, items: &[String]) {
    h.update((items.len() as u64).to_le_bytes());
    for i in items {
        field(h, i.as_bytes());
    }
}

/// A file's size and bytes, streamed.
fn hash_file(h: &mut Sha256, path: &Path) -> io::Result<()> {
    let mut f = std::fs::File::open(path)?;
    h.update(f.metadata()?.len().to_le_bytes());
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        h.update(&buf[..n]);
    }
}

/// What `tool --version` prints (stdout and stderr, trimmed). A tool that is missing or that
/// fails is an error: a key built without it would hit across a toolchain change.
fn tool_version(tool: &str) -> io::Result<String> {
    let mut cmd = Command::new(tool);
    cmd.arg("--version").stdin(Stdio::null()).env("LC_ALL", "C");
    let out = cmd.output().map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot run `{tool} --version` for the cache key: {e}"),
        )
    })?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "`{tool} --version` exited with {} (the cache key needs the tool's version)",
            out.status
        )));
    }
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(s.trim().to_string())
}
