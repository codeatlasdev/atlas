//! What a crewmate's git needs to work through the app, as environment: where to ask for a password and who commits.
//! None of it is a secret. The password is never here: git asks the helper ([`super::helper`]) per operation.
//!
//! The configuration goes in `GIT_CONFIG_COUNT`/`KEY_n`/`VALUE_n` (git 2.31+), not in a file: nothing is written to
//! the crewmate's `~/.gitconfig` or its checkout, and the agent's own git config stays the agent's.

use super::record::Identity;

/// The environment variables of one crewmate's agent. `exe` is the `atlas` binary that answers as the helper;
/// `gh_fallback` keeps `gh` as the second helper (a crewmate that is not its own Linux user, whose `gh` login is the
/// crew user's: the setup that worked before the app).
pub(crate) fn git_env(exe: &str, who: &Identity, gh_fallback: bool) -> Vec<(String, String)> {
    let github = "credential.https://github.com";
    let mut cfg = vec![
        // Reset: whatever helper the machine had (`gh auth setup-git`) no longer answers first.
        ("credential.helper".to_string(), String::new()),
        (format!("{github}.useHttpPath"), "true".to_string()),
        (
            format!("{github}.helper"),
            format!("!{} crew git-credential", sh_quote(exe)),
        ),
    ];
    if gh_fallback {
        cfg.push((
            format!("{github}.helper"),
            "!gh auth git-credential".to_string(),
        ));
    }
    let mut env = vec![("GIT_CONFIG_COUNT".to_string(), cfg.len().to_string())];
    for (i, (k, v)) in cfg.into_iter().enumerate() {
        env.push((format!("GIT_CONFIG_KEY_{i}"), k));
        env.push((format!("GIT_CONFIG_VALUE_{i}"), v));
    }
    for who_is in ["AUTHOR", "COMMITTER"] {
        env.push((format!("GIT_{who_is}_NAME"), who.name.clone()));
        env.push((format!("GIT_{who_is}_EMAIL"), who.email.clone()));
    }
    env
}

/// The `Co-authored-by:` trailer for the person who asked for the work, with the noreply email GitHub links to
/// their profile.
pub(crate) fn co_author(login: &str, id: u64) -> String {
    format!("Co-authored-by: {login} <{id}+{login}@users.noreply.github.com>")
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who() -> Identity {
        Identity {
            name: "atlas-crew-box[bot]".into(),
            email: "42+atlas-crew-box[bot]@users.noreply.github.com".into(),
        }
    }

    fn get<'a>(env: &'a [(String, String)], k: &str) -> Option<&'a str> {
        env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }

    // The helper list git ends up with: reset, then the app's helper (asking hostd), then, only when allowed, `gh`.
    #[test]
    fn the_environment_names_the_helper_and_the_bot_and_holds_no_password() {
        let env = git_env("/opt/at las/atlas", &who(), false);
        assert_eq!(get(&env, "GIT_CONFIG_COUNT"), Some("3"));
        assert_eq!(get(&env, "GIT_CONFIG_KEY_0"), Some("credential.helper"));
        assert_eq!(get(&env, "GIT_CONFIG_VALUE_0"), Some(""));
        assert_eq!(
            get(&env, "GIT_CONFIG_KEY_1"),
            Some("credential.https://github.com.useHttpPath")
        );
        assert_eq!(get(&env, "GIT_CONFIG_VALUE_1"), Some("true"));
        assert_eq!(
            get(&env, "GIT_CONFIG_KEY_2"),
            Some("credential.https://github.com.helper")
        );
        // The binary's path is quoted for the shell git runs it through.
        assert_eq!(
            get(&env, "GIT_CONFIG_VALUE_2"),
            Some("!'/opt/at las/atlas' crew git-credential")
        );
        for who in ["AUTHOR", "COMMITTER"] {
            assert_eq!(
                get(&env, &format!("GIT_{who}_NAME")),
                Some("atlas-crew-box[bot]")
            );
            assert_eq!(
                get(&env, &format!("GIT_{who}_EMAIL")),
                Some("42+atlas-crew-box[bot]@users.noreply.github.com")
            );
        }
        // x-access-token is the username git sends with a password: the password is not here.
        for (k, v) in &env {
            assert!(
                !k.contains("TOKEN") && !k.contains("PASSWORD") && !v.contains("x-access-token"),
                "{k}"
            );
        }
    }

    #[test]
    fn gh_is_the_second_helper_only_for_a_crewmate_that_may_use_the_crew_users_login() {
        let env = git_env("atlas", &who(), true);
        assert_eq!(get(&env, "GIT_CONFIG_COUNT"), Some("4"));
        assert_eq!(
            get(&env, "GIT_CONFIG_VALUE_3"),
            Some("!gh auth git-credential")
        );
        assert!(get(&git_env("atlas", &who(), false), "GIT_CONFIG_KEY_3").is_none());
    }

    // A quote in the path must not end the quoting: it would run the rest as a command.
    #[test]
    fn a_quote_in_the_binary_path_stays_inside_the_quoting() {
        let env = git_env("/tmp/it's/atlas", &who(), false);
        assert_eq!(
            get(&env, "GIT_CONFIG_VALUE_2"),
            Some(r"!'/tmp/it'\''s/atlas' crew git-credential")
        );
    }

    #[test]
    fn the_trailer_uses_the_noreply_email_with_the_user_id() {
        assert_eq!(
            co_author("octocat", 583231),
            "Co-authored-by: octocat <583231+octocat@users.noreply.github.com>"
        );
    }
}
