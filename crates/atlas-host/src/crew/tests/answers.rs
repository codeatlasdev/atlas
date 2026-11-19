//! A card takes ONE answer: the first one is accepted and moves the task on, and any other is refused out loud.
//! Incident: #524 (a `choose` card stayed live after its first answer, so a second click returned `{}` and was
//! dropped, while the stored answer stayed the first one).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, log_task, use_agent_bin,
    wait_task_state,
};
use crate::HostClient;

const CHOOSE: &str = r#"Which?\nATLAS_ACTION: {\"kind\":\"choose\",\"question\":\"Which?\",\"options\":[\"A\",\"B\"]}"#;

/// An omp whose first turn asks a `choose` question and whose second turn (the answer) works until
/// `<dir>/release` exists, writing each prompt it was given to `<dir>/prompt.<n>`.
fn choosing_omp(dir: &Path) -> PathBuf {
    let run = dir.join("run");
    std::fs::create_dir_all(&run).unwrap();
    fake_agent(
        dir,
        "omp",
        &format!(
            r#"d="{d}"
read -r prompt
n=$(ls "$d" | grep -c '^prompt\.')
printf '%s\n' "$prompt" > "$d/prompt.$n"
if [ "$n" = 0 ]; then
  printf '%s\n' '{{"type":"agent_end","messages":[{{"role":"assistant","content":"{CHOOSE}"}}]}}'
else
  while [ ! -f "$d/release" ]; do sleep 0.05; done
  printf '%s\n' '{{"type":"agent_end","messages":[{{"role":"assistant","content":"went with it"}}]}}'
fi
printf '%s\n' '{{"type":"prompt_result","id":"r1"}}'
read -r _state
printf '%s\n' '{{"type":"response","id":"g","data":{{"sessionFile":"/s.jsonl"}}}}'"#,
            d = run.display()
        ),
    )
}

async fn answer(client: &HostClient, id: &str, task: &str, value: &str) -> Result<Value, String> {
    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": value}),
        )
        .await
        .map_err(|e| e.to_string())
}

async fn task_json(client: &HostClient, id: &str, task: &str) -> Value {
    let v = client
        .call("crewmate_tasks", json!({"id": id, "limit": 20}))
        .await
        .unwrap();
    v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task)
        .unwrap()
        .clone()
}

async fn send(client: &HostClient, id: &str) -> String {
    let v = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "pick", "source": "test"}),
        )
        .await
        .unwrap();
    v["task"]["id"].as_str().unwrap().to_string()
}

struct Engine {
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl Engine {
    async fn omp(bin: &Path) -> Self {
        let lock = lock_agent_bin().await;
        *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() =
            Some(("omp".into(), bin.to_string_lossy().into_owned()));
        Self { _lock: lock }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
    }
}

// #524: after the owner picked A the task kept saying `waiting_action` with the live card for the whole follow-up
// turn (the runner set `answer` on its own copy and stored nothing), so a second click (B) reached `hostd` with
// nobody waiting, got `{}` back and was dropped. The first answer moves the task to `running` for every client,
// and the second one is refused instead of acknowledged.
#[tokio::test(flavor = "multi_thread")]
async fn an_answered_choose_card_leaves_waiting_and_a_second_answer_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = choosing_omp(tmp.path());
    let _engine = Engine::omp(&bin).await;
    let client = daemon(&tmp.path().join("hostd")).await;
    let v = client
        .call(
            "crewmate_create",
            json!({"name": "Gerente", "role": "r", "agent": "omp", "approval": "auto",
                   "engine": {"kind": "omp"}}),
        )
        .await
        .unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    let task = send(&client, &id).await;
    let parked = wait_task_state(
        &client,
        &id,
        &task,
        "waiting_action",
        atlas_core::scaling::HANG,
    )
    .await;
    assert_eq!(parked["action"]["kind"], "choose", "{parked}");

    answer(&client, &id, &task, "A").await.unwrap();
    // The agent is working on A (it is held until `release`): the stored task says so.
    let working = wait_task_state(&client, &id, &task, "running", Duration::from_secs(10)).await;
    assert_eq!(working["answer"], "A", "{working}");

    // B arrives while the agent works on A: refused, not acknowledged.
    let second = answer(&client, &id, &task, "B").await;
    assert!(
        second.is_err(),
        "the second answer was accepted: {second:?}"
    );

    std::fs::write(tmp.path().join("run/release"), "").unwrap();
    let done = wait_task_state(&client, &id, &task, "done", atlas_core::scaling::HANG).await;
    assert_eq!(done["answer"], "A", "{done}");
    let seen = std::fs::read_to_string(tmp.path().join("run/prompt.1")).unwrap();
    assert!(seen.contains("\"A\""), "the agent got A: {seen}");
    assert!(!seen.contains("\"B\""), "and never B: {seen}");
    client.call("shutdown", json!({})).await.ok();
}

// The same card with no waiting runner (it ended, or the server has not picked the wait up): an answer that nobody
// will read is an error, and has none of its side effects. Before, `crewmate_action_answer` returned `{}` for it.
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_nobody_is_waiting_for_is_an_error_with_no_side_effects() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Stale").await;
    // A parked task on disk with no runner behind it: what a card is when its runner is gone.
    let store = crate::crew::store::CrewStore::new(&dir);
    let mut t = crate::crew::store::Task::new("test", "pick");
    t.state = crate::crew::store::TaskState::WaitingAction;
    t.action = Some(crate::crew::store::TaskAction::ProvideSecret {
        name: "STALE_KEY".into(),
        label: "Key".into(),
        reason: "r".into(),
    });
    store.append_task(&id, &t).unwrap();

    let r = answer(&client, &id, &t.id, "s3cret").await;
    assert!(r.is_err(), "acknowledged an answer nobody reads: {r:?}");
    assert!(
        !store.secret_env(&id).contains_key("STALE_KEY"),
        "the secret was stored for an answer that was refused"
    );
    client.call("shutdown", json!({})).await.ok();
}

// The terminal runner (no `engine`) had the window too, between the answer and the next run starting.
#[tokio::test(flavor = "multi_thread")]
async fn a_choose_card_of_a_terminal_run_takes_one_answer_too() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new()
            .done(&CHOOSE.replace("\\n", "\n").replace("\\\"", "\""))
            .script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Pty").await;
    let task = send(&client, &id).await;
    wait_task_state(
        &client,
        &id,
        &task,
        "waiting_action",
        atlas_core::scaling::HANG,
    )
    .await;

    // The next run is slow, so the answer's follow-up is observable.
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(3).done("went with it").script(),
    ));
    answer(&client, &id, &task, "A").await.unwrap();
    let second = answer(&client, &id, &task, "B").await;
    assert!(
        second.is_err(),
        "the second answer was accepted: {second:?}"
    );
    let done = wait_task_state(&client, &id, &task, "done", atlas_core::scaling::HANG).await;
    assert_eq!(done["answer"], "A", "{done}");
    assert_eq!(done["result"], "went with it", "{done}");
    crate::crew::runner::set_test_agent_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

/// POST `body` to the hook server of the hostd in `dir` and return the reply body.
async fn post(dir: &Path, path: &str, body: Value) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let env = std::fs::read_to_string(crate::paths::hook_endpoint(dir)).unwrap();
    let get = |k: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap()
            .to_string()
    };
    let body = body.to_string();
    let mut s = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", get("ATLAS_HOOK_PORT")))
        .await
        .unwrap();
    let req = format!(
        "POST {path} HTTP/1.1\r\nX-Atlas-Hook-Token: {}\r\nContent-Length: {}\r\n\r\n{body}",
        get("ATLAS_HOOK_TOKEN"),
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).await.unwrap();
    r.split("\r\n\r\n").nth(1).unwrap_or_default().to_string()
}

// The agent's own `ask_user` card is the same shape: the owner's answer is recorded on the task, and a second one
// used to overwrite it (the agent had already gone on with the first) and answer `{}`.
#[tokio::test(flavor = "multi_thread")]
async fn an_ask_user_question_takes_one_answer() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Asker").await;
    let task = log_task(&dir, &id, "task-ask");
    let (d, crewmate) = (dir.clone(), id.clone());
    let asked = tokio::spawn(async move {
        post(
            &d,
            &format!("/crew/{crewmate}/ask"),
            json!({"ask_id": "q1", "question": "Which?", "pane_key": format!("crew:{crewmate}:task-ask"),
                   "options": [{"key": "a", "title": "A"}, {"key": "b", "title": "B"}]}),
        )
        .await
    });
    // Wait until the question is on the task (the agent is blocked on it).
    let start = std::time::Instant::now();
    while task_json(&client, &id, &task).await["ask"]["id"] != "q1" {
        assert!(start.elapsed() < atlas_core::scaling::HANG, "never asked");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let ans = |key: &'static str| {
        client.call(
            "crewmate_answer",
            json!({"id": id, "ask_id": "q1", "key": key}),
        )
    };
    ans("a").await.unwrap();
    let reply: Value = serde_json::from_str(&asked.await.unwrap()).unwrap();
    assert_eq!(reply["answer"], "a", "{reply}");

    let second = ans("b").await;
    assert!(second.is_err(), "a second answer was accepted: {second:?}");
    assert_eq!(
        task_json(&client, &id, &task).await["ask"]["answered"],
        json!({"kind": "key", "value": "a"}),
        "the stored answer is the one the agent got"
    );
    client.call("shutdown", json!({})).await.ok();
}
