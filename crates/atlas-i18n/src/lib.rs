//! Display-language catalogs for the Rust side of Atlas: the CLI, the TUI, hostd and the desktop
//! shell (tray, menus, native notifications).
//!
//! Catalogs are Fluent files, `locales/<lang>/<area>.ftl`, embedded into the binary by `build.rs`
//! (no file is read at run time: the locale of a catalog can never be missing on a user's disk).
//! A message is looked up by id in the language of [`lang()`] and falls back to English, then to
//! the id itself; a missing id panics in debug builds and never in a release.
//!
//! ```ignore
//! println!("{}", t!("config-lang-set", lang = "pt-BR"));
//! ```
//!
//! What is NEVER translated, so never goes through `t!`: `--json` output, MCP, logs, exit codes,
//! the hostd protocol, `step:` lines. `crates/atlas-cli/tests/machine_output.rs` enforces it.
//!
//! Plurals: pt-BR treats 0 as singular (CLDR), so `0 arquivo` is what a plain `[one]` variant
//! prints. Where that reads wrong, add an exact `[0]` variant before `[one]`; Fluent matches a
//! numeric key against the number first. Always keep a `*[other]` default: engines disagree on
//! the categories of large numbers.
//!
//! Numbers are not localized by Fluent's `NUMBER` (it ignores the locale): format a number in
//! Rust and pass the string, or write it in the message. A custom ICU4X-backed function costs
//! about 68 KB and nothing needs it yet.

mod shortcut;
pub use shortcut::{shortcut, shortcut_for};

use std::sync::{LazyLock, OnceLock};

use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
pub use fluent_bundle::{FluentArgs, FluentValue};
use unic_langid::LanguageIdentifier;

include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

type Bundle = FluentBundle<FluentResource>;

/// The languages with a catalog, as BCP-47 tags (the names of the `locales/` directories).
pub fn locales() -> impl Iterator<Item = &'static str> {
    CATALOGS.iter().map(|(tag, _)| *tag)
}

/// A bundle of `sources` for `tag`. Bidi isolation is off: it wraps every argument in invisible
/// marks that corrupt terminal output and copy-pasted paths.
fn build(tag: &str, sources: &[&str]) -> Bundle {
    let id: LanguageIdentifier = tag.parse().unwrap_or_default();
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    bundle.set_use_isolating(false);
    for src in sources {
        let res = match FluentResource::try_new((*src).to_owned()) {
            Ok(res) => res,
            // A malformed entry is Junk; the rest of the file still works. The catalog test
            // refuses a Junk-ful file in CI, so this is only reachable in a half-edited tree.
            Err((res, _)) => res,
        };
        // A duplicate id keeps the first definition; the catalog test refuses duplicates.
        let _ = bundle.add_resource(res);
    }
    bundle
}

fn bundle(tag: &str) -> Option<&'static Bundle> {
    static BUNDLES: LazyLock<Vec<OnceLock<Bundle>>> =
        LazyLock::new(|| CATALOGS.iter().map(|_| OnceLock::new()).collect());
    let i = CATALOGS.iter().position(|(t, _)| *t == tag)?;
    Some(BUNDLES[i].get_or_init(|| {
        let sources: Vec<&str> = CATALOGS[i].1.iter().map(|(_, s)| *s).collect();
        build(tag, &sources)
    }))
}

/// The operating system the process runs on, as the `$os` every message can select on:
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

/// The message `id` in `tag`, formatted with `args` for an explicit `os`; `None` when `tag` has no
/// such message. Every message may select on `$os` (`mac`, `windows`, `linux`): it is added here
/// for every call, so no call site passes it and a text such as "your Mac" or "Trash" is one
/// message with three variants. A caller's own `os` argument wins (tests).
pub fn lookup_on(
    tag: &str,
    id: &str,
    args: Option<&FluentArgs>,
    os: &'static str,
) -> Option<String> {
    let bundle = bundle(tag)?;
    let pattern = bundle.get_message(id)?.value()?;
    // `FluentArgs` is not `Clone`: copy the caller's values, then add `os`.
    let mut with_os = FluentArgs::new();
    for (name, value) in args.into_iter().flat_map(FluentArgs::iter) {
        with_os.set(name.to_owned(), value.clone());
    }
    if with_os.get("os").is_none() {
        with_os.set("os", os);
    }
    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, Some(&with_os), &mut errors);
    debug_assert!(errors.is_empty(), "message {id} ({tag}): {errors:?}");
    Some(text.into_owned())
}

/// [`lookup_on`] for the OS this process runs on.
pub fn lookup(tag: &str, id: &str, args: Option<&FluentArgs>) -> Option<String> {
    lookup_on(tag, id, args, os())
}

/// The message `id` in `tag`: that language, else English, else the id. Panics in debug builds
/// on an id no catalog has (a typo must not ship).
pub fn translate(tag: &str, id: &str, args: Option<&FluentArgs>) -> String {
    lookup(tag, id, args)
        .or_else(|| lookup(atlas_core::lang::DEFAULT, id, args))
        .unwrap_or_else(|| {
            debug_assert!(false, "no message with id {id:?} in any catalog");
            id.to_owned()
        })
}

/// The display language of this process (`atlas_core::lang::resolve()`, read once).
pub fn lang() -> &'static str {
    static LANG: LazyLock<&'static str> = LazyLock::new(atlas_core::lang::resolve);
    *LANG
}

/// A message in the display language: `t!("id")` or `t!("id", name = value, count = 3)`.
/// Values are strings or numbers; a number selects plural variants.
#[macro_export]
macro_rules! t {
    ($id:literal) => {
        $crate::translate($crate::lang(), $id, ::core::option::Option::None)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::FluentArgs::new();
        $( args.set(::core::stringify!($name), $value); )+
        $crate::translate($crate::lang(), $id, ::core::option::Option::Some(&args))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plural(tag: &str, ftl: &str, count: i64) -> String {
        let b = build(tag, &[ftl]);
        let mut errors = vec![];
        let mut args = FluentArgs::new();
        args.set("count", count);
        let p = b.get_message("n").unwrap().value().unwrap();
        b.format_pattern(p, Some(&args), &mut errors).into_owned()
    }

    // pt-BR: CLDR gives 0 the category `one`, so a plain [one] variant prints "0 arquivo".
    // An exact [0] variant placed first is how a translation says "Nenhum arquivo".
    #[test]
    fn pt_br_zero_is_singular_unless_the_translation_has_an_exact_zero() {
        let plain =
            "n = { $count ->\n    [one] { $count } arquivo\n   *[other] { $count } arquivos\n}\n";
        let with_zero = "n = { $count ->\n    [0] Nenhum arquivo\n    [one] { $count } arquivo\n   *[other] { $count } arquivos\n}\n";
        assert_eq!(plural("pt-BR", plain, 0), "0 arquivo");
        assert_eq!(plural("pt-BR", plain, 1), "1 arquivo");
        assert_eq!(plural("pt-BR", plain, 2), "2 arquivos");
        assert_eq!(plural("pt-BR", with_zero, 0), "Nenhum arquivo");
        assert_eq!(plural("pt-BR", with_zero, 1), "1 arquivo");
        // English treats 0 as plural.
        assert_eq!(plural("en", plain, 0), "0 arquivos");
    }

    #[test]
    fn arguments_are_not_wrapped_in_bidi_isolation_marks() {
        let b = build("en", &["hi = Hello { $name }\n"]);
        let mut args = FluentArgs::new();
        args.set("name", "Ana");
        let mut errors = vec![];
        let p = b.get_message("hi").unwrap().value().unwrap();
        assert_eq!(b.format_pattern(p, Some(&args), &mut errors), "Hello Ana");
    }

    // Every message can select on $os without any call site passing it: one shared term, three
    // variants, in both languages. A hand-spelled "Mac" in a message would not move with the OS.
    #[test]
    fn os_terms_select_by_os_in_every_language() {
        let cases: &[(&str, &str, &str, &str)] = &[
            ("term-your-computer", "mac", "en", "your Mac"),
            ("term-your-computer", "windows", "en", "this PC"),
            ("term-your-computer", "linux", "en", "this computer"),
            ("term-your-computer", "mac", "pt-BR", "seu Mac"),
            ("term-your-computer", "windows", "pt-BR", "este PC"),
            ("term-your-computer", "linux", "pt-BR", "este computador"),
            ("term-file-manager", "mac", "en", "Finder"),
            ("term-file-manager", "windows", "en", "File Explorer"),
            (
                "term-file-manager",
                "linux",
                "pt-BR",
                "o gerenciador de arquivos",
            ),
            ("term-trash", "windows", "en", "Recycle Bin"),
            ("term-trash", "windows", "pt-BR", "Lixeira"),
            ("term-system-settings", "mac", "pt-BR", "Ajustes do Sistema"),
            ("term-alt-key", "mac", "en", "Option"),
            ("term-alt-key", "linux", "en", "Alt"),
            ("term-enter-key", "mac", "en", "Return"),
            ("term-enter-key", "windows", "en", "Enter"),
            ("term-menu-bar", "mac", "en", "the menu bar"),
            ("term-menu-bar", "windows", "pt-BR", "a bandeja do sistema"),
        ];
        for (id, os, lang, want) in cases {
            assert_eq!(
                lookup_on(lang, id, None, os).as_deref(),
                Some(*want),
                "{id} on {os} in {lang}"
            );
        }
    }

    // A message that references a term keeps the caller's arguments, $os included.
    #[test]
    fn a_message_reference_passes_os_through() {
        let b = build(
            "pt-BR",
            &[
                "term = { $os ->\n    [mac] seu Mac\n   *[other] este PC\n}\nuses = Roda em { term }\n",
            ],
        );
        let mut args = FluentArgs::new();
        args.set("os", "mac");
        let mut errors = vec![];
        let p = b.get_message("uses").unwrap().value().unwrap();
        assert_eq!(
            b.format_pattern(p, Some(&args), &mut errors),
            "Roda em seu Mac"
        );
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn a_language_without_the_message_falls_back_to_english_then_to_the_id() {
        // `fr` has no catalog at all: English answers.
        let en = lookup("en", "config-lang-unset", None).unwrap();
        assert_eq!(translate("fr", "config-lang-unset", None), en);
        assert_eq!(lookup("fr", "config-lang-unset", None), None);
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn an_unknown_id_is_returned_as_is_in_release() {
        assert_eq!(translate("en", "no-such-id", None), "no-such-id");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "no message with id")]
    fn an_unknown_id_panics_in_debug() {
        translate("en", "no-such-id", None);
    }
}
