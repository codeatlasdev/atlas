//! What we tell servers about the client, and the settings we answer `workspace/configuration`
//! with.

use serde_json::{Value, json};

/// `initialize` params. Positions are UTF-16 (the LSP default and what the editor computes).
pub fn initialize_params(root: &str, name: &str, init_options: Value) -> Value {
    let kinds = |k: &[i64]| k.to_vec();
    json!({
        "processId": std::process::id(),
        "clientInfo": {"name": "atlas", "version": env!("CARGO_PKG_VERSION")},
        "locale": "en",
        "rootUri": uri_from_path(root),
        "rootPath": root,
        "workspaceFolders": [{"uri": uri_from_path(root), "name": name}],
        "initializationOptions": init_options,
        "trace": "off",
        "capabilities": {
            "general": {"positionEncodings": ["utf-16"], "markdown": {"parser": "marked"}},
            "window": {
                "workDoneProgress": true,
                "showMessage": {"messageActionItem": {"additionalPropertiesSupport": false}},
                "showDocument": {"support": false}
            },
            "workspace": {
                "applyEdit": true,
                "workspaceEdit": {
                    "documentChanges": true,
                    "resourceOperations": ["create", "rename", "delete"],
                    "failureHandling": "abort",
                    "normalizesLineEndings": false
                },
                "didChangeConfiguration": {"dynamicRegistration": false},
                "didChangeWatchedFiles": {"dynamicRegistration": false},
                "symbol": {
                    "dynamicRegistration": false,
                    "symbolKind": {"valueSet": kinds(&(1..=26).collect::<Vec<_>>())}
                },
                "configuration": true,
                "workspaceFolders": true,
                "inlayHint": {"refreshSupport": false},
                "diagnostics": {"refreshSupport": false}
            },
            "textDocument": {
                "synchronization": {
                    "dynamicRegistration": false,
                    "willSave": false,
                    "willSaveWaitUntil": false,
                    "didSave": true
                },
                "completion": {
                    "dynamicRegistration": false,
                    "contextSupport": true,
                    "insertTextMode": 2,
                    "completionItem": {
                        "snippetSupport": true,
                        "commitCharactersSupport": true,
                        "documentationFormat": ["markdown", "plaintext"],
                        "deprecatedSupport": true,
                        "preselectSupport": true,
                        "insertReplaceSupport": true,
                        "labelDetailsSupport": true,
                        "tagSupport": {"valueSet": [1]},
                        "resolveSupport": {
                            "properties": ["documentation", "detail", "additionalTextEdits", "command"]
                        },
                        "insertTextModeSupport": {"valueSet": [1, 2]}
                    },
                    "completionItemKind": {"valueSet": kinds(&(1..=25).collect::<Vec<_>>())},
                    "completionList": {
                        "itemDefaults": ["commitCharacters", "editRange", "insertTextFormat", "insertTextMode", "data"]
                    }
                },
                "hover": {"dynamicRegistration": false, "contentFormat": ["markdown", "plaintext"]},
                "signatureHelp": {
                    "dynamicRegistration": false,
                    "contextSupport": true,
                    "signatureInformation": {
                        "documentationFormat": ["markdown", "plaintext"],
                        "parameterInformation": {"labelOffsetSupport": true},
                        "activeParameterSupport": true
                    }
                },
                "declaration": {"dynamicRegistration": false, "linkSupport": true},
                "definition": {"dynamicRegistration": false, "linkSupport": true},
                "typeDefinition": {"dynamicRegistration": false, "linkSupport": true},
                "implementation": {"dynamicRegistration": false, "linkSupport": true},
                "references": {"dynamicRegistration": false},
                "documentHighlight": {"dynamicRegistration": false},
                "documentSymbol": {
                    "dynamicRegistration": false,
                    "hierarchicalDocumentSymbolSupport": true,
                    "symbolKind": {"valueSet": kinds(&(1..=26).collect::<Vec<_>>())},
                    "tagSupport": {"valueSet": [1]}
                },
                "codeAction": {
                    "dynamicRegistration": false,
                    "isPreferredSupport": true,
                    "disabledSupport": true,
                    "dataSupport": true,
                    "honorsChangeAnnotations": false,
                    "resolveSupport": {"properties": ["edit", "command"]},
                    "codeActionLiteralSupport": {
                        "codeActionKind": {
                            "valueSet": [
                                "", "quickfix", "refactor", "refactor.extract", "refactor.inline",
                                "refactor.rewrite", "source", "source.organizeImports", "source.fixAll"
                            ]
                        }
                    }
                },
                "formatting": {"dynamicRegistration": false},
                "rangeFormatting": {"dynamicRegistration": false},
                "rename": {
                    "dynamicRegistration": false,
                    "prepareSupport": true,
                    "prepareSupportDefaultBehavior": 1
                },
                "publishDiagnostics": {
                    "relatedInformation": true,
                    "tagSupport": {"valueSet": [1, 2]},
                    "versionSupport": true,
                    "codeDescriptionSupport": true,
                    "dataSupport": true
                },
                "inlayHint": {
                    "dynamicRegistration": false,
                    "resolveSupport": {"properties": ["tooltip", "textEdits", "label.tooltip", "label.location", "label.command"]}
                },
                "foldingRange": {"dynamicRegistration": false, "lineFoldingOnly": true},
                "selectionRange": {"dynamicRegistration": false}
            }
        }
    })
}

pub fn uri_from_path(path: &str) -> String {
    uri_from_path_on(path, cfg!(windows))
}

/// [`uri_from_path`] for a Windows (`C:\a\b` → `file:///C:/a/b`) or a Unix path.
fn uri_from_path_on(path: &str, windows: bool) -> String {
    let fixed;
    let path = if windows {
        fixed = windows_path_to_uri_path(path);
        fixed.as_str()
    } else {
        path
    };
    let mut out = String::from("file://");
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            // The drive colon stays literal, as VS Code and the servers write it.
            b':' if windows => out.push(':'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `C:\a\b` → `/C:/a/b`: forward slashes and a leading slash before the drive.
fn windows_path_to_uri_path(path: &str) -> String {
    let p = path.replace('\\', "/");
    if p.as_bytes().get(1) == Some(&b':') {
        format!("/{p}")
    } else {
        p
    }
}

/// `file://` URI → absolute path (percent-decoded). `None` for other schemes.
pub fn path_from_uri(uri: &str) -> Option<String> {
    let p = path_from_uri_on(uri)?;
    Some(if cfg!(windows) {
        windows_path_from_uri_path(&p)
    } else {
        p
    })
}

/// `/C:/a/b` (or `/c%3A/a/b`, already decoded) → `C:/a/b`: the forward-slash form the app keeps
/// every path in (`atlas_core::paths::normalize`), so pushed diagnostics match its documents.
fn windows_path_from_uri_path(p: &str) -> String {
    let b = p.as_bytes();
    let p = if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        &p[1..]
    } else {
        p
    };
    p.to_string()
}

fn path_from_uri_on(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Initialization options per server (sent in `initialize`).
pub fn init_options(server: &str) -> Value {
    match server {
        "typescript-language-server" => json!({
            "hostInfo": "atlas",
            "preferences": {
                "includeCompletionsForModuleExports": true,
                "includeCompletionsWithInsertText": true,
                "includeInlayParameterNameHints": "literals",
                "includeInlayVariableTypeHints": false
            }
        }),
        "intelephense" => json!({"clearCache": false, "licenceKey": ""}),
        "vscode-html-language-server" | "vscode-css-language-server" => json!({
            "provideFormatter": true,
            "embeddedLanguages": {"css": true, "javascript": true}
        }),
        "vscode-json-language-server" => json!({"provideFormatter": true}),
        _ => Value::Null,
    }
}

/// [`init_options`] plus what depends on the project: TypeScript runs on the project's own
/// `node_modules/typescript` (nearest ancestor of `root` that has `lib/tsserver.js`), so its
/// diagnostics match the version the project builds with. Without one, the bundled TypeScript serves.
pub fn init_options_for(server: &str, root: &str) -> Value {
    let mut opts = init_options(server);
    if server == "typescript-language-server"
        && let Some(path) = project_tsserver(std::path::Path::new(root))
    {
        opts["tsserver"] = json!({ "path": path });
    }
    opts
}

fn project_tsserver(root: &std::path::Path) -> Option<String> {
    root.ancestors()
        .map(|d| d.join("node_modules/typescript/lib/tsserver.js"))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Answer for one `workspace/configuration` item. `section` is e.g. `rust-analyzer` or
/// `python.analysis`; unknown sections get `null` (the spec's "no value").
pub fn configuration(server: &str, section: Option<&str>) -> Value {
    let sect = section.unwrap_or("");
    match server {
        "rust-analyzer" => {
            let all = json!({
                "checkOnSave": true,
                "check": {"command": "clippy"},
                "cargo": {"buildScripts": {"enable": true}},
                "procMacro": {"enable": true},
                "inlayHints": {
                    "typeHints": {"enable": true},
                    "parameterHints": {"enable": true},
                    "chainingHints": {"enable": true}
                },
                "completion": {"callable": {"snippets": "fill_arguments"}}
            });
            match sect {
                "rust-analyzer" | "" => all,
                s => s
                    .strip_prefix("rust-analyzer.")
                    .and_then(|k| dig(&all, k))
                    .unwrap_or(Value::Null),
            }
        }
        "gopls" => json!({
            "gofumpt": false, "staticcheck": true, "usePlaceholders": true,
            "completeUnimported": true, "hints": {"parameterNames": true, "assignVariableTypes": true}
        }),
        "basedpyright" => match sect {
            "python.analysis" | "basedpyright.analysis" => {
                json!({"typeCheckingMode": "standard", "autoSearchPaths": true, "useLibraryCodeForTypes": true, "diagnosticMode": "openFilesOnly"})
            }
            "python" => json!({"analysis": {"autoSearchPaths": true}}),
            _ => Value::Null,
        },
        "intelephense" => json!({"files": {"maxSize": 5_000_000}}),
        "typescript-language-server" => Value::Null,
        "vscode-html-language-server"
        | "vscode-css-language-server"
        | "vscode-json-language-server" => match sect {
            "html" => {
                json!({"format": {"enable": true}, "autoClosingTags": true, "validate": {"scripts": true, "styles": true}})
            }
            "css" | "scss" | "less" => json!({"validate": true, "lint": {}}),
            "json" => json!({"validate": {"enable": true}, "format": {"enable": true}}),
            "http" => json!({"proxy": ""}),
            _ => Value::Null,
        },
        "yaml-language-server" => match sect {
            "yaml" => {
                json!({"validate": true, "format": {"enable": true}, "completion": true, "hover": true})
            }
            _ => Value::Null,
        },
        "bash-language-server" => json!({"globPattern": "**/*@(.sh|.inc|.bash|.command)"}),
        _ => Value::Null,
    }
}

/// [`configuration`] with the user's `settings` merged over the defaults. The user's object is
/// keyed like the server's config tree (`{"rust-analyzer": {"check": {...}}}` or, for servers
/// whose root section is implicit, the tree itself); `section` digs into it the same way.
pub fn configuration_with(server: &str, section: Option<&str>, user: Option<&Value>) -> Value {
    let base = configuration(server, section);
    let Some(user) = user else { return base };
    let sect = section.unwrap_or("");
    // The user may nest under the server's own section name or give the bare tree.
    let tree = user.get(sect).filter(|_| !sect.is_empty()).or_else(|| {
        let key = sect.split('.').next().unwrap_or("");
        let nested = user.get(key)?;
        let rest = sect.strip_prefix(key)?.trim_start_matches('.');
        if rest.is_empty() {
            Some(nested)
        } else {
            dig_ref(nested, rest)
        }
    });
    let user_part = tree.cloned().or_else(|| {
        // Bare tree for the default (empty-section) question.
        sect.is_empty().then(|| user.clone())
    });
    match user_part {
        Some(u) => merge(base, u),
        None => base,
    }
}

fn dig_ref<'a>(v: &'a Value, dotted: &str) -> Option<&'a Value> {
    dotted.split('.').try_fold(v, |cur, k| cur.get(k))
}

/// Deep merge: objects merge key by key, anything else is replaced by `over`.
pub fn merge(base: Value, over: Value) -> Value {
    match (base, over) {
        (Value::Object(mut b), Value::Object(o)) => {
            for (k, v) in o {
                let cur = b.remove(&k).unwrap_or(Value::Null);
                b.insert(k, merge(cur, v));
            }
            Value::Object(b)
        }
        (_, o) => o,
    }
}

fn dig(v: &Value, dotted: &str) -> Option<Value> {
    dotted.split('.').try_fold(v, |cur, k| cur.get(k)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_ts(dir: &std::path::Path) {
        let lib = dir.join("node_modules/typescript/lib");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("tsserver.js"), "").unwrap();
    }

    #[test]
    fn typescript_uses_the_projects_own_typescript_when_it_has_one() {
        let tmp = tempfile::tempdir().unwrap();
        fake_ts(tmp.path());
        let root = tmp.path().to_string_lossy().into_owned();
        let opts = init_options_for("typescript-language-server", &root);
        // Same TypeScript version as the project's own build: no mismatched diagnostics.
        assert_eq!(
            opts["tsserver"]["path"],
            tmp.path()
                .join("node_modules/typescript/lib/tsserver.js")
                .to_string_lossy()
                .as_ref()
        );
        // Atlas's defaults are kept alongside.
        assert_eq!(opts["hostInfo"], "atlas");
    }

    #[test]
    fn a_monorepo_package_finds_typescript_in_a_parent_node_modules() {
        let tmp = tempfile::tempdir().unwrap();
        fake_ts(tmp.path());
        let pkg = tmp.path().join("packages/app");
        std::fs::create_dir_all(&pkg).unwrap();
        let opts = init_options_for("typescript-language-server", &pkg.to_string_lossy());
        assert!(
            opts["tsserver"]["path"]
                .as_str()
                .unwrap()
                .ends_with("node_modules/typescript/lib/tsserver.js")
        );
    }

    #[test]
    fn without_a_project_typescript_the_bundled_one_is_used() {
        let tmp = tempfile::tempdir().unwrap();
        let opts = init_options_for("typescript-language-server", &tmp.path().to_string_lossy());
        assert!(opts.get("tsserver").is_none(), "{opts}");
        assert_eq!(opts["hostInfo"], "atlas");
    }

    #[test]
    fn a_node_modules_without_tsserver_js_is_not_used() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("node_modules/typescript/lib")).unwrap();
        let opts = init_options_for("typescript-language-server", &tmp.path().to_string_lossy());
        assert!(opts.get("tsserver").is_none());
    }

    #[test]
    fn other_servers_are_unaffected_by_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        fake_ts(tmp.path());
        assert_eq!(
            init_options_for("gopls", &tmp.path().to_string_lossy()),
            Value::Null
        );
    }

    #[test]
    fn uris_roundtrip_with_special_characters() {
        for p in ["/a/b.rs", "/with space/ü/日本.rs", "/p/100%/a#b?c.ts"] {
            let u = uri_from_path_on(p, false);
            assert!(u.starts_with("file:///"));
            assert!(!u.contains(' ') && !u.contains('#') && !u.contains('?'));
            assert_eq!(path_from_uri_on(&u).as_deref(), Some(p));
        }
        assert_eq!(path_from_uri("untitled:Untitled-1"), None);
        assert_eq!(path_from_uri_on("file:///a%2"), Some("/a%2".into()));
    }

    #[test]
    fn windows_paths_become_drive_uris_and_come_back() {
        let u = uri_from_path_on(r"C:\Users\u\my app\src\main.rs", true);
        assert_eq!(u, "file:///C:/Users/u/my%20app/src/main.rs");
        let back = windows_path_from_uri_path(&path_from_uri_on(&u).unwrap());
        assert_eq!(back, "C:/Users/u/my app/src/main.rs");
        // The app's own form (forward slashes) makes the same URI and comes back unchanged.
        let app = "C:/Users/u/my app/src/main.rs";
        assert_eq!(uri_from_path_on(app, true), u);
        // Servers that percent-encode the colon (VS Code does) decode to the same path.
        let enc = path_from_uri_on("file:///c%3A/x/y.ts").unwrap();
        assert_eq!(windows_path_from_uri_path(&enc), "c:/x/y.ts");
    }

    #[test]
    fn rust_analyzer_config_sections() {
        assert_eq!(
            configuration("rust-analyzer", Some("rust-analyzer"))["checkOnSave"],
            true
        );
        assert_eq!(
            configuration("rust-analyzer", Some("rust-analyzer.check.command")),
            "clippy"
        );
        assert_eq!(
            configuration("rust-analyzer", Some("rust-analyzer.nope")),
            Value::Null
        );
        assert_eq!(configuration("unknown", Some("x")), Value::Null);
    }

    #[test]
    fn user_settings_merge_over_defaults_by_section() {
        let user = json!({"rust-analyzer": {"check": {"command": "check"}, "extra": 1}});
        // Whole-section question: deep merge keeps untouched defaults.
        let all = configuration_with("rust-analyzer", Some("rust-analyzer"), Some(&user));
        assert_eq!(all["check"]["command"], "check");
        assert_eq!(all["checkOnSave"], true);
        assert_eq!(all["extra"], 1);
        // Dotted section digs into the user tree.
        assert_eq!(
            configuration_with(
                "rust-analyzer",
                Some("rust-analyzer.check.command"),
                Some(&user)
            ),
            "check"
        );
        // Unrelated section is untouched.
        assert_eq!(
            configuration_with(
                "rust-analyzer",
                Some("rust-analyzer.cargo.buildScripts"),
                Some(&user)
            ),
            json!({"enable": true})
        );
        // No user settings: identical to the default.
        assert_eq!(
            configuration_with("gopls", None, None),
            configuration("gopls", None)
        );
        // A bare tree answers the empty-section question (gopls, bash).
        let bare = json!({"staticcheck": false});
        let g = configuration_with("gopls", None, Some(&bare));
        assert_eq!(g["staticcheck"], false);
        assert_eq!(g["usePlaceholders"], true);
    }

    #[test]
    fn merge_replaces_non_objects_and_recurses_objects() {
        assert_eq!(
            merge(json!({"a": {"b": 1, "c": 2}}), json!({"a": {"b": 9}})),
            json!({"a": {"b": 9, "c": 2}})
        );
        assert_eq!(
            merge(json!({"a": [1]}), json!({"a": [2, 3]})),
            json!({"a": [2, 3]})
        );
        assert_eq!(merge(json!(1), json!({"x": 1})), json!({"x": 1}));
    }

    #[test]
    fn initialize_advertises_utf16_and_core_features() {
        let p = initialize_params("/r", "r", Value::Null);
        assert_eq!(
            p["capabilities"]["general"]["positionEncodings"],
            json!(["utf-16"])
        );
        let td = &p["capabilities"]["textDocument"];
        assert_eq!(td["completion"]["completionItem"]["snippetSupport"], true);
        assert_eq!(td["rename"]["prepareSupport"], true);
        assert_eq!(
            td["documentSymbol"]["hierarchicalDocumentSymbolSupport"],
            true
        );
        assert_eq!(td["definition"]["linkSupport"], true);
        assert_eq!(p["rootUri"], "file:///r");
    }
}
