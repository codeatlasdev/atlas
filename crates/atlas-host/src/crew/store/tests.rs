//! The store's on-disk contract: where crewmates live, and that servers that ran them as "bots"
//! keep every one of them after the rename.

use super::*;

fn legacy_crewmate_json(id: &str) -> String {
    // A `bot.json` as the pre-Crew hostd wrote it: no `crewmate_id`, a bot-chain trigger.
    serde_json::json!({
        "id": id, "name": "Jorge", "role": "r", "agent": "claude", "approval": "auto",
        "triggers": [{
            "id": "t1", "kind": {"type": "bot", "from_bot": "other", "on": "done"},
            "enabled": true, "prompt": "p",
        }],
        "created_at": 1,
    })
    .to_string()
}

// Luan's servers keep every crewmate in `<dir>/bots/<id>/`: the first start after the update
// must find them under `<dir>/crew/`, with their tasks and memory.
#[test]
fn a_store_dir_under_the_old_name_is_adopted_with_every_crewmate() {
    let t = tempfile::tempdir().unwrap();
    let old = t.path().join("bots/abc");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("bot.json"), legacy_crewmate_json("abc")).unwrap();
    std::fs::write(old.join("memory.md"), "likes tabs").unwrap();

    let store = CrewStore::new(t.path());

    assert!(!t.path().join("bots").exists());
    let list = store.list_crewmates();
    assert_eq!(list.len(), 1, "the crewmate survived the move");
    assert_eq!(list[0].name, "Jorge");
    assert_eq!(store.read_memory("abc"), "likes tabs");
}

#[test]
fn a_store_that_only_has_the_new_dir_is_used_as_is() {
    let t = tempfile::tempdir().unwrap();
    let store = CrewStore::new(t.path());
    store.save_crewmate(&sample("new1")).unwrap();

    let again = CrewStore::new(t.path());

    assert_eq!(again.list_crewmates().len(), 1);
    assert!(!t.path().join("bots").exists());
}

// Both present: the new dir is the truth; the old one stays for a human, never merged.
#[test]
fn with_both_dirs_the_new_one_wins_and_the_old_is_left_untouched() {
    let t = tempfile::tempdir().unwrap();
    let old = t.path().join("bots/old1");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("bot.json"), legacy_crewmate_json("old1")).unwrap();
    CrewStore::new(t.path()); // nothing yet: this adopts the old dir...
    std::fs::create_dir_all(t.path().join("bots/old2")).unwrap(); // ...and a stray one reappears
    std::fs::write(
        t.path().join("bots/old2/bot.json"),
        legacy_crewmate_json("old2"),
    )
    .unwrap();

    let store = CrewStore::new(t.path());

    let ids: Vec<_> = store.list_crewmates().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, ["old1"], "only the new dir is read");
    assert!(
        t.path().join("bots/old2/bot.json").exists(),
        "old dir untouched"
    );
}

// A crewmate file still named `bot.json` (adopted dir) loads; the next save writes the new name
// and no stale copy is left to shadow later edits.
#[test]
fn a_legacy_bot_json_loads_and_is_replaced_on_the_next_save() {
    let t = tempfile::tempdir().unwrap();
    let store = CrewStore::new(t.path());
    let dir = store.crewmate_dir("abc");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bot.json"), legacy_crewmate_json("abc")).unwrap();

    let mut c = store.load_crewmate("abc").unwrap();
    c.name = "Renamed".into();
    store.save_crewmate(&c).unwrap();

    assert!(dir.join("crewmate.json").exists());
    assert!(!dir.join("bot.json").exists(), "no stale legacy file");
    assert_eq!(store.load_crewmate("abc").unwrap().name, "Renamed");
}

// The bot-chain trigger of a stored `bot.json` is `{"type":"bot","from_bot":…}`; the new wire is
// `{"type":"crewmate","from_crewmate":…}`. Old data loads, and is written back in the new shape.
#[test]
fn a_stored_bot_chain_trigger_loads_and_is_written_in_the_new_shape() {
    let t = tempfile::tempdir().unwrap();
    let store = CrewStore::new(t.path());
    let dir = store.crewmate_dir("abc");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bot.json"), legacy_crewmate_json("abc")).unwrap();

    let c = store.load_crewmate("abc").unwrap();
    assert!(matches!(
        &c.triggers[0].kind,
        TriggerKind::Crewmate { from_crewmate, on } if from_crewmate == "other" && on == "done"
    ));
    let v = serde_json::to_value(&c.triggers[0].kind).unwrap();
    assert_eq!(
        v,
        serde_json::json!({"type": "crewmate", "from_crewmate": "other", "on": "done"})
    );
}

// Usage records are served with the owning crewmate's id under the wire key `crewmate_id`.
#[test]
fn usage_records_carry_crewmate_id() {
    let t = tempfile::tempdir().unwrap();
    let store = CrewStore::new(t.path());
    store.save_crewmate(&sample("abc")).unwrap();
    store
        .append_usage("abc", "claude", 5, &UsageRecord::default())
        .unwrap();

    let recs = store.read_usage(None, 0);

    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0]["crewmate_id"], "abc");
    assert!(recs[0].get("bot_id").is_none());
}

fn sample(id: &str) -> Crewmate {
    serde_json::from_value(serde_json::json!({
        "id": id, "name": "n", "role": "r", "agent": "claude", "approval": "auto", "created_at": 1,
    }))
    .unwrap()
}

// CrewSettings: default has short_replies=true, explicit false parses correctly.
#[test]
fn crew_settings_parse_with_explicit_strings() {
    let default: CrewSettings = serde_json::from_str("{}").unwrap();
    assert!(default.short_replies, "default must be true");
    assert!(default.short_replies_model.is_none());

    let off: CrewSettings = serde_json::from_str(r#"{"short_replies": false}"#).unwrap();
    assert!(!off.short_replies);

    let with_model: CrewSettings =
        serde_json::from_str(r#"{"short_replies": true, "short_replies_model": "my-model"}"#)
            .unwrap();
    assert_eq!(with_model.short_replies_model.as_deref(), Some("my-model"));
}

// CrewSettings round-trip through the store.
#[test]
fn crew_settings_store_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let store = CrewStore::new(dir.path());
    // Default: short_replies=true when no file exists.
    let loaded = store.load_crew_settings();
    assert!(loaded.short_replies);
    // Save false and reload.
    let settings = CrewSettings {
        short_replies: false,
        short_replies_model: None,
        paused: true,
    };
    store.save_crew_settings(&settings).unwrap();
    let reloaded = store.load_crew_settings();
    assert!(!reloaded.short_replies);
    assert!(reloaded.paused, "paused is stored with the settings");
}
