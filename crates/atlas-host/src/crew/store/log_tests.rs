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

// ---- #295: usage.jsonl is rotated into usage-<date>.jsonl.gz, never trimmed ----

/// Every usage record of `c` as (ts, input_tokens), oldest first.
fn usage_rows(s: &CrewStore) -> Vec<(u64, u64)> {
    s.read_usage(Some("c"), 0)
        .iter()
        .map(|v| {
            (
                v["ts"].as_u64().unwrap(),
                v["input_tokens"].as_u64().unwrap(),
            )
        })
        .collect()
}

fn rotated(s: &CrewStore) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(s.crewmate_dir("c"))
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("usage-") && n.ends_with(".jsonl.gz"))
        .collect();
    v.sort();
    v
}

const DAY: u64 = 86_400;

// #295: the old cap cut usage.jsonl to its newest half, so every total older than the cut was lost. Rotation
// moves the old part to a gzip and the usage view sums live + rotated: same records, same totals, as if
// nothing had ever been rotated.
#[test]
fn usage_totals_are_identical_before_and_after_rotation() {
    let (_t, mut s) = store();
    s.usage_max = 4 * 1024;
    let (plain_t, mut plain) = store();
    plain.usage_max = u64::MAX; // never rotates: the reference
    let _keep = plain_t;
    let total = 400u64;
    for n in 0..total {
        // Spread over 5 days so rotation names differ.
        let ts = 1_700_000_000 + (n / 80) * DAY + n;
        s.append_usage("c", "claude", ts, &usage(n + 1)).unwrap();
        plain
            .append_usage("c", "claude", ts, &usage(n + 1))
            .unwrap();
    }
    assert!(!rotated(&s).is_empty(), "fixture: it rotated");
    assert!(rotated(&plain).is_empty());
    let live = std::fs::metadata(s.crewmate_dir("c").join("usage.jsonl"))
        .unwrap()
        .len();
    assert!(
        live <= s.usage_max + 256,
        "the live file stays under its cap: {live}"
    );

    let got = usage_rows(&s);
    assert_eq!(got, usage_rows(&plain), "same records, same order");
    assert_eq!(got.len() as u64, total);
    assert_eq!(
        got.iter().map(|r| r.1).sum::<u64>(),
        total * (total + 1) / 2,
        "no token is lost"
    );
    // `since` filters across the rotated part too.
    let cut = 1_700_000_000 + 2 * DAY;
    assert_eq!(
        s.read_usage(Some("c"), cut).len(),
        plain.read_usage(Some("c"), cut).len()
    );
    // The all-crewmates view sums them as well.
    assert_eq!(s.read_usage(None, 0).len() as u64, total);
}

// #295: rotated files are named by the day of their newest record, and a second rotation on the same day
// adds to that file instead of overwriting it (overwriting would lose history, the very bug).
#[test]
fn rotation_names_files_by_day_and_never_overwrites_one() {
    let (_t, mut s) = store();
    s.usage_max = 2 * 1024;
    let day1 = 1_700_000_000; // 2023-11-14 UTC
    for n in 0..150 {
        s.append_usage("c", "claude", day1 + n, &usage(1)).unwrap();
    }
    let names = rotated(&s);
    assert_eq!(
        names,
        vec!["usage-2023-11-14.jsonl.gz".to_string()],
        "one day, one file"
    );
    assert_eq!(
        usage_rows(&s).len(),
        150,
        "several rotations the same day lose nothing"
    );
}

// #295: the disk stays bounded: past the number of rotated files kept, the oldest go.
#[test]
fn rotated_usage_files_are_bounded() {
    let (_t, mut s) = store();
    s.usage_max = 1024;
    s.usage_keep = 5;
    let n_days = 11u64;
    for d in 0..n_days {
        for n in 0..40 {
            s.append_usage("c", "claude", 1_700_000_000 + d * DAY + n, &usage(1))
                .unwrap();
        }
    }
    let names = rotated(&s);
    assert!(
        names.len() <= s.usage_keep,
        "{} rotated files kept: {names:?}",
        names.len()
    );
    let rows = usage_rows(&s);
    assert!(!rows.is_empty());
    assert_eq!(
        rows.last().unwrap().0,
        1_700_000_000 + (n_days - 1) * DAY + 39,
        "the newest record is always there"
    );
    // What went was the oldest, whole files (no gap inside what is kept).
    assert!(
        rows.windows(2).all(|w| w[0].0 <= w[1].0),
        "still in time order"
    );
}

// #295: a rotated file that is damaged costs its own records, not the live file or the other days.
#[test]
fn a_damaged_rotated_file_does_not_hide_the_rest_of_the_usage() {
    let (_t, mut s) = store();
    s.usage_max = 1024;
    for d in 0..3 {
        for n in 0..40 {
            s.append_usage("c", "claude", 1_700_000_000 + d * DAY + n, &usage(1))
                .unwrap();
        }
    }
    let names = rotated(&s);
    assert!(names.len() >= 2, "fixture: {names:?}");
    let before = usage_rows(&s).len();
    std::fs::write(s.crewmate_dir("c").join(&names[0]), b"this is not gzip").unwrap();
    let after = usage_rows(&s);
    assert!(
        after.len() < before && !after.is_empty(),
        "{} of {before}",
        after.len()
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

// ---- #295: the index of the task log is kept next to it (`tasks.idx`) ----

/// A log where an old task has a RECENT record (so a tail-only read would reorder), with step deltas, built
/// through the real store. Returns the ids in creation order.
fn busy_log(s: &CrewStore) -> Vec<String> {
    let mut tasks: Vec<Task> = (0..6).map(|n| task(&format!("task {n}"))).collect();
    for t in &tasks {
        s.append_runner_state("c", t).unwrap();
    }
    for n in 0..4 {
        s.append_step("c", &tasks[1].id, &step(n)).unwrap();
        tasks[1].steps.push(step(n));
    }
    // The oldest task gets the newest record of the file.
    tasks[0].title = Some("touched last".into());
    tasks[0].state = TaskState::Done;
    s.append_task("c", &tasks[0]).unwrap();
    tasks.iter().map(|t| t.id.clone()).collect()
}

/// What a reader of a brand-new process sees, and the bytes that cost.
fn reopen(t: &tempfile::TempDir) -> (CrewStore, Vec<Task>, u64) {
    tasklog::LOGS.lock().as_mut().unwrap().clear(); // as a fresh process
    let s = CrewStore::new(t.path());
    let got = s.read_tasks("c", 100);
    let read = bytes_read(&s, "c");
    (s, got, read)
}

fn as_json(tasks: &[Task]) -> serde_json::Value {
    serde_json::to_value(tasks).unwrap()
}

// #295: the first read of a crewmate per process parsed the whole file. With the index persisted, a reopen
// parses at most what was appended after the index was saved (never the old part), a second reopen parses
// nothing, and both answer exactly what a full parse answers, including the order of a log where an old
// task has the newest record.
#[test]
fn a_reopen_reads_no_old_bytes_and_matches_a_full_parse() {
    let (t, s) = store();
    let created = busy_log(&s);
    let want = s.read_tasks("c", 100);
    assert_eq!(
        want[5].id, created[0],
        "fixture: the old task is the oldest by creation"
    );
    assert!(want[5].title.as_deref() == Some("touched last"));
    let log_len = file_len(&s, "c");
    drop(s);

    // First reopen: the index was saved when this process first read the log, so only the lines added
    // since are parsed, and that is less than the file.
    let (s, got, read) = reopen(&t);
    assert!(
        read < log_len,
        "parsed {read} of {log_len} bytes: the index was not used"
    );
    assert_eq!(as_json(&got), as_json(&want));
    drop(s);

    // Second reopen: the index was brought up to date by the first.
    let (_s, got, read) = reopen(&t);
    assert_eq!(read, 0, "a reopen must not parse the log again");
    assert_eq!(as_json(&got), as_json(&want));
    assert_eq!(
        ids(&got),
        created.iter().rev().map(String::as_str).collect::<Vec<_>>(),
        "newest-created first, by first appearance"
    );

    // And the same answer as a store with NO index at all (the full parse).
    std::fs::remove_file(t.path().join("crew/c/tasks.idx")).unwrap();
    let (_s, full, full_read) = reopen(&t);
    assert_eq!(
        full_read, log_len,
        "without an index the whole file is parsed"
    );
    assert_eq!(as_json(&full), as_json(&got));
}

// #295: records appended after the index was written (the process ended before it was refreshed, another
// writer) are picked up, and only they are parsed.
#[test]
fn records_appended_after_the_index_are_picked_up_and_only_they_are_parsed() {
    let (t, s) = store();
    busy_log(&s);
    drop(s);
    let (s, _, _) = reopen(&t); // writes/loads the index
    drop(s);

    let extra = task("added behind the index");
    raw_append(
        &t.path().join("crew/c/tasks.jsonl"),
        format!("{}\n", serde_json::to_string(&extra).unwrap()).as_bytes(),
    );
    let line = serde_json::to_string(&extra).unwrap().len() as u64 + 1;

    let (s, got, read) = reopen(&t);
    assert_eq!(got[0].id, extra.id);
    assert_eq!(got.len(), 7);
    assert_eq!(read, line, "only the appended line is parsed");

    // A write after that reopen survives the next one as well.
    let next = task("next");
    s.append_task("c", &next).unwrap();
    drop(s);
    let (_s, got, _) = reopen(&t);
    assert_eq!(ids(&got)[..2], [next.id.as_str(), extra.id.as_str()]);
}

// #295: a log that was replaced under its index (compaction by another process, a restored backup, a
// truncation) must never be read through the stale index: wrong data is worse than a slow open.
#[test]
fn a_replaced_or_truncated_log_with_a_stale_index_is_rebuilt() {
    let (t, s) = store();
    busy_log(&s);
    drop(s);
    let (s, _, _) = reopen(&t);
    drop(s);
    let log = t.path().join("crew/c/tasks.jsonl");
    let idx = t.path().join("crew/c/tasks.idx");
    let stale_idx = std::fs::read(&idx).unwrap();
    let original = std::fs::read(&log).unwrap();

    // 1. Replaced by a LONGER file with different content (same prefix length or more).
    let other: Vec<Task> = (0..12).map(|n| task(&format!("other {n}"))).collect();
    let mut text = String::new();
    for o in &other {
        text += &serde_json::to_string(o).unwrap();
        text.push('\n');
    }
    assert!(text.len() > original.len());
    std::fs::write(&log, &text).unwrap();
    std::fs::write(&idx, &stale_idx).unwrap();
    let (_s, got, read) = reopen(&t);
    assert_eq!(
        ids(&got),
        other
            .iter()
            .rev()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(read > 0);

    // 2. Replaced by a SHORTER file.
    std::fs::write(&log, &original[..original.len() / 2]).unwrap();
    std::fs::write(&idx, &stale_idx).unwrap();
    let want: Vec<String> = {
        std::fs::remove_file(&idx).unwrap();
        let (_s, full, _) = reopen(&t);
        full.iter().map(|t| t.id.clone()).collect()
    };
    std::fs::write(&idx, &stale_idx).unwrap();
    let (_s, got, _) = reopen(&t);
    assert_eq!(
        ids(&got),
        want.iter().map(String::as_str).collect::<Vec<_>>()
    );

    // 3. Same length, one byte changed in the middle.
    let mut same_len = original.clone();
    let mid = same_len.len() / 2;
    same_len[mid] = if same_len[mid] == b'x' { b'y' } else { b'x' };
    std::fs::write(&log, &same_len).unwrap();
    std::fs::write(&idx, &stale_idx).unwrap();
    let (_s, got, read) = reopen(&t);
    std::fs::remove_file(&idx).unwrap();
    let (_s2, full, _) = reopen(&t);
    assert_eq!(as_json(&got), as_json(&full));
    assert!(read > 0, "a changed log is parsed again");
}

// #295: compaction rewrites the file, so the index must follow it: the next process opens the compacted
// log from its index, with no stale offsets, and reads what the compacted file holds.
#[test]
fn compaction_refreshes_the_index() {
    let (t, mut s) = store();
    s.compact_min = 32 * 1024;
    let mut running = task(&"p".repeat(2_000));
    s.append_runner_state("c", &running).unwrap();
    for round in 0..120 {
        running.title = Some(format!("round {round}"));
        s.append_runner_state("c", &running).unwrap();
    }
    let size = std::fs::metadata(t.path().join("crew/c/tasks.jsonl"))
        .unwrap()
        .len();
    assert!(
        size < 32 * 1024 * 2,
        "fixture: the log was compacted ({size} bytes)"
    );
    let want = s.read_tasks("c", 10);
    drop(s);

    let (_s, got, read) = reopen(&t);
    assert_eq!(read, 0, "the index describes the compacted file");
    assert_eq!(as_json(&got), as_json(&want));
    assert_eq!(got[0].title.as_deref(), Some("round 119"));
}

// #295: an index that is garbage, empty, from a newer version or for another file is a rebuild: never an
// error, never wrong data.
#[test]
fn a_corrupt_or_foreign_index_is_rebuilt_not_trusted() {
    let (t, s) = store();
    busy_log(&s);
    let want = s.read_tasks("c", 100);
    drop(s);
    let (s, _, _) = reopen(&t);
    drop(s);
    let idx = t.path().join("crew/c/tasks.idx");
    let good = std::fs::read_to_string(&idx).unwrap();

    let mut wrong_version: serde_json::Value = serde_json::from_str(&good).unwrap();
    wrong_version["version"] = serde_json::json!(9999);
    let mut shifted: serde_json::Value = serde_json::from_str(&good).unwrap();
    shifted["order"] = serde_json::json!(["nope", "never-written"]);
    let cases: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"not json at all \xff\xfe".to_vec(),
        good.as_bytes()[..good.len() / 2].to_vec(),
        wrong_version.to_string().into_bytes(),
        shifted.to_string().into_bytes(),
    ];
    for (n, bad) in cases.into_iter().enumerate() {
        std::fs::write(&idx, bad).unwrap();
        let (s, got, read) = reopen(&t);
        assert_eq!(as_json(&got), as_json(&want), "case {n}");
        assert!(read > 0, "case {n}: a rejected index means a full parse");
        drop(s);
        // And the rebuild rewrote a usable one.
        let (_s, again, read) = reopen(&t);
        assert_eq!(
            (as_json(&again), read),
            (as_json(&want), 0),
            "case {n}: rewritten"
        );
    }
}
