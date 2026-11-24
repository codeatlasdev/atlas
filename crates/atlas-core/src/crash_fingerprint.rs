//! The fingerprint of a crash report file (#320): what an issue search needs to tell that two crash files are one bug.
//!
//! - A panic report (`crash::write_report_in`) carries `fingerprint = "<16 hex>"` of its message and location, written
//!   with the file; an older file without the line gets the same value computed from those two fields.
//! - A **native** report (`crash_native`: a SIGSEGV, an access violation) is written in signal context, where nothing
//!   can be hashed or resolved, so it has no line and the value is computed when the file is read. It hashes
//!   `native crash: <signal>` and the first frame of the crash site:
//!   - **With symbols**: the `resolved` text is the output of `scripts/symbols.sh resolve` (`<frame> <function>
//!     <file:line>`, `docs/release.md` "Symbols"). The key is `function|file` of the first resolved frame inside the
//!     app, as for a page error (no line, no directory, no hash), so the same crash on two releases is one
//!     fingerprint. `atlas crash-fingerprint <file> --resolved <resolved>` prints it.
//!   - **Without symbols** (what the app on a user's machine has: it carries no debug info): `module+offset`. The module
//!     is the build (`build_id`, else `commit`) and the offset is the frame's address minus `load_bias`, which ASLR
//!     does not change, so the same crash of the same build on two machines is one fingerprint. Two releases are two
//!     fingerprints: an offset means nothing outside its build. The frame is the first one inside the executable's
//!     own address range (`EXE_SPAN` around the handler's frame, where shared libraries do not map) that is not part
//!     of the handler itself: the frames above the signal trampoline.
//!
//!   A report with no frame to key on (a platform without an unwinder, a fault wholly inside a shared library) has no
//!   fingerprint, so it never matches another crash.

use crate::fingerprint::{fingerprint, frame_key, with_key};

/// How far from the handler's own frame the executable is taken to extend (1 GiB): its frames are within it, and the
/// shared libraries of a 64-bit process (libc, WebKit, the dyld shared cache, ntdll) map farther away.
const EXE_SPAN: usize = 1 << 30;

/// The fingerprint of the crash report `text`, and `None` when it has nothing to key on. `resolved` is the output of
/// `scripts/symbols.sh resolve` for a native report, when the symbols of its build are at hand.
pub fn of_report(text: &str, resolved: Option<&str>) -> Option<String> {
    let field = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k)?.trim_start().strip_prefix('='))
            .map(|v| {
                let v = v.trim();
                serde_json::from_str::<String>(v)
                    .unwrap_or_else(|_| v.trim_matches('"').to_string())
            })
    };
    if field("kind").as_deref() != Some("native") {
        return field("fingerprint")
            .filter(|f| f.len() == 16 && f.bytes().all(|b| b.is_ascii_hexdigit()))
            .or_else(|| Some(fingerprint(&field("msg")?, &field("location")?)));
    }
    let signal = field("signal")?;
    let frames: Vec<usize> = text
        .lines()
        .find_map(|l| l.strip_prefix("frames = ["))
        .map(|l| l.trim_end_matches(']'))
        .into_iter()
        .flat_map(|l| l.split(','))
        .filter_map(|f| parse_hex(f.trim().trim_matches('"')))
        .collect();
    let key = resolved.and_then(symbol_key).or_else(|| {
        offset_key(
            &frames,
            field("load_bias").and_then(|b| parse_hex(&b))?,
            || {
                field("build_id")
                    .or_else(|| field("commit"))
                    .unwrap_or_default()
            },
        )
    })?;
    Some(with_key(&format!("native crash: {signal}"), &key))
}

fn parse_hex(s: &str) -> Option<usize> {
    usize::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

/// `function|file` of the first frame of `scripts/symbols.sh resolve` output that is the app's, skipping the frames
/// the crash handler and the failed lookups (`??`) contribute. `None` when none resolved.
fn symbol_key(resolved: &str) -> Option<String> {
    let stack: Vec<String> = resolved
        .lines()
        .filter_map(|l| {
            let mut words = l.split_whitespace().skip(1).collect::<Vec<_>>();
            let location = words.pop()?;
            let function = words.join(" ");
            let function = strip_symbol_hash(&function);
            (!function.is_empty() && function != "??" && !function.contains("crash_native"))
                .then(|| format!("{function}@{location}"))
        })
        .collect();
    let key = frame_key(&stack.join("\n"));
    (!key.is_empty()).then_some(key)
}

/// `core::panicking::panic::h0123456789abcdef` -> `core::panicking::panic` (a legacy Rust symbol's hash changes with
/// every build).
fn strip_symbol_hash(function: &str) -> &str {
    match function.rsplit_once("::h") {
        Some((head, hash)) if hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => {
            head
        }
        _ => function,
    }
}

/// `module+0xoffset` of the crash site: the first frame inside the executable after the handler's own frames (the
/// leading run of frames inside the executable: the handler, then the signal trampoline in a shared library, then the
/// crash site), minus `bias`.
///
/// "Inside the executable" is "within [`EXE_SPAN`] of the first frame", which is the handler's own and so is certainly
/// in it: the executable is one contiguous image, and the shared libraries (libc, the dyld shared cache, ntdll) map far
/// from it on every platform, whatever address its own debug info starts at (a Mach-O image's offsets begin at
/// `0x100000000`, an ELF's at 0).
fn offset_key(frames: &[usize], bias: usize, module: impl FnOnce() -> String) -> Option<String> {
    let anchor = *frames.first()?;
    let inside = |f: usize| f.abs_diff(anchor) < EXE_SPAN;
    let handler = frames.iter().take_while(|f| inside(**f)).count();
    let site = frames[handler..].iter().copied().find(|f| inside(*f))?;
    Some(format!("{}+{:#x}", module(), site.checked_sub(bias)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native(bias: &str, frames: &[&str]) -> String {
        format!(
            "version = \"1.5.0\"\ncommit = \"abc\"\nbuild_id = \"b1\"\nload_bias = \"{bias}\"\ntimestamp = 1\nkind = \"native\"\nsignal = \"SIGSEGV\"\nmsg = \"native crash: SIGSEGV\"\nframes = [{}]\n",
            frames
                .iter()
                .map(|f| format!("\"{f}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    /// Frames of one crash on two machines: ASLR moves the executable (the bias) and every address with it, and
    /// libc lands elsewhere; the handler's frames lead, the trampoline follows them, then the crash site.
    #[test]
    fn the_same_crash_under_another_load_address_is_one_fingerprint() {
        let a = of_report(
            &native(
                "0x55d000000000",
                &[
                    "0x55d000a11111",
                    "0x55d000a22222",
                    "0x7f0000011111",
                    "0x55d000c33333",
                    "0x55d000c44444",
                ],
            ),
            None,
        );
        let b = of_report(
            &native(
                "0x5612a0000000",
                &[
                    "0x5612a0a11111",
                    "0x5612a0a22222",
                    "0x7fa000099999",
                    "0x5612a0c33333",
                    "0x5612a0c44444",
                ],
            ),
            None,
        );
        assert!(a.is_some());
        assert_eq!(a, b);
        // Another crash site of the same build is another bug.
        let other = of_report(
            &native(
                "0x55d000000000",
                &[
                    "0x55d000a11111",
                    "0x55d000a22222",
                    "0x7f0000011111",
                    "0x55d000d00000",
                ],
            ),
            None,
        );
        assert_ne!(a, other);
    }

    /// A Mach-O image's debug-info addresses start at 0x100000000, so `frame - load_bias` is above 4 GiB for every frame
    /// of the executable; the dyld shared cache is mapped well above the image (#320, found on the macOS runner).
    #[test]
    fn a_mach_o_executable_is_told_from_the_shared_cache_by_where_its_frames_are() {
        let report = |slide: usize, cache_slide: usize| {
            native(
                &format!("{slide:#x}"),
                &[
                    &format!("{:#x}", 0x100a11111 + slide),
                    &format!("{:#x}", 0x100a22222 + slide),
                    &format!("{:#x}", 0x1_8123_4567 + cache_slide),
                    &format!("{:#x}", 0x100c33333 + slide),
                ],
            )
        };
        let a = of_report(&report(0x4000, 0x20_0000), None);
        assert!(a.is_some());
        assert_eq!(a, of_report(&report(0xc00_0000, 0x1c0_0000), None));
    }

    #[test]
    fn a_report_with_nothing_to_key_on_has_no_fingerprint() {
        assert_eq!(of_report(&native("0x55d000000000", &[]), None), None);
        // Every frame in a shared library, or only the handler's: no crash site inside the executable.
        assert_eq!(
            of_report(
                &native("0x55d000000000", &["0x7f0000011111", "0x7f0000022222"]),
                None
            ),
            None
        );
        assert_eq!(
            of_report(
                &native("0x55d000000000", &["0x55d000a11111", "0x55d000a22222"]),
                None
            ),
            None
        );
    }

    /// With symbols the key is the function and file, so the same crash on two releases (other offsets, other line
    /// numbers, other directories and hashes) is one fingerprint; without them it is not.
    #[test]
    fn resolved_frames_give_the_same_fingerprint_on_every_release() {
        let release_a = native(
            "0x55d000000000",
            &[
                "0x55d000a11111",
                "0x55d000a22222",
                "0x7f0000011111",
                "0x55d000c33333",
            ],
        );
        let release_b = native(
            "0x5612a0000000",
            &[
                "0x5612a0b99999",
                "0x5612a0b88888",
                "0x7fa000099999",
                "0x5612a0d77777",
            ],
        );
        let resolved_a = "0x55d000a11111 atlas_core::crash_native::imp::write_report::h0123456789abcdef /build/crates/atlas-core/src/crash_native.rs:340\n\
            0x55d000a22222 atlas_core::crash_native::imp::handler /build/crates/atlas-core/src/crash_native.rs:250\n\
            0x7f0000011111 ?? ??:0\n\
            0x55d000c33333 atlas_desktop::term::paint::h1111111111111111 /home/ci/work/atlas/crates/atlas-desktop/src/term/paint.rs:88\n";
        let resolved_b = "0x5612a0b99999 atlas_core::crash_native::imp::write_report::hfedcba9876543210 /src/crates/atlas-core/src/crash_native.rs:352\n\
            0x5612a0b88888 atlas_core::crash_native::imp::handler /src/crates/atlas-core/src/crash_native.rs:261\n\
            0x7fa000099999 ?? ??:0\n\
            0x5612a0d77777 atlas_desktop::term::paint::h2222222222222222 /Users/ana/atlas/crates/atlas-desktop/src/term/paint.rs:131\n";
        let a = of_report(&release_a, Some(resolved_a)).unwrap();
        assert_eq!(Some(a.clone()), of_report(&release_b, Some(resolved_b)));
        assert_ne!(
            Some(a),
            of_report(&release_a, None),
            "symbols and offsets are two keys"
        );
        assert_ne!(of_report(&release_a, None), of_report(&release_b, None));
        // Nothing resolved: the offset key stands in.
        assert_eq!(
            of_report(&release_a, Some("0x55d000c33333 ?? ??:0\n")),
            of_report(&release_a, None)
        );
    }

    #[test]
    fn a_panic_report_gives_its_own_line_and_an_old_one_the_same_value_from_its_fields() {
        let (msg, loc) = (
            "called `Option::unwrap()` on a `None` value",
            "crates/atlas-core/src/fs.rs:88:9",
        );
        let want = fingerprint(msg, loc);
        let old = format!("msg = \"{msg}\"\nlocation = \"{loc}\"\n");
        assert_eq!(of_report(&old, None), Some(want.clone()));
        let new = format!("msg = \"other\"\nlocation = \"{loc}\"\nfingerprint = \"{want}\"\n");
        assert_eq!(of_report(&new, None), Some(want));
        assert_eq!(of_report("timestamp = 1\n", None), None);
    }
}
