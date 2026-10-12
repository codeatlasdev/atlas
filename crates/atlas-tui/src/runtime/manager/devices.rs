use tokio::process::Command;

use super::ServiceManager;

impl ServiceManager {
    /// Detect connected/available devices for a given platform.
    /// Returns Vec<(id, display_name)>.
    pub async fn detect_devices(
        platform: &crate::config::SessionPlatform,
    ) -> Vec<(String, String)> {
        match platform {
            crate::config::SessionPlatform::Ios => Self::detect_ios_devices().await,
            crate::config::SessionPlatform::Android => Self::detect_android_devices().await,
        }
    }

    async fn detect_ios_devices() -> Vec<(String, String)> {
        // Try xcrun simctl list devices --json first (simulators)
        let output = Command::new("xcrun")
            .args(["simctl", "list", "devices", "--json"])
            .output()
            .await;

        let mut devices: Vec<(String, String)> = Vec::new();

        if let Ok(out) = output
            && let Ok(text) = String::from_utf8(out.stdout)
        {
            devices.extend(parse_simctl_json(&text));
        }

        // Try xcrun devicectl for physical devices
        let phys = Command::new("xcrun")
            .args([
                "devicectl",
                "list",
                "devices",
                "--json-output",
                "/dev/stdout",
            ])
            .output()
            .await;

        if let Ok(out) = phys
            && let Ok(text) = String::from_utf8(out.stdout)
        {
            devices.extend(parse_devicectl_json(&text));
        }

        devices
    }

    async fn detect_android_devices() -> Vec<(String, String)> {
        let output = Command::new("adb").args(["devices", "-l"]).output().await;

        let mut devices = Vec::new();
        if let Ok(out) = output
            && let Ok(text) = String::from_utf8(out.stdout)
        {
            for line in text.lines().skip(1) {
                let line = line.trim();
                if line.is_empty() || line.starts_with('*') {
                    continue;
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == "device" {
                    let id = parts[0].to_string();
                    // Try to extract model from the -l output
                    let model = parts
                        .iter()
                        .find(|p| p.starts_with("model:"))
                        .map(|p| p.trim_start_matches("model:").replace('_', " "))
                        .unwrap_or_else(|| id.clone());
                    devices.push((id, model));
                }
            }
        }
        devices
    }
}

/// Parse `xcrun simctl list devices --json` output.
/// Returns booted simulators first, then available (shutdown) ones.
pub(crate) fn parse_simctl_json(text: &str) -> Vec<(String, String)> {
    // We do a lightweight string scan to avoid a serde dependency in the manager.
    // Format: {"devices": {"com.apple.CoreSimulator.SimRuntime.iOS-17-0": [{"udid":"…","name":"…","state":"Booted",…},…]}}
    let mut booted: Vec<(String, String)> = Vec::new();
    let mut available: Vec<(String, String)> = Vec::new();

    // Quick JSON field extractor — finds the value of `"key": "value"` pairs.
    let field = |haystack: &str, key: &str| -> Option<String> {
        let needle = format!("\"{}\":", key);
        let start = haystack.find(&needle)? + needle.len();
        let rest = haystack[start..].trim_start();
        if let Some(inner) = rest.strip_prefix('"') {
            let end = inner.find('"')?;
            Some(inner[..end].to_string())
        } else {
            None
        }
    };

    // Walk object blocks that look like simulator entries (contain "udid")
    let mut pos = 0;
    while let Some(start) = text[pos..]
        .find("{\"udid\"")
        .or_else(|| text[pos..].find("\"udid\""))
    {
        let abs = pos + start;
        // Find enclosing object end — simple brace counting
        let snippet_start = abs.saturating_sub(1);
        let snippet = &text[snippet_start..std::cmp::min(snippet_start + 512, text.len())];

        if let (Some(udid), Some(name)) = (field(snippet, "udid"), field(snippet, "name")) {
            let state = field(snippet, "state").unwrap_or_default();
            let os = field(snippet, "deviceTypeIdentifier")
                .map(|s| s.rsplit('.').next().unwrap_or("").to_string())
                .unwrap_or_default();
            let display = if os.is_empty() {
                name.clone()
            } else {
                format!("{name} ({os})")
            };
            if state.eq_ignore_ascii_case("booted") {
                booted.push((udid, display));
            } else if state.eq_ignore_ascii_case("shutdown")
                || state.eq_ignore_ascii_case("available")
            {
                available.push((udid, display));
            }
        }
        pos = abs + 8; // advance past this match
    }

    booted.extend(available);
    booted
}

/// Parse `xcrun devicectl list devices --json-output /dev/stdout` output.
pub(crate) fn parse_devicectl_json(text: &str) -> Vec<(String, String)> {
    let mut devices = Vec::new();

    // Quick field extractor (same approach as simctl)
    let field = |haystack: &str, key: &str| -> Option<String> {
        let needle = format!("\"{}\":", key);
        let start = haystack.find(&needle)? + needle.len();
        let rest = haystack[start..].trim_start();
        if let Some(inner) = rest.strip_prefix('"') {
            let end = inner.find('"')?;
            Some(inner[..end].to_string())
        } else {
            None
        }
    };

    // devicectl devices have "identifier" and "deviceProperties.name"
    let mut pos = 0;
    while let Some(start) = text[pos..].find("\"identifier\"") {
        let abs = pos + start;
        let snippet = &text[abs..std::cmp::min(abs + 512, text.len())];
        if let Some(udid) = field(snippet, "identifier") {
            // name can be nested under deviceProperties — search a wider window
            let wide = &text[abs..std::cmp::min(abs + 1024, text.len())];
            let name = field(wide, "name").unwrap_or_else(|| udid.clone());
            devices.push((udid, format!("{name} [device]")));
        }
        pos = abs + 14;
    }

    devices
}
