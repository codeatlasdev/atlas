//! The display language for the webviews. The choice and its resolution live in `atlas_core::lang`
//! (`ATLAS_LANG` > `~/.atlas/language` > POSIX env > OS > English), shared with the CLI and hostd; this only hands it
//! to the page: once before the first render (`locale_get`) and on a change from Settings (`locale_set`).

use atlas_core::lang;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Sent to every window when the language changes, with a [`LocaleState`] payload.
pub const CHANGED: &str = "locale://changed";

/// What the page needs: the language to render now, what the user chose, and what exists.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocaleState {
    /// The language the UI renders in: always one of `supported`.
    pub locale: &'static str,
    /// What the user saved in Settings; `None` means "Follow system".
    pub preference: Option<&'static str>,
    /// Which input decided `locale`: `env`, `file`, `posix`, `os` or `default` (an `ATLAS_LANG` override wins over
    /// the saved choice, and the page says so instead of silently ignoring the click).
    pub source: &'static str,
    /// The tags with a catalog.
    pub supported: &'static [&'static str],
}

fn state(resolved: lang::Resolved, preference: Option<&'static str>) -> LocaleState {
    LocaleState {
        locale: resolved.tag,
        preference,
        source: resolved.source.as_str(),
        supported: lang::SUPPORTED,
    }
}

fn current() -> LocaleState {
    state(lang::resolve_source(), lang::preference())
}

/// The language to render in, read once by the frontend before its first render.
#[tauri::command]
pub fn locale_get() -> LocaleState {
    current()
}

/// Save the language (`None` = follow the system) and tell every window. A tag without a catalog is refused with the
/// list of the ones that exist; nothing is written then.
#[tauri::command]
pub fn locale_set(app: AppHandle, tag: Option<String>) -> Result<LocaleState, String> {
    lang::set_preference(tag.as_deref()).map_err(|e| format!("{e:#}"))?;
    let now = current();
    // The native menus (menu bar, tray, dock) are Rust text: re-label them before the page is told, so nothing in the
    // window is in the old language for a moment.
    crate::menu::language_changed(&app, now.locale);
    let _ = app.emit(CHANGED, &now);
    Ok(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::lang::{Inputs, resolve_with};

    fn resolved(file: Option<&str>, os: &[&str]) -> LocaleState {
        let os: Vec<String> = os.iter().map(|s| s.to_string()).collect();
        let r = resolve_with(&Inputs {
            file,
            os: &os,
            ..Inputs::default()
        });
        state(r, file.and_then(lang::normalize))
    }

    // Contract: `apps/desktop/src/lib/__fixtures__/locale-state.json` is the REAL serialization of a `locale_get`
    // reply (a saved pt-BR choice); `lib/i18n/locale.test.ts` feeds it to the frontend's own `parseLocaleState`.
    // `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn locale_state_fixture_is_the_real_serialization() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/locale-state.json");
        let fresh = serde_json::to_value(resolved(Some("pt-BR"), &["en-US"])).unwrap();
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            // Tab-indented, as `prettier --check src` (useTabs) wants the committed file.
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut buf,
                serde_json::ser::PrettyFormatter::with_indent(b"\t"),
            );
            serde::Serialize::serialize(&fresh, &mut ser).unwrap();
            std::fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
        }
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it"),
        )
        .unwrap();
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }

    #[test]
    fn follow_system_has_no_preference_and_takes_the_os_language() {
        let s = resolved(None, &["pt-BR", "en"]);
        assert_eq!(s.preference, None);
        assert_eq!((s.locale, s.source), ("pt-BR", "os"));
    }

    #[test]
    fn a_saved_choice_beats_the_os_and_is_reported_as_the_file() {
        let s = resolved(Some("en"), &["pt-BR"]);
        assert_eq!(s.preference, Some("en"));
        assert_eq!((s.locale, s.source), ("en", "file"));
    }

    #[test]
    fn the_supported_list_is_the_catalog_list_of_atlas_core() {
        assert_eq!(resolved(None, &[]).supported, lang::SUPPORTED);
    }

    #[test]
    fn a_language_without_a_catalog_is_refused_before_anything_is_written() {
        let err = lang::set_preference(Some("fr")).unwrap_err().to_string();
        assert!(err.contains("fr") && err.contains("pt-BR"), "{err}");
    }
}
