//! The WebKit data store of an agent instance of Atlas Desktop. WKWebView ignores `HOME`, so each instance gets
//! `WKWebsiteDataStore(forIdentifier:)`, which macOS keeps in the real `~/Library/WebKit/<bundle id>/
//! WebsiteDataStore/<uuid>`. The app (`agent_mode`) derives the uuid from the instance's debug socket and the CLI
//! derives the same one, so `atlas desktop --agent --stop` can delete it and `--gc` can find the leftovers.
//! Only those per-instance stores are ever deleted; the bundle's default store (`WebsiteData`) is the user's.

use std::path::{Path, PathBuf};

/// Bundle ids an agent instance can run under: dev builds, and the release/legacy ids of older checkouts.
pub const BUNDLE_IDS: [&str; 3] = [
    "br.com.codeatlas.atlas.dev",
    "br.com.codeatlas.atlas",
    "dev.codeatlas.atlas",
];

/// 128-bit FNV-1a of the instance's socket path: stable across runs and builds.
pub fn id(socket: &str) -> [u8; 16] {
    let mut h: u128 = 0x6c62272e07bb014262b821756295c58d;
    for b in socket.bytes() {
        h ^= b as u128;
        h = h.wrapping_mul(0x0000000001000000000000000000013b);
    }
    h.to_be_bytes()
}

/// The directory name WebKit gives that store (lowercase UUID).
pub fn dir_name(id: [u8; 16]) -> String {
    let x: String = id.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &x[..8],
        &x[8..12],
        &x[12..16],
        &x[16..20],
        &x[20..]
    )
}

/// Every place the store of `socket` can be, under `home`.
pub fn paths(home: &Path, socket: &str) -> Vec<PathBuf> {
    let name = dir_name(id(socket));
    BUNDLE_IDS
        .iter()
        .map(|b| {
            home.join("Library/WebKit")
                .join(b)
                .join("WebsiteDataStore")
                .join(&name)
        })
        .collect()
}

/// Per-instance stores under `home` that belong to none of `live_sockets`.
pub fn orphans(home: &Path, live_sockets: &[String]) -> Vec<PathBuf> {
    let keep: Vec<String> = live_sockets.iter().map(|s| dir_name(id(s))).collect();
    let mut out = Vec::new();
    for b in BUNDLE_IDS {
        let root = home.join("Library/WebKit").join(b).join("WebsiteDataStore");
        for e in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if is_uuid(&n) && !keep.contains(&n) {
                out.push(e.path());
            }
        }
    }
    out.sort();
    out
}

fn is_uuid(n: &str) -> bool {
    n.len() == 36
        && n.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_matches_the_directory_webkit_created_for_a_real_instance() {
        // Observed on macOS 26 for `bun run desktop:agent -- --name lab` (2026-10-22).
        assert_eq!(
            dir_name(id("/private/tmp/atlas-agent-lab/debug.sock")),
            "a4e51717-e4dc-d6b1-f806-595d68cb667d"
        );
    }

    #[test]
    fn gc_finds_only_dead_instances_stores_and_never_the_default_store() {
        let t = tempfile::tempdir().unwrap();
        let live = "/tmp/atlas-agent-live/debug.sock".to_string();
        let dead = "/tmp/atlas-agent-dead/debug.sock";
        let wk = t.path().join("Library/WebKit/br.com.codeatlas.atlas.dev");
        for d in [
            wk.join("WebsiteDataStore").join(dir_name(id(&live))),
            wk.join("WebsiteDataStore").join(dir_name(id(dead))),
            wk.join("WebsiteDataStore/not-a-store"),
            wk.join("WebsiteData/LocalStorage"),
        ] {
            std::fs::create_dir_all(d).unwrap();
        }
        assert_eq!(
            orphans(t.path(), &[live]),
            vec![wk.join("WebsiteDataStore").join(dir_name(id(dead)))]
        );
    }
}
