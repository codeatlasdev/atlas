//! Validation of every [`Request`] before the helper acts on it. Pure functions: no filesystem, no
//! process, no uid lookups, so every rejection is a plain unit test.
//!
//! The rules are the helper's whole defence, since it runs as root: a fixed user shape, an argv
//! that is data (never a shell) with an allowed binary, a cwd inside that crewmate's directory,
//! and an environment without anything that changes how a binary loads.

use std::fmt;
use std::path::Path;

use super::protocol::{MAX_MESSAGE, Request, SpawnReq};

/// Prefix of every crewmate's Linux user.
pub const USER_PREFIX: &str = "atlas-cm-";
/// Parent of every crewmate's directory (`<CM_ROOT>/<slug>/{home,work,crew-runtime}`).
pub const CM_ROOT: &str = "/var/lib/atlas-cm";
/// The only binaries a crewmate can be started as (`atlas_core::agents`), by bare name.
pub const ENGINE_BINARIES: [&str; 4] = ["claude", "codex", "omp", "opencode"];
/// Signals hostd may send: HUP, INT, KILL, TERM, CONT.
pub const SIGNALS: [i32; 5] = [1, 2, 9, 15, 18];
const MAX_ARGS: usize = 256;
const MAX_ENV: usize = 128;
const MAX_ITEM: usize = 16 * 1024;
const MAX_DIM: u16 = 1000;
/// Hex characters of a crewmate id kept in its user name (Linux user names are at most 32 bytes).
const SLUG_LEN: usize = 12;

/// Why a request was refused. Reasons name the rule, never echo large input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    BadUser(String),
    BadBinary(String),
    BadArg(&'static str),
    BadCwd(String),
    BadEnv(String),
    BadSignal(i32),
    BadSize,
    TooLarge,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadUser(u) => write!(f, "not a provisioned crewmate user: {u:?}"),
            Self::BadBinary(b) => write!(f, "binary not allowed: {b:?}"),
            Self::BadArg(why) => write!(f, "bad argv: {why}"),
            Self::BadCwd(c) => write!(f, "cwd outside the crewmate directory: {c:?}"),
            Self::BadEnv(k) => write!(f, "environment variable not allowed: {k:?}"),
            Self::BadSignal(s) => write!(f, "signal not allowed: {s}"),
            Self::BadSize => f.write_str("terminal size out of range"),
            Self::TooLarge => f.write_str("message too large"),
        }
    }
}

impl std::error::Error for Refused {}

/// The user name of a crewmate: `atlas-cm-` and the first 12 hex digits of its id
/// (`3f2a9c1e-...` → `atlas-cm-3f2a9c1e....`). `None` when `id` is not a lowercase UUID.
pub fn user_for(id: &str) -> Option<String> {
    let hex: String = id.chars().filter(|c| *c != '-').collect();
    let shape = id.len() == 36
        && id.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_digit() || ('a'..='f').contains(&c),
        });
    shape.then(|| format!("{USER_PREFIX}{}", &hex[..SLUG_LEN]))
}

/// The slug of a valid user name (`atlas-cm-<12 hex>`), the directory name under [`CM_ROOT`].
pub fn slug_of(user: &str) -> Result<&str, Refused> {
    user.strip_prefix(USER_PREFIX)
        .filter(|s| {
            s.len() == SLUG_LEN
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or_else(|| Refused::BadUser(clip(user)))
}

/// The directory every path of this crewmate lives under.
pub fn dir_of(user: &str) -> Result<String, Refused> {
    dir_in(CM_ROOT, user)
}

/// [`dir_of`] under another root (tests use a temp dir; production is always [`CM_ROOT`]).
pub fn dir_in(root: &str, user: &str) -> Result<String, Refused> {
    Ok(format!("{root}/{}", slug_of(user)?))
}

fn clip(s: &str) -> String {
    s.chars().take(64).collect()
}

/// Check a whole request. `body_len` is the size of the JSON it arrived in.
pub fn check(req: &Request, body_len: usize) -> Result<(), Refused> {
    check_in(CM_ROOT, req, body_len)
}

/// [`check`] with the crewmates' parent directory given (the helper validates against the root it
/// was configured with, the same one `exec` re-checks).
pub fn check_in(root: &str, req: &Request, body_len: usize) -> Result<(), Refused> {
    if body_len > MAX_MESSAGE {
        return Err(Refused::TooLarge);
    }
    match req {
        Request::Spawn(s) => check_spawn_in(root, s),
        Request::Signal { sig, .. } => SIGNALS
            .contains(sig)
            .then_some(())
            .ok_or(Refused::BadSignal(*sig)),
        Request::Reap { user } => slug_of(user).map(|_| ()),
        Request::Wait { .. } | Request::Cwd { .. } => Ok(()),
    }
}

/// The rules for one spawn request (also what [`check`] runs for `Request::Spawn`).
pub fn check_spawn(s: &SpawnReq) -> Result<(), Refused> {
    check_spawn_in(CM_ROOT, s)
}

/// [`check_spawn`] with the crewmates' parent directory given.
pub fn check_spawn_in(root: &str, s: &SpawnReq) -> Result<(), Refused> {
    let dir = dir_in(root, &s.user)?;
    check_argv(&s.argv)?;
    check_cwd(&s.cwd, &dir)?;
    check_env(s)?;
    if s.cols == 0 || s.rows == 0 || s.cols > MAX_DIM || s.rows > MAX_DIM {
        return Err(Refused::BadSize);
    }
    Ok(())
}

fn check_argv(argv: &[String]) -> Result<(), Refused> {
    let bin = argv.first().ok_or(Refused::BadArg("empty"))?;
    if !ENGINE_BINARIES.contains(&bin.as_str()) {
        return Err(Refused::BadBinary(clip(bin)));
    }
    if argv.len() > MAX_ARGS {
        return Err(Refused::BadArg("too many arguments"));
    }
    for a in argv {
        if a.len() > MAX_ITEM {
            return Err(Refused::BadArg("argument too long"));
        }
        if a.contains('\0') {
            return Err(Refused::BadArg("NUL byte"));
        }
    }
    Ok(())
}

/// `cwd` is absolute, has no empty, `.` or `..` segment, and is `dir` or below it. Lexical on
/// purpose: the helper does the `chdir` only after dropping to the crewmate's user, so a symlink
/// inside the crewmate's own tree cannot lead anywhere that user could not already go.
fn check_cwd(cwd: &str, dir: &str) -> Result<(), Refused> {
    let bad = || Refused::BadCwd(clip(cwd));
    // `Path::components` folds `/./` and `//`, so the raw segments are checked as text.
    let segments_ok = cwd.strip_prefix('/').is_some_and(|rest| {
        rest.split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && !s.contains('\0'))
    });
    if !segments_ok && cwd != "/" {
        return Err(bad());
    }
    Path::new(cwd)
        .starts_with(dir)
        .then_some(())
        .ok_or_else(bad)
}

/// Variable names the child can never set: they change how any binary loads or what it trusts.
fn env_forbidden(k: &str) -> bool {
    k.starts_with("LD_")
        || k.starts_with("DYLD_")
        || matches!(
            k,
            "BASH_ENV" | "ENV" | "IFS" | "PATH" | "HOME" | "USER" | "LOGNAME" | "SHELL"
        )
}

fn check_env(s: &SpawnReq) -> Result<(), Refused> {
    if s.env.len() > MAX_ENV {
        return Err(Refused::BadEnv("too many variables".into()));
    }
    for (k, v) in &s.env {
        let name_ok = !k.is_empty()
            && !k.starts_with(|c: char| c.is_ascii_digit())
            && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if !name_ok || env_forbidden(k) || v.contains('\0') || v.len() > MAX_ITEM {
            return Err(Refused::BadEnv(clip(k)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::spawn::protocol::Mode;
    use std::collections::BTreeMap;

    const ID: &str = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
    const USER: &str = "atlas-cm-3f2a9c1e5b7d";

    fn spawn() -> SpawnReq {
        SpawnReq {
            user: USER.into(),
            argv: vec!["claude".into(), "-p".into(), "hi; rm -rf /".into()],
            env: BTreeMap::new(),
            cwd: format!("{CM_ROOT}/3f2a9c1e5b7d/work"),
            mode: Mode::Pty,
            cols: 120,
            rows: 40,
        }
    }

    fn refused(f: impl FnOnce(&mut SpawnReq)) -> Refused {
        let mut s = spawn();
        f(&mut s);
        check(&Request::Spawn(s), 100).unwrap_err()
    }

    #[test]
    fn a_well_formed_spawn_is_accepted_and_shell_text_in_argv_is_just_data() {
        assert_eq!(check(&Request::Spawn(spawn()), 100), Ok(()));
    }

    #[test]
    fn the_user_of_a_crewmate_is_its_prefix_and_twelve_id_digits() {
        assert_eq!(user_for(ID).as_deref(), Some(USER));
        for bad in [
            "",
            "3f2a9c1e",
            "3F2A9C1E-5B7D-4E8F-9A0B-1C2D3E4F5A6B",
            "../../etc/passwd",
            &format!("{ID}x"),
        ] {
            assert_eq!(user_for(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_a_provisioned_shaped_user_is_accepted() {
        for bad in [
            "root",
            "atlas-crew",
            "atlas-cm-",
            "atlas-cm-3f2a9c1e5b7",
            "atlas-cm-3f2a9c1e5b7dd",
            "atlas-cm-3F2A9C1E5B7D",
            "atlas-cm-3f2a9c1e5b7/",
            "atlas-cm-../..",
            "x-atlas-cm-3f2a9c1e5b7d",
            "atlas-cm-3f2a9c1e5b7d\0",
        ] {
            assert!(
                matches!(refused(|s| s.user = bad.into()), Refused::BadUser(_)),
                "{bad}"
            );
        }
    }

    #[test]
    fn only_the_four_engine_binaries_by_bare_name_may_run() {
        for ok in ENGINE_BINARIES {
            let mut s = spawn();
            s.argv = vec![ok.into()];
            assert_eq!(check(&Request::Spawn(s), 1), Ok(()));
        }
        for bad in [
            "sh",
            "bash",
            "/bin/sh",
            "/usr/bin/claude",
            "./claude",
            "claude ",
            "Claude",
            "env",
            "sudo",
            "su",
            "",
        ] {
            assert!(
                matches!(refused(|s| s.argv[0] = bad.into()), Refused::BadBinary(_)),
                "{bad:?}"
            );
        }
        assert_eq!(refused(|s| s.argv.clear()), Refused::BadArg("empty"));
    }

    #[test]
    fn argv_limits_and_nul_bytes_are_refused() {
        assert_eq!(
            refused(|s| s.argv.extend((0..MAX_ARGS).map(|i| i.to_string()))),
            Refused::BadArg("too many arguments")
        );
        assert_eq!(
            refused(|s| s.argv.push("x".repeat(MAX_ITEM + 1))),
            Refused::BadArg("argument too long")
        );
        assert_eq!(
            refused(|s| s.argv.push("a\0b".into())),
            Refused::BadArg("NUL byte")
        );
    }

    #[test]
    fn cwd_must_be_inside_this_crewmates_directory() {
        let ok = |p: &str| {
            let mut s = spawn();
            s.cwd = p.into();
            check(&Request::Spawn(s), 1)
        };
        assert_eq!(ok("/var/lib/atlas-cm/3f2a9c1e5b7d"), Ok(()));
        assert_eq!(ok("/var/lib/atlas-cm/3f2a9c1e5b7d/home/.claude"), Ok(()));
        for bad in [
            "/",
            "/tmp",
            "/var/lib/atlas-cm",
            "/var/lib/atlas-cm/",
            "/var/lib/atlas-cm/aaaaaaaaaaaa/work", // another crewmate
            "/var/lib/atlas-cm/3f2a9c1e5b7d/../aaaaaaaaaaaa", // traversal out
            "/var/lib/atlas-cm/3f2a9c1e5b7d/work/../../..",
            "/var/lib/atlas-cm/3f2a9c1e5b7dd/work", // prefix of the name, not the dir
            "/var/lib/atlas-cm/3f2a9c1e5b7d/./work",
            "/var/lib/atlas-cm/3f2a9c1e5b7d//work",
            "/var/lib/atlas-cm/3f2a9c1e5b7d/work/",
            "var/lib/atlas-cm/3f2a9c1e5b7d/work", // relative
            "work",
            "",
            "/var/lib/atlas-cm/3f2a9c1e5b7d/work\0/x",
            "/var/lib/atlas-crew/crew/x",
        ] {
            assert!(matches!(ok(bad), Err(Refused::BadCwd(_))), "{bad:?}");
        }
    }

    #[test]
    fn environment_that_changes_how_a_binary_loads_or_who_it_is_is_refused() {
        for k in [
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "DYLD_INSERT_LIBRARIES",
            "BASH_ENV",
            "ENV",
            "IFS",
            "PATH",
            "HOME",
            "USER",
            "LOGNAME",
            "SHELL",
            "",
            "1A",
            "A=B",
            "A B",
            "ÄÖ",
            "A-B",
        ] {
            assert!(
                matches!(
                    refused(|s| {
                        s.env.insert(k.into(), "v".into());
                    }),
                    Refused::BadEnv(_)
                ),
                "{k:?}"
            );
        }
        assert!(matches!(
            refused(|s| {
                s.env.insert("OK".into(), "a\0b".into());
            }),
            Refused::BadEnv(_)
        ));
        assert!(matches!(
            refused(|s| {
                s.env.insert("OK".into(), "x".repeat(MAX_ITEM + 1));
            }),
            Refused::BadEnv(_)
        ));
        assert!(matches!(
            refused(|s| s
                .env
                .extend((0..=MAX_ENV).map(|i| (format!("K{i}"), "v".into())))),
            Refused::BadEnv(_)
        ));
        // Ordinary agent environment, secrets included, passes.
        let mut s = spawn();
        s.env.extend([
            ("ANTHROPIC_API_KEY".into(), "sk-ant".into()),
            ("ATLAS_CREW".into(), "1".into()),
            ("_X".into(), "".into()),
        ]);
        assert_eq!(check(&Request::Spawn(s), 1), Ok(()));
    }

    #[test]
    fn terminal_size_is_bounded() {
        for (c, r) in [(0, 40), (120, 0), (MAX_DIM + 1, 40), (120, MAX_DIM + 1)] {
            assert_eq!(
                refused(|s| {
                    s.cols = c;
                    s.rows = r;
                }),
                Refused::BadSize
            );
        }
    }

    #[test]
    fn oversized_messages_are_refused_before_anything_else() {
        assert_eq!(
            check(&Request::Spawn(spawn()), MAX_MESSAGE + 1),
            Err(Refused::TooLarge)
        );
        assert_eq!(check(&Request::Spawn(spawn()), MAX_MESSAGE), Ok(()));
    }

    #[test]
    fn only_the_listed_signals_pass() {
        for sig in SIGNALS {
            assert_eq!(check(&Request::Signal { session: 1, sig }, 1), Ok(()));
        }
        for sig in [0, 3, 6, 11, 17, 19, 31, 64, -1, i32::MAX] {
            assert_eq!(
                check(&Request::Signal { session: 1, sig }, 1),
                Err(Refused::BadSignal(sig))
            );
        }
    }

    // Reap kills every process of a user as root: the one request where a wrong name is not a
    // failed spawn but a killed system service. Only the exact crewmate shape reaches it.
    #[test]
    fn reap_accepts_only_a_crewmate_user() {
        assert_eq!(check(&Request::Reap { user: USER.into() }, 1), Ok(()));
        for bad in [
            "root",
            "atlas-crew",
            "atlas-cm-",
            "atlas-cm-3f2a9c1e5b7d ",
            "atlas-cm-*",
            "-1",
            "",
            "atlas-cm-3F2A9C1E5B7D",
        ] {
            assert!(
                matches!(
                    check(&Request::Reap { user: bad.into() }, 1),
                    Err(Refused::BadUser(_))
                ),
                "{bad:?}"
            );
        }
        assert_eq!(
            check(&Request::Reap { user: USER.into() }, MAX_MESSAGE + 1),
            Err(Refused::TooLarge)
        );
    }

    #[test]
    fn long_hostile_input_is_clipped_in_the_reason() {
        let r = refused(|s| s.user = "x".repeat(10_000));
        assert!(r.to_string().len() < 200, "{r}");
    }
}
