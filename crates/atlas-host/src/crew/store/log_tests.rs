//! Crash safety (#293) and bounded growth (#295) of the per-crewmate logs (`tasks.jsonl`,
//! `usage.jsonl`). Every test drives the real store (`append_task`, `append_usage`, ...) and damages
//! the file the way a crash or a bad byte does; nothing hand-writes a record the store would not.

use std::io::Write;

use super::*;

fn store() -> (tempfile::TempDir, CrewStore) {
    let t = tempfile::tempdir().unwrap();
    let s = CrewStore::new(t.path());
    (t, s)
}

fn task(prompt: &str) -> Task {
    Task::new("test", prompt)
}

/// Bytes appended to the crewmate's `tasks.jsonl` as the crash would have left them.
fn raw_append(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    f.write_all(bytes).unwrap();
}

fn ids(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|t| t.id.as_str()).collect()
}

// #293: a torn final line (no newline) glued the next record onto it and both were lost.
#[test]
fn a_record_appended_after_a_torn_one_is_readable() {
    let (_t, s) = store();
    let a = task("a");
    s.append_task("c", &a).unwrap();
    let torn = serde_json::to_string(&task("torn")).unwrap();
    raw_append(&s.tasks_path("c"), &torn.as_bytes()[..torn.len() / 2]);

    let b = task("b");
    s.append_task("c", &b).unwrap();

    let got = s.read_tasks("c", 10);
    assert_eq!(ids(&got), vec![b.id.as_str(), a.id.as_str()]);
}

// #293: torn line, then valid records appended by the real writer: all valid records come back.
#[test]
fn a_torn_line_in_the_middle_costs_exactly_one_record() {
    let (_t, s) = store();
    let a = task("a");
    s.append_task("c", &a).unwrap();
    raw_append(&s.tasks_path("c"), b"{\"id\":\"half\",\"ts\":1,\"sou\n");
    let (b, c) = (task("b"), task("c"));
    s.append_task("c", &b).unwrap();
    s.append_task("c", &c).unwrap();

    let got = s.read_tasks("c", 10);
    assert_eq!(ids(&got), vec![c.id.as_str(), b.id.as_str(), a.id.as_str()]);
}

// #293: `map_while(Result::ok)` stopped at the first line that is not UTF-8 and hid every later task.
#[test]
fn invalid_utf8_mid_file_does_not_hide_later_tasks() {
    let (_t, s) = store();
    let a = task("a");
    s.append_task("c", &a).unwrap();
    raw_append(&s.tasks_path("c"), b"\xff\xfe\xfd not a record\n");
    let b = task("b");
    s.append_task("c", &b).unwrap();

    let got = s.read_tasks("c", 10);
    assert_eq!(ids(&got), vec![b.id.as_str(), a.id.as_str()]);
}

// #293: a multi-byte character cut in half inside a valid record (Portuguese text) loses at most the
// character: the record itself is kept.
#[test]
fn a_cut_multibyte_char_inside_a_record_keeps_the_record() {
    let (_t, s) = store();
    let mut line = serde_json::to_vec(&task("revisão do código")).unwrap();
    // "ã" is C3 A3: drop its second byte, leave the rest of the JSON intact.
    let at = line.windows(2).position(|w| w == [0xC3, 0xA3]).unwrap();
    line.remove(at + 1);
    line.push(b'\n');
    raw_append(&s.tasks_path("c"), &line);
    let b = task("b");
    s.append_task("c", &b).unwrap();

    let got = s.read_tasks("c", 10);
    assert_eq!(got.len(), 2, "both records survive: {got:?}");
    assert!(got[1].prompt.starts_with("revis"));
}

fn usage(tokens: u64) -> UsageRecord {
    UsageRecord {
        input_tokens: Some(tokens),
        ..Default::default()
    }
}

// #293: the usage log had the same two bugs as the task log.
#[test]
fn usage_after_a_torn_line_and_invalid_utf8_is_still_read() {
    let (_t, s) = store();
    s.append_usage("c", "claude", 10, &usage(1)).unwrap();
    let path = s.crewmate_dir("c").join("usage.jsonl");
    raw_append(&path, b"\xff\xfe garbage\n");
    s.append_usage("c", "claude", 20, &usage(2)).unwrap();
    raw_append(&path, b"{\"ts\":30,\"eng"); // torn, no newline
    s.append_usage("c", "claude", 40, &usage(4)).unwrap();

    let ts: Vec<u64> = s
        .read_usage(Some("c"), 0)
        .iter()
        .map(|v| v["ts"].as_u64().unwrap())
        .collect();
    assert_eq!(ts, vec![10, 20, 40]);
}

fn step(n: usize) -> Step {
    Step {
        ts: 1_000 + n as u64,
        text: format!("Bash step number {n}"),
        group: Some("STORE: group".into()),
    }
}

fn file_len(s: &CrewStore, crewmate: &str) -> u64 {
    std::fs::metadata(s.tasks_path(crewmate)).map_or(0, |m| m.len())
}

// #295: every step used to append the whole task (prompt and all steps so far): ~50 copies of a task.
#[test]
fn a_fifty_step_task_writes_its_prompt_once() {
    let (_t, s) = store();
    let t = task(&"a long prompt, as users write them. ".repeat(300)); // ~10 KB
    s.append_runner_state("c", &t).unwrap();
    let before = file_len(&s, "c");

    for n in 0..50 {
        s.append_step("c", &t.id, &step(n)).unwrap();
    }

    let written = file_len(&s, "c") - before;
    assert!(
        written < 50 * 160,
        "50 steps wrote {written} bytes, the prompt alone is {}",
        t.prompt.len()
    );
    let got = s.task("c", &t.id).unwrap();
    assert_eq!(got.steps.len(), 50);
    assert_eq!(got.steps[49].text, "Bash step number 49");
    assert_eq!(got.prompt, t.prompt);
}

// #295: the steps of a task survive a restart (a new store and index over the same file), in order,
// and a later full record (title, done) replaces the folded steps with its own copy.
#[test]
fn steps_are_read_back_after_a_restart_and_a_full_record_wins() {
    let (t, s) = store();
    let mut task = task("p");
    s.append_runner_state("c", &task).unwrap();
    for n in 0..3 {
        s.append_step("c", &task.id, &step(n)).unwrap();
        task.steps.push(step(n));
    }
    drop(s);
    tasklog::LOGS.lock().as_mut().unwrap().clear(); // as a fresh process

    let s = CrewStore::new(t.path());
    let got = s.task("c", &task.id).unwrap();
    assert_eq!(
        got.steps
            .iter()
            .map(|x| x.text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Bash step number 0",
            "Bash step number 1",
            "Bash step number 2"
        ]
    );
    task.state = TaskState::Done;
    s.append_runner_state("c", &task).unwrap();
    let got = s.task("c", &task.id).unwrap();
    assert_eq!((got.state, got.steps.len()), (TaskState::Done, 3));
}

// #295: a step for a task with no record is refused, so the caller stores the task whole: the log
// never holds a step that belongs to nothing.
#[test]
fn a_step_for_an_unknown_task_is_refused_and_writes_nothing() {
    let (_t, s) = store();
    let known = task("known");
    s.append_runner_state("c", &known).unwrap();
    let before = file_len(&s, "c");
    let err = s.append_step("c", "no-such-task", &step(0)).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    assert_eq!(file_len(&s, "c"), before);
    let err = s.append_step("nobody", &known.id, &step(0)).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    assert!(
        !s.crewmate_dir("nobody").exists(),
        "a refused step creates nothing"
    );
}

// #295: the runner's steps never take back what hook routes wrote (the contract of
// `append_runner_state`), whichever side wrote last.
#[test]
fn steps_and_hook_owned_fields_do_not_overwrite_each_other() {
    let (_t, s) = store();
    let mut t = task("p");
    s.append_runner_state("c", &t).unwrap();
    s.append_step("c", &t.id, &step(0)).unwrap();
    s.update_task("c", &t.id, |x| {
        x.ui = true;
        Ok(())
    })
    .unwrap();
    s.append_step("c", &t.id, &step(1)).unwrap();
    t.steps = vec![step(0), step(1)];
    t.title = Some("T".into());
    s.append_runner_state("c", &t).unwrap();

    let got = s.task("c", &t.id).unwrap();
    assert!(got.ui, "the hook's ui mark survives the runner's records");
    assert_eq!((got.steps.len(), got.title.as_deref()), (2, Some("T")));
}

// #295: `crewmate_list` and the 30 s cancel check read one task over and over: after the index is
// built a read costs no file bytes, and an append costs only its own line, with 10 000 records stored.
#[test]
fn reading_the_latest_task_does_not_parse_the_file_again() {
    let (_t, s) = store();
    let mut last = String::new();
    for n in 0..10_000 {
        let t = task(&format!("task {n}"));
        last = t.id.clone();
        s.append_task("c", &t).unwrap();
    }
    assert_eq!(s.read_tasks("c", 1)[0].id, last);
    let built = bytes_read(&s, "c");

    for _ in 0..200 {
        assert_eq!(s.read_tasks("c", 1)[0].id, last);
        assert!(s.task("c", &last).is_some());
    }
    assert_eq!(bytes_read(&s, "c"), built, "reads must come from the index");

    let t = task("one more");
    s.append_task("c", &t).unwrap();
    assert_eq!(s.read_tasks("c", 1)[0].id, t.id);
    assert_eq!(
        bytes_read(&s, "c"),
        built,
        "an append through the store is not read back"
    );
}

fn bytes_read(s: &CrewStore, crewmate: &str) -> u64 {
    tasklog::LOGS.lock().as_ref().unwrap()[&s.tasks_path(crewmate)].bytes_read
}

// #295: another writer appended to the file (an import, a tool): the next read sees it, and only it is parsed.
#[test]
fn a_record_added_to_the_file_behind_the_store_is_read() {
    let (_t, s) = store();
    let a = task("a");
    s.append_task("c", &a).unwrap();
    let _ = s.read_tasks("c", 1);
    let b = task("b");
    raw_append(
        &s.tasks_path("c"),
        format!("{}\n", serde_json::to_string(&b).unwrap()).as_bytes(),
    );
    assert_eq!(
        ids(&s.read_tasks("c", 10)),
        vec![b.id.as_str(), a.id.as_str()]
    );
}

// #295 with #297: deleting a crewmate drops its index (no tasks are read back from memory), and a late
// step from a runner that was still going writes nothing and does not recreate `crew/<id>/`.
#[test]
fn a_deleted_crewmates_tasks_and_late_steps_are_gone() {
    let (_t, s) = store();
    let t = task("old");
    s.append_task("c", &t).unwrap();
    assert_eq!(s.read_tasks("c", 10).len(), 1);

    s.delete_crewmate_dir("c").unwrap();

    assert!(s.read_tasks("c", 10).is_empty());
    assert!(s.task("c", &t.id).is_none());
    assert!(s.append_step("c", &t.id, &step(0)).is_err());
    assert!(
        !s.crewmate_dir("c").exists(),
        "a late step recreated crew/c/"
    );
}

// #295: a task log from before deltas and compaction (full records, even without a final newline)
// reads unchanged, and is compacted like any other once it is big enough.
#[test]
fn an_old_log_of_full_records_reads_unchanged() {
    let (_t, s) = store();
    let (a, mut b) = (task("a"), task("b"));
    let mut text = String::new();
    for rec in [&a, &b] {
        text += &serde_json::to_string(rec).unwrap();
        text.push('\n');
    }
    b.state = TaskState::Done;
    text += &serde_json::to_string(&b).unwrap(); // no newline at the end
    std::fs::create_dir_all(s.crewmate_dir("c")).unwrap();
    std::fs::write(s.tasks_path("c"), text).unwrap();

    let got = s.read_tasks("c", 10);
    assert_eq!(ids(&got), vec![b.id.as_str(), a.id.as_str()]);
    assert_eq!(got[0].state, TaskState::Done);
}

// #295: the log is rewritten to one record per task once it has grown, so it stays bounded, and
// nothing a reader sees changes: same tasks, order, steps, last state.
#[test]
fn the_log_is_compacted_to_one_record_per_task_and_reads_the_same() {
    let (_t, mut s) = store();
    s.compact_min = 64 * 1024;
    let tasks: Vec<Task> = (0..5)
        .map(|n| task(&format!("task {n} {}", "x".repeat(2_000))))
        .collect();
    for t in &tasks {
        s.append_runner_state("c", t).unwrap();
    }
    let mut running = tasks[2].clone();
    for n in 0..50 {
        s.append_step("c", &running.id, &step(n)).unwrap();
        running.steps.push(step(n));
    }
    let want = s.read_tasks("c", 100);
    assert_eq!(want[2].steps.len(), 50);

    // Churn: a state change per round is a full record each.
    let mut grew_to = 0;
    for round in 0..200 {
        running.title = Some(format!("round {round}"));
        s.append_runner_state("c", &running).unwrap();
        grew_to = grew_to.max(file_len(&s, "c"));
    }

    let after = file_len(&s, "c");
    assert!(
        grew_to < 3 * 64 * 1024,
        "the log reached {grew_to} bytes with a 64 KiB floor"
    );
    assert!(after < 4 * 64 * 1024, "{after} bytes for 5 tasks");
    let got = s.read_tasks("c", 100);
    assert_eq!(ids(&got), ids(&want));
    assert_eq!(got[2].steps.len(), 50);
    assert_eq!(got[2].title.as_deref(), Some("round 199"));
    // And a restart reads the compacted file to the same result.
    tasklog::LOGS.lock().as_mut().unwrap().clear();
    let again = CrewStore::new(s.base.as_path()).read_tasks("c", 100);
    assert_eq!(
        serde_json::to_value(&again).unwrap(),
        serde_json::to_value(&got).unwrap()
    );
}

// #295: compaction keeps a line it cannot read (a torn write, a newer hostd's record) instead of dropping it.
#[test]
fn compaction_keeps_lines_it_does_not_understand() {
    let (_t, mut s) = store();
    s.compact_min = 16 * 1024;
    let t = task("p");
    s.append_task("c", &t).unwrap();
    raw_append(
        &s.tasks_path("c"),
        b"{\"id\":\"future\",\"state\":\"from_a_newer_hostd\"}\n",
    );
    for round in 0..200 {
        let mut x = t.clone();
        x.title = Some(format!("{round} {}", "y".repeat(200)));
        s.append_task("c", &x).unwrap();
    }
    let text = std::fs::read_to_string(s.tasks_path("c")).unwrap();
    assert!(
        text.contains("from_a_newer_hostd"),
        "the unknown record was dropped"
    );
    assert!(
        text.len() < 16 * 1024 * 4,
        "and the log still shrank: {} bytes",
        text.len()
    );
}

// #295: usage.jsonl is cut back to its newest records once it is over its cap.
#[test]
fn usage_is_trimmed_to_the_newest_records() {
    let (_t, s) = store();
    let total = 60_000u64;
    for n in 0..total {
        s.append_usage("c", "claude", n, &usage(n)).unwrap();
    }
    let len = std::fs::metadata(s.crewmate_dir("c").join("usage.jsonl"))
        .unwrap()
        .len();
    assert!(len <= USAGE_MAX_BYTES + 256, "usage.jsonl is {len} bytes");
    let got = s.read_usage(Some("c"), 0);
    assert_eq!(
        got.last().unwrap()["ts"],
        total - 1,
        "the newest record is kept"
    );
    assert!(
        got.first().unwrap()["ts"].as_u64().unwrap() > 0,
        "the oldest went"
    );
    let ts: Vec<u64> = got.iter().map(|v| v["ts"].as_u64().unwrap()).collect();
    assert!(
        ts.windows(2).all(|w| w[0] + 1 == w[1]),
        "no gap inside what is kept"
    );
}

// #295: export and import move `tasks.jsonl` as bytes, so a log holding step deltas (and one that was
// compacted) must still arrive as the same tasks: the contract of `archive`.
#[test]
fn an_archive_of_a_log_with_step_deltas_and_compaction_round_trips() {
    let (_t, mut src) = store();
    src.compact_min = 32 * 1024;
    src.save_crewmate(
        &serde_json::from_value(serde_json::json!({
            "id": "c", "name": "n", "role": "r", "agent": "claude", "approval": "auto", "created_at": 1,
        }))
        .unwrap(),
    )
    .unwrap();
    let mut t = task(&"p".repeat(3_000));
    src.append_runner_state("c", &t).unwrap();
    for n in 0..30 {
        src.append_step("c", &t.id, &step(n)).unwrap();
        t.steps.push(step(n));
    }
    // One archive taken with deltas in the file, one after a compaction.
    let with_deltas = {
        let mut b = Vec::new();
        archive::export(&src, &["c".to_string()], false, &mut b).unwrap();
        b
    };
    for round in 0..80 {
        t.title = Some(format!("r{round}"));
        src.append_runner_state("c", &t).unwrap();
    }
    let compacted = {
        let mut b = Vec::new();
        archive::export(&src, &["c".to_string()], false, &mut b).unwrap();
        b
    };

    for bytes in [with_deltas, compacted] {
        let t2 = tempfile::tempdir().unwrap();
        let dst = CrewStore::new(t2.path());
        archive::import(&dst, &bytes[..], false).unwrap();
        let got = dst.task("c", &t.id).unwrap();
        assert_eq!(got.steps.len(), 30);
        assert_eq!(got.steps[29].text, "Bash step number 29");
        assert_eq!(got.prompt, t.prompt);
    }
}
