//! Repairs of what older Windows builds (1.5.0-alpha.21..24) left behind, run after the NSIS
//! installer so an upgrade "just works": a stale `atlas.exe` on the PATH, an old terminal host,
//! a window-state file that restores a second title bar off-screen, a first-run flag that never
//! got saved, and temp files and locks of dead runs.
//!
//! [`collect`] reads the machine into a [`State`], [`plan`] (pure) turns it into [`Action`]s and
//! [`execute`] performs them through [`Exec`]. A failed action is logged and the next one runs:
//! a repair never fails the install. Nothing here deletes user data: the only files touched are
//! `.window-state.json` and `settings.json` (a field each, the rest of the document is kept) and
//! the temp leftovers named by [`is_temp_leftover`].

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use atlas_core::channel;
use semver::Version;
use serde_json::Value;

use crate::fsutil::Log;
use crate::paths::Env;

/// A screen rectangle in virtual-desktop pixels (what `.window-state.json` stores).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    fn intersects(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

/// What is at `~/.atlas/bin/atlas.exe`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cli {
    Missing,
    /// A symlink: `atlas link` (a dev build). Never replaced.
    Linked,
    Version(Version),
    /// It exists but did not report a version (corrupt, or too old to have `version`).
    Unreadable,
}

/// A terminal host that answered `hostd --status`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hostd {
    /// `None` when the answer carried no readable version.
    pub version: Option<Version>,
}

/// Everything [`plan`] decides from. Raw file contents are passed as text so the planner is pure.
#[derive(Clone, Debug)]
pub struct State {
    pub bundled: Version,
    pub cli: Cli,
    pub hostd: Option<Hostd>,
    /// `%APPDATA%\br.com.codeatlas.atlas\.window-state.json` and its path.
    pub window_state: Option<(PathBuf, String)>,
    pub monitors: Vec<Rect>,
    /// `~/.atlas/desktop/settings.json` and its path.
    pub settings: Option<(PathBuf, String)>,
    /// Whether `projects.json` lists at least one project.
    pub has_projects: bool,
    pub temp_leftovers: Vec<PathBuf>,
    /// `~/.atlas/.update.lock` when the pid it holds is dead.
    pub stale_lock: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// `atlas.exe install --upgrade` of the bundled CLI: replaces an older `~/.atlas/bin/atlas.exe`.
    ReplaceCli,
    /// The bundled `atlas hostd --reconcile`: the old host goes when no terminal runs (never ends
    /// a terminal otherwise; the app asks the user).
    ReconcileHost,
    Write(PathBuf, String),
    Delete(PathBuf),
}

impl Action {
    pub fn describe(&self) -> String {
        match self {
            Action::ReplaceCli => "replace the CLI with the bundled one".into(),
            Action::ReconcileHost => "bring the terminal host to the bundled version".into(),
            Action::Write(p, _) => format!("rewrite {}", p.display()),
            Action::Delete(p) => format!("delete {}", p.display()),
        }
    }
}

/// The version in `atlas version` / `atlas --version` output (`atlas 1.5.0-alpha.24 ...`).
pub fn parse_cli_version(out: &str) -> Option<Version> {
    let first = out.lines().next()?;
    channel::parse_tag(first.strip_prefix("atlas")?.split_whitespace().next()?)
}

/// The version in `atlas hostd --status` output.
pub fn parse_hostd_version(out: &str) -> Option<Version> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    channel::parse_tag(v["version"].as_str()?)
}

/// Temp files of Atlas's own dead runs: the branded uninstaller's `atlas-uninstalled-*.exe`, an
/// unpacked `atlas-setup-*` payload, the CLI's `.atlas.*.__relocated__.exe` and NSIS's `~nsu*.tmp`.
pub fn is_temp_leftover(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    (n.starts_with("atlas-uninstalled-") && n.ends_with(".exe"))
        || n.starts_with("atlas-setup-")
        || (n.starts_with(".atlas.") && n.ends_with(".__relocated__.exe"))
        || (n.starts_with("~nsu") && n.ends_with(".tmp"))
}

/// `.window-state.json` without the entries that cannot be seen and without `decorated` (the frame
/// belongs to the platform config; a stale `true` gave two title bars). `None` when nothing
/// changes or the text is not the plugin's JSON object (never rewrite what is not understood).
pub fn sanitize_window_state(text: &str, monitors: &[Rect]) -> Option<String> {
    let mut doc: Value = serde_json::from_str(text).ok()?;
    let map = doc.as_object_mut()?;
    let before = map.clone();
    map.retain(|_, w| on_screen(w, monitors));
    for w in map.values_mut() {
        if let Some(o) = w.as_object_mut() {
            o.remove("decorated");
        }
    }
    (*map != before).then(|| serde_json::to_string(&doc).unwrap_or_default())
}

/// Does the saved window rect touch any monitor? Without a monitor list (the query failed) or
/// without a readable rect the entry is kept: only a rect known to be outside goes.
fn on_screen(w: &Value, monitors: &[Rect]) -> bool {
    if monitors.is_empty() {
        return true;
    }
    let n = |k: &str| w[k].as_i64().and_then(|v| i32::try_from(v).ok());
    let (Some(x), Some(y), Some(width), Some(height)) = (n("x"), n("y"), n("width"), n("height"))
    else {
        return true;
    };
    // A maximized window's x/y is the monitor's corner (the plugin keeps the real spot in
    // prev_x/prev_y), and a minimized one is parked at -32000: both are judged by what the
    // plugin would restore, which is the rect itself.
    let r = Rect {
        x,
        y,
        w: width.max(1),
        h: height.max(1),
    };
    monitors.iter().any(|m| m.intersects(&r))
}

/// `settings.json` with `onboarding.done` set to true; `None` when it already is, or the text is
/// not a JSON object. Every other field is kept as it was.
pub fn finish_onboarding(text: &str) -> Option<String> {
    let mut doc: Value = serde_json::from_str(text).ok()?;
    let root = doc.as_object_mut()?;
    let ob = root
        .entry("onboarding")
        .or_insert_with(|| Value::Object(Default::default()))
        .as_object_mut()?;
    if ob.get("done") == Some(&Value::Bool(true)) {
        return None;
    }
    ob.insert("done".into(), Value::Bool(true));
    serde_json::to_string_pretty(&doc).ok()
}

/// Does `projects.json` list a project? (an array with at least one entry)
pub fn lists_projects(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.as_array().map(|a| !a.is_empty()))
        .unwrap_or(false)
}

/// What to do for `s`. Pure.
pub fn plan(s: &State) -> Vec<Action> {
    let mut out = Vec::new();
    let older = |v: &Version| channel::precedence(v, &s.bundled) == Ordering::Less;
    // Linked dev builds and a missing CLI are not stale: the NSIS hook installs a missing one,
    // and a link is the developer's choice.
    if match &s.cli {
        Cli::Version(v) => older(v),
        Cli::Unreadable => true,
        Cli::Missing | Cli::Linked => false,
    } {
        out.push(Action::ReplaceCli);
    }
    if let Some(h) = &s.hostd
        && h.version.as_ref() != Some(&s.bundled)
    {
        out.push(Action::ReconcileHost);
    }
    if let Some((p, text)) = &s.window_state
        && let Some(fixed) = sanitize_window_state(text, &s.monitors)
    {
        out.push(Action::Write(p.clone(), fixed));
    }
    if s.has_projects
        && let Some((p, text)) = &s.settings
        && let Some(fixed) = finish_onboarding(text)
    {
        out.push(Action::Write(p.clone(), fixed));
    }
    out.extend(s.temp_leftovers.iter().cloned().map(Action::Delete));
    out.extend(s.stale_lock.iter().cloned().map(Action::Delete));
    out
}

/// The side effects of the two actions that run the bundled CLI, behind a trait so [`execute`] is
/// tested without processes.
pub trait Exec {
    fn replace_cli(&self) -> Result<(), String>;
    fn reconcile_host(&self) -> Result<(), String>;
}

/// Perform `actions`, logging each; a failure is logged and the rest still run. Returns how many
/// succeeded.
pub fn execute(actions: &[Action], exec: &dyn Exec, log: &Log) -> usize {
    let mut done = 0;
    for a in actions {
        let r = match a {
            Action::ReplaceCli => exec.replace_cli(),
            Action::ReconcileHost => exec.reconcile_host(),
            Action::Write(p, text) => write_atomic(p, text),
            Action::Delete(p) => remove(p),
        };
        match r {
            Ok(()) => {
                done += 1;
                log.line(format!("repair: {}", a.describe()));
            }
            Err(e) => log.line(format!("repair failed ({}): {e}", a.describe())),
        }
    }
    done
}

fn write_atomic(p: &Path, text: &str) -> Result<(), String> {
    let tmp = p.with_extension("atlas-setup.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

fn remove(p: &Path) -> Result<(), String> {
    let r = if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    };
    match r {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

/// Entries of `temp` that are Atlas's own leftovers, minus `keep` (this run's own payload dir).
pub fn find_temp_leftovers(temp: &Path, keep: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(temp) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| is_temp_leftover(&e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .filter(|p| p != keep)
        .collect()
}

/// `%APPDATA%\br.com.codeatlas.atlas\.window-state.json` (the plugin's file in `app_config_dir`).
pub fn window_state_path(app_data: &Path) -> PathBuf {
    app_data
        .join("br.com.codeatlas.atlas")
        .join(".window-state.json")
}

/// Read the text files of the state; everything process-related (`cli`, `hostd`, `monitors`,
/// `stale_lock`) is filled by the caller.
pub fn collect_files(env: &Env, app_data: &Path, own_payload: &Path) -> FileFacts {
    let read = |p: PathBuf| std::fs::read_to_string(&p).ok().map(|t| (p, t));
    let desktop = env.data_dir().join("desktop");
    FileFacts {
        window_state: read(window_state_path(app_data)),
        settings: read(desktop.join("settings.json")),
        has_projects: read(desktop.join("projects.json")).is_some_and(|(_, t)| lists_projects(&t)),
        temp_leftovers: find_temp_leftovers(&env.temp, own_payload),
    }
}

pub struct FileFacts {
    pub window_state: Option<(PathBuf, String)>,
    pub settings: Option<(PathBuf, String)>,
    pub has_projects: bool,
    pub temp_leftovers: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn base() -> State {
        State {
            bundled: v("1.5.0-alpha.25"),
            cli: Cli::Version(v("1.5.0-alpha.25")),
            hostd: None,
            window_state: None,
            monitors: vec![Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            }],
            settings: None,
            has_projects: false,
            temp_leftovers: vec![],
            stale_lock: None,
        }
    }

    #[test]
    fn a_clean_machine_needs_nothing() {
        assert_eq!(plan(&base()), []);
    }

    /// Incident (alpha.24 machine): the NSIS hook installs the CLI only when absent, so
    /// `~/.atlas/bin/atlas.exe` stayed on an older build.
    #[test]
    fn an_older_or_unreadable_cli_is_replaced_but_never_a_link_or_a_newer_one() {
        let mut s = base();
        s.cli = Cli::Version(v("1.5.0-alpha.24"));
        assert_eq!(plan(&s), [Action::ReplaceCli]);
        s.cli = Cli::Unreadable;
        assert_eq!(plan(&s), [Action::ReplaceCli]);
        s.cli = Cli::Linked;
        assert_eq!(plan(&s), [], "an `atlas link` dev build is the developer's");
        s.cli = Cli::Missing;
        assert_eq!(plan(&s), [], "the NSIS hook installs a missing one");
        s.cli = Cli::Version(v("1.5.1"));
        assert_eq!(plan(&s), [], "a newer CLI is never downgraded");
        s.cli = Cli::Version(v("1.5.0"));
        assert_eq!(plan(&s), [], "the release is newer than its alphas");
    }

    #[test]
    fn prerelease_counters_compare_as_numbers_not_text() {
        // Through the real decision: is the installed CLI older than the bundled one?
        let stale = |bundled: &str, installed: &str| {
            let mut s = base();
            s.bundled = v(bundled);
            s.cli = Cli::Version(v(installed));
            plan(&s) == [Action::ReplaceCli]
        };
        assert!(
            stale("1.5.0-alpha.24", "1.5.0-alpha.9"),
            "alpha.9 is older than alpha.24"
        );
        assert!(
            !stale("1.5.0-alpha.9", "1.5.0-alpha.24"),
            "and not the other way round"
        );
        assert!(
            stale("1.5.0", "1.5.0-alpha.24"),
            "a release is newer than its alphas"
        );
        assert!(!stale("1.5.0-alpha.24", "1.5.0-alpha.24"));
        assert!(
            !stale("1.5.0-alpha.24", "1.5.0-alpha.24+build5"),
            "build metadata is not precedence"
        );
        assert!(stale("1.5.0-alpha.24", "1.4.9"));
    }

    #[test]
    fn versions_are_read_from_the_real_command_outputs() {
        // `atlas version` (crates/atlas-cli/src/commands/version.rs) and `hostd --status` (JSON).
        assert_eq!(
            parse_cli_version("atlas 1.5.0-alpha.24\n  os:   windows\n  arch: x86_64"),
            Some(v("1.5.0-alpha.24"))
        );
        assert_eq!(parse_cli_version("error: unrecognized subcommand"), None);
        assert_eq!(parse_cli_version(""), None);
        assert_eq!(
            parse_hostd_version(r#"{"version":"1.5.0-alpha.24","pid":672}"#),
            Some(v("1.5.0-alpha.24"))
        );
        assert_eq!(parse_hostd_version("atlas hostd is not running"), None);
    }

    /// Incident: an old hostd kept serving the new app (version skew in hostd.log).
    #[test]
    fn a_host_that_is_not_the_bundled_version_is_reconciled() {
        let mut s = base();
        s.hostd = Some(Hostd {
            version: Some(v("1.5.0-alpha.24")),
        });
        assert_eq!(plan(&s), [Action::ReconcileHost]);
        s.hostd = Some(Hostd { version: None });
        assert_eq!(
            plan(&s),
            [Action::ReconcileHost],
            "one with no readable version too"
        );
        s.hostd = Some(Hostd {
            version: Some(v("1.5.0-alpha.25")),
        });
        assert_eq!(plan(&s), [], "a current host is left alone");
    }

    const BAD_STATE: &str = r#"{
        "main": {"width":1200,"height":800,"x":-32000,"y":-32000,"prev_x":-32000,"prev_y":-32000,
                 "maximized":true,"visible":true,"decorated":true,"fullscreen":false},
        "other": {"width":800,"height":600,"x":100,"y":100,"prev_x":100,"prev_y":100,
                  "maximized":false,"visible":true,"decorated":true,"fullscreen":false}
    }"#;

    /// Incident (alpha.24): `"decorated": true` brought back a native title bar, and a window
    /// saved on a monitor that is gone opened invisibly.
    #[test]
    fn the_window_state_loses_decorated_and_unseen_entries_but_keeps_the_rest() {
        let m = base().monitors;
        let fixed: Value =
            serde_json::from_str(&sanitize_window_state(BAD_STATE, &m).unwrap()).unwrap();
        assert!(fixed.get("main").is_none(), "fully outside every monitor");
        let other = &fixed["other"];
        assert!(other.get("decorated").is_none());
        assert_eq!(other["width"], 800);
        assert_eq!(other["x"], 100);
        assert_eq!(other["maximized"], false);
    }

    #[test]
    fn a_window_that_touches_a_second_monitor_stays_and_a_clean_file_is_not_rewritten() {
        let monitors = vec![
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            Rect {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ];
        let ok = r#"{"main":{"width":1000,"height":700,"x":-1500,"y":50,"maximized":false}}"#;
        assert_eq!(sanitize_window_state(ok, &monitors), None);
        // Half on the screen is still reachable.
        let edge = r#"{"main":{"width":1000,"height":700,"x":1500,"y":50}}"#;
        assert_eq!(sanitize_window_state(edge, &monitors), None);
        // The monitor query failed: nothing is judged off-screen, only `decorated` goes.
        let fixed = sanitize_window_state(BAD_STATE, &[]).unwrap();
        assert!(fixed.contains("main") && !fixed.contains("decorated"));
    }

    #[test]
    fn text_that_is_not_the_plugins_json_is_never_rewritten() {
        let m = base().monitors;
        assert_eq!(sanitize_window_state("not json", &m), None);
        assert_eq!(sanitize_window_state("[1,2]", &m), None);
        assert_eq!(sanitize_window_state("", &m), None);
        assert_eq!(finish_onboarding("not json"), None);
        assert_eq!(finish_onboarding("[]"), None);
    }

    /// Incident (alpha.24): `done` was saved only by the last "Continue", so the deck came back.
    #[test]
    fn onboarding_is_finished_only_when_projects_exist_and_nothing_else_changes() {
        let text = r#"{"onboarding":{"checklist":{"dismissed":false,"done":["project"]},
            "done":false,"toured":false},"updates":{"auto":true,"lastSeen":"1.5.0-alpha.24"},
            "theme":"dark"}"#;
        let mut s = base();
        s.settings = Some((PathBuf::from("settings.json"), text.into()));
        assert_eq!(plan(&s), [], "no project: the welcome is still due");
        s.has_projects = true;
        let [Action::Write(p, out)] = &plan(&s)[..] else {
            panic!("one write expected");
        };
        assert_eq!(p, Path::new("settings.json"));
        let before: Value = serde_json::from_str(text).unwrap();
        let after: Value = serde_json::from_str(out).unwrap();
        assert_eq!(after["onboarding"]["done"], true);
        let mut expect = before.clone();
        expect["onboarding"]["done"] = Value::Bool(true);
        assert_eq!(
            after, expect,
            "only done changed: toured, checklist, theme, updates stay"
        );
    }

    #[test]
    fn onboarding_that_is_done_or_a_settings_file_without_the_key_is_handled() {
        let mut s = base();
        s.has_projects = true;
        s.settings = Some((
            PathBuf::from("s.json"),
            r#"{"onboarding":{"done":true}}"#.into(),
        ));
        assert_eq!(plan(&s), []);
        s.settings = Some((PathBuf::from("s.json"), r#"{"theme":"dark"}"#.into()));
        let [Action::Write(_, out)] = &plan(&s)[..] else {
            panic!("one write expected");
        };
        let after: Value = serde_json::from_str(out).unwrap();
        assert_eq!(
            (after["onboarding"]["done"].clone(), after["theme"].clone()),
            (true.into(), "dark".into())
        );
        assert!(lists_projects(r#"[{"root":"C:/x"}]"#));
        assert!(!lists_projects("[]") && !lists_projects("garbage"));
    }

    #[test]
    fn only_atlas_temp_files_are_leftovers() {
        for yes in [
            "atlas-uninstalled-1234.exe",
            "Atlas-Uninstalled-9.EXE",
            "atlas-setup-4242",
            ".atlas.1a2b.__relocated__.exe",
            "~nsu3F.tmp",
        ] {
            assert!(is_temp_leftover(yes), "{yes}");
        }
        for no in [
            "atlas-notes.txt",
            "atlas-setup.log",
            "atlas-uninstalled.txt",
            "report.exe",
            "~nsu3F.dat",
            "my-atlas-setup-1",
        ] {
            // atlas-setup.log is the run's own log: `atlas-setup-` needs the dash.
            assert!(!is_temp_leftover(no), "{no}");
        }
    }

    /// Never-touch-data rule: whatever the machine holds, the plan only writes the two named
    /// files and deletes only temp leftovers and the stale lock.
    #[test]
    fn the_plan_never_touches_user_data() {
        let t = tempfile::tempdir().unwrap();
        let env = Env {
            local_app_data: t.path().join("Local"),
            home: t.path().join("home"),
            temp: t.path().join("tmp"),
        };
        let desktop = env.data_dir().join("desktop");
        let app_data = t.path().join("Roaming");
        for d in [
            &desktop,
            &env.temp,
            &window_state_path(&app_data).parent().unwrap().to_path_buf(),
        ] {
            std::fs::create_dir_all(d).unwrap();
        }
        let w = |p: PathBuf, c: &str| std::fs::write(p, c).unwrap();
        w(desktop.join("projects.json"), r#"[{"root":"C:/dev/app"}]"#);
        w(
            desktop.join("settings.json"),
            r#"{"onboarding":{"done":false}}"#,
        );
        w(desktop.join("workspace.json"), "{}");
        w(window_state_path(&app_data), BAD_STATE);
        w(env.temp.join("atlas-uninstalled-1.exe"), "x");
        w(env.temp.join("notes-about-atlas.txt"), "mine");
        std::fs::create_dir_all(env.temp.join("atlas-setup-77")).unwrap();
        std::fs::create_dir_all(env.temp.join("atlas-setup-own")).unwrap();
        std::fs::create_dir_all(env.data_dir().join("worktrees")).unwrap();

        let f = collect_files(&env, &app_data, &env.temp.join("atlas-setup-own"));
        let mut s = base();
        s.window_state = f.window_state;
        s.settings = f.settings;
        s.has_projects = f.has_projects;
        s.temp_leftovers = f.temp_leftovers;
        s.stale_lock = Some(env.data_dir().join(".update.lock"));
        let actions = plan(&s);
        for a in &actions {
            match a {
                Action::Write(p, _) => assert!(
                    p.ends_with("settings.json") || p.ends_with(".window-state.json"),
                    "{p:?}"
                ),
                Action::Delete(p) => {
                    let n = p.file_name().unwrap().to_string_lossy().into_owned();
                    assert!(is_temp_leftover(&n) || n == ".update.lock", "{p:?}");
                }
                _ => {}
            }
        }
        let deleted: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Delete(p) => Some(p.clone()),
                _ => None,
            })
            .collect();
        assert!(deleted.contains(&env.temp.join("atlas-uninstalled-1.exe")));
        assert!(deleted.contains(&env.temp.join("atlas-setup-77")));
        assert!(
            !deleted.contains(&env.temp.join("atlas-setup-own")),
            "this run's payload"
        );
        assert!(!deleted.iter().any(|p| p.ends_with("notes-about-atlas.txt")));

        struct No;
        impl Exec for No {
            fn replace_cli(&self) -> Result<(), String> {
                Ok(())
            }
            fn reconcile_host(&self) -> Result<(), String> {
                Ok(())
            }
        }
        let log = Log::create(env.temp.join("atlas-setup.log"));
        std::fs::write(env.data_dir().join(".update.lock"), "999999").unwrap();
        execute(&actions, &No, &log);
        assert!(
            desktop.join("projects.json").is_file() && desktop.join("workspace.json").is_file()
        );
        assert!(env.data_dir().join("worktrees").is_dir());
        assert!(env.temp.join("notes-about-atlas.txt").is_file());
        assert!(!env.temp.join("atlas-uninstalled-1.exe").exists());
        assert!(!env.data_dir().join(".update.lock").exists());
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(desktop.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["onboarding"]["done"], true);
        let ws = std::fs::read_to_string(window_state_path(&app_data)).unwrap();
        assert!(!ws.contains("decorated"));
        let logged = std::fs::read_to_string(log.path()).unwrap();
        assert!(
            logged.contains("repair: delete") && logged.contains("repair: rewrite"),
            "{logged}"
        );
    }

    #[test]
    fn a_failing_action_is_logged_and_the_rest_still_run() {
        struct Spy(RefCell<Vec<&'static str>>);
        impl Exec for Spy {
            fn replace_cli(&self) -> Result<(), String> {
                self.0.borrow_mut().push("cli");
                Err("access denied".into())
            }
            fn reconcile_host(&self) -> Result<(), String> {
                self.0.borrow_mut().push("host");
                Ok(())
            }
        }
        let t = tempfile::tempdir().unwrap();
        let log = Log::create(t.path().join("l.log"));
        let junk = t.path().join("atlas-setup-1");
        std::fs::create_dir(&junk).unwrap();
        let spy = Spy(RefCell::new(vec![]));
        let n = execute(
            &[
                Action::ReplaceCli,
                Action::ReconcileHost,
                Action::Delete(junk.clone()),
            ],
            &spy,
            &log,
        );
        assert_eq!(n, 2);
        assert_eq!(*spy.0.borrow(), ["cli", "host"]);
        assert!(!junk.exists());
        let logged = std::fs::read_to_string(log.path()).unwrap();
        assert!(
            logged.contains("repair failed") && logged.contains("access denied"),
            "{logged}"
        );
        // A leftover that is already gone is not a failure.
        assert_eq!(execute(&[Action::Delete(junk)], &spy, &log), 1);
    }

    /// Order matters: the CLI first (the host is restarted by the bundled one), files after.
    #[test]
    fn the_cli_is_replaced_before_the_host_is_reconciled() {
        let mut s = base();
        s.cli = Cli::Version(v("1.5.0-alpha.20"));
        s.hostd = Some(Hostd {
            version: Some(v("1.5.0-alpha.20")),
        });
        assert_eq!(plan(&s), [Action::ReplaceCli, Action::ReconcileHost]);
    }
}
