//! The engine's sentences: every code renders its English byte for byte (the bytes the engine wrote before the
//! codes existed, which the status file, MCP and `--ci` still carry), renders in Portuguese with its values, falls
//! back to the English `message` when it cannot, and every code a producer writes is one this build knows.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::note::{CODES, Note};

const MB: u64 = 1 << 20;
const GB: u64 = 1 << 30;

/// One sample per code: its args, and the English the engine wrote for them before the codes existed.
fn samples() -> Vec<(&'static str, Value, String)> {
    let waiting = Note::new("sched.waiting_port", json!({"port": 3000, "secs": 12}));
    let held = Note::new(
        "port.held",
        json!({"port": 3000, "pid": 4182, "cmd": "node", "stop": "kill 4182"}),
    );
    let not_found_fix = if cfg!(windows) {
        "Add its folder to your user Path (Environment Variables in the System settings) and restart Atlas"
    } else {
        "Add its directory to PATH in your shell's rc file (~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish) and restart Atlas, or pick the right shell in Settings → Integrations → Shell & links"
    };
    let failed = Note::new(
        "docker.compose_failed",
        json!({"exit": Note::new("docker.exit_code", json!({"code": 1})).wire(), "detail": "network not found"}),
    );
    let s = |code, args, english: &str| (code, args, english.to_string());
    vec![
        s("sched.stable", json!({"secs": 60}), "stable for 60s, restart counter reset"),
        s("sched.restarting", json!({}), "restarting"),
        s("sched.dependency_failed", json!({"dep": "api"}), "dependency api failed"),
        s(
            "sched.waiting_dep",
            json!({"dep": "db", "condition": "healthy"}),
            "waiting for db (healthy)",
        ),
        s(
            "sched.exited_restart",
            json!({"secs": 2, "attempt": 1, "limit": "5"}),
            "exited; restarting in 2s (1/5)",
        ),
        s(
            "sched.crashed_restart",
            json!({"secs": 4, "attempt": 2, "limit": "∞"}),
            "crashed; restarting in 4s (2/∞)",
        ),
        s(
            "sched.restarting_in",
            json!({"secs": 3, "attempt": 2, "limit": "5"}),
            "restarting in 3s (2/5)",
        ),
        s("sched.gave_up", json!({"attempts": 5}), "gave up after 5 restarts"),
        s("sched.completed", json!({}), "completed successfully"),
        s("sched.exited_cleanly", json!({}), "exited cleanly"),
        s("sched.exited", json!({}), "exited"),
        s("sched.exited_error", json!({}), "exited with error"),
        s(
            "sched.still_starting_port",
            json!({"waited": 30, "port": 3000}),
            "still starting after 30s: waiting for :3000",
        ),
        s(
            "sched.still_starting_log",
            json!({"waited": 30}),
            "still starting after 30s: waiting for ready_log",
        ),
        s(
            "sched.waiting_port",
            json!({"port": 3000, "secs": 12}),
            "waiting for :3000 (12s)",
        ),
        s("sched.waiting_log", json!({"secs": 12}), "waiting for ready_log (12s)"),
        s(
            "sched.start_timeout_port",
            json!({"port": 3000, "secs": 45}),
            "start timeout: still waiting for :3000 (45s)",
        ),
        s(
            "sched.start_timeout_log",
            json!({"secs": 45}),
            "start timeout: still waiting for ready_log (45s)",
        ),
        s("sched.done", json!({}), "done"),
        s("sched.restarted", json!({"attempts": 2}), "restarted ×2"),
        (
            "note.pair",
            json!({"first": waiting.wire(), "second": held.wire()}),
            "waiting for :3000 (12s) · port 3000 held by PID 4182 (node): stop it yourself (kill 4182) or change the service port".into(),
        ),
        (
            "note.lines",
            json!({"first": Note::new("docker.daemon_down", json!({})).wire(), "second": failed.wire()}),
            format!(
                "docker daemon is not running. Start it (Linux: sudo systemctl start docker; macOS: open OrbStack or Docker Desktop)\n{}",
                "compose up failed (exit 1): network not found"
            ),
        ),
        s(
            "health.unhealthy_for",
            json!({"secs": 7, "misses": 3}),
            "unhealthy for 7s: 3 missed probes",
        ),
        s(
            "health.hang",
            json!({"misses": 3, "timeout": 5}),
            "unhealthy: no answer from the liveness probe (3 misses × 5s timeout)",
        ),
        s(
            "health.hang_restarting",
            json!({"misses": 3, "timeout": 5}),
            "unhealthy: no answer from the liveness probe (3 misses × 5s timeout), restarting",
        ),
        s(
            "health.hang_spent",
            json!({}),
            "unhealthy again after its restarts: left running",
        ),
        s(
            "probe.ready_failed",
            json!({"error": "GET /ready → 500"}),
            "ready probe failed: GET /ready → 500",
        ),
        s(
            "probe.repeating",
            json!({"line": "ERROR db connect timeout"}),
            "repeating error: ERROR db connect timeout",
        ),
        s(
            "memory.over_warn",
            json!({"used": 1300 * MB, "warn": GB}),
            "using 1.3 GB, over memory_warn 1.0 GB",
        ),
        s(
            "memory.over_max",
            json!({"used": 2000 * MB, "max": GB}),
            "using 2.0 GB, over max_memory 1.0 GB",
        ),
        s(
            "memory.notify",
            json!({"used": 2000 * MB, "max": GB}),
            "using 2.0 GB (max_memory 1.0 GB), left running (on_memory_max: notify)",
        ),
        s(
            "memory.stopped",
            json!({"used": 2000 * MB, "max": GB, "restarts": 3}),
            "using 2.0 GB (max_memory 1.0 GB) after 3 restarts, stopped",
        ),
        s(
            "memory.restarting",
            json!({"used": 2000 * MB, "max": GB, "attempt": 1, "limit": 3}),
            "using 2.0 GB (max_memory 1.0 GB), restarting (1/3)",
        ),
        s("memory.back_under", json!({}), "memory back under budget"),
        s(
            "memory.leak_reloads",
            json!({"growth": 600 * MB, "cycles": 12}),
            "leak suspected: +600 MB over 12 reloads",
        ),
        s(
            "memory.leak_restarts",
            json!({"growth": 400 * MB, "cycles": 4}),
            "leak suspected: +400 MB over 4 restarts",
        ),
        s(
            "infra.port_closed",
            json!({"port": 5432, "secs": 30}),
            "nothing is listening on :5432 after 30s: the container did not start, exited, or maps another port (check `docker compose ps` and `docker compose logs`)",
        ),
        s(
            "docker.permission",
            json!({}),
            "docker: permission denied on the docker socket. Add your user to the docker group: sudo usermod -aG docker $USER, then log out and in",
        ),
        s(
            "docker.daemon_down",
            json!({}),
            "docker daemon is not running. Start it (Linux: sudo systemctl start docker; macOS: open OrbStack or Docker Desktop)",
        ),
        s(
            "docker.compose_missing",
            json!({}),
            "docker compose plugin is missing. Install docker-compose-plugin (Linux) or update Docker",
        ),
        s(
            "docker.compose_failed",
            json!({"exit": Note::new("docker.exit_signal", json!({})).wire(), "detail": Note::new("docker.no_output", json!({})).wire()}),
            "compose up failed (killed by a signal): no output",
        ),
        s("docker.exit_code", json!({"code": 3}), "exit 3"),
        s("docker.exit_signal", json!({}), "killed by a signal"),
        s("docker.no_output", json!({}), "no output"),
        s(
            "docker.not_found",
            json!({"path": "/usr/bin:/bin"}),
            "docker not found. Install Docker (https://docs.docker.com/engine/install/) or fix PATH; searched: /usr/bin:/bin",
        ),
        s(
            "docker.permission_run",
            json!({"error": "Permission denied (os error 13)"}),
            "docker could not be run: permission denied (Permission denied (os error 13))",
        ),
        s(
            "docker.run_failed",
            json!({"error": "Exec format error (os error 8)"}),
            "docker could not be run: Exec format error (os error 8)",
        ),
        s("docker.unreachable", json!({}), "Docker daemon unreachable."),
        s("docker.recovered", json!({}), "Docker daemon recovered."),
        s(
            "spawn.empty_command",
            json!({}),
            "could not start: the service command is empty",
        ),
        s(
            "spawn.failed",
            json!({"command": "bun dev", "dir": "apps/web", "error": "No such file or directory (os error 2)"}),
            "could not start `bun dev` in apps/web: No such file or directory (os error 2)",
        ),
        s("deps.exit", json!({"code": 3, "tail": "boom"}), "exit 3: boom"),
        s("deps.killed", json!({"tail": "boom"}), "exit signal: boom"),
        s(
            "deps.could_not_run",
            json!({"error": "No such file or directory (os error 2)"}),
            "could not run: No such file or directory (os error 2)",
        ),
        (
            "deps.not_found",
            json!({"code": 127, "tail": "sh: 1: bun: not found", "bin": "bun", "path": "/usr/bin:/bin"}),
            format!(
                "exit 127: sh: 1: bun: not found. `bun` was not found. PATH used by Atlas: /usr/bin:/bin. {not_found_fix}."
            ),
        ),
        // `PortConflict::describe` and `PortOwner::describe` say the same thing (see `port_held_says_what_the_core_says`).
        (
            "port.held",
            json!({"port": 3000, "pid": 4182, "cmd": "node", "stop": "kill 4182"}),
            "port 3000 held by PID 4182 (node): stop it yourself (kill 4182) or change the service port".into(),
        ),
        (
            "port.held_cwd",
            json!({"port": 3000, "pid": 4182, "cmd": "node", "cwd": "/work/app", "stop": "kill 4182"}),
            "port 3000 held by PID 4182 (node, cwd /work/app): stop it yourself (kill 4182) or change the service port".into(),
        ),
        s(
            "tunnel.failed",
            json!({"host": "box", "reason": "Permission denied (publickey)."}),
            "SSH tunnel to box failed: Permission denied (publickey).",
        ),
    ]
}

fn note(code: &'static str, args: &Value) -> Note {
    Note::new(code, args.clone())
}

#[test]
fn every_code_has_exactly_one_sample() {
    let sampled: Vec<&str> = samples().iter().map(|(c, ..)| *c).collect();
    let unique: BTreeSet<&str> = sampled.iter().copied().collect();
    assert_eq!(
        sampled.len(),
        unique.len(),
        "a code sampled twice: {sampled:?}"
    );
    let known: BTreeSet<&str> = CODES.iter().copied().collect();
    assert_eq!(known.len(), CODES.len(), "a code listed twice");
    assert_eq!(unique, known, "CODES and the samples of this test differ");
}

/// The English of the status file, MCP and `--ci` is the English the engine wrote before the codes: not one byte moved.
#[test]
fn english_is_byte_for_byte_what_the_engine_always_wrote() {
    for (code, args, english) in samples() {
        let n = note(code, &args);
        assert_eq!(n.message, english, "{code}");
        assert_eq!(n.to_string(), english, "{code}: Display is the message");
        assert_eq!(
            n.render_in("en"),
            english,
            "{code}: en renders the same text"
        );
    }
}

#[test]
fn portuguese_is_not_the_english_and_keeps_every_value() {
    for (code, args, english) in samples() {
        let n = note(code, &args);
        let pt = n.render_in("pt-BR");
        assert_ne!(pt, english, "{code}: not translated");
        // Every plain value of the sentence (not the sizes the renderer formats, not nested notes) reaches the text.
        for (k, v) in args.as_object().unwrap() {
            let shown = match v {
                Value::String(s)
                    if !matches!(k.as_str(), "first" | "second" | "exit" | "detail") =>
                {
                    s.clone()
                }
                Value::Number(x) if !matches!(k.as_str(), "used" | "max" | "warn" | "growth") => {
                    x.to_string()
                }
                _ => continue,
            };
            assert!(
                pt.contains(&shown),
                "{code}: `{k}` = {shown} is missing from {pt:?}"
            );
        }
        assert!(
            !pt.contains("undefined") && !pt.contains('{'),
            "{code}: {pt}"
        );
    }
}

#[test]
fn a_size_is_written_the_way_the_language_writes_it() {
    let n = note("memory.over_warn", &json!({"used": 1300 * MB, "warn": GB}));
    assert_eq!(n.render_in("en"), "using 1.3 GB, over memory_warn 1.0 GB");
    assert_eq!(
        n.render_in("pt-BR"),
        "usando 1,3 GB, acima de memory_warn 1,0 GB"
    );
    let small = note(
        "memory.leak_reloads",
        &json!({"growth": 600 * MB, "cycles": 12}),
    );
    assert_eq!(
        small.render_in("pt-BR"),
        "suspeita de vazamento: +600 MB em 12 recarregamentos"
    );
}

#[test]
fn restarts_agree_in_number_in_portuguese_and_keep_the_english_words() {
    let one = note("sched.gave_up", &json!({"attempts": 1}));
    // English has always read "1 restarts": the bytes stay, the Portuguese is right.
    assert_eq!(one.message, "gave up after 1 restarts");
    assert_eq!(one.render_in("pt-BR"), "desistiu após 1 reinício");
    let many = note("sched.gave_up", &json!({"attempts": 5}));
    assert_eq!(many.render_in("pt-BR"), "desistiu após 5 reinícios");
}

/// Nested notes render in the language of the outer one, not in the English they carry.
#[test]
fn a_nested_note_is_rendered_in_the_language_of_the_sentence() {
    let (_, args, _) = samples()
        .into_iter()
        .find(|(c, ..)| *c == "note.pair")
        .unwrap();
    let pair = note("note.pair", &args);
    assert_eq!(
        pair.render_in("pt-BR"),
        "aguardando :3000 (12s) · porta 3000 ocupada pelo PID 4182 (node): encerre-o você mesmo (kill 4182) ou mude a porta do serviço"
    );
    let failed = Note::new(
        "docker.compose_failed",
        json!({"exit": Note::new("docker.exit_signal", json!({})).wire(), "detail": Note::new("docker.no_output", json!({})).wire()}),
    );
    assert_eq!(
        failed.render_in("pt-BR"),
        "compose up falhou (encerrado por um sinal): sem saída"
    );
}

#[test]
fn a_code_this_build_does_not_know_or_a_missing_arg_shows_the_english() {
    let future = Note::from_wire(
        &json!({"code": "sched.from_the_future", "args": {}, "message": "English words"}),
    );
    assert_eq!(future.render_in("pt-BR"), "English words");
    // An older runner sent a message and no code at all.
    let plain = Note::from_wire(&json!({"t": "note", "name": "api", "message": "restarting"}));
    assert_eq!(
        (plain.code.as_str(), plain.message.as_str()),
        ("", "restarting")
    );
    assert_eq!(plain.render_in("pt-BR"), "restarting");
    // A runner whose args went missing or changed type must not print `undefined` or panic.
    for bad in [
        json!({}),
        json!({"secs": "soon", "attempt": 1, "limit": "5"}),
    ] {
        let n = Note::from_wire(
            &json!({"code": "sched.restarting_in", "args": bad, "message": "restarting in 3s (2/5)"}),
        );
        assert_eq!(n.render_in("pt-BR"), "restarting in 3s (2/5)");
    }
}

#[test]
fn wire_and_flat_round_trip_the_note() {
    for (code, args, _) in samples() {
        let n = note(code, &args);
        assert_eq!(Note::from_wire(&n.wire()), n, "{code}");
        let mut row = json!({"note": n.message});
        Note::put_flat(&mut row, "note", Some(&n));
        assert_eq!(Note::from_flat(&row, "note"), Some(n.clone()), "{code}");
    }
    // A row from before the codes: English only, rendered as it is.
    let old = Note::from_flat(&json!({"note": "restarting in 4s (2/5)"}), "note").unwrap();
    assert_eq!(old.render_in("pt-BR"), "restarting in 4s (2/5)");
    assert!(Note::from_flat(&json!({"note": null}), "note").is_none());
    assert!(Note::from_flat(&json!({}), "note").is_none());
    // No note: the keys are written as null, so a client clears what it showed.
    let mut row = json!({"note": null});
    Note::put_flat(&mut row, "note", None);
    assert!(row["note_code"].is_null() && row["note_args"].is_null());
}

#[test]
fn only_the_first_line_of_a_failure_is_the_card_note() {
    let failed = Note::new(
        "docker.compose_failed",
        json!({"exit": Note::new("docker.exit_code", json!({"code": 1})).wire(), "detail": "first\nsecond"}),
    );
    let two = Note::lines(Note::new("docker.daemon_down", json!({})), failed);
    // The bytes the engine wrote before the codes: the hint, then the compose failure with stderr's lines as they came.
    assert_eq!(
        two.message,
        "docker daemon is not running. Start it (Linux: sudo systemctl start docker; macOS: open OrbStack or Docker Desktop)\ncompose up failed (exit 1): first\nsecond"
    );
    assert_eq!(
        two.line(),
        "docker daemon is not running. Start it (Linux: sudo systemctl start docker; macOS: open OrbStack or Docker Desktop)"
    );
    let pt = two.render_in("pt-BR");
    assert_eq!(
        pt.lines().next().unwrap(),
        "o daemon do docker não está rodando. Inicie-o (Linux: sudo systemctl start docker; macOS: abra o OrbStack ou o Docker Desktop)"
    );
    assert_eq!(
        pt.lines().count(),
        3,
        "the card shows the first line; the rest stays for the details"
    );
}

/// The code a producer writes is one of `CODES`: typed as a literal in the source, so a typo is caught here and not
/// by a user (`Note::new` only checks in a debug run that reaches it), and no code is listed that nothing writes.
#[test]
fn the_codes_in_the_source_are_the_codes_of_the_catalog() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let known: BTreeSet<&str> = CODES.iter().copied().collect();
    let new_call = regex::Regex::new(r#"Note::new\(\s*"([a-z_]+\.[a-z_]+)""#).unwrap();
    let mut written: BTreeSet<String> = BTreeSet::new();
    let mut unknown: Vec<String> = Vec::new();
    for f in files {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        // The other tests may name a made-up code on purpose.
        if name.ends_with("_tests.rs") || name == "tests.rs" {
            continue;
        }
        let mut text = std::fs::read_to_string(&f).unwrap();
        // note.rs lists every code in CODES (that list proves nothing) and writes note.pair / note.lines itself.
        if name == "note.rs" {
            let start = text.find("pub const CODES").expect("CODES in note.rs");
            let end = start + text[start..].find("];").expect("end of CODES");
            text.replace_range(start..end, "");
        }
        for c in new_call.captures_iter(&text) {
            if !known.contains(&c[1]) {
                unknown.push(format!("{}: {}", f.display(), &c[1]));
            }
        }
        for code in CODES {
            if text.contains(&format!("\"{code}\"")) {
                written.insert((*code).to_string());
            }
        }
    }
    assert!(unknown.is_empty(), "codes not in CODES: {unknown:?}");
    let dead: Vec<&&str> = CODES.iter().filter(|c| !written.contains(**c)).collect();
    assert!(dead.is_empty(), "codes nothing writes: {dead:?}");
}

/// The English of `port.held*` is what the core's `PortOwner::describe` has always said, with and without a cwd.
#[test]
fn port_held_says_what_the_core_says() {
    use atlas_core::sys::PortOwner;
    for cwd in [None, Some("/work/app")] {
        let owner = PortOwner {
            pid: 12345,
            cmd: "node".into(),
            cwd: cwd.map(Into::into),
        };
        let conflict = super::port_conflict::PortConflict::from_owner(&owner);
        assert_eq!(conflict.note(4321).message, owner.describe(4321));
        assert_eq!(conflict.describe(4321), owner.describe(4321));
    }
    let nameless = PortOwner {
        pid: 7,
        cmd: String::new(),
        cwd: None,
    };
    let c = super::port_conflict::PortConflict::from_owner(&nameless);
    assert_eq!(c.describe(80), nameless.describe(80));
}
