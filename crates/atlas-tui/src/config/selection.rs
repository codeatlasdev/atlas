//! Which services `atlas dev` starts at boot: positional `SERVICE...`,
//! `--profile NAME` (repeatable, union) and `--no-deps`.
//!
//! With no request every service is selected (`Selection::all`). A request
//! selects the named services (and the members of the named profiles) plus,
//! unless `--no-deps`, everything they transitively `depends_on` — infra
//! services included, so a database is started only when something selected
//! needs it. Services outside the selection stay `disabled` (`off`): visible,
//! startable by hand, never started at boot.

use std::collections::BTreeSet;

use super::project::ProjectConfig;
use super::validate::suggest;

/// What the user asked for on the command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionRequest {
    /// Positional `SERVICE...`.
    pub services: Vec<String>,
    /// `--profile NAME` (repeatable).
    pub profiles: Vec<String>,
    /// `--no-deps`: start exactly the requested services, no dependency closure.
    pub no_deps: bool,
}

impl SelectionRequest {
    /// True when the user narrowed the run (services or profiles given).
    pub fn is_narrowed(&self) -> bool {
        !self.services.is_empty() || !self.profiles.is_empty()
    }
}

/// The services started at boot. `None` inside = everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    names: Option<BTreeSet<String>>,
}

impl Selection {
    /// No narrowing: every service starts (the default).
    pub fn all() -> Self {
        Self { names: None }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.as_ref().is_none_or(|n| n.contains(name))
    }

    /// True when the run was narrowed to a subset.
    pub fn is_narrowed(&self) -> bool {
        self.names.is_some()
    }

    /// Selected names in sorted order (`None` when everything is selected).
    pub fn names(&self) -> Option<&BTreeSet<String>> {
        self.names.as_ref()
    }
}

fn unknown(kind: &str, name: &str, known: &BTreeSet<&str>) -> String {
    let hint = suggest(name, known.iter().copied())
        .map(|s| format!(" (did you mean '{s}'?)"))
        .unwrap_or_default();
    let list = if known.is_empty() {
        format!("no {kind}s are defined")
    } else {
        format!(
            "available: {}",
            known.iter().copied().collect::<Vec<_>>().join(", ")
        )
    };
    format!("unknown {kind} '{name}'{hint}; {list}")
}

/// Resolve `req` against `config`. Errors name every unknown service/profile
/// (fail fast, nothing is started).
pub fn resolve(config: &ProjectConfig, req: &SelectionRequest) -> Result<Selection, String> {
    if !req.is_narrowed() {
        if req.no_deps {
            return Err("--no-deps needs at least one SERVICE or --profile".to_string());
        }
        return Ok(Selection::all());
    }

    let services: BTreeSet<&str> = config.services.keys().map(String::as_str).collect();
    let profiles: BTreeSet<&str> = config.profiles.keys().map(String::as_str).collect();

    let mut errors = Vec::new();
    let mut roots: BTreeSet<String> = BTreeSet::new();
    for name in &req.services {
        if services.contains(name.as_str()) {
            roots.insert(name.clone());
        } else {
            errors.push(unknown("service", name, &services));
        }
    }
    for profile in &req.profiles {
        match config.profiles.get(profile) {
            Some(members) => roots.extend(members.iter().cloned()),
            None => errors.push(unknown("profile", profile, &profiles)),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    if req.no_deps {
        return Ok(Selection { names: Some(roots) });
    }

    // Transitive depends_on closure (iterative; cycles are a config error
    // reported elsewhere, the visited set keeps this terminating regardless).
    let mut selected: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = roots.into_iter().collect();
    while let Some(name) = stack.pop() {
        if !selected.insert(name.clone()) {
            continue;
        }
        if let Some(def) = config.services.get(&name) {
            stack.extend(def.deps().into_iter().map(|(dep, _)| dep));
        }
    }
    Ok(Selection {
        names: Some(selected),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::project::{DependsOn, ServiceDef};

    fn svc(deps: &[&str]) -> ServiceDef {
        ServiceDef {
            command: Some("true".into()),
            depends_on: (!deps.is_empty())
                .then(|| DependsOn::List(deps.iter().map(|d| d.to_string()).collect())),
            ..Default::default()
        }
    }

    /// db <- api <- web ; worker (standalone); db is infra-like (no command).
    fn project() -> ProjectConfig {
        let mut services = std::collections::HashMap::new();
        services.insert(
            "db".to_string(),
            ServiceDef {
                command: None,
                port: Some(5432),
                ..Default::default()
            },
        );
        services.insert("api".to_string(), svc(&["db"]));
        services.insert("web".to_string(), svc(&["api"]));
        services.insert("worker".to_string(), svc(&[]));
        ProjectConfig {
            name: "p".into(),
            services,
            profiles: [
                ("backend".to_string(), vec!["api".into(), "worker".into()]),
                ("front".to_string(), vec!["web".into()]),
            ]
            .into(),
            ..Default::default()
        }
    }

    fn req(services: &[&str], profiles: &[&str], no_deps: bool) -> SelectionRequest {
        SelectionRequest {
            services: services.iter().map(|s| s.to_string()).collect(),
            profiles: profiles.iter().map(|s| s.to_string()).collect(),
            no_deps,
        }
    }

    fn names(s: &Selection) -> Vec<&str> {
        s.names()
            .expect("narrowed")
            .iter()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn no_request_selects_everything() {
        let s = resolve(&project(), &SelectionRequest::default()).unwrap();
        assert!(!s.is_narrowed());
        assert!(s.contains("db") && s.contains("worker") && s.contains("anything"));
    }

    #[test]
    fn service_pulls_in_transitive_dependencies() {
        let s = resolve(&project(), &req(&["web"], &[], false)).unwrap();
        assert_eq!(names(&s), ["api", "db", "web"]);
        assert!(!s.contains("worker"));
    }

    #[test]
    fn standalone_service_pulls_in_nothing_else() {
        let s = resolve(&project(), &req(&["worker"], &[], false)).unwrap();
        assert_eq!(names(&s), ["worker"]);
        assert!(
            !s.contains("db"),
            "infra only when a selected service needs it"
        );
    }

    #[test]
    fn no_deps_selects_exactly_the_requested() {
        let s = resolve(&project(), &req(&["web"], &[], true)).unwrap();
        assert_eq!(names(&s), ["web"]);
    }

    #[test]
    fn profiles_are_unioned_with_each_other_and_services() {
        let p = project();
        let s = resolve(&p, &req(&[], &["backend"], false)).unwrap();
        assert_eq!(names(&s), ["api", "db", "worker"]);
        let s = resolve(&p, &req(&[], &["backend", "front"], false)).unwrap();
        assert_eq!(names(&s), ["api", "db", "web", "worker"]);
        let s = resolve(&p, &req(&["worker"], &["front"], false)).unwrap();
        assert_eq!(names(&s), ["api", "db", "web", "worker"]);
        let s = resolve(&p, &req(&[], &["backend"], true)).unwrap();
        assert_eq!(names(&s), ["api", "worker"]);
    }

    #[test]
    fn unknown_service_fails_with_did_you_mean() {
        let e = resolve(&project(), &req(&["wev"], &[], false)).unwrap_err();
        assert!(e.contains("unknown service 'wev'"), "{e}");
        assert!(e.contains("did you mean 'web'?"), "{e}");
        assert!(e.contains("available: api, db, web, worker"), "{e}");
    }

    #[test]
    fn unknown_profile_fails_with_did_you_mean() {
        let e = resolve(&project(), &req(&[], &["backnd"], false)).unwrap_err();
        assert!(e.contains("unknown profile 'backnd'"), "{e}");
        assert!(e.contains("did you mean 'backend'?"), "{e}");
    }

    #[test]
    fn every_unknown_name_is_reported() {
        let e = resolve(&project(), &req(&["nope"], &["zzz"], false)).unwrap_err();
        assert!(e.contains("unknown service 'nope'") && e.contains("unknown profile 'zzz'"));
    }

    #[test]
    fn unknown_profile_with_no_profiles_defined_says_so() {
        let mut p = project();
        p.profiles.clear();
        let e = resolve(&p, &req(&[], &["x"], false)).unwrap_err();
        assert!(e.contains("no profiles are defined"), "{e}");
    }

    #[test]
    fn no_deps_alone_is_an_error() {
        let e = resolve(&project(), &req(&[], &[], true)).unwrap_err();
        assert!(e.contains("--no-deps"), "{e}");
    }

    #[test]
    fn dependency_cycle_terminates() {
        let mut p = project();
        p.services.insert("a".into(), svc(&["b"]));
        p.services.insert("b".into(), svc(&["a"]));
        let s = resolve(&p, &req(&["a"], &[], false)).unwrap();
        assert_eq!(names(&s), ["a", "b"]);
    }
}
