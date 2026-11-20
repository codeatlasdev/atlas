//! Spoken replies and voice answers (#648) against a real hostd. The words' meaning is unit-tested in
//! `voice_answer`, the destructive-call list in `approval`; these check that every door applies them: a spoken
//! allow never reaches a prompt that needs a tap, an exact phrase decides once and is audited as voice, anything
//! else is only a comment, a choice card is answered by number or label, and a finished task says what to read
//! aloud. The last test writes the fixtures the apps read from these real replies.
//!
//! Regenerate the fixtures: `UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::voice_answers`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use super::answers::Engine;
use super::attachments::wait_for;
use super::fixture::{daemon, fake_agent, wait_task_state};
use super::permissions::{Rig as Perm, rig};
use super::voice::{Rig as Tg, fixture};
use crate::HostClient;

const HANG: Duration = atlas_core::scaling::HANG;

// ── approvals by voice ───────────────────────────────────────────────────────

async fn say(rig: &Perm, ev: &Value, transcript: &str, lang: Option<&str>) -> Value {
    let mut args = json!({"pane": ev["pane_key"], "req": ev["req"], "transcript": transcript});
    if let Some(l) = lang {
        args["lang"] = json!(l);
    }
    rig.client.call("agent_approve_voice", args).await.unwrap()
}

fn verdict(v: &Value) -> &str {
    v["verdict"].as_str().unwrap_or_else(|| panic!("{v}"))
}

/// Nothing decided: the call still waits, and no `approval_decision` was written.
async fn still_waiting(rig: &Perm, call: &tokio::task::JoinHandle<Value>, why: &str) {
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!call.is_finished(), "decided anyway: {why}");
    assert_eq!(rig.pending().await.len(), 1, "{why}");
    assert!(
        rig.audit("approval_decision").await.is_empty(),
        "an approval_decision was written: {why}"
    );
}

// "allow" approves a normal prompt, once: the same call asks again. Audited like a tap, plus how and what was heard.
#[tokio::test(flavor = "multi_thread")]
async fn a_spoken_allow_approves_a_normal_prompt_once_and_is_audited_as_voice() {
    let rig = rig("claude", "ask").await;
    let input = json!({"command": "cargo test"});
    let (call, ev) = rig.held("t1", "Bash", input.clone()).await;
    let said = say(&rig, &ev, "Allow.", None).await;
    assert_eq!(said, json!({"verdict": "allow"}));
    assert_eq!(call.await.unwrap(), json!({"behavior": "allow"}));

    let audit = rig.audit("approval_decision").await;
    assert_eq!(audit.len(), 1, "{audit:?}");
    let d = &audit[0];
    assert_eq!(d["decision"], "allow");
    assert_eq!(d["source"], "person");
    assert_eq!(d["device"], "desktop");
    assert_eq!(d["via"], "voice", "{d}");
    assert_eq!(d["transcript"], "Allow.");
    assert!(
        d.get("comment").is_none(),
        "a spoken allow carries no comment: {d}"
    );

    // Once: no grant was made for the task, the same command is asked again.
    let (again, ev2) = rig.held("t1", "Bash", input).await;
    rig.answer(&ev2, false, None, None).await;
    assert_eq!(again.await.unwrap()["behavior"], "deny");
}

// The exact-phrase rule at the door: every qualified or double phrase comes back as a comment, nothing is decided,
// and a deny by voice then works.
#[tokio::test(flavor = "multi_thread")]
async fn a_qualified_answer_decides_nothing_and_comes_back_as_a_comment() {
    let rig = rig("claude", "ask").await;
    let (call, ev) = rig.held("t1", "Bash", json!({"command": "ls"})).await;
    for said in [
        "allow but only once",
        "allow but only this file",
        "don't allow",
        "não permitir",
        "allow allow",
        "permitir nesta tarefa",
    ] {
        let v = say(&rig, &ev, said, None).await;
        assert_eq!(v, json!({"verdict": "comment", "comment": said}), "{said}");
        still_waiting(&rig, &call, said).await;
    }
    for said in ["", "   ", "..."] {
        assert_eq!(
            say(&rig, &ev, said, None).await,
            json!({"verdict": "unclear"})
        );
    }
    still_waiting(&rig, &call, "unclear").await;

    // Deny by voice is allowed, and audited as voice.
    assert_eq!(
        say(&rig, &ev, "no deny", Some("en")).await,
        json!({"verdict": "deny"})
    );
    let denied = call.await.unwrap();
    assert_eq!(denied["behavior"], "deny", "{denied}");
    let d = &rig.audit("approval_decision").await[0];
    assert_eq!(
        (d["decision"].as_str(), d["via"].as_str()),
        (Some("deny"), Some("voice")),
        "{d}"
    );
    assert_eq!(d["transcript"], "no deny");
}

// A phrase of the other language is not one when the language is known; unknown language reads both tables.
#[tokio::test(flavor = "multi_thread")]
async fn the_language_picks_the_phrase_table() {
    let rig = rig("claude", "ask").await;
    let (call, ev) = rig.held("t1", "Bash", json!({"command": "ls"})).await;
    assert_eq!(
        verdict(&say(&rig, &ev, "permitir", Some("en")).await),
        "comment"
    );
    assert_eq!(
        verdict(&say(&rig, &ev, "allow", Some("pt")).await),
        "comment"
    );
    still_waiting(&rig, &call, "wrong language").await;
    assert_eq!(
        verdict(&say(&rig, &ev, "Permitir.", Some("pt")).await),
        "allow"
    );
    assert_eq!(call.await.unwrap()["behavior"], "allow");
    let (call, ev) = rig.held("t1", "Bash", json!({"command": "pwd"})).await;
    assert_eq!(verdict(&say(&rig, &ev, "Negar.", None).await), "deny");
    assert_eq!(call.await.unwrap()["behavior"], "deny");
}

// A rail holds the prompt: voice never allows it, says why, audits the refusal; a deny is still fine, and the tap
// still works.
#[tokio::test(flavor = "multi_thread")]
async fn a_rail_prompt_cannot_be_approved_by_voice() {
    let rig = rig("claude", "ask").await;
    let env = json!({"file_path": rig.tree(".env")});
    let (call, ev) = rig.held("t1", "Edit", env.clone()).await;
    assert_eq!(ev["rail"], "protected_path", "{ev}");
    for said in ["allow", "Permitir", "yes allow"] {
        assert_eq!(
            say(&rig, &ev, said, None).await,
            json!({"verdict": "needs_tap", "reason": "rail"}),
            "{said}"
        );
    }
    still_waiting(&rig, &call, "rail").await;
    let refused = rig.audit("approval_voice_refused").await;
    assert_eq!(refused.len(), 3, "{refused:?}");
    assert_eq!(refused[0]["reason"], "rail");
    assert_eq!(refused[0]["transcript"], "yes allow");
    assert_eq!(refused[0]["device"], "desktop");

    // A comment on a rail prompt is still only a comment; deny works by voice; allow works by tap.
    assert_eq!(
        verdict(&say(&rig, &ev, "allow but only this file", None).await),
        "comment"
    );
    assert_eq!(
        say(&rig, &ev, "deny", None).await,
        json!({"verdict": "deny"})
    );
    assert_eq!(call.await.unwrap()["behavior"], "deny");
    let (call, ev) = rig.held("t1", "Edit", env).await;
    rig.answer(&ev, true, None, None).await;
    assert_eq!(call.await.unwrap(), json!({"behavior": "allow"}));
}

// A crewmate in Bypass: any prompt it still has (a rail's) needs the tap, and says it is Bypass.
#[tokio::test(flavor = "multi_thread")]
async fn a_bypass_crewmates_prompt_cannot_be_approved_by_voice() {
    let rig = rig("claude", "bypass").await;
    let (call, ev) = rig
        .held("t1", "Write", json!({"file_path": rig.tree(".git/config")}))
        .await;
    assert_eq!(
        say(&rig, &ev, "allow", None).await,
        json!({"verdict": "needs_tap", "reason": "bypass"})
    );
    still_waiting(&rig, &call, "bypass").await;
    let refused = rig.audit("approval_voice_refused").await;
    assert_eq!(refused[0]["reason"], "bypass", "{refused:?}");
    assert_eq!(
        say(&rig, &ev, "deny", None).await,
        json!({"verdict": "deny"})
    );
    assert_eq!(call.await.unwrap()["behavior"], "deny");
}

// A destructive command needs the tap, in either language; a harmless one next to it does not.
#[tokio::test(flavor = "multi_thread")]
async fn a_destructive_command_cannot_be_approved_by_voice() {
    let rig = rig("claude", "ask").await;
    for command in ["git push --force origin main", "ls && rm -rf build"] {
        let (call, ev) = rig.held("t1", "Bash", json!({"command": command})).await;
        for (said, lang) in [
            ("allow", Some("en")),
            ("permitir", Some("pt")),
            ("allow", None),
        ] {
            assert_eq!(
                say(&rig, &ev, said, lang).await,
                json!({"verdict": "needs_tap", "reason": "destructive"}),
                "{command}: {said}"
            );
        }
        assert!(!call.is_finished(), "{command}");
        assert_eq!(
            say(&rig, &ev, "deny", None).await,
            json!({"verdict": "deny"})
        );
        assert_eq!(call.await.unwrap()["behavior"], "deny");
    }
    let (call, ev) = rig
        .held("t1", "Bash", json!({"command": "git status"}))
        .await;
    assert_eq!(
        say(&rig, &ev, "allow", None).await,
        json!({"verdict": "allow"})
    );
    assert_eq!(call.await.unwrap()["behavior"], "allow");
}

// A request that is gone, or belongs to another pane, is the error a tap gets, whatever was said.
#[tokio::test(flavor = "multi_thread")]
async fn a_stale_or_foreign_request_is_refused_like_a_tap() {
    let rig = rig("claude", "ask").await;
    let (call, ev) = rig.held("t1", "Bash", json!({"command": "ls"})).await;
    let wrong_req = rig
        .client
        .call(
            "agent_approve_voice",
            json!({"pane": ev["pane_key"], "req": "nope", "transcript": "allow"}),
        )
        .await;
    assert!(wrong_req.is_err(), "{wrong_req:?}");
    let wrong_pane = rig
        .client
        .call(
            "agent_approve_voice",
            json!({"pane": rig.pane("other"), "req": ev["req"], "transcript": "allow"}),
        )
        .await;
    assert!(wrong_pane.is_err(), "{wrong_pane:?}");
    still_waiting(&rig, &call, "stale").await;
    rig.answer(&ev, false, None, None).await;
    call.await.unwrap();
}

// A phone answers by voice with the same grant as by tap, and an edit-only grant never reaches the choice card.
#[test]
fn the_voice_commands_need_the_scopes_their_taps_need() {
    use crate::mobile::{Scope, required_scope};
    let parse = |cmd: &str, args: Value| {
        serde_json::from_value::<crate::protocol::Request>(
            json!({"id": 1, "cmd": cmd, "args": args}),
        )
        .unwrap()
        .cmd
    };
    let approve = parse(
        "agent_approve_voice",
        json!({"pane": "p", "req": "r", "transcript": "allow"}),
    );
    let answer = parse(
        "crewmate_action_answer_voice",
        json!({"id": "c", "task": "t", "transcript": "one", "lang": "en"}),
    );
    assert_eq!(required_scope(&approve), Ok(Scope::Approve));
    assert_eq!(required_scope(&answer), Ok(Scope::Act));
}

// ── choice cards ─────────────────────────────────────────────────────────────

const CHOOSE: &str = r#"Which base?\nATLAS_ACTION: {\"kind\":\"choose\",\"question\":\"Which base?\",\"options\":[\"Use main\",\"Create a branch\"]}"#;
const SECRET: &str = r#"Need it.\nATLAS_ACTION: {\"kind\":\"provide_secret\",\"name\":\"API_KEY\",\"label\":\"API key\",\"reason\":\"to call it\"}"#;

/// An omp whose turn `n` answers `replies[n]` (the last one for any later turn), recording each prompt it got in
/// `<dir>/run/prompt.<n>`. A reply is the JSON-escaped content of its message, without single quotes.
fn scripted_omp(dir: &Path, replies: &[&str]) -> PathBuf {
    let run = dir.join("run");
    std::fs::create_dir_all(&run).unwrap();
    let mut cases = String::new();
    for (i, r) in replies.iter().enumerate() {
        let n = if i + 1 == replies.len() {
            "*".to_string()
        } else {
            i.to_string()
        };
        cases.push_str(&format!("{n}) content='{r}';;\n"));
    }
    let script = r#"d="@DIR@"
read -r prompt
n=$(ls "$d" | grep -c '^prompt\.')
printf '%s\n' "$prompt" > "$d/prompt.$n"
case "$n" in
@CASES@esac
printf '%s\n' '{"type":"agent_end","messages":[{"role":"assistant","content":"'"$content"'"}]}'
printf '%s\n' '{"type":"prompt_result","id":"r1"}'
read -r _state
printf '%s\n' '{"type":"response","id":"g","data":{"sessionFile":"/s.jsonl"}}'"#
        .replace("@DIR@", &run.to_string_lossy())
        .replace("@CASES@", &cases);
    fake_agent(dir, "omp", &script)
}

struct Flow {
    tmp: tempfile::TempDir,
    client: HostClient,
    id: String,
    _engine: Engine,
}

impl Flow {
    async fn start(replies: &[&str]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let bin = scripted_omp(tmp.path(), replies);
        let engine = Engine::omp(&bin).await;
        let client = daemon(&tmp.path().join("hostd")).await;
        let v = client
            .call(
                "crewmate_create",
                json!({"name": "Gerente", "role": "r", "agent": "omp", "approval": "auto",
                       "engine": {"kind": "omp"}}),
            )
            .await
            .unwrap();
        Self {
            id: v["id"].as_str().unwrap().to_string(),
            tmp,
            client,
            _engine: engine,
        }
    }

    async fn send(&self) -> String {
        let v = self
            .client
            .call(
                "crewmate_task",
                json!({"id": self.id, "prompt": "pick", "source": "test"}),
            )
            .await
            .unwrap();
        v["task"]["id"].as_str().unwrap().to_string()
    }

    async fn task(&self, task: &str) -> Value {
        let v = self
            .client
            .call("crewmate_tasks", json!({"id": self.id, "limit": 20}))
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

    async fn say(&self, task: &str, transcript: &str) -> Value {
        self.client
            .call(
                "crewmate_action_answer_voice",
                json!({"id": self.id, "task": task, "transcript": transcript}),
            )
            .await
            .unwrap()
    }

    async fn parked(&self) -> String {
        let task = self.send().await;
        let t = wait_task_state(&self.client, &self.id, &task, "waiting_action", HANG).await;
        assert_eq!(t["action"]["kind"], "choose", "{t}");
        task
    }

    fn prompt(&self, n: usize) -> String {
        std::fs::read_to_string(self.tmp.path().join(format!("run/prompt.{n}"))).unwrap()
    }

    async fn audit(&self, kind: &str) -> Vec<Value> {
        let v = self
            .client
            .call(
                "crewmate_audit",
                json!({"id": self.id, "kind": kind, "limit": 50}),
            )
            .await
            .unwrap();
        v["entries"].as_array().unwrap().clone()
    }
}

// A choice card is answered by the option's number or label, in both languages: the agent gets the option as the
// card has it, the chat shows it as the owner's pick, and the audit says it was spoken.
#[tokio::test(flavor = "multi_thread")]
async fn a_choice_card_is_answered_by_number_and_by_label() {
    for (said, option) in [
        ("The second.", "Create a branch"),
        ("USE MAIN", "Use main"),
        ("um", "Use main"),
        ("crea", "Create a branch"),
    ] {
        let flow = Flow::start(&[CHOOSE, "went with it"]).await;
        let task = flow.parked().await;
        assert_eq!(
            flow.say(&task, said).await,
            json!({"verdict": "choice", "choice": option}),
            "{said}"
        );
        let done = wait_task_state(&flow.client, &flow.id, &task, "done", HANG).await;
        assert_eq!(done["answer"], option, "{done}");
        assert!(
            flow.prompt(1).contains(&format!("\"{option}\"")),
            "the agent got the option as the card has it: {}",
            flow.prompt(1)
        );
        let entry = &flow.audit("card_answered").await[0];
        assert_eq!(entry["via"], "voice", "{entry}");
        assert_eq!(entry["choice"], option);
        assert_eq!(entry["transcript"], said);
        assert_eq!(entry["task_id"], task);
        // The card took its one answer.
        let again = flow
            .client
            .call(
                "crewmate_action_answer_voice",
                json!({"id": flow.id, "task": task, "transcript": "one"}),
            )
            .await;
        assert!(
            again.is_err(),
            "a second spoken answer was accepted: {again:?}"
        );
        flow.client.call("shutdown", json!({})).await.ok();
    }
}

// Words that name no option, or two, decide nothing and leave the card waiting; a card that is not a choice is
// answered on screen.
#[tokio::test(flavor = "multi_thread")]
async fn unclear_words_leave_the_card_waiting_and_other_cards_need_a_tap() {
    let flow = Flow::start(&[CHOOSE, "went with it"]).await;
    let task = flow.parked().await;
    for said in ["banana", "one or two", "three", "", "allow"] {
        assert_eq!(
            flow.say(&task, said).await,
            json!({"verdict": "unclear"}),
            "{said:?}"
        );
    }
    let t = flow.task(&task).await;
    assert_eq!(t["state"], "waiting_action", "{t}");
    assert!(flow.audit("card_answered").await.is_empty());
    assert_eq!(flow.say(&task, "two").await["verdict"], "choice");
    flow.client.call("shutdown", json!({})).await.ok();
    // The engine override is one at a time: the next flow starts after this one ends.
    drop(flow);

    let flow = Flow::start(&[SECRET, "ok"]).await;
    let task = flow.send().await;
    wait_task_state(&flow.client, &flow.id, &task, "waiting_action", HANG).await;
    assert_eq!(
        flow.say(&task, "my key is hunter2").await,
        json!({"verdict": "needs_tap", "reason": null})
    );
    assert_eq!(
        flow.say(&task, "one").await,
        json!({"verdict": "needs_tap", "reason": null})
    );
    let t = flow.task(&task).await;
    assert_eq!(t["state"], "waiting_action", "{t}");
    assert!(
        flow.audit("secret_set").await.is_empty(),
        "a secret was stored from a transcript"
    );
    flow.client.call("shutdown", json!({})).await.ok();
}

// ── spoken replies ───────────────────────────────────────────────────────────

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

// The flag is on the crewmate and in its list row; while on, the agent gets one line asking for `spoken:`; a reply
// with one keeps only the rest visible and exposes the summary.
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_with_a_spoken_line_shows_the_rest_and_exposes_the_summary() {
    let flow =
        Flow::start(&[r#"Fixed it in src/a.rs.\n\nSpoken: I fixed it. Everything is in place."#])
            .await;
    let off = flow.client.call("crewmate_list", json!({})).await.unwrap();
    assert_eq!(off["crewmates"][0]["spoken_replies"], false, "{off}");
    let updated = flow
        .client
        .call(
            "crewmate_update",
            json!({"id": flow.id, "spoken_replies": true}),
        )
        .await
        .unwrap();
    assert_eq!(updated["spoken_replies"], true, "{updated}");
    let list = flow.client.call("crewmate_list", json!({})).await.unwrap();
    assert_eq!(list["crewmates"][0]["spoken_replies"], true, "{list}");

    let task = flow.send().await;
    let done = wait_task_state(&flow.client, &flow.id, &task, "done", HANG).await;
    assert_eq!(done["result"], "Fixed it in src/a.rs.", "{done}");
    assert_eq!(done["reply"], "Fixed it in src/a.rs.", "{done}");
    assert_eq!(
        done["spoken"], "I fixed it. Everything is in place.",
        "{done}"
    );
    let prompt = flow.prompt(0);
    assert_eq!(
        count(&prompt, "`spoken: <"),
        1,
        "one instruction line: {prompt}"
    );
    // The audit of a config change names the flag.
    let changes = flow.audit("config_changed").await;
    assert!(
        changes
            .iter()
            .any(|c| c["field"] == "spoken_replies" && c["to"] == true),
        "{changes:?}"
    );
    flow.client.call("shutdown", json!({})).await.ok();
}

// No `spoken:` line (and the flag off: no instruction): the first sentences, without code, URLs or paths.
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_without_one_exposes_its_first_sentences() {
    let flow = Flow::start(&[
        r#"I renamed the helper and fixed the typo. The change is in `src/a.rs` and https://example.com/x.\nMore detail follows here."#,
        r#"```\ncargo test\n```"#,
    ])
    .await;
    let task = flow.send().await;
    let done = wait_task_state(&flow.client, &flow.id, &task, "done", HANG).await;
    assert_eq!(
        done["spoken"],
        "I renamed the helper and fixed the typo. The change is in and More detail follows here.",
        "{done}"
    );
    assert!(
        done["result"].as_str().unwrap().contains("`src/a.rs`"),
        "the reply itself is whole: {done}"
    );
    assert_eq!(
        count(&flow.prompt(0), "spoken:"),
        0,
        "no instruction while the flag is off"
    );

    // A reply that is only code has nothing to read.
    let second = flow.send().await;
    let done = wait_task_state(&flow.client, &flow.id, &second, "done", HANG).await;
    assert_eq!(done["spoken"], "", "{done}");
    // Every task of the list carries `spoken` as a plain string.
    let tasks = flow
        .client
        .call("crewmate_tasks", json!({"id": flow.id, "limit": 20}))
        .await
        .unwrap();
    for t in tasks["tasks"].as_array().unwrap() {
        assert!(t["spoken"].is_string(), "{t}");
    }
    flow.client.call("shutdown", json!({})).await.ok();
}

// ── Telegram ─────────────────────────────────────────────────────────────────

async fn open_prompt(
    rig: &Tg,
    tool: &str,
    input: Value,
) -> (tokio::task::JoinHandle<Value>, Value) {
    let (dir, id) = (rig.dir.clone(), rig.crewmate.clone());
    let pane = format!("crew:{id}:t1");
    let args = json!({"tool_name": tool, "tool_input": input});
    let handle = tokio::spawn(async move {
        let r = crate::crew::mcp::call_for_test(&dir, &id, &pane, "approve", args)
            .await
            .unwrap();
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    });
    let start = std::time::Instant::now();
    let ev = loop {
        let st = rig
            .client
            .call("agent_status_all", json!({}))
            .await
            .unwrap();
        if let Some(ev) = st["pending_approvals"].as_array().and_then(|a| a.first()) {
            break ev.clone();
        }
        assert!(start.elapsed() < HANG, "nothing asked for an approval");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    (handle, ev)
}

async fn chat_said(rig: &Tg, needle: &str) -> String {
    wait_for(&format!("a message with {needle:?}"), || {
        rig.calls
            .lock()
            .iter()
            .filter(|(k, _)| k == "sendMessage")
            .filter_map(|(_, v)| v["text"].as_str().map(str::to_string))
            .find(|t| t.contains(needle))
    })
    .await
}

// The owner's voice note while a prompt waits goes through the same rules: it answers the prompt (and never
// becomes a task), a qualified one is only a comment, and Bypass needs the tap.
#[tokio::test(flavor = "multi_thread")]
async fn a_voice_note_from_the_owner_answers_the_prompt_that_waits() {
    let mut rig = Tg::new("Allow.", false, true).await;
    rig.client
        .call(
            "crewmate_set_approval",
            json!({"id": rig.crewmate, "mode": "ask"}),
        )
        .await
        .unwrap();
    let (call, _ev) = open_prompt(&rig, "Bash", json!({"command": "cargo test"})).await;
    rig.note(42, 40_000, None);
    chat_said(&rig, "Allowed once, by voice").await;
    assert_eq!(call.await.unwrap(), json!({"behavior": "allow"}));
    assert!(
        rig.tasks().is_empty(),
        "a note that answered a prompt is not a task"
    );
    let audit = rig
        .client
        .call(
            "crewmate_audit",
            json!({"id": rig.crewmate, "kind": "approval_decision", "limit": 10}),
        )
        .await
        .unwrap();
    let d = &audit["entries"][0];
    assert_eq!(
        (d["via"].as_str(), d["device"].as_str()),
        (Some("voice"), Some("telegram:42")),
        "{d}"
    );
    rig.done().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_voice_note_that_is_not_an_exact_phrase_or_meets_bypass_decides_nothing() {
    let mut rig = Tg::new("allow but only this file", false, true).await;
    rig.client
        .call(
            "crewmate_set_approval",
            json!({"id": rig.crewmate, "mode": "ask"}),
        )
        .await
        .unwrap();
    let (call, _ev) = open_prompt(&rig, "Bash", json!({"command": "ls"})).await;
    rig.note(42, 40_000, None);
    chat_said(&rig, "Nothing was decided").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!call.is_finished(), "a qualified note decided the prompt");
    call.abort();
    rig.done().await;

    // Bypass: even the exact phrase waits for the tap.
    let mut rig = Tg::new("allow", false, true).await;
    let (call, _ev) = open_prompt(&rig, "Edit", json!({"file_path": "/work/.env"})).await;
    rig.note(42, 40_000, None);
    chat_said(&rig, "crewmate is in Bypass").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!call.is_finished(), "Bypass was approved by voice");
    call.abort();
    rig.done().await;
}

// ── the apps' contract ───────────────────────────────────────────────────────

/// `v` with the ids of this run replaced by stand-ins and every clock by one instant, so a fixture is the same on
/// every run: the SHAPE is hostd's, the identities are not.
fn stable(v: &mut Value, ids: &[(&str, &str)]) {
    match v {
        Value::String(s) => {
            for (real, stand_in) in ids {
                if s.contains(real) {
                    *s = s.replace(real, stand_in);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| stable(x, ids)),
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                if matches!(
                    k.as_str(),
                    "ts" | "started_at" | "finished_at" | "created_at"
                ) && x.is_number()
                {
                    *x = json!(1_700_000_000);
                } else {
                    stable(x, ids);
                }
            }
        }
        _ => {}
    }
}

// The replies the apps parse are the REAL ones of a hostd, one per verdict, written to the fixtures the desktop and
// the phone read (`UPDATE_FIXTURES=1`).
#[tokio::test(flavor = "multi_thread")]
async fn the_voice_answer_fixtures_are_real_replies() {
    const ID: &str = "0123456789abcdef";
    const TASK: &str = "fedcba9876543210";

    // Approvals: each verdict, on a real prompt.
    let approve = {
        let rig = rig("claude", "ask").await;
        let (call, ev) = rig
            .held("t1", "Bash", json!({"command": "cargo test"}))
            .await;
        let comment = say(&rig, &ev, "allow but only this file", Some("en")).await;
        let unclear = say(&rig, &ev, "", None).await;
        let allow = say(&rig, &ev, "allow", Some("en")).await;
        call.await.unwrap();
        let (call, ev) = rig.held("t1", "Bash", json!({"command": "ls"})).await;
        let deny = say(&rig, &ev, "negar", Some("pt")).await;
        call.await.unwrap();
        let (call, ev) = rig
            .held("t1", "Edit", json!({"file_path": rig.tree(".env")}))
            .await;
        let rail = say(&rig, &ev, "allow", None).await;
        rig.answer(&ev, false, None, None).await;
        call.await.unwrap();
        let (call, ev) = rig
            .held("t1", "Bash", json!({"command": "git push --force"}))
            .await;
        let destructive = say(&rig, &ev, "allow", None).await;
        rig.answer(&ev, false, None, None).await;
        call.await.unwrap();
        rig.set_mode("bypass").await.unwrap();
        let (call, ev) = rig
            .held("t1", "Edit", json!({"file_path": rig.tree(".env")}))
            .await;
        let bypass = say(&rig, &ev, "allow", None).await;
        rig.answer(&ev, false, None, None).await;
        call.await.unwrap();
        rig.client.call("shutdown", json!({})).await.ok();
        json!({
            "allow": allow,
            "deny": deny,
            "comment": comment,
            "unclear": unclear,
            "needs_tap_bypass": bypass,
            "needs_tap_rail": rail,
            "needs_tap_destructive": destructive,
            // A transcript of a chat that is not the owner's cannot come through a client (hostd names the
            // connection), so this is the verdict's own value.
            "needs_tap_untrusted": crate::crew::voice_card::needs_tap(Some("untrusted")),
        })
    };

    // A choice card, then a card that is not a choice, and a task that finished with an agent's `spoken:` line.
    let (answer, crewmate, task_agent_line) = {
        let flow = Flow::start(&[
            CHOOSE,
            r#"Went with it in src/a.rs.\n\nspoken: I used a new branch. Everything is in place."#,
        ])
        .await;
        flow.client
            .call(
                "crewmate_update",
                json!({"id": flow.id, "spoken_replies": true}),
            )
            .await
            .unwrap();
        let task = flow.parked().await;
        let unclear = flow.say(&task, "banana").await;
        let choice = flow.say(&task, "the second").await;
        let mut done = wait_task_state(&flow.client, &flow.id, &task, "done", HANG).await;
        let mut list = flow.client.call("crewmate_list", json!({})).await.unwrap();
        let mut row = list["crewmates"][0].take();
        let ids = [(flow.id.as_str(), ID), (task.as_str(), TASK)];
        stable(&mut row, &ids);
        stable(&mut done, &ids);
        flow.client.call("shutdown", json!({})).await.ok();
        (json!({"choice": choice, "unclear": unclear}), row, done)
    };
    let needs_tap_card = {
        let flow = Flow::start(&[SECRET, "ok"]).await;
        let task = flow.send().await;
        wait_task_state(&flow.client, &flow.id, &task, "waiting_action", HANG).await;
        let v = flow.say(&task, "one").await;
        flow.client.call("shutdown", json!({})).await.ok();
        v
    };
    let task_first_sentences = {
        let flow =
            Flow::start(&["I renamed the helper and fixed the typo. More detail follows."]).await;
        let task = flow.send().await;
        let mut done = wait_task_state(&flow.client, &flow.id, &task, "done", HANG).await;
        stable(&mut done, &[(flow.id.as_str(), ID), (task.as_str(), TASK)]);
        flow.client.call("shutdown", json!({})).await.ok();
        done
    };
    let mut answer = answer;
    answer["needs_tap_card"] = needs_tap_card;

    let value = json!({
        "approve": approve,
        "answer": answer,
        "crewmate": crewmate,
        "task_agent_line": task_agent_line,
        "task_first_sentences": task_first_sentences,
    });
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture(
        &root.join("apps/desktop/src/lib/__fixtures__/crew-voice-answers.json"),
        &value,
        true,
    );
    fixture(
        &root.join("apps/mobile/test/fixtures/crew_voice_answers.json"),
        &value,
        false,
    );
}
