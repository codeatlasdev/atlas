//! `agents:` — what an AI agent may do through the control socket (MCP control tools).
//!
//! ```yaml
//! agents:
//!   control: deny                 # every control tool, unless the tool has its own entry
//!   restart_service:              # per tool: allow | deny | { allow: [...] } | { deny: [...] }
//!     allow: [worker]             # only these services
//!   run_action: { deny: [Wipe DB] }
//! ```
//!
//! Absent `agents:` = everything allowed (how atlas behaved before the policy existed).
//! Precedence: the tool's own entry, then `control:`, then allow. The policy only ever
//! applies to callers that identify as agents; the developer (TUI, desktop, CLI) is never
//! restricted by it. `wait_ready` only reads, so it has no entry.

use schemars::JsonSchema;
use serde::Deserialize;

/// The control tools a policy can restrict (the control-socket command names).
pub const POLICY_TOOLS: [&str; 4] = [
    "restart_service",
    "stop_service",
    "start_service",
    "run_action",
];

/// Allow or deny, for a whole tool.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Allow,
    Deny,
}

/// One tool's rule: a bare `allow`/`deny`, or the same with a list of targets
/// (services for the service tools, action keys or labels for `run_action`).
#[derive(Debug, Clone, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum Rule {
    Mode(Mode),
    /// `{ allow: [a, b] }`: only these targets are allowed, every other one is denied.
    Allow(AllowList),
    /// `{ deny: [a, b] }`: these targets are denied, every other one is allowed.
    Deny(DenyList),
}

#[derive(Debug, Clone, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct AllowList {
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct DenyList {
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct AgentsPolicy {
    /// Default for every control tool without its own entry: `allow` (default) or `deny`.
    pub control: Option<Mode>,
    pub restart_service: Option<Rule>,
    pub stop_service: Option<Rule>,
    pub start_service: Option<Rule>,
    pub run_action: Option<Rule>,
}

/// Outcome of [`AgentsPolicy::decide`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    /// Agent-readable reason, already worded as the tool error.
    Denied(String),
}

impl AgentsPolicy {
    fn rule(&self, tool: &str) -> Option<&Rule> {
        match tool {
            "restart_service" => self.restart_service.as_ref(),
            "stop_service" => self.stop_service.as_ref(),
            "start_service" => self.start_service.as_ref(),
            "run_action" => self.run_action.as_ref(),
            _ => None,
        }
    }

    /// May an agent run `tool` (a control command) on `target`, as the agent wrote it?
    /// `aliases` are other names of the same thing (an action's key and label), so a list
    /// naming either one matches whichever the agent used.
    pub fn decide(&self, tool: &str, target: &str, aliases: &[&str]) -> Verdict {
        let names = || std::iter::once(target).chain(aliases.iter().copied());
        let listed = |list: &[String]| {
            names().any(|n| list.iter().any(|l| l == n || l.eq_ignore_ascii_case(n)))
        };
        let allowed = match self.rule(tool) {
            Some(Rule::Mode(m)) => *m == Mode::Allow,
            Some(Rule::Allow(a)) => listed(&a.allow),
            Some(Rule::Deny(d)) => !listed(&d.deny),
            None => self.control != Some(Mode::Deny),
        };
        if allowed {
            return Verdict::Allowed;
        }
        Verdict::Denied(format!(
            "this project's atlas.yaml denies agents {tool} on {target}: ask the developer to \
             run it, or to allow it under `agents:` in atlas.yaml"
        ))
    }

    /// True when the policy restricts nothing (the default).
    pub fn is_open(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(yaml: &str) -> AgentsPolicy {
        serde_norway::from_str(yaml).unwrap()
    }

    fn allowed(p: &AgentsPolicy, tool: &str, target: &str) -> bool {
        p.decide(tool, target, &[]) == Verdict::Allowed
    }

    #[test]
    fn absent_policy_allows_every_tool() {
        let p = AgentsPolicy::default();
        assert!(p.is_open());
        for t in POLICY_TOOLS {
            assert!(allowed(&p, t, "api"), "{t}");
        }
    }

    #[test]
    fn control_deny_is_the_one_line_read_only_switch() {
        let p = policy("control: deny");
        for t in POLICY_TOOLS {
            assert!(!allowed(&p, t, "api"), "{t}");
        }
        assert!(allowed(&policy("control: allow"), "restart_service", "api"));
    }

    #[test]
    fn the_tools_own_entry_beats_control() {
        let p = policy("control: deny\nrestart_service: allow");
        assert!(allowed(&p, "restart_service", "api"));
        assert!(!allowed(&p, "stop_service", "api"));
        let p = policy("control: allow\nrun_action: deny");
        assert!(!allowed(&p, "run_action", "m"));
        assert!(allowed(&p, "restart_service", "api"));
    }

    #[test]
    fn allow_list_admits_only_the_listed_services() {
        let p = policy("control: deny\nrestart_service: { allow: [worker] }");
        assert!(allowed(&p, "restart_service", "worker"));
        assert!(!allowed(&p, "restart_service", "api"));
        assert!(
            !allowed(&p, "start_service", "worker"),
            "other tools stay denied"
        );
    }

    #[test]
    fn deny_list_blocks_only_the_listed_services() {
        let p = policy("stop_service: { deny: [db] }");
        assert!(!allowed(&p, "stop_service", "db"));
        assert!(allowed(&p, "stop_service", "api"));
        assert!(allowed(&p, "restart_service", "db"), "a list is per tool");
    }

    #[test]
    fn an_empty_allow_list_denies_everything_and_an_empty_deny_list_nothing() {
        let p = policy("restart_service: { allow: [] }\nstop_service: { deny: [] }");
        assert!(!allowed(&p, "restart_service", "api"));
        assert!(allowed(&p, "stop_service", "api"));
    }

    #[test]
    fn action_lists_match_the_key_or_the_label_whichever_the_agent_used() {
        let p = policy("run_action: { allow: [Migrate, x] }");
        // The agent said "m"; the resolved action is {key m, label Migrate}.
        assert_eq!(
            p.decide("run_action", "m", &["m", "Migrate"]),
            Verdict::Allowed
        );
        assert_eq!(
            p.decide("run_action", "migrate", &["m", "Migrate"]),
            Verdict::Allowed
        );
        assert!(matches!(
            p.decide("run_action", "w", &["w", "Wipe DB"]),
            Verdict::Denied(_)
        ));
        let p = policy("run_action: { deny: [Wipe DB] }");
        assert!(matches!(
            p.decide("run_action", "w", &["w", "Wipe DB"]),
            Verdict::Denied(_)
        ));
        assert_eq!(
            p.decide("run_action", "m", &["m", "Migrate"]),
            Verdict::Allowed
        );
    }

    #[test]
    fn denial_names_tool_target_and_what_to_do() {
        let Verdict::Denied(msg) = policy("control: deny").decide("restart_service", "api", &[])
        else {
            panic!("expected a denial");
        };
        assert!(
            msg.contains("atlas.yaml denies agents restart_service on api"),
            "{msg}"
        );
        assert!(msg.contains("ask the developer"), "{msg}");
    }

    #[test]
    fn unknown_keys_and_bad_values_are_errors_not_silent_allow_all() {
        // A typo must never leave the project open while looking restricted.
        assert!(serde_norway::from_str::<AgentsPolicy>("restart_servce: deny").is_err());
        assert!(serde_norway::from_str::<AgentsPolicy>("control: maybe").is_err());
        assert!(serde_norway::from_str::<AgentsPolicy>("restart_service: { alow: [a] }").is_err());
    }

    #[test]
    fn project_config_parses_agents_and_defaults_to_open() {
        let cfg: crate::config::ProjectConfig = serde_norway::from_str(
            "name: p\nservices: { api: { command: x } }\nagents: { control: deny }",
        )
        .unwrap();
        assert_eq!(cfg.agents.control, Some(Mode::Deny));
        let cfg: crate::config::ProjectConfig =
            serde_norway::from_str("name: p\nservices: { api: { command: x } }").unwrap();
        assert!(cfg.agents.is_open());
    }
}
