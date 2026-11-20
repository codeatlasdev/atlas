//! A crewmate's own hook token (issue #200). The hook server's global token opens every route, and
//! it lives in a file only the crew user can read; an isolated crewmate's agent cannot read that file
//! and must not hold that token. It gets a token of its own that opens its own routes only (see
//! `agents::hook_server`), generated once and kept in `<crewmate dir>/hook-token` (0600).

use std::io;

use super::CrewStore;
use crate::paths;

const FILE: &str = "hook-token";

impl CrewStore {
    /// The crewmate's hook token, created on first use.
    pub fn ensure_hook_token(&self, id: &str) -> io::Result<String> {
        if let Some(t) = self.hook_token_of(id) {
            return Ok(t);
        }
        let t = paths::random_hex(24)?;
        std::fs::create_dir_all(self.crewmate_dir(id))?;
        paths::write_private(&self.crewmate_dir(id).join(FILE), t.as_bytes())?;
        Ok(t)
    }

    /// The crewmate's hook token, if it has one yet.
    pub fn hook_token_of(&self, id: &str) -> Option<String> {
        let t = std::fs::read_to_string(self.crewmate_dir(id).join(FILE)).ok()?;
        let t = t.trim();
        (t.len() >= 16).then(|| t.to_string())
    }

    /// Which crewmate a hook token belongs to. Compares every crewmate's token (a handful, read from
    /// small files) and returns the first match: tokens are random, so a collision is not a case.
    pub fn crewmate_of_hook_token(&self, token: &str) -> Option<String> {
        if token.len() < 16 {
            return None;
        }
        self.list_crewmates()
            .into_iter()
            .find(|c| {
                self.hook_token_of(&c.id)
                    .is_some_and(|t| constant_time_eq(t.as_bytes(), token.as_bytes()))
            })
            .map(|c| c.id)
    }
}

/// Equal without stopping at the first different byte, so response time does not tell an attacker how
/// much of a token they guessed.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::store::{Crewmate, Mode};

    fn crewmate(id: &str) -> Crewmate {
        Crewmate {
            id: id.into(),
            name: "n".into(),
            role: String::new(),
            agent: "omp".into(),
            workdir: None,
            mode: Mode::Bypass,
            routines: vec![],
            triggers: vec![],
            look: None,
            engine: None,
            project: None,
            setup_step: None,
            short_replies: None,
            created_at: 0,
        }
    }

    const A: &str = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
    const B: &str = "4f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";

    #[test]
    fn a_token_is_created_once_kept_and_private() {
        let t = tempfile::tempdir().unwrap();
        let s = CrewStore::new(t.path());
        s.save_crewmate(&crewmate(A)).unwrap();
        assert_eq!(s.hook_token_of(A), None);
        let one = s.ensure_hook_token(A).unwrap();
        assert!(one.len() >= 32 && one.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(s.ensure_hook_token(A).unwrap(), one, "stable across calls");
        assert_eq!(s.hook_token_of(A).as_deref(), Some(one.as_str()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(s.crewmate_dir(A).join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }
    }

    // The isolation property: a token identifies exactly one crewmate, and an unknown, short or
    // empty one identifies none.
    #[test]
    fn a_token_belongs_to_exactly_one_crewmate() {
        let t = tempfile::tempdir().unwrap();
        let s = CrewStore::new(t.path());
        s.save_crewmate(&crewmate(A)).unwrap();
        s.save_crewmate(&crewmate(B)).unwrap();
        let (ta, tb) = (
            s.ensure_hook_token(A).unwrap(),
            s.ensure_hook_token(B).unwrap(),
        );
        assert_ne!(ta, tb);
        assert_eq!(s.crewmate_of_hook_token(&ta).as_deref(), Some(A));
        assert_eq!(s.crewmate_of_hook_token(&tb).as_deref(), Some(B));
        for bad in [
            "",
            "x",
            &ta[..ta.len() - 1],
            &format!("{ta}0"),
            "0".repeat(48).as_str(),
        ] {
            assert_eq!(s.crewmate_of_hook_token(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn constant_time_eq_is_plain_equality() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }
}
