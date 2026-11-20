//! The GitHub App manifest: what the desktop POSTs (with its own `redirect_url` and `state`) to GitHub's
//! "register a GitHub App from a manifest" page. hostd builds it so the permissions live in one place, next to
//! the code that uses them.
//!
//! Minimal on purpose: only what the crew features read and write. Editing `.github/workflows` needs the separate
//! Workflows permission, so it is opt-in. `public: false`: the app belongs to this crew server alone.

use serde_json::{Value, json};

use crate::protocol::invalid_argument;
use atlas_core::progress::Coded;

/// Where the app's homepage points, and the (inactive) webhook placeholder: `hook_attributes.url` is required by
/// GitHub, and without a public webhook URL the hook stays off (`active: false`).
const HOME: &str = "https://atlas.codeatlas.com.br";
/// GitHub refuses an app name over 34 characters.
const NAME_MAX: usize = 34;

/// The permissions every app is created with: the minimum the crew uses.
pub(crate) const PERMISSIONS: [(&str, &str); 6] = [
    ("contents", "write"),
    ("pull_requests", "write"),
    ("issues", "write"),
    ("checks", "read"),
    ("actions", "read"),
    ("metadata", "read"),
];

/// The events the crew's GitHub triggers care about.
const EVENTS: [&str; 7] = [
    "issue_comment",
    "issues",
    "pull_request",
    "pull_request_review_comment",
    "check_run",
    "workflow_run",
    "release",
];

/// `atlas-crew-<server>`, lower case with only letters, digits and `-`, at most [`NAME_MAX`] long.
pub(crate) fn app_name(server: &str) -> String {
    let slug: String = server
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    let mut name = format!(
        "atlas-crew-{}",
        if slug.is_empty() { "server" } else { slug }
    );
    name.truncate(NAME_MAX);
    name.trim_end_matches('-').to_string()
}

/// A GitHub login: letters, digits and single hyphens, up to 39.
fn valid_login(s: &str) -> bool {
    (1..=39).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// `{manifest, post_url, server_name}` for the account that will own the app.
pub(crate) fn build(
    owner_kind: &str,
    org: Option<&str>,
    workflows: bool,
    server: &str,
) -> Result<Value, Coded> {
    let post_url = match (owner_kind, org) {
        ("user", _) => "https://github.com/settings/apps/new".to_string(),
        ("org", Some(o)) if valid_login(o) => {
            format!("https://github.com/organizations/{o}/settings/apps/new")
        }
        ("org", _) => {
            return Err(invalid_argument(
                "org",
                "invalid",
                "org: the organization's login is missing or not a GitHub login",
            ));
        }
        _ => {
            return Err(invalid_argument(
                "owner_kind",
                "invalid",
                "owner_kind must be \"user\" or \"org\"",
            ));
        }
    };
    let mut perms: serde_json::Map<String, Value> = PERMISSIONS
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    if workflows {
        perms.insert("workflows".into(), json!("write"));
    }
    let name = app_name(server);
    Ok(json!({
        "manifest": {
            "name": name,
            "url": HOME,
            // i18n-ignore: GitHub's own page shows it, in English, as the app's description
            "description": format!("Atlas Crew on {server}: crewmates that work on the repositories you pick."),
            "hook_attributes": {"url": HOME, "active": false},
            "public": false,
            "default_permissions": perms,
            "default_events": EVENTS,
        },
        "post_url": post_url,
        "server_name": server,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(workflows: bool) -> Value {
        build("user", None, workflows, "my-box").unwrap()["manifest"].clone()
    }

    // The minimum the crew uses, and nothing else: a permission added here widens every token.
    #[test]
    fn the_manifest_asks_for_the_minimum_and_stays_private() {
        let m = manifest(false);
        assert_eq!(m["public"], false);
        assert_eq!(m["name"], "atlas-crew-my-box");
        assert_eq!(
            m["default_permissions"],
            json!({
                "contents": "write", "pull_requests": "write", "issues": "write",
                "checks": "read", "actions": "read", "metadata": "read"
            })
        );
        assert_eq!(
            m["default_events"],
            json!([
                "issue_comment",
                "issues",
                "pull_request",
                "pull_request_review_comment",
                "check_run",
                "workflow_run",
                "release"
            ])
        );
        // The webhook needs a URL; with no public one it is off.
        assert_eq!(m["hook_attributes"]["active"], false);
        assert!(
            m["hook_attributes"]["url"]
                .as_str()
                .unwrap()
                .starts_with("https://")
        );
        // The desktop adds these two: they belong to its loopback listener.
        assert!(m.get("redirect_url").is_none() && m.get("state").is_none());
    }

    #[test]
    fn workflows_is_opt_in() {
        assert!(
            manifest(false)["default_permissions"]
                .get("workflows")
                .is_none()
        );
        assert_eq!(manifest(true)["default_permissions"]["workflows"], "write");
    }

    #[test]
    fn the_post_url_follows_the_owner() {
        let user = build("user", None, false, "x").unwrap();
        assert_eq!(user["post_url"], "https://github.com/settings/apps/new");
        let org = build("org", Some("acme-inc"), false, "x").unwrap();
        assert_eq!(
            org["post_url"],
            "https://github.com/organizations/acme-inc/settings/apps/new"
        );
    }

    // The login ends up in a URL the desktop posts a form to: nothing that can change the path or the host.
    #[test]
    fn an_org_that_is_not_a_login_is_refused() {
        for org in ["", "a/b", "a b", "evil.com/x?", "..", &"x".repeat(40)] {
            assert!(build("org", Some(org), false, "x").is_err(), "{org:?}");
        }
        assert!(build("org", None, false, "x").is_err());
        assert!(build("team", None, false, "x").is_err());
    }

    #[test]
    fn the_name_is_a_valid_app_name_for_any_server() {
        assert_eq!(app_name("My Box.local"), "atlas-crew-my-box-local");
        assert_eq!(app_name(""), "atlas-crew-server");
        let long = app_name(&"a".repeat(80));
        assert!(long.len() <= NAME_MAX && !long.ends_with('-'), "{long}");
    }
}
