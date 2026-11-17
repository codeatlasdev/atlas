//! The message format, shared by the runtime and `build.rs` (which refuses a bad message at build
//! time): the restricted inlang message-format subset of `i18n/messages/<area>/<locale>.json`.
//!
//! - text with `{name}` variables and `{@key}` references to another message (evaluated with the
//!   same arguments, so an `os` select inside the referenced message still sees `os`);
//!   `\{` and `\}` are literal braces;
//! - a variants object `[{ declarations, selectors, match }]` for plural, exact (`0`) and `os`
//!   selects: `local countPlural = count: plural` selects by CLDR category, `local countExact =
//!   count` or `input os` by the value itself, `*` matches anything. The most specific variant
//!   wins, earlier selectors first (the MF2 preference order).
//!
//! Markup (`{#tag}`) belongs to rich text on the web and is refused here.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Var(String),
    Ref(String),
}

pub type Pattern = Vec<Part>;

#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    /// The variable the selector reads (`count`, `os`).
    pub var: String,
    /// `true`: its CLDR plural category; `false`: the value as text.
    pub plural: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Text(Pattern),
    Select {
        selectors: Vec<Selector>,
        /// One key per selector (`*` = any) and the pattern.
        variants: Vec<(Vec<String>, Pattern)>,
    },
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|f| f.is_ascii_alphabetic() || f == '_')
        && c.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn parse_pattern(text: &str) -> Result<Pattern, String> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if matches!(chars.peek(), Some('{' | '}')) => buf.push(chars.next().unwrap()),
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') | None => return Err(format!("unclosed `{{` in {text:?}")),
                        Some(c) => inner.push(c),
                    }
                }
                if !buf.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut buf)));
                }
                let inner = inner.trim();
                if let Some(key) = inner.strip_prefix('@') {
                    parts.push(Part::Ref(key.trim().to_owned()));
                } else if inner.starts_with(['#', '/']) {
                    return Err(format!(
                        "markup `{{{inner}}}` is not supported in Rust catalogs"
                    ));
                } else if is_ident(inner) {
                    parts.push(Part::Var(inner.to_owned()));
                } else {
                    return Err(format!("`{{{inner}}}` is not a variable or a reference"));
                }
            }
            '}' => return Err(format!("unescaped `}}` in {text:?}")),
            c => buf.push(c),
        }
    }
    if !buf.is_empty() {
        parts.push(Part::Text(buf));
    }
    Ok(parts)
}

pub fn parse(value: &Value) -> Result<Message, String> {
    if let Some(text) = value.as_str() {
        return parse_pattern(text).map(Message::Text);
    }
    let [variants] = value
        .as_array()
        .map(Vec::as_slice)
        .ok_or("not a string or a variants array")?
    else {
        return Err("a message has exactly one variants object".into());
    };
    let decls = variants["declarations"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    // name -> (variable, plural)
    let mut declared: Vec<(String, Selector)> = Vec::new();
    for d in decls {
        let d = d.as_str().ok_or("a declaration is a string")?.trim();
        if let Some(var) = d.strip_prefix("input ") {
            let var = var.trim().to_owned();
            declared.push((var.clone(), Selector { var, plural: false }));
        } else if let Some(rest) = d.strip_prefix("local ") {
            let (name, expr) = rest
                .split_once('=')
                .ok_or_else(|| format!("bad declaration {d:?}"))?;
            let (var, func) = match expr.split_once(':') {
                Some((v, f)) => (v.trim(), f.trim()),
                None => (expr.trim(), ""),
            };
            let plural = match func {
                "plural" => true,
                "" | "number" | "string" | "integer" => false,
                other => return Err(format!("unsupported function `:{other}` in {d:?}")),
            };
            declared.push((
                name.trim().to_owned(),
                Selector {
                    var: var.to_owned(),
                    plural,
                },
            ));
        } else {
            return Err(format!("bad declaration {d:?}"));
        }
    }
    let names: Vec<&str> = variants["selectors"]
        .as_array()
        .ok_or("a variants object needs its `selectors`")?
        .iter()
        .map(|s| s.as_str().ok_or("a selector is a string"))
        .collect::<Result<_, _>>()?;
    let selectors: Vec<Selector> = names
        .iter()
        .map(|n| {
            declared
                .iter()
                .find(|(d, _)| d == n)
                .map(|(_, s)| s.clone())
                .ok_or_else(|| format!("selector `{n}` is not declared"))
        })
        .collect::<Result<_, _>>()?;
    let arms = variants["match"]
        .as_object()
        .ok_or("a variants object needs its `match`")?;
    let mut out = Vec::new();
    for (key, text) in arms {
        let mut keys = vec!["*".to_owned(); names.len()];
        for part in key.split(',') {
            let (name, lit) = part
                .split_once('=')
                .ok_or_else(|| format!("bad match key {key:?}"))?;
            let i = names
                .iter()
                .position(|n| *n == name.trim())
                .ok_or_else(|| format!("match key {key:?} names an unknown selector"))?;
            keys[i] = lit.trim().to_owned();
        }
        let text = text.as_str().ok_or("a match value is a string")?;
        out.push((keys, parse_pattern(text)?));
    }
    if !out.iter().any(|(k, _)| k.iter().all(|k| k == "*")) {
        return Err("a variants object needs a catch-all arm (`*`)".into());
    }
    Ok(Message::Select {
        selectors,
        variants: out,
    })
}
