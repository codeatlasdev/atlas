//! Incremental OSC 0/2 window-title parser fed with raw pty output.

/// Incremental OSC 0/2 title parser (`ESC ] 0;title BEL` or `ESC ] 2;title ESC \`).
#[derive(Default)]
pub(super) struct TitleParser {
    st: u8,
    buf: Vec<u8>,
}

impl TitleParser {
    pub(super) fn feed(&mut self, bytes: &[u8]) -> Option<String> {
        let mut last = None;
        for &c in bytes {
            self.st = match (self.st, c) {
                (0, 0x1b) => 1,
                (0, _) => 0,
                (1, b']') => 2,
                (1, 0x1b) => 1,
                (1, _) => 0,
                (2, b'0' | b'2') => 3,
                (2, _) => 0,
                (3, b';') => {
                    self.buf.clear();
                    4
                }
                (3, _) => 0,
                (4, 0x07) => {
                    last = Some(String::from_utf8_lossy(&self.buf).into_owned());
                    0
                }
                (4, 0x1b) => 5,
                (4, _) => {
                    if self.buf.len() < 4096 {
                        self.buf.push(c);
                    }
                    4
                }
                (5, b'\\') => {
                    last = Some(String::from_utf8_lossy(&self.buf).into_owned());
                    0
                }
                (5, _) => 0,
                _ => 0,
            };
        }
        last
    }
}

/// Title as a tab shows it. ConPTY titles a Windows console with the full path of its program
/// (`C:\WINDOWS\System32\…\powershell.exe`, `…\cmd.exe - npm run dev`): the path becomes the
/// program's name. Every other title is the program's own and passes through.
pub(super) fn shown(t: String) -> String {
    let b = t.as_bytes();
    let start = (0..b.len().saturating_sub(2)).find(|&i| {
        (i == 0 || b[i - 1] == b' ')
            && b[i].is_ascii_alphabetic()
            && b[i + 1] == b':'
            && b[i + 2] == b'\\'
    });
    let Some(start) = start else { return t };
    let lower = t.to_ascii_lowercase();
    let Some(end) = lower[start..]
        .match_indices(".exe")
        .map(|(i, _)| start + i + 4)
        .find(|&e| e == t.len() || b[e] == b' ')
    else {
        return t;
    };
    let path = &t[start..end - 4];
    let name = path.rsplit('\\').next().unwrap_or(path);
    format!("{}{name}{}", &t[..start], &t[end..])
}
