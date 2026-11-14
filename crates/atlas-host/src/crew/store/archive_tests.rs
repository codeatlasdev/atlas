//! Issue #202: a crew server's data leaves as one versioned archive and comes back into an empty (or
//! non-empty) store. These tests use real stores and the real tar.gz bytes, never a mock.

use super::archive::{
    self, FORMAT_VERSION, ImportError, MANIFEST, Manifest, is_exported, plan_ids, remap_crewmate,
};
use super::*;

fn crewmate(id: &str, name: &str) -> Crewmate {
    serde_json::from_value(serde_json::json!({
        "id": id, "name": name, "role": "reviews PRs", "agent": "claude", "approval": "ask",
        "created_at": 7,
        "triggers": [
            {"id": "t-sched", "kind": {"type": "schedule", "cron": "0 9 * * *"}, "enabled": true, "prompt": "daily"},
            {"id": "t-hook", "kind": {"type": "webhook", "token": "s3cret-hook"}, "enabled": true, "prompt": "hook"},
        ],
    }))
    .unwrap()
}

fn chained(id: &str, name: &str, after: &str) -> Crewmate {
    let mut c = crewmate(id, name);
    c.triggers = vec![Trigger {
        id: "t-chain".into(),
        kind: TriggerKind::Crewmate {
            from_crewmate: after.into(),
            on: "done".into(),
        },
        enabled: true,
        prompt: "after {{event}}".into(),
        last_fired: Some(5),
        tz: None,
        next_fire: None,
    }];
    c
}

/// A store with two crewmates and everything a server accumulates, including files that must
/// never travel.
fn populated() -> (tempfile::TempDir, CrewStore) {
    let t = tempfile::tempdir().unwrap();
    let s = CrewStore::new(t.path());
    s.save_crewmate(&crewmate("aaa", "Aria")).unwrap();
    s.save_crewmate(&chained("bbb", "Bo", "aaa")).unwrap();
    for id in ["aaa", "bbb"] {
        s.write_memory(id, &format!("memory of {id}")).unwrap();
        let mut task = Task::new("chat", format!("prompt of {id}"));
        task.state = TaskState::Done;
        task.result = Some("ok".into());
        s.append_task(id, &task).unwrap();
        s.append_usage(
            id,
            "claude",
            9,
            &UsageRecord {
                cost_usd: Some(0.5),
                ..Default::default()
            },
        )
        .unwrap();
        s.set_secret(id, "API_KEY", "hunter2").unwrap();
        s.save_session(
            id,
            &CrewmateSession {
                session_id: Some("sess".into()),
                session_file: None,
                created_at: 1,
            },
        )
        .unwrap();
        std::fs::create_dir_all(s.crewmate_dir(id).join("work/.git")).unwrap();
        std::fs::write(s.crewmate_dir(id).join("work/src.rs"), "fn main(){}").unwrap();
        std::fs::write(s.crewmate_dir(id).join("t-gh_seen.jsonl"), "{}\n").unwrap();
    }
    s.save_crew_settings(&CrewSettings {
        short_replies: false,
        short_replies_model: Some("m".into()),
        paused: true,
    })
    .unwrap();
    (t, s)
}

fn export_bytes(s: &CrewStore, ids: &[String], with_secrets: bool) -> Vec<u8> {
    let mut out = Vec::new();
    archive::export(s, ids, with_secrets, &mut out).unwrap();
    out
}

fn entry_names(bytes: &[u8]) -> Vec<String> {
    let mut a = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    a.entries()
        .unwrap()
        .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
        .collect()
}

// Round trip: export from a populated store, import into an empty one: crewmates, memory and task
// history are equal, usage and the crew settings come along.
#[test]
fn an_archive_round_trips_crewmates_memory_tasks_and_usage_into_an_empty_store() {
    let (_t, src) = populated();
    let bytes = export_bytes(&src, &[], false);
    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());

    let report = archive::import(&dst, &bytes[..], false).unwrap();

    assert_eq!(report.imported.len(), 2);
    for id in ["aaa", "bbb"] {
        let (a, b) = (src.load_crewmate(id).unwrap(), dst.load_crewmate(id).unwrap());
        assert_eq!(a.name, b.name);
        assert_eq!(a.role, b.role);
        assert_eq!(a.created_at, b.created_at);
        assert_eq!(src.read_memory(id), dst.read_memory(id));
        let (ta, tb) = (src.read_tasks(id, 99), dst.read_tasks(id, 99));
        assert_eq!(ta.len(), 1);
        assert_eq!(
            serde_json::to_value(&ta).unwrap(),
            serde_json::to_value(&tb).unwrap(),
            "task history is equal"
        );
        assert_eq!(src.read_usage(Some(id), 0), dst.read_usage(Some(id), 0));
    }
    let s = dst.load_crew_settings();
    assert!(!s.short_replies);
    assert_eq!(s.short_replies_model.as_deref(), Some("m"));
    assert!(
        !s.paused,
        "a restored crew never starts paused by the source's kill switch"
    );
}

// Secrets are opt-in: not in the bytes, not on the target, and a webhook token (a credential in
// crewmate.json) goes too, its automation arriving disabled.
#[test]
fn secrets_are_excluded_by_default() {
    let (_t, src) = populated();
    let bytes = export_bytes(&src, &[], false);

    let names = entry_names(&bytes);
    assert!(
        !names.iter().any(|n| n.ends_with("secrets.json")),
        "{names:?}"
    );
    let raw = String::from_utf8_lossy(&bytes).to_string();
    assert!(!raw.contains("hunter2"));

    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());
    archive::import(&dst, &bytes[..], false).unwrap();
    assert!(!dst.secrets_path("aaa").exists());
    let hook = dst
        .load_crewmate("aaa")
        .unwrap()
        .triggers
        .into_iter()
        .find(|t| t.id == "t-hook")
        .unwrap();
    assert!(!hook.enabled, "a webhook without its token cannot fire");
    assert!(matches!(hook.kind, TriggerKind::Webhook { token, .. } if token.is_empty()));
    // Only the webhook was touched.
    let sched = dst
        .load_crewmate("aaa")
        .unwrap()
        .triggers
        .into_iter()
        .find(|t| t.id == "t-sched")
        .unwrap();
    assert!(sched.enabled);
}

#[test]
fn with_secrets_the_secrets_and_webhook_tokens_travel_and_the_manifest_says_so() {
    let (_t, src) = populated();
    let bytes = export_bytes(&src, &[], true);
    assert!(entry_names(&bytes).iter().any(|n| n == "crew/aaa/secrets.json"));
    assert!(archive::read_manifest(&bytes[..]).unwrap().secrets);

    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());
    archive::import(&dst, &bytes[..], false).unwrap();
    assert_eq!(dst.secret_env("aaa").get("API_KEY").unwrap(), "hunter2");
    let hook = dst
        .load_crewmate("aaa")
        .unwrap()
        .triggers
        .into_iter()
        .find(|t| t.id == "t-hook")
        .unwrap();
    assert!(hook.enabled);
    assert!(matches!(hook.kind, TriggerKind::Webhook { token, .. } if token == "s3cret-hook"));
}

// Never in an archive: the work/ checkout, agent session pointers, per-machine GitHub dedupe
// files, anything unknown. (Agent sign-in files live outside the crew dir and are never read.)
#[test]
fn only_the_allowed_files_are_exported() {
    let (_t, src) = populated();
    let names = entry_names(&export_bytes(&src, &[], true));
    for n in &names {
        assert!(
            n == MANIFEST
                || n == "crew.json"
                || ["crewmate.json", "memory.md", "tasks.jsonl", "usage.jsonl", "secrets.json"]
                    .iter()
                    .any(|f| n.ends_with(&format!("/{f}"))),
            "unexpected entry {n}"
        );
    }
    assert!(!names.iter().any(|n| n.contains("work")));
    assert!(!names.iter().any(|n| n.contains("session")));
    assert!(!names.iter().any(|n| n.contains("_seen")));
    assert_eq!(names[0], MANIFEST, "the manifest comes first");
}

#[test]
fn is_exported_decides_by_file_name_alone() {
    assert!(is_exported("crewmate.json", false));
    assert!(is_exported("bot.json", false), "a pre-rename definition");
    assert!(is_exported("memory.md", false));
    assert!(is_exported("tasks.jsonl", false));
    assert!(is_exported("usage.jsonl", false));
    assert!(!is_exported("secrets.json", false));
    assert!(is_exported("secrets.json", true));
    for no in ["session.json", "work", "x_seen.jsonl", "auth.json", ".env"] {
        assert!(!is_exported(no, true), "{no}");
    }
}

#[test]
fn a_selection_exports_only_those_crewmates() {
    let (_t, src) = populated();
    let bytes = export_bytes(&src, &["bbb".to_string()], false);
    let m = archive::read_manifest(&bytes[..]).unwrap();
    assert_eq!(m.crewmates.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["bbb"]);
    assert!(!entry_names(&bytes).iter().any(|n| n.starts_with("crew/aaa")));
}

#[test]
fn exporting_an_unknown_crewmate_is_an_error_not_an_empty_archive() {
    let (_t, src) = populated();
    let mut out = Vec::new();
    let e = archive::export(&src, &["nope".to_string()], false, &mut out).unwrap_err();
    assert!(e.to_string().contains("nope"), "{e}");
}

#[test]
fn the_manifest_records_version_format_crewmates_and_time() {
    let (_t, src) = populated();
    let m = archive::read_manifest(&export_bytes(&src, &[], false)[..]).unwrap();
    assert_eq!(m.format, FORMAT_VERSION);
    assert_eq!(m.atlas_version, env!("CARGO_PKG_VERSION"));
    assert!(m.created_at > 0);
    assert!(!m.secrets);
    let mut ids: Vec<_> = m.crewmates.iter().map(|c| (c.id.as_str(), c.name.as_str())).collect();
    ids.sort();
    assert_eq!(ids, [("aaa", "Aria"), ("bbb", "Bo")]);
}

// A format this version does not know is refused whole, before anything is written.
#[test]
fn an_unknown_format_version_is_refused_and_nothing_is_written() {
    let (_t, src) = populated();
    let good = export_bytes(&src, &[], false);
    let future = rewrite_manifest(&good, |m| m.format = FORMAT_VERSION + 1);
    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());

    let e = archive::import(&dst, &future[..], false).unwrap_err();

    assert!(matches!(e, ImportError::UnsupportedFormat(v) if v == FORMAT_VERSION + 1), "{e}");
    assert!(dst.list_crewmates().is_empty());
    assert!(!t2.path().join("crew").exists() || std::fs::read_dir(t2.path().join("crew")).unwrap().next().is_none());
}

#[test]
fn a_file_that_is_not_an_archive_or_has_no_manifest_is_refused() {
    let t = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t.path());
    assert!(matches!(
        archive::import(&dst, &b"not gzip at all"[..], false),
        Err(ImportError::Unreadable(_))
    ));
    let mut b = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    ));
    let mut h = tar::Header::new_gnu();
    h.set_size(0);
    h.set_cksum();
    b.append_data(&mut h, "crew/x/crewmate.json", &b""[..]).unwrap();
    let no_manifest = b.into_inner().unwrap().finish().unwrap();
    assert!(matches!(
        archive::import(&dst, &no_manifest[..], false),
        Err(ImportError::NoManifest)
    ));
}

// An id that already exists is never overwritten silently.
#[test]
fn a_colliding_id_is_refused_without_rename_and_the_store_is_untouched() {
    let (_t, src) = populated();
    let bytes = export_bytes(&src, &[], false);
    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());
    dst.save_crewmate(&crewmate("aaa", "Already here")).unwrap();
    dst.write_memory("aaa", "mine").unwrap();

    let e = archive::import(&dst, &bytes[..], false).unwrap_err();

    assert!(matches!(&e, ImportError::Collision(ids) if ids == &["aaa".to_string()]), "{e}");
    assert_eq!(dst.load_crewmate("aaa").unwrap().name, "Already here");
    assert_eq!(dst.read_memory("aaa"), "mine");
    assert!(dst.load_crewmate("bbb").is_err(), "nothing of the archive was written");
}

// With --rename the colliding crewmates get new ids, and a chain trigger that pointed at one of
// them follows it; everything else is untouched.
#[test]
fn rename_gives_new_ids_and_remaps_chain_triggers_to_them() {
    let (_t, src) = populated();
    let bytes = export_bytes(&src, &[], false);
    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());
    dst.save_crewmate(&crewmate("aaa", "Already here")).unwrap();

    let report = archive::import(&dst, &bytes[..], true).unwrap();

    let new_a = report.imported.iter().find(|i| i.old_id == "aaa").unwrap();
    assert_ne!(new_a.new_id, "aaa");
    assert_eq!(dst.load_crewmate("aaa").unwrap().name, "Already here", "the original is untouched");
    assert_eq!(dst.load_crewmate(&new_a.new_id).unwrap().id, new_a.new_id);
    assert_eq!(dst.read_memory(&new_a.new_id), "memory of aaa");
    // bbb did not collide: it keeps its id, and its chain follows aaa's new id.
    let b = report.imported.iter().find(|i| i.old_id == "bbb").unwrap();
    assert_eq!(b.new_id, "bbb");
    let chain = &dst.load_crewmate("bbb").unwrap().triggers[0];
    assert!(
        matches!(&chain.kind, TriggerKind::Crewmate { from_crewmate, .. } if from_crewmate == &new_a.new_id),
        "{:?}",
        chain.kind
    );
    assert_eq!(chain.last_fired, None, "a copy has not fired anything yet");
}

#[test]
fn rename_without_a_collision_changes_no_id() {
    let (_t, src) = populated();
    let bytes = export_bytes(&src, &[], false);
    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());
    let report = archive::import(&dst, &bytes[..], true).unwrap();
    assert!(report.imported.iter().all(|i| i.old_id == i.new_id));
}

#[test]
fn plan_ids_keeps_free_ids_and_replaces_taken_ones_with_fresh_unique_ids() {
    let incoming = ["a".to_string(), "b".to_string()];
    let taken = |id: &str| id == "a";
    let mut n = 0;
    let map = plan_ids(&incoming, taken, true, || {
        n += 1;
        // The generator hands back "a" once (taken) and "b" (also incoming): both must be skipped.
        ["a", "b", "fresh"][n - 1].to_string()
    })
    .unwrap();
    assert_eq!(map["a"], "fresh");
    assert_eq!(map["b"], "b");

    let e = plan_ids(&incoming, taken, false, || unreachable!()).unwrap_err();
    assert!(matches!(e, ImportError::Collision(ids) if ids == ["a"]));
}

#[test]
fn remap_rewrites_only_chain_triggers_that_point_at_a_remapped_id() {
    let mut c = chained("bbb", "Bo", "aaa");
    c.triggers.push(Trigger {
        id: "t-other".into(),
        kind: TriggerKind::Crewmate {
            from_crewmate: "elsewhere".into(),
            on: "failed".into(),
        },
        enabled: true,
        prompt: "p".into(),
        last_fired: None,
        tz: None,
        next_fire: None,
    });
    let map = std::collections::HashMap::from([
        ("aaa".to_string(), "zzz".to_string()),
        ("bbb".to_string(), "yyy".to_string()),
    ]);

    remap_crewmate(&mut c, &map, true);

    assert_eq!(c.id, "yyy");
    let froms: Vec<_> = c
        .triggers
        .iter()
        .map(|t| match &t.kind {
            TriggerKind::Crewmate { from_crewmate, .. } => from_crewmate.clone(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(froms, ["zzz", "elsewhere"]);
}

// An archive entry must never write outside the crewmate's own directory.
#[test]
fn hostile_entry_paths_are_refused() {
    let (_t, src) = populated();
    let good = export_bytes(&src, &[], false);
    for bad in ["crew/aaa/../../evil", "/etc/passwd", "crew/../x/memory.md", "crew/aaa/sub/memory.md"] {
        let evil = with_extra_entry(&good, bad);
        let t2 = tempfile::tempdir().unwrap();
        let dst = CrewStore::new(t2.path());
        let e = archive::import(&dst, &evil[..], false).unwrap_err();
        assert!(matches!(e, ImportError::BadEntry(_)), "{bad}: {e}");
        assert!(dst.list_crewmates().is_empty(), "{bad}");
    }
}

// A crewmate id from the archive is also a directory name: refuse separators and dots.
#[test]
fn a_manifest_id_that_is_not_a_plain_name_is_refused() {
    let (_t, src) = populated();
    let good = export_bytes(&src, &[], false);
    let bad = rewrite_manifest(&good, |m| m.crewmates[0].id = "../x".into());
    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());
    assert!(matches!(
        archive::import(&dst, &bad[..], false),
        Err(ImportError::BadEntry(_))
    ));
}

// A pre-rename server's definition (bot.json, from_bot) exports and imports in the new shape.
#[test]
fn a_pre_rename_server_exports_its_crewmates_in_the_new_shape() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("bots/old");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("bot.json"),
        serde_json::json!({
            "id": "old", "name": "Jorge", "role": "r", "agent": "claude", "approval": "auto",
            "triggers": [{"id": "t1", "kind": {"type": "bot", "from_bot": "other", "on": "done"}, "enabled": true, "prompt": "p"}],
            "created_at": 1,
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("memory.md"), "old memory").unwrap();
    let src = CrewStore::new(t.path());
    let bytes = export_bytes(&src, &[], false);

    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());
    archive::import(&dst, &bytes[..], false).unwrap();

    assert!(dst.crewmate_dir("old").join("crewmate.json").exists());
    assert_eq!(dst.read_memory("old"), "old memory");
}

// An import restores crew.json only on a crew that has none: a running server's settings (and
// its kill switch) are never replaced by an archive's.
#[test]
fn an_import_never_replaces_the_settings_of_a_crew_that_has_some() {
    let (_t, src) = populated();
    let bytes = export_bytes(&src, &[], false);
    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());
    dst.save_crew_settings(&CrewSettings {
        short_replies: true,
        short_replies_model: None,
        paused: true,
    })
    .unwrap();

    archive::import(&dst, &bytes[..], false).unwrap();

    let s = dst.load_crew_settings();
    assert!(s.short_replies && s.paused, "{s:?}");
}

#[cfg(unix)]
#[test]
fn imported_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (_t, src) = populated();
    let bytes = export_bytes(&src, &[], true);
    let t2 = tempfile::tempdir().unwrap();
    let dst = CrewStore::new(t2.path());
    archive::import(&dst, &bytes[..], false).unwrap();
    for f in ["secrets.json", "crewmate.json", "memory.md"] {
        let mode = std::fs::metadata(dst.crewmate_dir("aaa").join(f)).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{f} is {mode:o}");
    }
}

// ── helpers that edit a real archive ──────────────────────────────────────────

fn read_all(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    use std::io::Read;
    let mut a = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    a.entries()
        .unwrap()
        .map(|e| {
            let mut e = e.unwrap();
            let p = e.path().unwrap().to_string_lossy().into_owned();
            let mut v = Vec::new();
            e.read_to_end(&mut v).unwrap();
            (p, v)
        })
        .collect()
}

fn build(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut b = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    ));
    for (p, data) in entries {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        // `append_data` refuses `..` and absolute paths; write the raw name like a hostile tar would.
        let name = h.as_old_mut().name.as_mut();
        name.fill(0);
        name[..p.len()].copy_from_slice(p.as_bytes());
        h.set_cksum();
        b.append(&h, &data[..]).unwrap();
    }
    b.into_inner().unwrap().finish().unwrap()
}

fn rewrite_manifest(bytes: &[u8], f: impl FnOnce(&mut Manifest)) -> Vec<u8> {
    let mut entries = read_all(bytes);
    let mut m: Manifest = serde_json::from_slice(&entries[0].1).unwrap();
    f(&mut m);
    entries[0].1 = serde_json::to_vec(&m).unwrap();
    build(&entries)
}

fn with_extra_entry(bytes: &[u8], path: &str) -> Vec<u8> {
    let mut entries = read_all(bytes);
    entries.push((path.to_string(), b"x".to_vec()));
    build(&entries)
}
