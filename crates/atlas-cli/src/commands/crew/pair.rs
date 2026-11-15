//! `atlas crew pair <target> [--port N]`: pair a phone with a crew server over ssh, with no desktop and no
//! Mac in between. The crew server's own hostd issues the invite (`mobile_pair_start`, run as the crew user
//! by `atlas hostd --mobile-pair`), and the phone pairs with that server directly: it has its own iroh
//! identity, so the crew keeps working with every other computer off. The pairing protocol is the one the
//! desktop's QR uses (`atlas_host::mobile`): one code, 120 s, five wrong tries.

use anyhow::{Context, Result, bail};
use clap::Args;
use qrcode::{EcLevel, QrCode};
use serde_json::Value;

use super::layout::{INSTALL_DIR, WHICH_USER};
use crate::commands::AtlasCommand;

#[derive(Debug, Args)]
pub struct PairArgs {
    /// SSH target for the crew server (e.g. user@hostname or hostname)
    pub ssh_target: String,
    #[command(flatten)]
    pub ssh: super::SshPort,
}

/// Shell that asks the crew server's hostd for a pairing invite as the crew user, in its data dir (the
/// daemon `status` reports). The reply is one JSON object on stdout; a failure must reach the caller.
pub fn pair_script() -> String {
    format!(
        "u=$({WHICH_USER}); sudo -u \"$u\" {INSTALL_DIR}/atlas hostd --mobile-pair --dir \"/var/lib/$u/.atlas\""
    )
}

/// The pairing reply of `atlas hostd --mobile-pair` → the link to scan. The reply is the daemon's, so a
/// line that is not JSON (an ssh banner, a sudo notice before it) is skipped: the invite is the last
/// object on stdout.
pub fn invite_from_output(stdout: &str) -> Result<(Value, String)> {
    let start: Value = stdout
        .lines()
        .rev()
        .find_map(|l| {
            serde_json::from_str::<Value>(l.trim())
                .ok()
                .filter(Value::is_object)
        })
        .context("the crew server did not answer with a pairing invite")?;
    let url = atlas_host::mobile::pair_url(&start).map_err(anyhow::Error::msg)?;
    Ok((start, url))
}

/// `text` as a terminal QR code: two modules per character cell using half blocks, with a 2-module quiet
/// zone. Every line is black on white (fixed SGR colours, not the theme's), so it scans on a dark terminal
/// too: a dark module is the black part of the glyph.
pub fn terminal_qr(text: &str) -> Result<String> {
    let code = QrCode::with_error_correction_level(text, EcLevel::L)
        .context("the pairing link is too long for a QR code")?;
    let w = code.width();
    let quiet = 2usize;
    let dark = |x: isize, y: isize| {
        x >= 0
            && y >= 0
            && (x as usize) < w
            && (y as usize) < w
            && code[(x as usize, y as usize)] == qrcode::Color::Dark
    };
    let side = (w + 2 * quiet) as isize;
    let q = quiet as isize;
    let mut out = String::new();
    let mut y = -q;
    while y < side - q {
        out.push_str("\x1b[30;47m");
        for x in -q..side - q {
            // Upper and lower module of one cell, black ink on white paper.
            out.push(match (dark(x, y), dark(x, y + 1)) {
                (false, false) => ' ',
                (true, false) => '▀',
                (false, true) => '▄',
                (true, true) => '█',
            });
        }
        out.push_str("\x1b[0m\n");
        y += 2;
    }
    Ok(out)
}

impl AtlasCommand for PairArgs {
    async fn run(self) -> Result<()> {
        let o = self
            .ssh
            .remote(&self.ssh_target)
            .exec(&pair_script())
            .await?;
        if o.code != 0 {
            let why = if o.stderr.trim().is_empty() {
                o.stdout.trim()
            } else {
                o.stderr.trim()
            };
            bail!(
                "could not get a pairing invite from {}: {why}\n  (is the crew server installed and running? `atlas crew status {}`)",
                self.ssh_target,
                self.ssh_target
            );
        }
        let (start, url) = invite_from_output(&o.stdout)?;
        println!("{}", terminal_qr(&url)?);
        println!(
            "Open Atlas Mobile → Add a computer → scan this code, or type it.\n  code  {}\n  link  {url}\n\nIt works once and expires in {} seconds. The phone pairs with {} directly: the crew keeps working with your computer off.",
            start["code"].as_str().unwrap_or_default(),
            start["expires_in_s"].as_u64().unwrap_or(120),
            start["hostname"].as_str().unwrap_or(&self.ssh_target),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // The server side runs as the crew user in its data dir, like `pause`: the daemon `status` reports.
    #[test]
    fn the_script_asks_the_crew_users_hostd_in_its_dir_and_lets_a_failure_through() {
        let sh = pair_script();
        assert!(sh.contains(WHICH_USER), "{sh}");
        assert!(sh.contains("sudo -u \"$u\""), "{sh}");
        assert!(
            sh.contains("/usr/local/lib/atlas-crew/atlas hostd --mobile-pair"),
            "{sh}"
        );
        assert!(sh.contains("--dir \"/var/lib/$u/.atlas\""), "{sh}");
        assert!(!sh.contains("|| true"), "{sh}");
    }

    // The reply may follow a banner or a sudo notice; the invite is the last JSON object.
    #[test]
    fn the_invite_is_the_last_json_object_after_noise() {
        let reply = json!({"node_id": "N", "code": "012345", "expires_in_s": 120,
            "addrs": ["10.0.0.5:4433"], "relay_url": "https://relay.example/", "hostname": "box"});
        let out = format!("Welcome to box\nsudo: notice\n{reply}\n");
        let (start, url) = invite_from_output(&out).unwrap();
        assert_eq!(start["code"], "012345");
        let query = url.strip_prefix("atlas-mobile://pair?").expect(&url);
        // `%3A`/`%2F` decoded by hand: the values here use only those two escapes.
        let dec = |s: &str| s.replace("%3A", ":").replace("%2F", "/");
        let q: Vec<(String, String)> = query
            .split('&')
            .map(|kv| kv.split_once('=').unwrap())
            .map(|(k, v)| (k.to_string(), dec(v)))
            .collect();
        assert!(q.contains(&("node".into(), "N".into())), "{url}");
        assert!(q.contains(&("code".into(), "012345".into())), "{url}");
        assert!(
            q.contains(&("addr".into(), "10.0.0.5:4433".into())),
            "{url}"
        );
        assert!(
            q.contains(&("relay".into(), "https://relay.example/".into())),
            "{url}"
        );
    }

    #[test]
    fn output_without_an_invite_is_an_error_not_an_empty_qr() {
        assert!(invite_from_output("").is_err());
        assert!(invite_from_output("Permission denied\n").is_err());
        // An object that is no invite (no node or code) is refused too.
        assert!(invite_from_output("{\"ok\":true}\n").is_err());
    }

    // Shape of the QR (a decode round trip needs a camera): size follows the link, quiet zone is light.
    #[test]
    fn the_terminal_qr_is_a_rectangle_of_cells_at_the_size_the_link_needs() {
        let url = "atlas-mobile://pair?node=N&code=012345&name=box";
        let qr = terminal_qr(url).unwrap();
        // Strip the colour codes: what is left is one glyph per cell.
        let plain = qr.replace("\x1b[30;47m", "").replace("\x1b[0m", "");
        let rows: Vec<&str> = plain.lines().collect();
        let code = QrCode::with_error_correction_level(url, EcLevel::L).unwrap();
        let cells = code.width() + 4;
        assert_eq!(rows.len(), cells.div_ceil(2), "two modules per row");
        assert!(rows.iter().all(|r| r.chars().count() == cells), "{qr}");
        // The quiet zone is light on every side: the first row and first column hold no dark half.
        assert!(rows[0].chars().all(|c| c == ' '), "{qr}");
        assert!(rows.iter().all(|r| r.starts_with(' ')), "{qr}");
        assert!(qr.starts_with("\x1b[30;47m"), "black on white");
    }
}
