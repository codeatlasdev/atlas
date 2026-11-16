//! `atlas doctor --bundle`: the diagnostics bundle of Atlas Desktop's "Copy details" (distro,
//! session, GPU, packaging, crashes, versions, the tails of `app.log`, `stderr.log` and
//! `hostd.log`, all redacted) printed on a terminal, for a user whose app does not open at all.
//! The probe and the text are `atlas_host::bundle`, the same code the app calls, so the two
//! cannot drift; this file only gathers what the CLI knows (its own version, the answer of hostd).

use std::time::Duration;

use anyhow::Result;
use atlas_core::probe::Runtime;
use atlas_host::HostClient;
use atlas_host::bundle::{self, Diagnostics, Versions};
use serde_json::json;

const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the bundle says it came from (the `Where:` line of the text).
const PLACE: &str = "atlas doctor --bundle";

/// The text `--bundle` prints: the report head, then the same lines as "Copy details".
pub fn render(d: &Diagnostics) -> String {
    bundle::details_text(
        Some(CLI_VERSION),
        PLACE,
        None,
        "Diagnostics bundle (requested; nothing failed)",
        "",
        d,
    )
}

pub async fn run(json: bool) -> Result<()> {
    let base = atlas_host::paths::base();
    // A dead or stuck hostd is a field ("not running"), never a failure of the bundle.
    let info = match HostClient::connect_dir(&base, "cli").await {
        Ok(c) => tokio::time::timeout(Duration::from_secs(2), c.call("info", json!({})))
            .await
            .ok()
            .and_then(Result::ok),
        Err(_) => None,
    };
    // The app's own version is not known from here (it may not even start): the CLI ships in
    // lock-step with it, so `cli` is this binary and `app` stays empty rather than guessed.
    let versions = Versions::from_info("", Some(CLI_VERSION.to_string()), info.as_ref());
    // The probe runs bounded subprocess queries: not on the runtime.
    let d = tokio::task::spawn_blocking(move || {
        bundle::gather(
            &base,
            &atlas_core::crash::crash_dir(),
            versions,
            std::env::current_exe().ok(),
            &Runtime::default(),
            Vec::new(),
        )
    })
    .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&d)?);
    } else {
        println!("{}", render(&d));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CLI prints the bundle with the app's own renderer: its head names the CLI and the
    /// command, the shared block is the app's, and a dead hostd is a line, not an error.
    #[test]
    fn the_bundle_text_has_the_cli_head_and_the_shared_report_lines() {
        let base = tempfile::tempdir().unwrap();
        let d = bundle::gather(
            base.path(),
            &base.path().join("crashes"),
            Versions::from_info("", Some(CLI_VERSION.to_string()), None),
            None,
            &Runtime::default(),
            Vec::new(),
        );
        let text = render(&d);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], format!("Atlas {CLI_VERSION}"));
        assert_eq!(lines[1], "Where: atlas doctor --bundle");
        for want in [
            "Terminal host: not running".to_string(),
            format!("CLI: {CLI_VERSION}"),
            format!("OS: {}", d.os),
            "Atlas crash reports: none".to_string(),
        ] {
            assert!(
                lines.contains(&want.as_str()),
                "missing {want:?} in:\n{text}"
            );
        }
    }
}
