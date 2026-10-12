//! `atlas hostd`: the per-user execution host behind Atlas Desktop.
//!
//! Owns PTYs (they outlive the app), one-shot `exec`, filesystem access and
//! scrollback. Clients speak NDJSON over `~/.atlas/hostd.sock` (see
//! [`protocol`]); the same binary runs on SSH hosts.

pub mod client;
mod exec;
mod fs;
pub mod handoff;
pub mod paths;
pub mod protocol;
mod pty;
pub mod server;
pub mod upgrade;

pub use client::HostClient;

/// Remove CSI/OSC/other escape sequences and C0 controls (keeps `\t`, `\n`).
/// `\r` resets the current line, so progress bars collapse to their last frame.
pub fn strip_ansi(s: &str) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum St {
        Ground,
        Esc,
        Csi,
        Osc,
        OscEsc,
    }
    let mut st = St::Ground;
    let mut out = String::with_capacity(s.len());
    let mut line_start = 0;
    let mut cr = false;
    for ch in s.chars() {
        if cr && st == St::Ground && ch != '\n' && ch != '\r' {
            out.truncate(line_start);
        }
        if st == St::Ground {
            cr = ch == '\r';
        }
        st = match (st, ch) {
            (St::Ground, '\x1b') => St::Esc,
            (St::Ground, '\n') => {
                out.push('\n');
                line_start = out.len();
                St::Ground
            }
            (St::Ground, '\r') => St::Ground,
            (St::Ground, '\t') => {
                out.push('\t');
                St::Ground
            }
            (St::Ground, c) if (c as u32) < 0x20 || c == '\x7f' => St::Ground,
            (St::Ground, c) => {
                out.push(c);
                St::Ground
            }
            (St::Esc, '[') => St::Csi,
            (St::Esc, ']' | 'P' | '_' | '^' | 'X') => St::Osc,
            (St::Esc, _) => St::Ground,
            (St::Csi, c) if ('\x40'..='\x7e').contains(&c) => St::Ground,
            (St::Csi, _) => St::Csi,
            (St::Osc, '\x07') => St::Ground,
            (St::Osc, '\x1b') => St::OscEsc,
            (St::Osc, _) => St::Osc,
            (St::OscEsc, '\\') => St::Ground,
            (St::OscEsc, _) => St::Osc,
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_ansi;

    #[test]
    fn strips_sequences_and_collapses_carriage_returns() {
        let s = "\x1b[31mred\x1b[0m\n\x1b]0;title\x07ok\r\nprog 1\rprog 2\n";
        assert_eq!(strip_ansi(s), "red\nok\nprog 2\n");
    }
}
