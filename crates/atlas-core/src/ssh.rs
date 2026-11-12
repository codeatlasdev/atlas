//! ssh targets handed to `ssh` as an argument (Atlas Desktop's SSH hosts, the bot computers
//! hostd relays to Atlas Mobile).

/// An ssh target: a `~/.ssh/config` alias or `user@host`. Only `[A-Za-z0-9._-]` on each side of
/// at most one `@`, and neither side may start with `-`, so it can never be read as an ssh option.
pub fn valid_target(a: &str) -> Result<(), String> {
    let part = |s: &str| {
        !s.is_empty()
            && !s.starts_with('-')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    let ok = match a.split_once('@') {
        Some((user, host)) => part(user) && part(host),
        None => part(a),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("invalid ssh host alias {a:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::valid_target;

    // Bot computers are added as `atlas-bot@host`; the old check rejected every `@`, so
    // `bot_hosts_add` could never connect one.
    #[test]
    fn user_at_host_and_plain_aliases_are_accepted() {
        for a in [
            "atlas-bot@myserver.example.com",
            "atlas-bot@10.0.0.5",
            "devbox",
            "my_host-1",
        ] {
            assert!(valid_target(a).is_ok(), "{a}");
        }
    }

    // Anything ssh could parse as an option, or a shell could expand, stays rejected.
    #[test]
    fn option_like_or_malformed_targets_are_rejected() {
        for a in [
            "",
            "-oProxyCommand=x",
            "u@-oProxyCommand=x",
            "-u@host",
            "@host",
            "user@",
            "a@b@c",
            "u@h;rm",
            "u@h x",
            "u@$(id)",
        ] {
            assert!(valid_target(a).is_err(), "{a}");
        }
    }
}
