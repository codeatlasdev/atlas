//! Display-language catalogs for the Rust side of Atlas: the CLI, the TUI, hostd and the desktop
//! shell (tray, menus, native notifications).
//!
//! The source is `i18n/messages/<area>/<locale>.json`, the same files the desktop app and the site
//! read (one tree, no copies). `build.rs` embeds the areas this crate serves into the binary, so no
//! file is read at run time and a malformed message fails the build. A message is looked up by
//! key in the language of [`lang()`] and falls back to English, then to the key itself; a missing
//! key panics in debug builds and never in a release.
//!
//! ```ignore
//! println!("{}", t!("config_lang_set", lang = "pt-BR"));
//! ```
//!
//! What is NEVER translated, so never goes through `t!`: `--json` output, MCP, logs, exit codes,
//! the hostd protocol, `step:` lines. `crates/atlas-cli/tests/machine_output.rs` enforces it.
//!
//! The format (`message.rs`): `{name}` variables, `{@key}` references, plural and `os` selects.
//! pt-BR treats 0 as singular (CLDR), so a plain `one` variant prints "0 arquivo": add an exact
//! `0` variant (`local countExact = count`, key `countExact=0`) where that reads wrong. Always
//! keep a `*` arm. Numbers are printed as given; format a number in Rust and pass the string
//! when grouping matters.

mod message;
mod shortcut;
pub use shortcut::{shortcut, shortcut_for};

use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock, RwLock};

use message::{Message, Part, Selector};

include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

/// A value passed to a message: text or a number (a number selects plural variants).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
}

impl Value {
    fn text(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            Value::Num(n) => n.to_string(),
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.to_owned())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Str(s)
    }
}
impl From<&String> for Value {
    fn from(s: &String) -> Self {
        Value::Str(s.clone())
    }
}
macro_rules! num {
    ($($t:ty),*) => {$(impl From<$t> for Value { fn from(n: $t) -> Self { Value::Num(n as f64) } })*};
}
num!(i32, i64, u32, u64, usize, f64);

/// The named values of one message call.
#[derive(Debug, Default, Clone)]
pub struct Args(Vec<(String, Value)>);

impl Args {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: &str, value: impl Into<Value>) {
        let value = value.into();
        match self.0.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = value,
            None => self.0.push((name.to_owned(), value)),
        }
    }

    fn get(&self, name: &str) -> Option<&Value> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

type Map = HashMap<String, Message>;

/// The languages with a catalog, as BCP-47 tags (the names of the locale files).
pub fn locales() -> impl Iterator<Item = &'static str> {
    CATALOGS.iter().map(|(tag, _)| *tag)
}

fn catalog(tag: &str) -> Option<&'static Map> {
    static MAPS: LazyLock<Vec<OnceLock<Map>>> =
        LazyLock::new(|| CATALOGS.iter().map(|_| OnceLock::new()).collect());
    let i = CATALOGS.iter().position(|(t, _)| *t == tag)?;
    Some(MAPS[i].get_or_init(|| {
        let mut map = Map::new();
        for (area, json) in CATALOGS[i].1 {
            // build.rs parsed every message already; this cannot fail on an embedded file.
            let value: serde_json::Value =
                serde_json::from_str(json).unwrap_or_else(|e| panic!("area {area}: {e}"));
            for (key, v) in value.as_object().into_iter().flatten() {
                if let Ok(m) = message::parse(v) {
                    map.insert(key.clone(), m);
                }
            }
        }
        map
    }))
}

/// The operating system the process runs on, as the `os` every message can select on:
/// `mac`, `windows` or `linux`.
pub fn os() -> &'static str {
    if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

fn plural_category(tag: &str, n: f64) -> &'static str {
    use intl_pluralrules::{PluralCategory, PluralRuleType, PluralRules};
    // The rule tables are keyed by language (`pt`, `pt-PT`), not by region: try the whole tag,
    // then its language alone (`pt-BR` -> `pt`, which is what CLDR says for Brazil).
    let rules = tag
        .parse::<unic_langid::LanguageIdentifier>()
        .ok()
        .and_then(|id| {
            let mut bare = id.clone();
            bare.region = None;
            bare.clear_variants();
            PluralRules::create(id, PluralRuleType::CARDINAL)
                .or_else(|_| PluralRules::create(bare, PluralRuleType::CARDINAL))
                .ok()
        });
    let picked = rules.and_then(|r| {
        if n.fract() == 0.0 {
            r.select(n as i64)
        } else {
            r.select(n)
        }
        .ok()
    });
    match picked {
        Some(PluralCategory::ZERO) => "zero",
        Some(PluralCategory::ONE) => "one",
        Some(PluralCategory::TWO) => "two",
        Some(PluralCategory::FEW) => "few",
        Some(PluralCategory::MANY) => "many",
        _ => "other",
    }
}

/// `key` in `tag`, else in English (a reference may point at a common message a language lacks).
fn find(tag: &str, key: &str) -> Option<(&'static str, &'static Message)> {
    for t in [tag, atlas_core::lang::DEFAULT] {
        if let Some(m) = catalog(t).and_then(|c| c.get(key)) {
            let name = locales().find(|l| *l == t)?;
            return Some((name, m));
        }
    }
    None
}

fn pattern(parts: &[Part], tag: &str, args: &Args, depth: usize, out: &mut String) -> Option<()> {
    for part in parts {
        match part {
            Part::Text(t) => out.push_str(t),
            Part::Var(name) => out.push_str(&args.get(name)?.text()),
            Part::Ref(key) => out.push_str(&render(tag, key, args, depth + 1)?),
        }
    }
    Some(())
}

fn select<'m>(
    selectors: &[Selector],
    variants: &'m [(Vec<String>, message::Pattern)],
    tag: &str,
    args: &Args,
) -> Option<&'m message::Pattern> {
    let resolved: Vec<String> = selectors
        .iter()
        .map(|s| {
            let v = args.get(&s.var)?;
            Some(match (s.plural, v) {
                (true, Value::Num(n)) => plural_category(tag, *n).to_owned(),
                (true, Value::Str(_)) => "other".to_owned(),
                (false, v) => v.text(),
            })
        })
        .collect::<Option<_>>()?;
    // Most specific wins, earlier selectors first: a literal beats `*` at the first position
    // where the candidates differ.
    // Reversed so that, of equally specific arms, the first one in the file wins.
    variants
        .iter()
        .rev()
        .filter(|(keys, _)| keys.iter().zip(&resolved).all(|(k, r)| k == "*" || k == r))
        .max_by_key(|(keys, _)| keys.iter().map(|k| k != "*").collect::<Vec<_>>())
        .map(|(_, p)| p)
}

fn render(tag: &str, key: &str, args: &Args, depth: usize) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let (name, msg) = find(tag, key)?;
    let mut out = String::new();
    match msg {
        Message::Text(p) => pattern(p, name, args, depth, &mut out)?,
        Message::Select {
            selectors,
            variants,
        } => pattern(
            select(selectors, variants, name, args)?,
            name,
            args,
            depth,
            &mut out,
        )?,
    }
    Some(out)
}

/// `key` in `tag` for an explicit `os` (`mac`, `windows`, `linux`), formatted with `args`; `None`
/// when no catalog has it or it cannot be formatted (a missing argument). Every message may
/// select on `os`: it is added here for every call, so no call site passes it and a text such as
/// "your Mac" or "Trash" is one message with three variants. A caller's own `os` argument wins.
pub fn lookup_on(tag: &str, key: &str, args: Option<&Args>, os: &str) -> Option<String> {
    let mut with_os = args.cloned().unwrap_or_default();
    if with_os.get("os").is_none() {
        with_os.set("os", os);
    }
    // Only this language: English is the caller's fallback (`translate`), except inside a
    // reference, which falls back by itself.
    catalog(tag)?.get(key)?;
    render(tag, key, &with_os, 0)
}

/// [`lookup_on`] for the OS this process runs on.
pub fn lookup(tag: &str, key: &str, args: Option<&Args>) -> Option<String> {
    lookup_on(tag, key, args, os())
}

/// `key` in `tag`: that language, else English, else the key. Panics in debug builds on a key no
/// catalog has (a typo must not ship; `bun run i18n:check` also reports it).
pub fn translate(tag: &str, key: &str, args: Option<&Args>) -> String {
    lookup(tag, key, args)
        .or_else(|| lookup(atlas_core::lang::DEFAULT, key, args))
        .unwrap_or_else(|| {
            debug_assert!(false, "no message with key {key:?} in any catalog");
            key.to_owned()
        })
}

/// The display language of this process (`atlas_core::lang::resolve()`, read once), unless a long-lived
/// process changed it with [`set_lang`].
pub fn lang() -> &'static str {
    static LANG: LazyLock<&'static str> = LazyLock::new(atlas_core::lang::resolve);
    LIVE.read()
        .map(|l| *l)
        .unwrap_or_else(|e| *e.into_inner())
        .unwrap_or(*LANG)
}

/// The language a long-lived process switched to (the desktop shell, when Settings changes it).
static LIVE: RwLock<Option<&'static str>> = RwLock::new(None);

/// Switch [`lang()`] (so every later `t!`) to `tag` without a restart. A tag without a catalog is ignored and the
/// current language stays; the CLI never calls this, it reads the language once.
pub fn set_lang(tag: &str) {
    if let Some(tag) = atlas_core::lang::normalize(tag).filter(|t| locales().any(|l| l == *t)) {
        *LIVE.write().unwrap_or_else(|e| e.into_inner()) = Some(tag);
    }
}

/// A message in the display language: `t!("key")` or `t!("key", name = value, count = 3)`.
/// Values are strings or numbers; a number selects plural variants.
#[macro_export]
macro_rules! t {
    ($id:literal) => {
        $crate::translate($crate::lang(), $id, ::core::option::Option::None)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::Args::new();
        $( args.set(::core::stringify!($name), $value); )+
        $crate::translate($crate::lang(), $id, ::core::option::Option::Some(&args))
    }};
}

/// A message in an explicit language: `t_in!(tag, "key", name = value)`. The form for code that builds text for
/// more than one language at a time (a native menu model compared in `en` and `pt-BR`) instead of the process's.
#[macro_export]
macro_rules! t_in {
    ($tag:expr, $id:literal) => {
        $crate::translate($tag, $id, ::core::option::Option::None)
    };
    ($tag:expr, $id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::Args::new();
        $( args.set(::core::stringify!($name), $value); )+
        $crate::translate($tag, $id, ::core::option::Option::Some(&args))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Format a one-off message in `tag` (no catalog needed).
    fn fmt(tag: &str, value: serde_json::Value, args: &[(&str, Value)]) -> String {
        let msg = message::parse(&value).unwrap();
        let mut a = Args::new();
        for (n, v) in args {
            a.set(n, v.clone());
        }
        let mut out = String::new();
        match &msg {
            Message::Text(p) => pattern(p, tag, &a, 0, &mut out).unwrap(),
            Message::Select {
                selectors,
                variants,
            } => pattern(
                select(selectors, variants, tag, &a).unwrap(),
                tag,
                &a,
                0,
                &mut out,
            )
            .unwrap(),
        }
        out
    }

    fn files(plural_zero: bool) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        if plural_zero {
            m.insert(
                "countExact=0, countPlural=*".into(),
                "Nenhum arquivo".into(),
            );
        }
        m.insert(
            "countExact=*, countPlural=one".into(),
            "{count} arquivo".into(),
        );
        m.insert(
            "countExact=*, countPlural=*".into(),
            "{count} arquivos".into(),
        );
        json!([{
            "declarations": ["input count", "local countExact = count", "local countPlural = count: plural"],
            "selectors": ["countExact", "countPlural"],
            "match": m
        }])
    }

    // pt-BR: CLDR gives 0 the category `one`, so a plain `one` variant prints "0 arquivo". An exact
    // 0 arm is how a translation says "Nenhum arquivo".
    #[test]
    fn pt_br_zero_is_singular_unless_the_translation_has_an_exact_zero() {
        let n =
            |tag, plural_zero, c: i64| fmt(tag, files(plural_zero), &[("count", Value::from(c))]);
        assert_eq!(n("pt-BR", false, 0), "0 arquivo");
        assert_eq!(n("pt-BR", false, 1), "1 arquivo");
        assert_eq!(n("pt-BR", false, 2), "2 arquivos");
        assert_eq!(n("pt-BR", true, 0), "Nenhum arquivo");
        assert_eq!(n("pt-BR", true, 1), "1 arquivo");
        // English treats 0 as plural.
        assert_eq!(n("en", false, 0), "0 arquivos");
    }

    #[test]
    fn the_most_specific_variant_wins_earlier_selectors_first() {
        let v = json!([{
            "declarations": ["input a", "input b"],
            "selectors": ["a", "b"],
            "match": { "a=x, b=*": "ax", "a=*, b=y": "by", "a=*, b=*": "none" }
        }]);
        let on = |a: &str, b: &str| fmt("en", v.clone(), &[("a", a.into()), ("b", b.into())]);
        assert_eq!(
            on("x", "y"),
            "ax",
            "the first selector's literal outranks the second's"
        );
        assert_eq!(on("z", "y"), "by");
        assert_eq!(on("z", "z"), "none");
    }

    #[test]
    fn braces_can_be_escaped_and_bad_messages_are_refused() {
        assert_eq!(
            fmt("en", json!("a \\{b\\} {x}"), &[("x", "1".into())]),
            "a {b} 1"
        );
        for bad in ["{", "}", "{#link}x{/link}", "{a b}"] {
            assert!(message::parse(&json!(bad)).is_err(), "{bad}");
        }
        let undeclared = json!([{ "declarations": [], "selectors": ["n"], "match": {"n=*": "x"} }]);
        assert!(
            message::parse(&undeclared)
                .unwrap_err()
                .contains("not declared")
        );
        let no_default =
            json!([{ "declarations": ["input n"], "selectors": ["n"], "match": {"n=1": "x"} }]);
        assert!(
            message::parse(&no_default)
                .unwrap_err()
                .contains("catch-all")
        );
    }

    // Every message can select on `os` without any call site passing it: one shared term, three
    // variants, in both languages. A hand-spelled "Mac" in a message would not move with the OS.
    #[test]
    fn os_terms_select_by_os_in_every_language() {
        let cases: &[(&str, &str, &str, &str)] = &[
            ("common_os_your_computer", "mac", "en", "your Mac"),
            ("common_os_your_computer", "windows", "en", "this PC"),
            ("common_os_your_computer", "linux", "en", "this computer"),
            ("common_os_your_computer", "mac", "pt-BR", "seu Mac"),
            ("common_os_your_computer", "windows", "pt-BR", "este PC"),
            (
                "common_os_your_computer",
                "linux",
                "pt-BR",
                "este computador",
            ),
            ("common_os_file_manager", "mac", "en", "Finder"),
            ("common_os_file_manager", "windows", "en", "File Explorer"),
            (
                "common_os_file_manager",
                "linux",
                "pt-BR",
                "o gerenciador de arquivos",
            ),
            ("common_os_trash", "windows", "en", "Recycle Bin"),
            ("common_os_trash", "windows", "pt-BR", "Lixeira"),
            (
                "common_os_system_settings",
                "mac",
                "pt-BR",
                "Ajustes do Sistema",
            ),
            ("common_os_alt_key", "mac", "en", "Option"),
            ("common_os_alt_key", "linux", "en", "Alt"),
            ("common_os_enter_key", "mac", "en", "Return"),
            ("common_os_enter_key", "windows", "en", "Enter"),
            ("common_os_menu_bar", "mac", "en", "the menu bar"),
            (
                "common_os_menu_bar",
                "windows",
                "pt-BR",
                "a bandeja do sistema",
            ),
        ];
        for (key, os, lang, want) in cases {
            assert_eq!(
                lookup_on(lang, key, None, os).as_deref(),
                Some(*want),
                "{key} on {os} in {lang}"
            );
        }
    }

    // A message that references another keeps the caller's arguments, `os` included, and a
    // language without the referenced message falls back to English for that part.
    #[test]
    fn a_reference_evaluates_with_the_callers_arguments() {
        let v = json!("Moved to {@common_os_trash}");
        assert_eq!(
            fmt("en", v.clone(), &[("os", "windows".into())]),
            "Moved to Recycle Bin"
        );
        assert_eq!(fmt("pt-BR", v, &[("os", "mac".into())]), "Moved to Lixeira");
    }

    #[test]
    fn a_language_without_the_message_falls_back_to_english_then_to_the_key() {
        // `fr` has no catalog at all: English answers.
        let en = lookup("en", "config_lang_unset", None).unwrap();
        assert_eq!(translate("fr", "config_lang_unset", None), en);
        assert_eq!(lookup("fr", "config_lang_unset", None), None);
    }

    /// Every variable a message reads: its own `{name}`s and selectors, and those of the messages it references.
    fn variables(map: &Map, msg: &Message, out: &mut Vec<String>, depth: usize) {
        let patterns: Vec<&message::Pattern> = match msg {
            Message::Text(p) => vec![p],
            Message::Select {
                selectors,
                variants,
            } => {
                out.extend(selectors.iter().map(|s| s.var.clone()));
                variants.iter().map(|(_, p)| p).collect()
            }
        };
        for part in patterns.into_iter().flatten() {
            match part {
                Part::Var(name) => out.push(name.clone()),
                Part::Ref(key) if depth < 8 => {
                    if let Some(m) = map.get(key) {
                        variables(map, m, out, depth + 1);
                    }
                }
                _ => {}
            }
        }
    }

    // A message the build embedded must format with the arguments its call site passes: every
    // key of every language renders when given every variable it reads (a plural selector gets a
    // number, `source` the word the tests use for it, any other variable a number too).
    #[test]
    fn every_embedded_message_renders_in_every_language() {
        for tag in locales() {
            let map = catalog(tag).unwrap();
            for (key, msg) in map {
                let mut names = Vec::new();
                variables(map, msg, &mut names, 0);
                let mut all = Args::new();
                for v in names {
                    all.set(&v, 2);
                }
                all.set("source", "env");
                assert!(
                    lookup(tag, key, Some(&all)).is_some(),
                    "{key} ({tag}) does not render: a bad reference, or a variable the message reads that a selector cannot take"
                );
            }
        }
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn an_unknown_key_is_returned_as_is_in_release() {
        assert_eq!(translate("en", "no_such_key", None), "no_such_key");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "no message with key")]
    fn an_unknown_key_panics_in_debug() {
        translate("en", "no_such_key", None);
    }

    #[test]
    fn t_in_renders_the_tag_it_is_given_whatever_the_process_language() {
        assert_eq!(t_in!("en", "cli_chrome_usage"), "Usage:");
        assert_eq!(t_in!("pt-BR", "cli_chrome_usage"), "Uso:");
    }

    // The only test that touches the process language (a global): nothing else here reads `lang()`.
    #[test]
    fn set_lang_switches_t_live_and_ignores_a_tag_without_a_catalog() {
        set_lang("pt-BR");
        assert_eq!(lang(), "pt-BR");
        assert_eq!(t!("cli_chrome_usage"), "Uso:");
        set_lang("fr");
        assert_eq!(lang(), "pt-BR", "an unknown tag keeps the current language");
        set_lang("en");
        assert_eq!(t!("cli_chrome_usage"), "Usage:");
    }
}
