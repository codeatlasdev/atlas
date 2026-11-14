//! The machine-wide heavy-job queue (`atlas run --heavy`) for the Activity drawer: who holds the slots,
//! how many wait. Read-only; the queue itself lives in `atlas_core::heavy`.

use atlas_core::heavy::{Dir, Status};

/// The queue as `atlas heavy status --json` prints it. Cheap (a few small files), polled by the
/// drawer only while it is open.
#[tauri::command]
pub async fn heavy_status() -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(|| Dir::default_dir().status(atlas_core::heavy::slots()))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use atlas_core::heavy::{Entry, Job, State, Status};

    fn entry(label: &str, state: State, slot: Option<usize>, eta: Option<u64>) -> Entry {
        Entry {
            job: Job {
                id: format!("0001763040000000-4242-0000-{label}"),
                pid: 4242,
                label: label.into(),
                command: "bun run check".into(),
                cwd: "/work/atlas-wt-a".into(),
                project: "atlas-wt-a".into(),
                state,
                queued_at: 1_763_040_000_000,
                started_at: slot.map(|_| 1_763_040_004_000),
                slot,
            },
            age_s: 61,
            avg_s: Some(240),
            eta_s: eta,
        }
    }

    // Contract: `apps/desktop/src/lib/__fixtures__/heavy-status.json` is the REAL serialization of the
    // queue status the `heavy_status` command returns; `panel/heavy.test.ts` feeds it to the drawer's
    // own reader. `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn heavy_status_fixture_is_the_real_serialization() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/heavy-status.json");
        let real = Status {
            slots: 2,
            running: vec![entry("gate", State::Running, Some(0), Some(179))],
            queued: vec![
                entry("gate", State::Queued, None, Some(179)),
                entry("bench", State::Queued, None, None),
            ],
            load: Some(3.5),
            mem_available_pct: Some(42),
        };
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
