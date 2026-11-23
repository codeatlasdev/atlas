//! The real hostd inside a test process, for tests whose contract is with hostd itself.

use atlas_host::HostClient;
use serde_json::json;
use std::time::Duration;

/// The real hostd, in this process, on a temp dir (its registry too: never the user's `~/.atlas`).
pub(crate) async fn real_hostd() -> (tempfile::TempDir, HostClient) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let registry = tmp.path().join("registry");
    std::fs::create_dir_all(&registry).unwrap();
    // One update check on file, so the daemon does not go to the network for one.
    let followed = atlas_core::channel::effective(
        std::env::var(atlas_core::channel::ENV).ok().as_deref(),
        atlas_core::channel::Channel::default(),
    );
    std::fs::write(
        registry.join(atlas_core::update_cache::FILE),
        json!({"last_checked": 1_700_000_000u64, "available_version": null, "channel": followed})
            .to_string(),
    )
    .unwrap();
    let (d, opts) = (
        dir.clone(),
        atlas_host::server::ServeOpts {
            registry: Some(registry),
            ..Default::default()
        },
    );
    tokio::spawn(async move { atlas_host::server::serve_with(&d, opts).await });
    let client = loop {
        if let Ok(c) = HostClient::connect_dir(&dir, "test").await {
            break c;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    (tmp, client)
}
