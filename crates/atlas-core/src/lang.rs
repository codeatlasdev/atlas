//! The display language, resolved once for the CLI, TUI, hostd and the desktop shell.
//!
//! Precedence (first that is set wins):
//! 1. `ATLAS_LANG`, an explicit override (`en` forces English for a bug report or an agent).
//! 2. `~/.atlas/language`, one BCP-47 tag, written by Desktop Settings and `atlas config lang`.
//! 3. The POSIX environment: `LC_ALL`, then `LC_MESSAGES`, then `LANG`; `LANGUAGE` (a colon list)
//!    refines it, except when that locale is `C`/`POSIX`, which means English (the GNU rule scripts
//!    and CI rely on with `LC_ALL=C`).
//! 4. The OS preferred languages (macOS ignores the environment, so this comes after it).
//! 5. English.
//!
//! The catalogs ship `en` and `pt-BR`. A tag negotiates by language: `pt`, `pt-PT` and `pt_BR.UTF-8`
//! all select `pt-BR` until a `pt-PT` catalog exists. A language we have no catalog for (`fr`)
//! selects English: asking for something unavailable never falls through to a lower-precedence
//! source. [`resolve_with`] is the pure core; [`resolve`] only gathers the real inputs.

use std::path::{Path, PathBuf};

/// The languages with a catalog, as canonical BCP-47 tags. `atlas-i18n` has a test that its
/// `locales/` directories match this list; `bun run i18n:add-locale` edits it.
pub const SUPPORTED: &[&str] = &["en", "pt-BR"];
/// What no input selects.
pub const DEFAULT: &str = "en";
/// The environment override.
pub const ENV: &str = "ATLAS_LANG";

/// Every input of the resolution, so the precedence is testable without touching the process.
#[derive(Debug, Default, Clone, Copy)]
pub struct Inputs<'a> {
    /// `ATLAS_LANG`.
    pub atlas_lang: Option<&'a str>,
    /// The content of `~/.atlas/language`.
    pub file: Option<&'a str>,
    /// `LANGUAGE`, a colon-separated list.
    pub language: Option<&'a str>,
    pub lc_all: Option<&'a str>,
    pub lc_messages: Option<&'a str>,
    pub lang: Option<&'a str>,
    /// The OS preferred languages, most preferred first.
    pub os: &'a [String],
}

/// A tag as the BCP-47 subtags of a POSIX or BCP-47 string: `pt_BR.UTF-8@euro` becomes `pt-BR`.
fn subtags(tag: &str) -> Vec<String> {
    let base = tag.split(['.', '@']).next().unwrap_or("");
    base.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `tag` as a supported canonical tag, only when it names one exactly (`en`, `pt-BR`, any case,
/// `_` or `-`). Validation for what a user types.
pub fn normalize(tag: &str) -> Option<&'static str> {
    let parts = subtags(tag.trim());
    let joined = parts.join("-");
    SUPPORTED
        .iter()
        .copied()
        .find(|s| s.eq_ignore_ascii_case(&joined))
}

/// The supported language `tag` belongs to: the exact tag when we have it, else the first
/// supported tag of the same language (`pt-PT` -> `pt-BR`, `en-GB` -> `en`).
fn negotiate(tag: &str) -> Option<&'static str> {
    if let Some(exact) = normalize(tag) {
        return Some(exact);
    }
    let lang = subtags(tag).into_iter().next()?.to_ascii_lowercase();
    SUPPORTED
        .iter()
        .copied()
        .find(|s| s.split('-').next().is_some_and(|l| l == lang))
}

fn is_c_locale(v: &str) -> bool {
    let base = v.split(['.', '@']).next().unwrap_or("");
    base == "C" || base == "POSIX"
}

fn set(v: Option<&str>) -> Option<&str> {
    v.map(str::trim).filter(|s| !s.is_empty())
}

/// Which input decided the language; `atlas config lang` prints it so a surprising language is
/// one command away from its cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `ATLAS_LANG`.
    Env,
    /// `~/.atlas/language`.
    File,
    /// `LC_ALL`, `LC_MESSAGES`, `LANG` or `LANGUAGE`.
    Posix,
    /// The OS preferred languages.
    Os,
    /// Nothing was set.
    Default,
}

impl Source {
    /// A stable machine name (`--json`, tests).
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Env => "env",
            Source::File => "file",
            Source::Posix => "posix",
            Source::Os => "os",
            Source::Default => "default",
        }
    }
}

/// The language and the input that chose it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    pub tag: &'static str,
    pub source: Source,
}

/// The language for `inputs`; see the module docs for the precedence.
pub fn resolve_with(inputs: &Inputs) -> Resolved {
    let got = |tag, source| Resolved { tag, source };
    // An explicit source selects: a language without a catalog means English, not "ask the next one".
    for (explicit, source) in [
        (inputs.atlas_lang, Source::Env),
        (inputs.file, Source::File),
    ] {
        if let Some(v) = set(explicit) {
            return got(negotiate(v).unwrap_or(DEFAULT), source);
        }
    }
    let effective = [inputs.lc_all, inputs.lc_messages, inputs.lang]
        .into_iter()
        .find_map(set);
    if let Some(locale) = effective {
        if is_c_locale(locale) {
            return got(DEFAULT, Source::Posix);
        }
        let listed = set(inputs.language)
            .into_iter()
            .flat_map(|l| l.split(':'))
            .filter(|e| !e.trim().is_empty() && !is_c_locale(e.trim()));
        let tag = listed
            .chain(std::iter::once(locale))
            .find_map(|e| negotiate(e.trim()))
            .unwrap_or(DEFAULT);
        return got(tag, Source::Posix);
    }
    match inputs.os.iter().find_map(|l| negotiate(l)) {
        Some(tag) => got(tag, Source::Os),
        None => got(DEFAULT, Source::Default),
    }
}

/// `~/.atlas/language`.
pub fn preference_path() -> PathBuf {
    crate::paths::atlas_home().join("language")
}

/// The saved preference in `path`: a supported tag, or `None` when absent, empty or unreadable.
pub fn read_preference_at(path: &Path) -> Option<&'static str> {
    let raw = std::fs::read_to_string(path).ok()?;
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    negotiate(line)
}

/// Save `tag` (validated, canonical) to `path`, or remove the file for `None` (follow the system).
/// Returns what was saved.
pub fn write_preference_at(path: &Path, tag: Option<&str>) -> anyhow::Result<Option<&'static str>> {
    let Some(tag) = tag else {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        return Ok(None);
    };
    let canonical = normalize(tag).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported language {tag:?}; supported: {}",
            SUPPORTED.join(", ")
        )
    })?;
    crate::fs::write_atomic(
        path,
        format!("{canonical}\n").as_bytes(),
        crate::fs::Mode::Default,
    )?;
    Ok(Some(canonical))
}

/// The saved preference (`~/.atlas/language`), if any.
pub fn preference() -> Option<&'static str> {
    read_preference_at(&preference_path())
}

/// Save the preference to `~/.atlas/language`; `None` removes it. Returns the canonical tag saved.
pub fn set_preference(tag: Option<&str>) -> anyhow::Result<Option<&'static str>> {
    write_preference_at(&preference_path(), tag)
}

/// The display language of this process and what decided it, from the real environment, the saved
/// preference and the OS.
pub fn resolve_source() -> Resolved {
    let env = |k: &str| std::env::var(k).ok();
    let (atlas_lang, language, lc_all, lc_messages, lang) = (
        env(ENV),
        env("LANGUAGE"),
        env("LC_ALL"),
        env("LC_MESSAGES"),
        env("LANG"),
    );
    let file = std::fs::read_to_string(preference_path()).ok();
    let file_line = file
        .as_deref()
        .and_then(|f| f.lines().map(str::trim).find(|l| !l.is_empty()));
    // The OS lookup is the only slow input (a CoreFoundation call on macOS): skip it when a
    // higher-precedence source already settles the answer.
    let settled = set(atlas_lang.as_deref()).is_some()
        || set(file_line).is_some()
        || [&lc_all, &lc_messages, &lang]
            .into_iter()
            .any(|v| set(v.as_deref()).is_some());
    let os: Vec<String> = if settled {
        Vec::new()
    } else {
        sys_locale::get_locales().collect()
    };
    resolve_with(&Inputs {
        atlas_lang: atlas_lang.as_deref(),
        file: file_line,
        language: language.as_deref(),
        lc_all: lc_all.as_deref(),
        lc_messages: lc_messages.as_deref(),
        lang: lang.as_deref(),
        os: &os,
    })
}

/// The display language of this process.
pub fn resolve() -> &'static str {
    resolve_source().tag
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(i: Inputs) -> &'static str {
        resolve_with(&i).tag
    }

    #[test]
    fn precedence_table() {
        let os = vec!["pt-BR".to_string()];
        let cases: &[(&str, Inputs, &str)] = &[
            ("nothing set is English", Inputs::default(), "en"),
            (
                "ATLAS_LANG beats the saved file",
                Inputs {
                    atlas_lang: Some("en"),
                    file: Some("pt-BR"),
                    ..Default::default()
                },
                "en",
            ),
            (
                "the file beats POSIX env and the OS",
                Inputs {
                    file: Some("pt-BR"),
                    lang: Some("en_US.UTF-8"),
                    os: &os,
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "POSIX env beats the OS (macOS ignores env, so env must come first)",
                Inputs {
                    lang: Some("en_US.UTF-8"),
                    os: &os,
                    ..Default::default()
                },
                "en",
            ),
            (
                "OS is used when nothing else is set",
                Inputs {
                    os: &os,
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "LC_ALL beats LC_MESSAGES beats LANG",
                Inputs {
                    lc_all: Some("pt_BR.UTF-8"),
                    lc_messages: Some("en_US"),
                    lang: Some("en_US"),
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "LC_MESSAGES beats LANG",
                Inputs {
                    lc_messages: Some("pt_BR"),
                    lang: Some("en_US"),
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "empty LC_ALL is ignored (POSIX: empty counts as unset)",
                Inputs {
                    lc_all: Some(""),
                    lang: Some("pt_BR"),
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "LC_ALL=C is English even with LANGUAGE set and a Portuguese OS (scripts and CI)",
                Inputs {
                    lc_all: Some("C"),
                    language: Some("pt_BR"),
                    os: &os,
                    ..Default::default()
                },
                "en",
            ),
            (
                "C.UTF-8 is the C locale too",
                Inputs {
                    lang: Some("C.UTF-8"),
                    language: Some("pt_BR"),
                    ..Default::default()
                },
                "en",
            ),
            (
                "POSIX locale ignores LANGUAGE",
                Inputs {
                    lang: Some("POSIX"),
                    language: Some("pt_BR"),
                    ..Default::default()
                },
                "en",
            ),
            (
                "LANGUAGE refines a real locale: first entry we have wins",
                Inputs {
                    lang: Some("en_US.UTF-8"),
                    language: Some("fr:pt_BR:en"),
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "LANGUAGE without a real locale is not used (GNU: needs LANG)",
                Inputs {
                    language: Some("pt_BR"),
                    ..Default::default()
                },
                "en",
            ),
            (
                "pt-PT negotiates to pt-BR until a pt-PT catalog exists",
                Inputs {
                    atlas_lang: Some("pt-PT"),
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "bare pt is Brazilian Portuguese (CLDR)",
                Inputs {
                    atlas_lang: Some("pt"),
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "case and underscore are forgiven",
                Inputs {
                    atlas_lang: Some("PT_br"),
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "a language we lack selects English and does not fall to the OS",
                Inputs {
                    atlas_lang: Some("fr"),
                    os: &os,
                    ..Default::default()
                },
                "en",
            ),
            (
                "a POSIX locale we lack selects English and does not fall to the OS",
                Inputs {
                    lang: Some("fr_FR.UTF-8"),
                    os: &os,
                    ..Default::default()
                },
                "en",
            ),
            (
                "OS list: first entry we have",
                Inputs {
                    os: &["fr-FR".to_string(), "pt-PT".to_string()],
                    ..Default::default()
                },
                "pt-BR",
            ),
            (
                "en-GB is English",
                Inputs {
                    atlas_lang: Some("en-GB"),
                    ..Default::default()
                },
                "en",
            ),
            (
                "blank ATLAS_LANG is ignored",
                Inputs {
                    atlas_lang: Some("  "),
                    file: Some("pt-BR"),
                    ..Default::default()
                },
                "pt-BR",
            ),
        ];
        for (name, input, want) in cases {
            assert_eq!(run(*input), *want, "{name}");
        }
    }

    #[test]
    fn normalize_accepts_only_supported_tags() {
        assert_eq!(normalize("pt-br"), Some("pt-BR"));
        assert_eq!(normalize("pt_BR.UTF-8"), Some("pt-BR"));
        assert_eq!(normalize("en"), Some("en"));
        assert_eq!(
            normalize("pt"),
            None,
            "bare pt is negotiated, never saved as is"
        );
        assert_eq!(normalize("fr"), None);
        assert_eq!(normalize(""), None);
    }

    #[test]
    fn preference_file_round_trips_and_unset_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("language");
        assert_eq!(read_preference_at(&path), None);
        assert_eq!(
            write_preference_at(&path, Some("pt_br")).unwrap(),
            Some("pt-BR")
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "pt-BR\n");
        assert_eq!(read_preference_at(&path), Some("pt-BR"));
        assert_eq!(write_preference_at(&path, None).unwrap(), None);
        assert!(!path.exists());
        // Unsetting twice is fine.
        assert_eq!(write_preference_at(&path, None).unwrap(), None);
    }

    #[test]
    fn invalid_tag_is_rejected_and_leaves_the_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("language");
        write_preference_at(&path, Some("pt-BR")).unwrap();
        let err = write_preference_at(&path, Some("klingon")).unwrap_err();
        assert!(err.to_string().contains("supported: en, pt-BR"), "{err}");
        assert_eq!(read_preference_at(&path), Some("pt-BR"));
    }

    #[test]
    fn a_hand_edited_file_is_read_leniently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("language");
        std::fs::write(&path, "\n  pt_BR.UTF-8 \nignored\n").unwrap();
        assert_eq!(read_preference_at(&path), Some("pt-BR"));
        std::fs::write(&path, "").unwrap();
        assert_eq!(read_preference_at(&path), None);
    }

    #[test]
    fn the_source_names_the_input_that_decided() {
        let os = vec!["pt-BR".to_string()];
        let src = |i: Inputs| resolve_with(&i).source;
        assert_eq!(
            src(Inputs {
                atlas_lang: Some("en"),
                ..Default::default()
            }),
            Source::Env
        );
        assert_eq!(
            src(Inputs {
                file: Some("pt-BR"),
                ..Default::default()
            }),
            Source::File
        );
        assert_eq!(
            src(Inputs {
                lang: Some("C"),
                ..Default::default()
            }),
            Source::Posix
        );
        assert_eq!(
            src(Inputs {
                os: &os,
                ..Default::default()
            }),
            Source::Os
        );
        assert_eq!(src(Inputs::default()), Source::Default);
        // The OS answer is Os only when it actually matched a catalog.
        assert_eq!(
            src(Inputs {
                os: &["fr-FR".to_string()],
                ..Default::default()
            }),
            Source::Default
        );
    }
}
