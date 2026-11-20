//! Who may call which hook route (issue #200).
//!
//! The hook server has one global token: it opens every route, and it sits in a file only the crew user
//! reads. A crewmate that runs as its own Linux user cannot read that file and must not hold that token:
//! with it any crewmate could answer another's approvals or ask as another. So an isolated crewmate
//! carries a token of its own ([`crate::crew::store::CrewStore::ensure_hook_token`]) that opens only its
//! own routes, and this module is where that is decided. Pure: no sockets, so every refusal is a test.

use serde_json::Value;

use crate::crew::pane;

/// The crew routes (`POST /crew/<id>/<route>`) an agent's MCP server calls with its own token: the one
/// list, read by [`authorize`]; `hook_server::route_crew` dispatches the same names (a test over a real
/// socket keeps the two in step). The webhook route is not here: it is for outsiders.
pub(crate) const AGENT_ROUTES: [&str; 7] = [
    "ask",
    "set_profile",
    "capture",
    "locate",
    "ui_gate",
    "set_ui",
    // The git credential helper of the agent (`atlas crew git-credential`), not its MCP server.
    "git_credential",
];

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
        return verdict(target == id && AGENT_ROUTES.contains(&sub));
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

/// What a caller's hook event may carry into hostd. A crewmate's own token (an isolated crewmate) comes from a
/// process that controls everything it says, and `session_file` is a path hostd opens as the crew user to name the
/// chat and to serve its history (`agents::history`, `agents::tree`): a crewmate could name another crewmate's
/// files or `hostd.token`. The session files of such a crewmate live in its own home, which hostd cannot read
/// anyway, so the path is dropped, not checked. The global token (hostd's own agents, the desktop's) keeps it.
pub(crate) fn scrub(caller: &Caller, body: &mut Value) {
    if matches!(caller, Caller::Crewmate(_))
        && let Some(p) = body.get_mut("payload").and_then(Value::as_object_mut)
    {
        p.remove("session_file");
    }
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

    // Regression (merging #200 with the crew tools of #211, #213, #215, #216 and #367): the crew routes an
    // agent's MCP server posts to are one list, `AGENT_ROUTES`. #223 listed only `ask` and `set_profile`,
    // so with its own token an isolated crewmate got a 403 from `capture`, `locate`, `ui_gate` and
    // `set_ui`: no UI task could ever prove itself.
    #[test]
    fn a_crewmate_may_call_every_agent_route_only_as_itself() {
        // The names the agent's tools post to (`crew/mcp.rs`, the git credential helper): written out, so
        // shrinking the constant fails here.
        let mut routes = AGENT_ROUTES.to_vec();
        routes.sort_unstable();
        assert_eq!(
            routes,
            [
                "ask",
                "capture",
                "git_credential",
                "locate",
                "set_profile",
                "set_ui",
                "ui_gate"
            ]
        );
        for sub in AGENT_ROUTES {
            assert_eq!(
                authorize(&me(), &format!("/crew/{A}/{sub}"), &Value::Null),
                Verdict::Allow,
                "{sub}"
            );
            assert_eq!(
                authorize(&me(), &format!("/crew/{B}/{sub}"), &Value::Null),
                Verdict::Forbidden,
                "{sub} as another"
            );
        }
        // A route that is not an agent's (and does not exist) stays closed.
        for sub in ["hook", "delete", "capture2", "Capture", ""] {
            assert_eq!(
                authorize(&me(), &format!("/crew/{A}/{sub}"), &Value::Null),
                Verdict::Forbidden,
                "{sub:?}"
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

    // Security review 5 (the class): a crewmate's hook event named `session_file: <another crewmate's file>` and
    // hostd, as the crew user, read it for the chat title and the history the phone shows.
    #[test]
    fn a_crewmates_event_cannot_name_a_session_file_for_hostd_to_open() {
        let ev = || json!({"pane_key": "p", "payload": {"event": "agent_start", "session_file": "/var/lib/atlas-cm/x/work/.env", "model": "m"}});
        let mut mine = ev();
        scrub(&me(), &mut mine);
        assert!(mine["payload"].get("session_file").is_none(), "{mine}");
        assert_eq!(
            mine["payload"]["model"], "m",
            "the rest of the event is kept"
        );
        let mut global = ev();
        scrub(&Caller::Global, &mut global);
        assert_eq!(
            global["payload"]["session_file"],
            "/var/lib/atlas-cm/x/work/.env"
        );
        let mut none = json!({"pane_key": "p"});
        scrub(&me(), &mut none);
        assert_eq!(none, json!({"pane_key": "p"}));
    }
}
