//! Who may call which hook route (issue #200).
//!
//! The hook server has one global token: it opens every route, and it sits in a file only the crew user
//! reads. A crewmate that runs as its own Linux user cannot read that file and must not hold that token:
//! with it any crewmate could answer another's approvals or ask as another. So an isolated crewmate
//! carries a token of its own ([`crate::crew::store::CrewStore::ensure_hook_token`]) that opens only its
//! own routes, and this module is where that is decided. Pure: no sockets, so every refusal is a test.

use serde_json::Value;

use crate::crew::pane;

/// Who the presented token says is calling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Caller {
    /// The global token: hostd's own tools, the desktop's agents, `atlas hook` of a terminal.
    Global,
    /// A crewmate's own token: this crewmate and nothing else.
    Crewmate(String),
}

/// What to do with a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Allow,
    /// Authenticated, but not for this route or this crewmate.
    Forbidden,
}

/// Decide whether `caller` may `POST path` with `body` (the parsed JSON, `Null` for a body that is not
/// JSON, as on `/crew/…` routes).
pub(crate) fn authorize(caller: &Caller, path: &str, body: &Value) -> Verdict {
    let id = match caller {
        Caller::Global => return Verdict::Allow,
        Caller::Crewmate(id) => id.as_str(),
    };
    if let Some(rest) = path.strip_prefix("/crew/") {
        // `/crew/<id>/ask`, `/crew/<id>/set_profile`: its own id only. The webhook route
        // (`/crew/<id>/hook/<token>`) is for outsiders holding the webhook's own token and never for an
        // agent: refused here whatever the id.
        let Some((target, sub)) = rest.split_once('/') else {
            return Verdict::Forbidden;
        };
        let routable = matches!(sub, "ask" | "set_profile");
        return verdict(target == id && routable);
    }
    let Some(rest) = path.strip_prefix("/hook/") else {
        return Verdict::Forbidden;
    };
    // `/hook/<agent>` or `/hook/<agent>/decision`, for a pane of this crewmate. An event with no pane key
    // cannot be attributed, so it is not this crewmate's to send.
    let agent_ok = {
        let agent = rest.strip_suffix("/decision").unwrap_or(rest);
        !agent.is_empty() && !agent.contains('/')
    };
    let pane_ok = body["pane_key"]
        .as_str()
        .is_some_and(|p| pane::is_pane_of(p, id));
    verdict(agent_ok && pane_ok)
}

fn verdict(ok: bool) -> Verdict {
    if ok {
        Verdict::Allow
    } else {
        Verdict::Forbidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const A: &str = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
    const B: &str = "4f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";

    fn me() -> Caller {
        Caller::Crewmate(A.into())
    }

    #[test]
    fn the_global_token_opens_everything_as_before() {
        for path in [
            "/hook/claude",
            "/hook/omp/decision",
            "/crew/x/ask",
            "/crew/x/hook/tok",
            "/nope",
        ] {
            assert_eq!(
                authorize(&Caller::Global, path, &Value::Null),
                Verdict::Allow,
                "{path}"
            );
        }
    }

    #[test]
    fn a_crewmate_may_ask_and_set_its_profile_only_as_itself() {
        for sub in ["ask", "set_profile"] {
            assert_eq!(
                authorize(&me(), &format!("/crew/{A}/{sub}"), &Value::Null),
                Verdict::Allow
            );
            // The incident this module exists for: asking, or renaming, as another crewmate.
            assert_eq!(
                authorize(&me(), &format!("/crew/{B}/{sub}"), &Value::Null),
                Verdict::Forbidden,
                "{sub} as another"
            );
        }
    }

    // An agent holding its own token must not be able to fire a webhook trigger (those are for outsiders
    // with the webhook's token), nor probe routes that do not exist.
    #[test]
    fn a_crewmate_cannot_use_the_webhook_route_or_anything_unlisted() {
        for path in [
            format!("/crew/{A}/hook/sometoken"),
            format!("/crew/{A}"),
            format!("/crew/{A}/"),
            format!("/crew/{A}/ask/extra"),
            "/crew/".to_string(),
            "/bots/x/hook/t".to_string(),
            "/".to_string(),
            "/other".to_string(),
            format!("/crew/{A}x/ask"),
            format!("/crew/{}/ask", &A[..35]),
        ] {
            assert_eq!(
                authorize(&me(), &path, &json!({})),
                Verdict::Forbidden,
                "{path}"
            );
        }
    }

    #[test]
    fn agent_hooks_and_decisions_are_allowed_only_for_the_crewmates_own_panes() {
        let own = json!({"pane_key": format!("crew:{A}:t1"), "payload": {}});
        let other = json!({"pane_key": format!("crew:{B}:t1"), "payload": {}});
        for path in ["/hook/claude", "/hook/omp", "/hook/codex/decision"] {
            assert_eq!(authorize(&me(), path, &own), Verdict::Allow, "{path}");
            assert_eq!(
                authorize(&me(), path, &other),
                Verdict::Forbidden,
                "{path}: another's pane"
            );
        }
        // A terminal pane of the person's own desktop is not a crewmate's.
        for pane in [
            json!("p1"),
            json!(""),
            json!(null),
            json!(7),
            json!(format!("crew:{A}x:t")),
        ] {
            assert_eq!(
                authorize(&me(), "/hook/claude", &json!({"pane_key": pane})),
                Verdict::Forbidden,
                "{pane}"
            );
        }
        // No pane key at all: unattributable, so not allowed.
        assert_eq!(
            authorize(&me(), "/hook/claude", &json!({})),
            Verdict::Forbidden
        );
        assert_eq!(
            authorize(&me(), "/hook/claude", &Value::Null),
            Verdict::Forbidden
        );
    }

    #[test]
    fn malformed_hook_paths_are_forbidden_even_with_an_own_pane() {
        let own = json!({"pane_key": format!("crew:{A}:t1")});
        for path in [
            "/hook/",
            "/hook//decision",
            "/hook/a/b",
            "/hook/a/b/decision",
            "/hook",
        ] {
            assert_eq!(authorize(&me(), path, &own), Verdict::Forbidden, "{path}");
        }
    }
}
