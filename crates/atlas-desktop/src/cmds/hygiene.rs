//! The Hygiene panel of Settings > Workspaces: the same report and fix as `atlas doctor
//! --hygiene` (one producer, `atlas_core::hygiene`), run off the async runtime because the scan
//! reads every process's environment and sizes directory trees.

use atlas_core::hygiene::{self, Fixed, Registry, Report};

/// Orphaned tagged processes, disk per project, registered dirs, indexed heavy dirs. Changes
/// nothing.
#[tauri::command]
pub async fn hygiene_report() -> Result<Report, String> {
    tauri::async_runtime::spawn_blocking(|| {
        hygiene::report(&Registry::open(), &atlas_core::paths::atlas_home())
    })
    .await
    .map_err(|e| e.to_string())
}

/// End the orphans and free what Atlas owns; `target/`, `node_modules` and worktrees are never
/// removed. Returns what was done, and the report as it is afterwards.
#[tauri::command]
pub async fn hygiene_fix() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let (registry, base) = (Registry::open(), atlas_core::paths::atlas_home());
        let fixed: Fixed = hygiene::fix(&registry, &base);
        let report = hygiene::report(&registry, &base);
        serde_json::json!({ "fixed": fixed, "report": report })
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Contract: `apps/desktop/src/lib/__fixtures__/hygiene-report.json` is the REAL serialization
    // of a report with an orphan, a project and a registered dir; `settings/hygiene.test.ts`
    // feeds it to the panel's own summary code. `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn hygiene_report_fixture_is_the_real_serialization() {
        use hygiene::{DirKind, DirUse, Freed, Kind, Orphan, ProjectDisk};
        let real = Report {
            orphans: vec![Orphan {
                pid: 4242,
                start: 1_700_000_000,
                name: "chrome".into(),
                session: "s1".into(),
                kind: Some(Kind::Agent),
                label: Some("agent".into()),
                project: Some("/work/app".into()),
            }],
            live: 2,
            projects: vec![ProjectDisk {
                root: "/work/app".into(),
                name: "app".into(),
                target: 3 << 30,
                node_modules: 1 << 29,
                logs: 1024,
                worktrees: vec![("feat".into(), 1 << 30)],
                indexed: vec!["/work/app/target".into()],
            }],
            dirs: vec![DirUse {
                path: "/tmp/agent-1".into(),
                kind: DirKind::Temp,
                bytes: 2048,
                cap_bytes: 4096,
                orphaned: true,
            }],
            freeable: Freed {
                dirs_removed: 1,
                entries_removed: 0,
                bytes: 2048,
            },
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/hygiene-report.json");
        let fresh = serde_json::to_value(&real).unwrap();
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
}
