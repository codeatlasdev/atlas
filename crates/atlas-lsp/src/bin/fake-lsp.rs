//! A tiny scriptable language server for the manager's behavior tests (not shipped).
//!
//! Behaviors, all driven by the protocol itself:
//! - `initialize` → capabilities with incremental sync (`FAKE_SYNC=full|none` changes it).
//! - `textDocument/didOpen|didChange` → `publishDiagnostics` carrying the doc version and
//!   the number of `didChange` events seen, so tests can observe sync.
//! - requests: `test/echo` → params, `test/slow` → never answers until `$/cancelRequest`
//!   (then records it), `test/state` → what the server observed so far, `test/pid`,
//!   `test/applyEdit` → asks the client to apply an edit and reports the answer,
//!   `test/crash` → exits(1) immediately, `test/init` → the `initializationOptions` the client
//!   sent in `initialize`.
//! - `FAKE_SEMANTIC=1`: a word-level "language" so the whole editor round trip can be tested
//!   without a real server: `fn <name>` declares a symbol; diagnostics flag every `TODO`;
//!   `completion` offers the declared names matching the prefix; `definition` jumps to the
//!   `fn` declaration; `references`/`rename` cover every whole-word occurrence; `hover`
//!   answers `fn <name>`; `documentSymbol` lists the `fn`s. Text is kept in sync from
//!   `didChange` (incremental or full).
//! - `FAKE_XFILE_DIR=<dir>` (with `FAKE_SEMANTIC`): `definition` of a name not declared in the open
//!   document is looked up in the files of `<dir>` and answered as a `LocationLink` (what
//!   typescript-language-server sends), with a percent-encoded `file://` URI. `<dir>` may be outside
//!   the project root, like the Rust std sources or a global `node_modules`.
//! - `FAKE_INIT_DELAY_MS=<ms>`: wait before answering `initialize`, so "the server is still starting"
//!   is observable.
//! - Every spawn increments a counter file when `FAKE_COUNT_FILE` is set, so tests can
//!   count restarts.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};

use atlas_lsp::text;
use serde_json::{Value, json};

mod semantic {
    //! Word-level "language" used when `FAKE_SEMANTIC` is set.
    use super::{Value, json};

    fn is_word(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// `(line, start_char, end_char, word)` for every identifier-like word, UTF-16 columns.
    pub fn words(text: &str) -> Vec<(u64, u64, u64, String)> {
        let mut out = vec![];
        for (ln, line) in text.lines().enumerate() {
            let mut col = 0u64;
            let mut cur = String::new();
            let mut start = 0u64;
            for c in line.chars() {
                if is_word(c) {
                    if cur.is_empty() {
                        start = col;
                    }
                    cur.push(c);
                } else if !cur.is_empty() {
                    out.push((ln as u64, start, col, std::mem::take(&mut cur)));
                }
                col += c.len_utf16() as u64;
            }
            if !cur.is_empty() {
                out.push((ln as u64, start, col, cur));
            }
        }
        out
    }

    fn range(l: u64, a: u64, b: u64) -> Value {
        json!({"start": {"line": l, "character": a}, "end": {"line": l, "character": b}})
    }

    /// The word under `(line, character)`.
    pub fn word_at(text: &str, line: u64, character: u64) -> Option<(u64, u64, u64, String)> {
        words(text)
            .into_iter()
            .find(|(l, a, b, _)| *l == line && *a <= character && character <= *b)
    }

    /// Declared names: the word after each `fn`.
    pub fn decls(text: &str) -> Vec<(u64, u64, u64, String)> {
        let w = words(text);
        w.windows(2)
            .filter(|p| p[0].3 == "fn" && p[0].0 == p[1].0)
            .map(|p| p[1].clone())
            .collect()
    }

    pub fn diagnostics(text: &str) -> Value {
        Value::Array(
            words(text)
                .into_iter()
                .filter(|w| w.3 == "TODO")
                .map(|(l, a, b, _)| {
                    json!({"range": range(l, a, b), "message": "TODO left in code", "severity": 2, "source": "fake", "code": "todo"})
                })
                .collect(),
        )
    }

    pub fn completion(text: &str, line: u64, character: u64) -> Value {
        let prefix = word_at(text, line, character)
            .map(|(_, a, _, w)| w.chars().take((character - a) as usize).collect::<String>())
            .unwrap_or_default();
        let items: Vec<Value> = decls(text)
            .into_iter()
            .map(|d| d.3)
            .filter(|n| n.starts_with(&prefix))
            .map(|n| json!({"label": n, "kind": 3, "detail": "fn"}))
            .collect();
        json!({"isIncomplete": false, "items": items})
    }

    pub fn definition(uri: &str, text: &str, line: u64, character: u64) -> Value {
        let Some((_, _, _, name)) = word_at(text, line, character) else {
            return Value::Null;
        };
        decls(text)
            .into_iter()
            .find(|d| d.3 == name)
            .map(|(l, a, b, _)| json!([{"uri": uri, "range": range(l, a, b)}]))
            .unwrap_or(Value::Null)
    }

    fn encode(path: &std::path::Path) -> String {
        let mut out = String::from("file://");
        for b in path.to_string_lossy().bytes() {
            match b {
                b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'~'
                | b'/'
                | b':' => out.push(b as char),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }

    /// `definition` of a name declared in a file of `dir`: a `LocationLink` onto its `fn` line.
    pub fn xfile_definition(dir: &str, text: &str, line: u64, character: u64) -> Value {
        let Some((_, _, _, name)) = word_at(text, line, character) else {
            return Value::Null;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Value::Null;
        };
        let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        files.sort();
        for path in files.into_iter().filter(|p| p.is_file()) {
            let Ok(body) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some((l, a, b, _)) = decls(&body).into_iter().find(|d| d.3 == name) {
                let whole_start = body
                    .lines()
                    .nth(l as usize)
                    .map_or(0, |ln| ln.chars().count() as u64);
                return json!([{
                    "targetUri": encode(&path),
                    "targetRange": {"start": {"line": l, "character": 0},
                                    "end": {"line": l, "character": whole_start}},
                    "targetSelectionRange": range(l, a, b),
                }]);
            }
        }
        Value::Null
    }

    pub fn references(uri: &str, text: &str, line: u64, character: u64) -> Value {
        let Some((_, _, _, name)) = word_at(text, line, character) else {
            return Value::Null;
        };
        Value::Array(
            words(text)
                .into_iter()
                .filter(|w| w.3 == name)
                .map(|(l, a, b, _)| json!({"uri": uri, "range": range(l, a, b)}))
                .collect(),
        )
    }

    pub fn rename(uri: &str, text: &str, line: u64, character: u64, new: &str) -> Value {
        let Some((_, _, _, name)) = word_at(text, line, character) else {
            return Value::Null;
        };
        let edits: Vec<Value> = words(text)
            .into_iter()
            .filter(|w| w.3 == name)
            .map(|(l, a, b, _)| json!({"range": range(l, a, b), "newText": new}))
            .collect();
        json!({"changes": {uri: edits}})
    }

    pub fn hover(text: &str, line: u64, character: u64) -> Value {
        match word_at(text, line, character) {
            Some((_, _, _, w)) if decls(text).iter().any(|d| d.3 == w) => {
                json!({"contents": {"kind": "markdown", "value": format!("```\nfn {w}\n```")}})
            }
            _ => Value::Null,
        }
    }

    pub fn symbols(uri: &str, text: &str) -> Value {
        Value::Array(
            decls(text)
                .into_iter()
                .map(|(l, a, b, n)| {
                    json!({"name": n, "kind": 12, "location": {"uri": uri, "range": range(l, a, b)}})
                })
                .collect(),
        )
    }
}

fn read_msg(r: &mut impl BufRead) -> Option<Value> {
    let mut len = 0usize;
    loop {
        let mut l = String::new();
        if r.read_line(&mut l).ok()? == 0 {
            return None;
        }
        let l = l.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some(v) = l.strip_prefix("Content-Length: ") {
            len = v.parse().ok()?;
        }
    }
    let mut b = vec![0; len];
    std::io::Read::read_exact(r, &mut b).ok()?;
    serde_json::from_slice(&b).ok()
}

fn send(v: &Value) {
    let s = v.to_string();
    let mut o = std::io::stdout().lock();
    let _ = write!(o, "Content-Length: {}\r\n\r\n{}", s.len(), s);
    let _ = o.flush();
}

fn main() {
    if let Ok(p) = std::env::var("FAKE_COUNT_FILE") {
        let n: u32 = std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let _ = std::fs::write(&p, (n + 1).to_string());
    }
    if std::env::var("FAKE_EXIT_ON_START").is_ok() {
        std::process::exit(3);
    }
    let semantic = std::env::var("FAKE_SEMANTIC").is_ok();
    let sync = match std::env::var("FAKE_SYNC").as_deref() {
        Ok("full") => 1,
        Ok("none") => 0,
        _ => 2,
    };
    let mut r = BufReader::new(std::io::stdin().lock());
    let mut versions: HashMap<String, i64> = HashMap::new();
    let mut texts: HashMap<String, String> = HashMap::new();
    let mut changes = 0u64;
    let mut cancelled: Vec<i64> = vec![];
    let mut change_log: Vec<Value> = vec![];
    let mut next_server_id = 1000;
    let mut init_options = Value::Null;
    let mut edit_waiters: HashMap<i64, i64> = HashMap::new(); // our request id → client id

    while let Some(m) = read_msg(&mut r) {
        let method = m.get("method").and_then(Value::as_str).unwrap_or("");
        let id = m.get("id").cloned();
        let params = m.get("params").cloned().unwrap_or(Value::Null);
        match (method, id) {
            ("initialize", Some(id)) => {
                init_options = params["initializationOptions"].clone();
                if let Some(ms) = std::env::var("FAKE_INIT_DELAY_MS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                }
                send(&json!({"jsonrpc":"2.0","id":id,"result":{
                    "capabilities": {
                        "textDocumentSync": {"openClose": true, "change": sync, "save": {"includeText": true}},
                        "hoverProvider": true,
                        "renameProvider": {"prepareProvider": true},
                        "completionProvider": semantic.then(|| json!({"triggerCharacters": ["."]})),
                        "definitionProvider": semantic,
                        "referencesProvider": semantic,
                        "documentSymbolProvider": semantic
                    },
                    "serverInfo": {"name": "fake-lsp"}
                }}))
            }
            ("shutdown", Some(id)) => send(&json!({"jsonrpc":"2.0","id":id,"result":null})),
            ("exit", _) => std::process::exit(0),
            ("textDocument/didOpen", _) => {
                let td = &params["textDocument"];
                let uri = td["uri"].as_str().unwrap_or("").to_string();
                versions.insert(uri.clone(), td["version"].as_i64().unwrap_or(0));
                texts.insert(uri.clone(), td["text"].as_str().unwrap_or("").into());
                diag_for(semantic, &uri, &versions, changes, &texts);
            }
            ("textDocument/didChange", _) => {
                let uri = params["textDocument"]["uri"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                versions.insert(
                    uri.clone(),
                    params["textDocument"]["version"].as_i64().unwrap_or(0),
                );
                changes += 1;
                change_log.push(params["contentChanges"].clone());
                if let Some(t) = texts.get_mut(&uri) {
                    for c in params["contentChanges"].as_array().into_iter().flatten() {
                        text::apply(t, c);
                    }
                }
                diag_for(semantic, &uri, &versions, changes, &texts);
            }
            ("textDocument/didClose", _) => {
                let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
                versions.remove(uri);
            }
            ("$/cancelRequest", _) => cancelled.push(params["id"].as_i64().unwrap_or(-1)),
            ("test/echo", Some(id)) => send(&json!({"jsonrpc":"2.0","id":id,"result":params})),
            ("test/slow", Some(_)) => {} // answered only by the client giving up
            ("test/pid", Some(id)) => {
                send(&json!({"jsonrpc":"2.0","id":id,"result":std::process::id()}))
            }
            ("test/state", Some(id)) => send(&json!({"jsonrpc":"2.0","id":id,"result":{
                "open": versions.keys().collect::<Vec<_>>(),
                "changes": changes,
                "cancelled": cancelled,
                "change_log": change_log,
                "texts": texts,
            }})),
            ("test/crash", _) => std::process::exit(1),
            (
                "textDocument/completion"
                | "textDocument/definition"
                | "textDocument/references"
                | "textDocument/rename"
                | "textDocument/hover"
                | "textDocument/documentSymbol",
                Some(id),
            ) if semantic => {
                let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
                let t = texts.get(uri).map(String::as_str).unwrap_or("");
                let l = params["position"]["line"].as_u64().unwrap_or(0);
                let c = params["position"]["character"].as_u64().unwrap_or(0);
                let result = match method {
                    "textDocument/completion" => semantic::completion(t, l, c),
                    "textDocument/definition" => {
                        let local = semantic::definition(uri, t, l, c);
                        match (local.is_null(), std::env::var("FAKE_XFILE_DIR")) {
                            (true, Ok(dir)) => semantic::xfile_definition(&dir, t, l, c),
                            _ => local,
                        }
                    }
                    "textDocument/references" => semantic::references(uri, t, l, c),
                    "textDocument/rename" => {
                        semantic::rename(uri, t, l, c, params["newName"].as_str().unwrap_or(""))
                    }
                    "textDocument/hover" => semantic::hover(t, l, c),
                    _ => semantic::symbols(uri, t),
                };
                send(&json!({"jsonrpc":"2.0","id":id,"result":result}));
            }
            ("test/applyEdit", Some(id)) => {
                next_server_id += 1;
                edit_waiters.insert(next_server_id, id.as_i64().unwrap_or(0));
                send(
                    &json!({"jsonrpc":"2.0","id":next_server_id,"method":"workspace/applyEdit",
                    "params":{"edit":{"changes":{"file:///x":[]}}}}),
                );
            }
            ("test/init", Some(id)) => {
                send(&json!({"jsonrpc":"2.0","id":id,"result":init_options}))
            }
            ("test/config", Some(id)) => {
                next_server_id += 1;
                edit_waiters.insert(next_server_id, id.as_i64().unwrap_or(0));
                send(
                    &json!({"jsonrpc":"2.0","id":next_server_id,"method":"workspace/configuration",
                    "params":{"items":[{"section":"rust-analyzer"}]}}),
                );
            }
            (_, Some(id)) if method.is_empty() => {
                // response to one of our requests: forward to the waiting client request
                if let Some(cid) = id.as_i64().and_then(|i| edit_waiters.remove(&i)) {
                    send(
                        &json!({"jsonrpc":"2.0","id":cid,"result":m.get("result").cloned().unwrap_or(Value::Null)}),
                    );
                }
            }
            (_, Some(id)) => {
                send(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"unknown"}}))
            }
            _ => {}
        }
    }
}

fn diag_for(
    semantic: bool,
    uri: &str,
    versions: &HashMap<String, i64>,
    changes: u64,
    texts: &HashMap<String, String>,
) {
    if !semantic {
        return diag(uri, versions, changes, texts);
    }
    send(
        &json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{
            "uri": uri,
            "version": versions.get(uri),
            "diagnostics": semantic::diagnostics(texts.get(uri).map_or("", String::as_str)),
        }}),
    );
}

fn diag(uri: &str, versions: &HashMap<String, i64>, changes: u64, texts: &HashMap<String, String>) {
    send(
        &json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{
            "uri": uri,
            "version": versions.get(uri),
            "diagnostics": [{
                "range": {"start":{"line":0,"character":0},"end":{"line":0,"character":1}},
                "message": format!("v{} changes={} len={}", versions.get(uri).copied().unwrap_or(-1), changes, texts.get(uri).map_or(0, String::len)),
                "severity": 2
            }]
        }}),
    );
}
